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
                    if self.is_half(r.ty()) {
                        return Err(self.half_error(span, &format!("`{}`", ir_op.as_str())));
                    }
                    if is_vec(self.cx, r.ty()) {
                        return Err(self.vec_cmp_error(span, op));
                    }
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
                if self.is_half(l.ty()) {
                    return Err(self.half_error(span, &format!("`{}`", ir_op.as_str())));
                }
                if is_vec(self.cx, l.ty()) {
                    return Err(self.vec_cmp_error(span, op));
                }
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
                if let Some(rw) = self.operator_fallback(span, op, &l, rhs)? {
                    return Ok(rw);
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
                if matches!(op, parse::BinOp::Eq | parse::BinOp::Ne) {
                    if eq_needs_operator(self.cx, l.ty()) {
                        let sym = ast_binop(op).as_str();
                        return Err(CheckError::new(
                            self.path,
                            span,
                            format!("`{sym}` does not compare structs or enums with payloads; compare the fields, or define `fn operator{sym}(this, other: T): bool` on it"),
                        ));
                    }
                }
                if matches!(op, parse::BinOp::Eq | parse::BinOp::Ne) && l.ty() == self.cx.common.string {
                    return Ok(self.string_eq(span, op == parse::BinOp::Ne, l, r));
                }
                self.ord_only(op, l.ty(), span)?;
                Ok(Rvalue::binary(span, self.cx.common.bool, ir_op, l, r))
            }
            parse::BinOp::Add
            | parse::BinOp::Sub
            | parse::BinOp::Mul
            | parse::BinOp::Div
            | parse::BinOp::Rem
            | parse::BinOp::Shl
            | parse::BinOp::Shr
            | parse::BinOp::BitAnd
            | parse::BinOp::BitOr
            | parse::BinOp::BitXor => {
                if bare_num(lhs) && !bare_num(rhs) {
                    let right_expect = match expected {
                        Some(t) if is_numeric(self.cx, t) => Some(t),
                        _ => None,
                    };
                    let r = self.operand(rhs, right_expect)?;
                    if self.is_half(r.ty()) {
                        return Err(self.half_error(span, &format!("`{}`", ir_op.as_str())));
                    }
                    if is_vec(self.cx, r.ty()) {
                        let l = self.operand(lhs, self.cx.vec_lane(r.ty()))?;
                        return self.vec_arith(span, op, l, r);
                    }
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
                    self.int_only(op, r.ty(), span)?;
                    return Ok(Rvalue::binary(span, r.ty(), ir_op, l, r));
                }
                // Both sides untyped: the left takes the destination's numeric type.
                let left_expect = match expected {
                    Some(t) if bare_num(lhs) && is_numeric(self.cx, t) => Some(t),
                    _ => None,
                };
                let l = self.operand(lhs, left_expect)?;
                if self.is_half(l.ty()) {
                    return Err(self.half_error(span, &format!("`{}`", ir_op.as_str())));
                }
                if is_vec(self.cx, l.ty()) {
                    let r = self.vec_rhs(rhs, l.ty())?;
                    return self.vec_arith(span, op, l, r);
                }
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
                self.int_only(op, l.ty(), span)?;
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
                // a plain name, field or index may be a vector (`2.0 * v`): no expected type
                let plain = matches!(rhs.kind, ExprKind::Name(_) | ExprKind::Field { .. } | ExprKind::Index { .. });
                let r = if (ty == self.cx.common.f32 || ty == self.cx.common.i32) && plain {
                    self.operand(rhs, None)?
                } else {
                    self.operand(rhs, Some(ty))?
                };
                if is_vec(self.cx, r.ty()) && self.cx.vec_lane(r.ty()) == Some(l.ty()) {
                    return self.vec_arith(span, op, l, r);
                }
                if r.ty() != ty {
                    return Err(CheckError::new(self.path, span, "arithmetic type mismatch"));
                }
                if matches!(op, parse::BinOp::Div | parse::BinOp::Rem) && is_int(self.cx, ty) {
                    self.abort_if_zero(span, &r)?;
                }
                self.int_only(op, ty, span)?;
                let _ = expected;
                Ok(Rvalue::binary(span, ty, ir_op, l, r))
            }
        }
    }

    /// No explicit `operator!=` / `>` / `<=` / `>=` on a struct: `a != b` is `!(a == b)`, `a > b` is
    /// `b < a`, `a <= b` is `!(b < a)`, `a >= b` is `!(a < b)`. An explicit operator never reaches
    /// here (the caller found it first), and no `operator==` / `operator<` means no rewrite.
    fn operator_fallback(&mut self, span: Span, op: parse::BinOp, l: &Operand, rhs: &Expr) -> Result<Option<Rvalue>, CheckError> {
        use parse::BinOp as B;
        let (base, swap, negate) = match op {
            B::Ne => (B::Eq, false, true),
            B::Gt => (B::Lt, true, false),
            B::Le => (B::Lt, true, true),
            B::Ge => (B::Lt, false, true),
            _ => return Ok(None),
        };
        let Some(id) = find_operator(self.cx, l.ty(), base, self.methods) else { return Ok(None) };
        let fty = self.cx.func(id).ty;
        let Some((params, ret)) = self.cx.fn_sig(fty).map(|s| (s.0.clone(), s.1)) else {
            return Err(CheckError::new(self.path, span, "not a function"));
        };
        if params.len() != 2 {
            return Err(CheckError::new(self.path, span, "binary operator needs two parameters"));
        }
        let (want_l, want_r) = if swap { (params[1], params[0]) } else { (params[0], params[1]) };
        if l.ty() != want_l {
            return Err(CheckError::new(self.path, span, "operator `this` type mismatch"));
        }
        let r = self.operand(rhs, Some(want_r))?;
        if negate && ret != self.cx.common.bool {
            return Err(CheckError::new(self.path, span, "an operator used for a comparison fallback must return bool"));
        }
        let args = if swap { vec![r, l.clone()] } else { vec![l.clone(), r] };
        let call = Rvalue::call(span, ret, Operand::fn_addr(span, fty, id), args);
        if !negate {
            return Ok(Some(call));
        }
        let n = self.fresh("cmp");
        let tmp = self.b.tmp(n, ret, span);
        let place = self.b.local_place(tmp, span);
        self.b.assign(span, place.clone(), call);
        Ok(Some(Rvalue { ty: ret, span, kind: RvalueKind::Unary { op: UnaryOp::Not, arg: Operand::Copy(place) } }))
    }

    /// `<< >> & | ^` take integers.
    fn int_only(&self, op: parse::BinOp, ty: crate::ir::Ty, span: Span) -> Result<(), CheckError> {
        use parse::BinOp::*;
        if !matches!(op, Shl | Shr | BitAnd | BitOr | BitXor) || is_int(self.cx, ty) {
            return Ok(());
        }
        let sym = ast_binop(op).as_str();
        let hint = if ty == self.cx.common.bool { "; use `&&` / `||` for bools" } else { "" };
        Err(CheckError::new(self.path, span, format!("`{sym}` takes integers{hint}")))
    }

    /// `< <= > >=` take numbers. A `string` has no order here: `==` / `!=` compare bytes
    /// (`string_eq`).
    fn ord_only(&self, op: parse::BinOp, ty: crate::ir::Ty, span: Span) -> Result<(), CheckError> {
        use parse::BinOp::*;
        if !matches!(op, Lt | Le | Gt | Ge) || ty != self.cx.common.string {
            return Ok(());
        }
        let sym = ast_binop(op).as_str();
        Err(CheckError::new(
            self.path,
            span,
            format!("`{sym}` does not order strings; compare bytes in a loop; `==` / `!=` test equality"),
        ))
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
            Some(t) if self.is_half(t) => self.half_const(span, v as f64),
            Some(t) if t == self.cx.common.f32 => Operand::f32_const(span, t, v as f32),
            Some(t) if is_float(self.cx, t) => Operand::f64_const(span, t, v as f64),
            _ => Operand::int(span, self.cx.common.int, v as u64),
        }
    }

    pub(super) fn num_from_float(&self, span: Span, v: f64, expected: Option<Ty>) -> Operand {
        match expected {
            Some(t) if self.is_half(t) => self.half_const(span, v),
            Some(t) if t == self.cx.common.f32 => Operand::f32_const(span, t, v as f32),
            Some(t) if is_float(self.cx, t) => Operand::f64_const(span, t, v),
            _ => Operand::f64_const(span, self.cx.common.float, v),
        }
    }
}
