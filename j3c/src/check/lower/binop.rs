use super::*;

impl<'a> Lower<'a> {
    pub(super) fn binary(
        &mut self,
        span: Span,
        op: parse::BinOp,
        lhs: &Expr,
        rhs: &Expr,
        expected: Option<Ty>,
    ) -> Result<Rvalue, CheckError> {
        let ir_op = ast_binop(op);
        match op {
            parse::BinOp::And | parse::BinOp::Or => {
                let l = self.operand(lhs, Some(self.cx.common.bool))?;
                let bool_ty = self.cx.common.bool;
                let n = self.fresh("sc");
                let id = self.b.tmp(n, bool_ty, span);
                let place = self.b.local_place(id, span);
                if op == parse::BinOp::And {
                    let saved = self.b.begin_block();
                    let r = self.operand(rhs, Some(bool_ty))?;
                    self.b.assign(span, place.clone(), Rvalue::use_op(span, r));
                    let then_block = self.b.end_block(saved);
                    let saved = self.b.begin_block();
                    self.b.assign(
                        span,
                        place.clone(),
                        Rvalue::use_op(span, Operand::bool_const(span, bool_ty, false)),
                    );
                    let else_block = self.b.end_block(saved);
                    self.b.push(crate::ir::stmt::Stmt {
                        ty: self.cx.common.void,
                        span,
                        kind: crate::ir::stmt::StmtKind::If {
                            cond: l,
                            then_block,
                            else_block: Some(else_block),
                        },
                    });
                } else {
                    let saved = self.b.begin_block();
                    self.b.assign(
                        span,
                        place.clone(),
                        Rvalue::use_op(span, Operand::bool_const(span, bool_ty, true)),
                    );
                    let then_block = self.b.end_block(saved);
                    let saved = self.b.begin_block();
                    let r = self.operand(rhs, Some(bool_ty))?;
                    self.b.assign(span, place.clone(), Rvalue::use_op(span, r));
                    let else_block = self.b.end_block(saved);
                    self.b.push(crate::ir::stmt::Stmt {
                        ty: self.cx.common.void,
                        span,
                        kind: crate::ir::stmt::StmtKind::If {
                            cond: l,
                            then_block,
                            else_block: Some(else_block),
                        },
                    });
                }
                let _ = ir_op;
                Ok(Rvalue::use_op(span, Operand::Copy(place)))
            }
            parse::BinOp::Eq
            | parse::BinOp::Ne
            | parse::BinOp::Lt
            | parse::BinOp::Le
            | parse::BinOp::Gt
            | parse::BinOp::Ge => {
                if bare_num(lhs) && !bare_num(rhs) {
                    let r = self.operand(rhs, None)?;
                    if is_numeric(self.cx, r.ty()) {
                        let l = self.operand(lhs, Some(r.ty()))?;
                        if l.ty() != r.ty() {
                            return Err(CheckError::new(self.path, span, "compare type mismatch"));
                        }
                        return Ok(Rvalue::binary(span, self.cx.common.bool, ir_op, l, r));
                    }
                    if self.is_ptr_ty(r.ty()) && self.fold_int(lhs) == Some(0) {
                        let l = Operand::null(lhs.span, r.ty());
                        return Ok(Rvalue::binary(span, self.cx.common.bool, ir_op, l, r));
                    }
                    return Err(CheckError::new(self.path, span, "compare type mismatch"));
                }
                let l = self.operand(lhs, None)?;
                if let Some(id) = find_operator(self.cx, l.ty(), op, self.methods) {
                    let fty = self.cx.func(id).ty;
                    let (params, ret) = match self.cx.fn_sig(fty) {
                        Some(s) => (s.0.clone(), s.1),
                        None => {
                            return Err(CheckError::new(self.path, span, "not a function"));
                        }
                    };
                    if params.len() != 2 {
                        return Err(CheckError::new(
                            self.path,
                            span,
                            "binary operator needs two parameters",
                        ));
                    }
                    if l.ty() != params[0] {
                        return Err(CheckError::new(
                            self.path,
                            span,
                            "operator `this` type mismatch",
                        ));
                    }
                    let r = self.operand(rhs, Some(params[1]))?;
                    return Ok(Rvalue::call(
                        span,
                        ret,
                        Operand::fn_addr(span, fty, id),
                        vec![l, r],
                    ));
                }
                let r_expected = Some(l.ty());
                let r = if self.is_ptr_ty(l.ty()) && self.fold_int(rhs) == Some(0) {
                    Operand::null(rhs.span, l.ty())
                } else {
                    self.operand(rhs, r_expected)?
                };
                if l.ty() != r.ty() {
                    if self.is_ptr_ty(r.ty()) && self.fold_int(lhs) == Some(0) {
                        let l = Operand::null(lhs.span, r.ty());
                        return Ok(Rvalue::binary(span, self.cx.common.bool, ir_op, l, r));
                    }
                    return Err(CheckError::new(self.path, span, "compare type mismatch"));
                }
                Ok(Rvalue::binary(span, self.cx.common.bool, ir_op, l, r))
            }
            parse::BinOp::Add
            | parse::BinOp::Sub
            | parse::BinOp::Mul
            | parse::BinOp::Div
            | parse::BinOp::Rem => {
                if bare_num(lhs) && !bare_num(rhs) {
                    let right_expect = match expected {
                        Some(t) if is_numeric(self.cx, t) => Some(t),
                        _ => None,
                    };
                    let r = self.operand(rhs, right_expect)?;
                    if !is_numeric(self.cx, r.ty()) {
                        return Err(CheckError::new(
                            self.path,
                            span,
                            "arithmetic type mismatch",
                        ));
                    }
                    let l = self.operand(lhs, Some(r.ty()))?;
                    if l.ty() != r.ty() {
                        return Err(CheckError::new(
                            self.path,
                            span,
                            "arithmetic type mismatch",
                        ));
                    }
                    if matches!(op, parse::BinOp::Div | parse::BinOp::Rem) && is_int(self.cx, r.ty()) {
                        self.abort_if_zero(span, &r)?;
                    }
                    return Ok(Rvalue::binary(span, r.ty(), ir_op, l, r));
                }
                let l = self.operand(lhs, None)?;
                if let Some(id) = find_operator(self.cx, l.ty(), op, self.methods) {
                    let fty = self.cx.func(id).ty;
                    let (params, ret) = match self.cx.fn_sig(fty) {
                        Some(s) => (s.0.clone(), s.1),
                        None => {
                            return Err(CheckError::new(self.path, span, "not a function"));
                        }
                    };
                    if params.len() != 2 {
                        return Err(CheckError::new(
                            self.path,
                            span,
                            "binary operator needs two parameters",
                        ));
                    }
                    if l.ty() != params[0] {
                        return Err(CheckError::new(
                            self.path,
                            span,
                            "operator `this` type mismatch",
                        ));
                    }
                    let r = self.operand(rhs, Some(params[1]))?;
                    return Ok(Rvalue::call(
                        span,
                        ret,
                        Operand::fn_addr(span, fty, id),
                        vec![l, r],
                    ));
                }
                let ty = if is_numeric(self.cx, l.ty()) {
                    l.ty()
                } else if let Some(t) = expected {
                    if is_numeric(self.cx, t) {
                        t
                    } else {
                        self.cx.common.int
                    }
                } else {
                    self.cx.common.int
                };
                if l.ty() != ty {
                    return Err(CheckError::new(self.path, span, "arithmetic type mismatch"));
                }
                let r = self.operand(rhs, Some(ty))?;
                if r.ty() != ty {
                    return Err(CheckError::new(self.path, span, "arithmetic type mismatch"));
                }
                if matches!(op, parse::BinOp::Div | parse::BinOp::Rem) && is_int(self.cx, ty) {
                    self.abort_if_zero(span, &r)?;
                }
                let _ = expected;
                Ok(Rvalue::binary(span, ty, ir_op, l, r))
            }
        }
    }

    pub(super) fn abort_if_zero(&mut self, span: Span, rhs: &Operand) -> Result<(), CheckError> {
        let bool_ty = self.cx.common.bool;
        let n = self.fresh("dz");
        let id = self.b.tmp(n, bool_ty, span);
        let p = self.b.local_place(id, span);
        self.b.assign(
            span,
            p.clone(),
            Rvalue::binary(
                span,
                bool_ty,
                BinOp::Eq,
                rhs.clone(),
                Operand::int(span, rhs.ty(), 0),
            ),
        );
        let saved = self.b.begin_block();
        self.b.push(crate::ir::stmt::Stmt {
            ty: self.cx.common.void,
            span,
            kind: crate::ir::stmt::StmtKind::Div0,
        });
        let then_block = self.b.end_block(saved);
        self.b.push(crate::ir::stmt::Stmt {
            ty: self.cx.common.void,
            span,
            kind: crate::ir::stmt::StmtKind::If {
                cond: Operand::Copy(p),
                then_block,
                else_block: None,
            },
        });
        Ok(())
    }

    /// `alias.NAME` for a const exported by a `use`d module. Locals, aliases, and globals win.
    pub(super) fn qualified_const(&self, expr: &Expr) -> Option<ConstVal> {
        let ExprKind::Field { base, field } = &expr.kind else { return None };
        let ExprKind::Name(n) = &base.kind else { return None };
        if self.shadows(&n.name) {
            return None;
        }
        self.consts.get(&crate::check::qual_key(&n.name, &field.name)).cloned()
    }

    pub(super) fn shadows(&self, name: &str) -> bool {
        self.locals.contains_key(name) || self.aliases.contains_key(name) || self.globals.contains_key(name)
    }

    pub(super) fn fold_int(&self, expr: &Expr) -> Option<i64> {
        match &expr.kind {
            ExprKind::Int(v) => Some(*v),
            ExprKind::Field { .. } => match self.qualified_const(expr) {
                Some(ConstVal {
                    kind: ConstPayload::Int(v),
                    ..
                }) => Some(v),
                _ => None,
            },
            ExprKind::Name(n) => match self.consts.get(&n.name) {
                Some(ConstVal {
                    kind: ConstPayload::Int(v),
                    ..
                }) => Some(*v),
                _ => None,
            },
            ExprKind::Binary { op, lhs, rhs } => {
                let a = self.fold_int(lhs)?;
                let b = self.fold_int(rhs)?;
                crate::ir::wrap::fold_bin(ast_binop(*op), a, b)
            }
            ExprKind::Unary {
                op: parse::UnaryOp::Neg,
                expr,
            } => Some(crate::ir::wrap::fold_neg(self.fold_int(expr)?)),
            _ => None,
        }
    }

    pub(super) fn num_from_int(&self, span: Span, v: i64, expected: Option<Ty>) -> Operand {
        match expected {
            Some(t) if v == 0 && self.is_ptr_ty(t) => Operand::null(span, t),
            Some(t) if is_int(self.cx, t) => Operand::int(span, t, v as u64),
            Some(t) if t == self.cx.common.f32 => Operand::f32_const(span, t, v as f32),
            Some(t) if is_float(self.cx, t) => Operand::f64_const(span, t, v as f64),
            _ => Operand::int(span, self.cx.common.int, v as u64),
        }
    }

    pub(super) fn num_from_float(&self, span: Span, v: f64, expected: Option<Ty>) -> Operand {
        match expected {
            Some(t) if t == self.cx.common.f32 => Operand::f32_const(span, t, v as f32),
            Some(t) if is_float(self.cx, t) => Operand::f64_const(span, t, v),
            _ => Operand::f64_const(span, self.cx.common.float, v),
        }
    }
}
