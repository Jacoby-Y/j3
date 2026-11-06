use super::*;

impl<'a> Lower<'a> {
    pub(super) fn place(&mut self, expr: &Expr) -> Result<Place, CheckError> {
        match self.try_place(expr) {
            Ok(p) => Ok(p),
            Err(e) => Err(e),
        }
    }

    /// Base of a field or index: a place, or any other expression (`f().x`, `xs[a..b].len`, `(a + b).x`),
    /// spilled to a temp. Stores into it are rejected in `Assign`/`AssignOp` (`writes_temporary`).
    pub(super) fn base_place(&mut self, expr: &Expr) -> Result<Place, CheckError> {
        if matches!(
            expr.kind,
            ExprKind::Name(_)
                | ExprKind::String(_)
                | ExprKind::Index { .. }
                | ExprKind::Field { .. }
                | ExprKind::Unary { op: parse::UnaryOp::Deref, .. }
        ) {
            return self.try_place(expr);
        }
        let keep = self.store_indirect;
        let rv = self.rvalue(expr, None)?;
        self.store_indirect = keep;
        let n = self.fresh("i");
        let id = self.b.tmp(n, rv.ty, expr.span);
        let p = self.b.local_place(id, expr.span);
        self.b.assign(expr.span, p.clone(), rv);
        Ok(p)
    }

    /// True when a store into `place` (lowered from `expr`) would only hit a spilled temporary (`f().x = 1`).
    /// A store through a reference result (`f().x` with `f(): &P`) has a Deref and is fine.
    pub(super) fn writes_temporary(&self, expr: &Expr, place: &Place) -> bool {
        fn rooted(expr: &Expr) -> bool {
            match &expr.kind {
                ExprKind::Field { base, .. } | ExprKind::Index { base, .. } => rooted(base),
                ExprKind::Name(_) | ExprKind::String(_) | ExprKind::Unary { op: parse::UnaryOp::Deref, .. } => false,
                _ => true,
            }
        }
        rooted(expr) && !self.store_indirect && !place.proj.iter().any(|p| matches!(p, crate::ir::place::Proj::Deref))
    }

    /// Names the array const a store would hit, for the error message.
    pub(super) fn readonly_global(&self, place: &Place) -> Option<String> {
        match place.root {
            crate::ir::place::PlaceRoot::Global(g) if self.cx.global(g).readonly => Some(format!(
                "cannot assign into const array `{}`",
                self.cx.name_str(self.cx.global(g).name)
            )),
            _ => None,
        }
    }

    /// Array consts are read-only views: reads and slices work, stores do not.
    pub(super) fn global_access(&self, gid: GlobalId) -> crate::ir::place::Access {
        if self.cx.global(gid).readonly {
            crate::ir::place::Access::read_only_view()
        } else {
            crate::ir::place::Access::let_binding()
        }
    }

    pub(super) fn try_place(&mut self, expr: &Expr) -> Result<Place, CheckError> {
        match &expr.kind {
            ExprKind::Name(n) => {
                if let Some(place) = self.aliases.get(&n.name).cloned() {
                    return Ok(place);
                }
                if let Some(id) = self.locals.get(&n.name).copied() {
                    return Ok(self.b.local_place(id, n.span));
                }
                if let Some(&gid) = self.globals.get(&n.name) {
                    let ty = self.cx.global(gid).ty;
                    return Ok(Place::global(gid, ty, self.global_access(gid), n.span));
                }
                Err(CheckError::new(
                    self.path,
                    n.span,
                    format!("`{}` is not a place", n.name),
                ))
            }
            ExprKind::String(bytes) => {
                let n = self.fresh("s");
                let id = self.b.tmp(n, self.cx.common.string, expr.span);
                let place = self.b.local_place(id, expr.span);
                self.b.assign(
                    expr.span,
                    place.clone(),
                    Rvalue::use_op(
                        expr.span,
                        Operand::bytes(expr.span, self.cx.common.string, bytes),
                    ),
                );
                Ok(place)
            }
            ExprKind::Index { base, index } => {
                let mut place = self.base_place(base)?;
                while self.cx.ref_inner(place.ty).is_some() {
                    place = self.b.deref(self.cx, place, expr.span);
                }
                if is_vec(self.cx, place.ty) {
                    return self.vec_index_place(place, index, expr.span);
                }
                if matches!(self.cx.ty_kind(place.ty), TyKind::Slice { .. }) {
                    self.store_indirect = true;
                }
                if self.cx.elem_ty(place.ty).is_none() {
                    return Err(CheckError::new(
                        self.path,
                        expr.span,
                        "index of non-array/slice",
                    ));
                }
                let keep = self.store_indirect;
                let idx = self.operand(index, Some(self.cx.common.int))?;
                self.store_indirect = keep;
                let idx = self.bind_operand(idx, expr.span);
                let len = self.seq_len(&place, expr.span)?;
                self.bounds_check(expr.span, &idx, &len)?;
                Ok(self.b.index(self.cx, place, idx, expr.span))
            }
            ExprKind::Field { base, field } => {
                let mut place = self.base_place(base)?;
                while self.cx.ref_inner(place.ty).is_some() {
                    place = self.b.deref(self.cx, place, field.span);
                }
                match self.cx.ty_kind(place.ty) {
                    TyKind::Struct(sid) => {
                        let sid = *sid;
                        if self.cx.struct_def(sid).script.is_some() && self.script_sid != Some(sid)
                        {
                            return Err(CheckError::new(
                                self.path,
                                field.span,
                                "script fields are private",
                            )
                            .help("add a `pub fn` to expose this value"));
                        }
                        match field_path(self.cx, sid, &field.name) {
                            Some(path) => {
                                let mut place = place;
                                let prop_field = path.first().map(|f| f.index as usize);
                                for fid in path {
                                    place = self.b.field(self.cx, place, fid, field.span);
                                }
                                if self.script_sid == Some(sid) {
                                    if let (Some(meta), Some(index)) =
                                        (&self.cx.struct_def(sid).script, prop_field)
                                    {
                                        if index < meta.prop_spans.len() {
                                            place.access.assign = false;
                                        }
                                    }
                                }
                                Ok(place)
                            }
                            None => Err(crate::check::error::no_field(self.cx, self.path, field.span, sid, &field.name)),
                        }
                    }
                    TyKind::String | TyKind::Slice { .. } => match field.name.as_str() {
                        "ptr" => Ok(self.b.slice_ptr(self.cx, place, field.span)),
                        "len" => Ok(self.b.slice_len(self.cx, place, field.span)),
                        other => Err(CheckError::new(
                            self.path,
                            field.span,
                            format!("no field `.{other}`"),
                        )),
                    },
                    TyKind::Vec(_) => self.vec_field_place(place, field),
                    _ => Err(CheckError::new(
                        self.path,
                        field.span,
                        format!("no field `{}`", field.name),
                    )),
                }
            }
            ExprKind::Unary {
                op: parse::UnaryOp::Deref,
                expr: inner,
            } => {
                let inner_place = self.try_place(inner)?;
                Ok(self.b.deref(self.cx, inner_place, expr.span))
            }
            _ => Err(CheckError::new(
                self.path,
                expr.span,
                "expression is not a place",
            )),
        }
    }

    pub(super) fn bounds_check(
        &mut self,
        span: Span,
        index: &Operand,
        len: &Operand,
    ) -> Result<(), CheckError> {
        let int = self.cx.common.int;
        let bool_ty = self.cx.common.bool;
        let lt = {
            let n = self.fresh("bc");
            let id = self.b.tmp(n, bool_ty, span);
            let p = self.b.local_place(id, span);
            self.b.assign(
                span,
                p.clone(),
                Rvalue::binary(
                    span,
                    bool_ty,
                    BinOp::Lt,
                    index.clone(),
                    Operand::int(span, int, 0),
                ),
            );
            Operand::Copy(p)
        };
        let ge = {
            let n = self.fresh("bc");
            let id = self.b.tmp(n, bool_ty, span);
            let p = self.b.local_place(id, span);
            self.b.assign(
                span,
                p.clone(),
                Rvalue::binary(span, bool_ty, BinOp::Ge, index.clone(), len.clone()),
            );
            Operand::Copy(p)
        };
        let cond = {
            let n = self.fresh("bc");
            let id = self.b.tmp(n, bool_ty, span);
            let p = self.b.local_place(id, span);
            self.b.assign(
                span,
                p.clone(),
                Rvalue::binary(span, bool_ty, BinOp::Or, lt, ge),
            );
            Operand::Copy(p)
        };
        let saved = self.b.begin_block();
        self.b.push(crate::ir::stmt::Stmt {
            ty: self.cx.common.void,
            span,
            kind: crate::ir::stmt::StmtKind::Abort(crate::ir::stmt::AbortKind::Index, vec![index.clone(), len.clone()]),
        });
        let then_block = self.b.end_block(saved);
        self.b.push(crate::ir::stmt::Stmt {
            ty: self.cx.common.void,
            span,
            kind: crate::ir::stmt::StmtKind::If {
                cond,
                then_block,
                else_block: None,
            },
        });
        Ok(())
    }

    pub(super) fn slice_bounds(
        &mut self,
        span: Span,
        start: &Operand,
        end: &Operand,
        len: &Operand,
    ) -> Result<(), CheckError> {
        self.bounds_check_inclusive(span, start, len)?;
        self.bounds_check_inclusive(span, end, len)?;
        let bool_ty = self.cx.common.bool;
        let n = self.fresh("bc");
        let id = self.b.tmp(n, bool_ty, span);
        let p = self.b.local_place(id, span);
        self.b.assign(
            span,
            p.clone(),
            Rvalue::binary(span, bool_ty, BinOp::Lt, end.clone(), start.clone()),
        );
        let saved = self.b.begin_block();
        self.b.push(crate::ir::stmt::Stmt {
            ty: self.cx.common.void,
            span,
            kind: crate::ir::stmt::StmtKind::Abort(crate::ir::stmt::AbortKind::SliceOrder, vec![end.clone(), start.clone()]),
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

    pub(super) fn bounds_check_inclusive(
        &mut self,
        span: Span,
        index: &Operand,
        len: &Operand,
    ) -> Result<(), CheckError> {
        let int = self.cx.common.int;
        let bool_ty = self.cx.common.bool;
        let lt = {
            let n = self.fresh("bc");
            let id = self.b.tmp(n, bool_ty, span);
            let p = self.b.local_place(id, span);
            self.b.assign(
                span,
                p.clone(),
                Rvalue::binary(
                    span,
                    bool_ty,
                    BinOp::Lt,
                    index.clone(),
                    Operand::int(span, int, 0),
                ),
            );
            Operand::Copy(p)
        };
        let gt = {
            let n = self.fresh("bc");
            let id = self.b.tmp(n, bool_ty, span);
            let p = self.b.local_place(id, span);
            self.b.assign(
                span,
                p.clone(),
                Rvalue::binary(span, bool_ty, BinOp::Gt, index.clone(), len.clone()),
            );
            Operand::Copy(p)
        };
        let cond = {
            let n = self.fresh("bc");
            let id = self.b.tmp(n, bool_ty, span);
            let p = self.b.local_place(id, span);
            self.b.assign(
                span,
                p.clone(),
                Rvalue::binary(span, bool_ty, BinOp::Or, lt, gt),
            );
            Operand::Copy(p)
        };
        let saved = self.b.begin_block();
        self.b.push(crate::ir::stmt::Stmt {
            ty: self.cx.common.void,
            span,
            kind: crate::ir::stmt::StmtKind::Abort(crate::ir::stmt::AbortKind::Slice, vec![index.clone(), len.clone()]),
        });
        let then_block = self.b.end_block(saved);
        self.b.push(crate::ir::stmt::Stmt {
            ty: self.cx.common.void,
            span,
            kind: crate::ir::stmt::StmtKind::If {
                cond,
                then_block,
                else_block: None,
            },
        });
        Ok(())
    }

    pub(super) fn seq_len(&mut self, place: &Place, span: Span) -> Result<Operand, CheckError> {
        match self.cx.ty_kind(place.ty) {
            TyKind::Array { len, .. } => Ok(Operand::int(span, self.cx.common.int, *len)),
            TyKind::Slice { .. } | TyKind::String => {
                let p = self.b.slice_len(self.cx, place.clone(), span);
                Ok(Operand::Copy(p))
            }
            _ => Err(CheckError::new(self.path, span, "value has no length")),
        }
    }
}
