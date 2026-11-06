use super::*;

impl<'a> Lower<'a> {
    pub(super) fn stmt(&mut self, stmt: &Stmt) -> Result<(), CheckError> {
        match stmt {
            Stmt::While(_) | Stmt::For(_) | Stmt::ForEach(_) => {
                let label = self.pending_label.take();
                self.loops.push(label);
                let r = self.stmt_inner(stmt);
                self.loops.pop();
                r
            }
            Stmt::Labeled(s) => {
                if self.loops.iter().any(|l| l.as_deref() == Some(s.label.name.as_str())) {
                    return Err(CheckError::new(
                        self.path,
                        s.label.span,
                        format!("label `'{}` is already used by an enclosing loop", s.label.name),
                    ));
                }
                self.pending_label = Some(s.label.name.clone());
                self.stmt(&s.body)
            }
            _ => self.stmt_inner(stmt),
        }
    }

    fn stmt_inner(&mut self, stmt: &Stmt) -> Result<(), CheckError> {
        match stmt {
            Stmt::Let(s) => {
                if self.is_declared(&s.name.name) {
                    return Err(CheckError::new(
                        self.path,
                        s.name.span,
                        format!("shadowing `{}` is a compile error", s.name.name),
                    ));
                }
                let want = match &s.ty {
                    Some(t) => Some(self.resolve_ty(t)?),
                    None => None,
                };
                let rv = self.rvalue(&s.value, want)?;
                if let Some(want) = want {
                    if rv.ty != want {
                        return Err(self.mismatch(
                            s.value.span,
                            "let type does not match initializer",
                            rv.ty,
                            want,
                        ));
                    }
                }
                let n = self.cx.intern_name(&s.name.name);
                let id = self.b.let_local(n, rv.ty, s.name.span);
                self.b.set_doc(id, s.doc.clone());
                let place = self.b.local_place(id, s.name.span);
                self.b.assign(s.span, place, rv);
                self.locals.insert(s.name.name.clone(), id);
                Ok(())
            }
            Stmt::Return(s) => match &s.value {
                None => {
                    if self.ret != self.cx.common.void {
                        return Err(CheckError::new(self.path, s.span, "return needs a value"));
                    }
                    self.b.return_op(s.span, None);
                    Ok(())
                }
                Some(e) => {
                    if self.ret == self.cx.common.void {
                        return Err(CheckError::new(
                            self.path,
                            s.span,
                            "return value in void function",
                        ));
                    }
                    let op = self.operand(e, Some(self.ret))?;
                    if op.ty() != self.ret {
                        return Err(self.mismatch(s.span, "return type mismatch", op.ty(), self.ret));
                    }
                    self.b.return_op(s.span, Some(op));
                    Ok(())
                }
            },
            Stmt::Assign(s) => {
                self.store_indirect = false;
                let place = self.place(&s.place)?;
                if self.writes_temporary(&s.place, &place) {
                    return Err(CheckError::new(
                        self.path,
                        temp_span(&s.place),
                        "cannot assign to a temporary (the result of a call or expression)",
                    ));
                }
                if !place.access.assign {
                    if let Some(prop) = self.script_prop_name(&s.place) {
                        return Err(CheckError::new(
                            self.path,
                            s.place.span,
                            format!("prop `{prop}` is read-only in the script"),
                        )
                        .help("props are read-only in the script; copy it into a `let`"));
                    }
                    let e = CheckError::new(
                        self.path,
                        s.span,
                        self.readonly_global(&place).unwrap_or("cannot assign to this place".to_string()),
                    );
                    let in_string = match &s.place.kind {
                        parse::ExprKind::Index { base, .. } => {
                            self.try_place(base).is_ok_and(|p| p.ty == self.cx.common.string)
                        }
                        _ => false,
                    };
                    return Err(if in_string {
                        e.help("a `string` is a read-only byte view: build or edit bytes with `j3.text` (`j3c doc j3.text`)")
                    } else {
                        e
                    });
                }
                let rv = self.rvalue(&s.value, Some(place.ty))?;
                if rv.ty != place.ty {
                    return Err(self.mismatch(s.span, "assign type mismatch", rv.ty, place.ty));
                }
                self.b.assign(s.span, place, rv);
                Ok(())
            }
            Stmt::AssignOp(s) => {
                self.store_indirect = false;
                let place = self.place(&s.place)?;
                if self.writes_temporary(&s.place, &place) {
                    return Err(CheckError::new(
                        self.path,
                        temp_span(&s.place),
                        "cannot assign to a temporary (the result of a call or expression)",
                    ));
                }
                if !place.access.assign {
                    if let Some(prop) = self.script_prop_name(&s.place) {
                        return Err(CheckError::new(
                            self.path,
                            s.place.span,
                            format!("prop `{prop}` is read-only in the script"),
                        )
                        .help("props are read-only in the script; copy it into a `let`"));
                    }
                    return Err(CheckError::new(
                        self.path,
                        s.span,
                        self.readonly_global(&place).unwrap_or("cannot assign-op this place".to_string()),
                    ));
                }
                if let Some(id) = find_operator(self.cx, place.ty, s.op, self.methods) {
                    let fty = self.cx.func(id).ty;
                    let (params, ret) = match self.cx.fn_sig(fty) {
                        Some(sig) => (sig.0.clone(), sig.1),
                        None => {
                            return Err(CheckError::new(self.path, s.span, "not a function"));
                        }
                    };
                    if params.len() != 2 {
                        return Err(CheckError::new(
                            self.path,
                            s.span,
                            "binary operator needs two parameters",
                        ));
                    }
                    let rhs = self.operand(&s.value, Some(params[1]))?;
                    let lhs = Operand::Copy(place.clone());
                    let call = Rvalue::call(
                        s.span,
                        ret,
                        Operand::fn_addr(s.span, fty, id),
                        vec![lhs, rhs],
                    );
                    if call.ty != place.ty {
                        return Err(self.mismatch(
                            s.span,
                            "assign-op type mismatch",
                            call.ty,
                            place.ty,
                        ));
                    }
                    self.b.assign(s.span, place, call);
                    return Ok(());
                }
                if self.is_half(place.ty) {
                    return Err(self.half_error(s.span, &format!("`{}=`", ast_binop(s.op).as_str())));
                }
                if is_vec(self.cx, place.ty) {
                    let r = self.vec_rhs(&s.value, place.ty)?;
                    let rv = self.vec_arith(s.span, s.op, Operand::Copy(place.clone()), r)?;
                    self.b.assign(s.span, place, rv);
                    return Ok(());
                }
                let rhs = self.operand(&s.value, Some(place.ty))?;
                if rhs.ty() != place.ty {
                    return Err(self.mismatch(s.value.span, "assign-op type mismatch", rhs.ty(), place.ty));
                }
                if matches!(s.op, parse::BinOp::Div | parse::BinOp::Rem) && is_int(self.cx, place.ty) {
                    self.abort_if_zero(s.span, &rhs)?;
                }
                let ir_op = ast_binop(s.op);
                self.b.assign_op(s.span, place, ir_op, rhs);
                Ok(())
            }
            Stmt::Expr(e) => {
                let rv = self.rvalue(e, None)?;
                self.b.expr(e.span, rv);
                Ok(())
            }
            Stmt::If(s) => {
                let cond = self.operand(&s.cond, Some(self.cx.common.bool))?;
                let then_block = self.lower_block(&s.then_block)?;
                let else_block = match &s.else_block {
                    Some(b) => Some(self.lower_block(b)?),
                    None => None,
                };
                self.b.push(crate::ir::stmt::Stmt {
                    ty: self.cx.common.void,
                    span: s.span,
                    kind: crate::ir::stmt::StmtKind::If {
                        cond,
                        then_block,
                        else_block,
                    },
                });
                Ok(())
            }
            Stmt::While(s) => {
                let saved = self.b.begin_block();
                let cond = self.operand(&s.cond, Some(self.cx.common.bool))?;
                let not_n = self.fresh("not");
                let not_id = self.b.tmp(not_n, self.cx.common.bool, s.cond.span);
                let not_place = self.b.local_place(not_id, s.cond.span);
                self.b.assign(
                    s.cond.span,
                    not_place.clone(),
                    Rvalue {
                        ty: self.cx.common.bool,
                        span: s.cond.span,
                        kind: RvalueKind::Unary {
                            op: UnaryOp::Not,
                            arg: cond,
                        },
                    },
                );
                let br = self.b.begin_block();
                self.b.push(crate::ir::stmt::Stmt {
                    ty: self.cx.common.void,
                    span: s.span,
                    kind: crate::ir::stmt::StmtKind::Break(0),
                });
                let br_block = self.b.end_block(br);
                self.b.push(crate::ir::stmt::Stmt {
                    ty: self.cx.common.void,
                    span: s.span,
                    kind: crate::ir::stmt::StmtKind::If {
                        cond: Operand::Copy(not_place),
                        then_block: br_block,
                        else_block: None,
                    },
                });
                self.scoped_stmts(&s.body.stmts)?;
                let body = self.b.end_block(saved);
                self.b.push(crate::ir::stmt::Stmt {
                    ty: self.cx.common.void,
                    span: s.span,
                    kind: crate::ir::stmt::StmtKind::While {
                        cond: Operand::bool_const(s.span, self.cx.common.bool, true),
                        body,
                    },
                });
                Ok(())
            }
            Stmt::UseLocal(s) => self.use_local(s),
            Stmt::ForEach(s) => self.for_each(s),
            Stmt::For(s) => {
                if self.is_declared(&s.name.name) {
                    return Err(CheckError::new(
                        self.path,
                        s.name.span,
                        format!("shadowing `{}` is a compile error", s.name.name),
                    ));
                }
                let start = self.operand(&s.start, Some(self.cx.common.int))?;
                let mut end = self.operand(&s.end, Some(self.cx.common.int))?;
                if s.inclusive {
                    let one = Operand::int(s.end.span, self.cx.common.int, 1);
                    let n = self.fresh("inc");
                    let tid = self.b.tmp(n, self.cx.common.int, s.end.span);
                    let p = self.b.local_place(tid, s.end.span);
                    self.b.assign(
                        s.span,
                        p.clone(),
                        Rvalue::binary(s.end.span, self.cx.common.int, BinOp::Add, end, one),
                    );
                    end = Operand::Copy(p);
                }
                let n = self.cx.intern_name(&s.name.name);
                let id = self.b.let_local(n, self.cx.common.int, s.name.span);
                let place = self.b.local_place(id, s.name.span);
                self.b
                    .assign(s.span, place.clone(), Rvalue::use_op(s.start.span, start));
                self.locals.insert(s.name.name.clone(), id);
                let body = self.lower_block(&s.body)?;
                self.b.push(crate::ir::stmt::Stmt {
                    ty: self.cx.common.void,
                    span: s.span,
                    kind: crate::ir::stmt::StmtKind::ForRange {
                        index: place,
                        end,
                        body,
                    },
                });
                self.locals.remove(&s.name.name);
                Ok(())
            }
            Stmt::Defer(s) => {
                let body = self.lower_block(&s.body)?;
                self.b.push(crate::ir::stmt::Stmt {
                    ty: self.cx.common.void,
                    span: s.span,
                    kind: crate::ir::stmt::StmtKind::Defer { body },
                });
                Ok(())
            }
            Stmt::Break(span, label) | Stmt::Continue(span, label) => {
                let what = if matches!(stmt, Stmt::Break(..)) { "break" } else { "continue" };
                let depth = match label {
                    None if self.loops.is_empty() => {
                        return Err(self.loop_err(*span, format!("`{what}` outside a loop")));
                    }
                    None => 0,
                    Some(l) => match self.loops.iter().rev().position(|x| x.as_deref() == Some(l.name.as_str())) {
                        Some(d) => d as u32,
                        None => {
                            return Err(self.loop_err(
                                l.span,
                                format!("no enclosing loop is labeled `'{}`", l.name),
                            ))
                        }
                    },
                };
                let kind = if what == "break" {
                    crate::ir::stmt::StmtKind::Break(depth)
                } else {
                    crate::ir::stmt::StmtKind::Continue(depth)
                };
                self.b.push(crate::ir::stmt::Stmt { ty: self.cx.common.void, span: *span, kind });
                Ok(())
            }
            Stmt::Match(s) => self.lower_match(s),
            Stmt::Labeled(_) => self.stmt(stmt), // `stmt` handles labels; never reached
            Stmt::Block(b) => {
                let body = self.lower_block(b)?;
                self.b.push(crate::ir::stmt::Stmt {
                    ty: self.cx.common.void,
                    span: b.span,
                    kind: crate::ir::stmt::StmtKind::Block(body),
                });
                Ok(())
            }
        }
    }

    pub(super) fn lower_block(&mut self, block: &parse::Block) -> Result<crate::ir::body::Block, CheckError> {
        let saved = self.b.begin_block();
        self.scoped_stmts(&block.stmts)?;
        Ok(self.b.end_block(saved))
    }

    /// Lowers `stmts` as one block: a `let` or `use` inside ends with it.
    pub(super) fn scoped_stmts(&mut self, stmts: &[Stmt]) -> Result<(), CheckError> {
        let saved_locals = self.locals.clone();
        let saved_aliases = self.aliases.clone();
        for stmt in stmts {
            self.stmt(stmt)?;
        }
        self.locals = saved_locals;
        self.aliases = saved_aliases;
        Ok(())
    }

    pub(super) fn for_each(&mut self, s: &parse::ForEachStmt) -> Result<(), CheckError> {
        if let Some(i) = &s.index {
            if self.is_declared(&i.name) {
                return Err(CheckError::new(
                    self.path,
                    i.span,
                    format!("shadowing `{}` is a compile error", i.name),
                ));
            }
        }
        if self.is_declared(&s.name.name) {
            return Err(CheckError::new(
                self.path,
                s.name.span,
                format!("shadowing `{}` is a compile error", s.name.name),
            ));
        }
        let mut iter = self.try_place(&s.iter)?;
        while self.cx.ref_inner(iter.ty).is_some() {
            iter = self.b.deref(self.cx, iter, s.span);
        }
        let elem = match self.cx.elem_ty(iter.ty) {
            Some(e) => e,
            None => {
                return Err(CheckError::new(
                    self.path,
                    s.iter.span,
                    "`for` over non-array/slice",
                ));
            }
        };
        let end = self.seq_len(&iter, s.span)?;
        let iname = match &s.index {
            Some(i) => i.name.clone(),
            None => format!("_i{}", self.tmp),
        };
        let ispan = match &s.index {
            Some(i) => i.span,
            None => s.span,
        };
        let n = self.cx.intern_name(&iname);
        let iid = self.b.let_local(n, self.cx.common.int, ispan);
        let iplace = self.b.local_place(iid, ispan);
        self.b.assign(
            s.span,
            iplace.clone(),
            Rvalue::use_op(s.span, Operand::int(s.span, self.cx.common.int, 0)),
        );
        if s.index.is_some() {
            self.locals.insert(iname.clone(), iid);
        }
        let nn = self.cx.intern_name(&s.name.name);
        let nid = self.b.let_local(nn, elem, s.name.span);
        self.locals.insert(s.name.name.clone(), nid);
        let saved = self.b.begin_block();
        let elem_place = self
            .b
            .index(self.cx, iter, Operand::Copy(iplace.clone()), s.span);
        let nplace = self.b.local_place(nid, s.name.span);
        self.b.assign(
            s.span,
            nplace,
            Rvalue::use_op(s.span, Operand::Copy(elem_place)),
        );
        self.scoped_stmts(&s.body.stmts)?;
        let body = self.b.end_block(saved);
        self.b.push(crate::ir::stmt::Stmt {
            ty: self.cx.common.void,
            span: s.span,
            kind: crate::ir::stmt::StmtKind::ForRange {
                index: iplace,
                end,
                body,
            },
        });
        if s.index.is_some() {
            self.locals.remove(&iname);
        }
        self.locals.remove(&s.name.name);
        Ok(())
    }

    pub(super) fn use_local(&mut self, s: &parse::UseLocalStmt) -> Result<(), CheckError> {
        let mut place = match self.try_place(&Expr {
            span: s.name.span,
            kind: ExprKind::Name(s.name.clone()),
        }) {
            Ok(p) => p,
            Err(_) => {
                return Err(CheckError::new(
                    self.path,
                    s.name.span,
                    format!("unknown name `{}`", s.name.name),
                ));
            }
        };
        while self.cx.ref_inner(place.ty).is_some() {
            place = self.b.deref(self.cx, place, s.span);
        }
        let sid = match self.cx.ty_kind(place.ty) {
            TyKind::Struct(id) => *id,
            _ => {
                return Err(CheckError::new(
                    self.path,
                    s.name.span,
                    "`use` of a local only works on a struct",
                ));
            }
        };
        let names = embed_alias_names(self.cx, sid);
        for name in names {
            if self.is_declared(&name) || self.aliases.contains_key(&name) {
                return Err(CheckError::new(
                    self.path,
                    s.name.span,
                    format!("shadowing `{name}` is a compile error"),
                ));
            }
            let path = field_path(self.cx, sid, &name).expect("embed name");
            let mut p = place.clone();
            for fid in path {
                p = self.b.field(self.cx, p, fid, s.span);
            }
            self.aliases.insert(name, p);
        }
        Ok(())
    }
}
