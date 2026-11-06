use super::*;

impl<'a> Lower<'a> {
    pub(super) fn operand(&mut self, expr: &Expr, expected: Option<Ty>) -> Result<Operand, CheckError> {
        if let Some(v) = self.fold_int(expr) {
            if v == 0 {
                if let Some(want) = expected {
                    if self.is_ptr_ty(want) {
                        return Ok(Operand::null(expr.span, want));
                    }
                }
            }
            return Ok(self.num_from_int(expr.span, v, expected));
        }
        if let ExprKind::Float(v) = expr.kind {
            let op = self.num_from_float(expr.span, v, expected);
            if let Some(want) = expected {
                if op.ty() != want {
                    return Err(self.mismatch(expr.span, "type mismatch", op.ty(), want));
                }
            }
            return Ok(op);
        }
        if let Ok(place) = self.try_place(expr) {
            if self.is_script_value(place.ty) {
                return Err(self.script_copy(expr.span, place.ty));
            }
            if let Some(want) = expected {
                if place.ty == want {
                    return Ok(Operand::Copy(place));
                }
                if let Some(op) = self.array_to_slice(expr.span, &place, want)? {
                    return Ok(op);
                }
                if let Some(op) = self.text_to_string(expr.span, &place, want)? {
                    return Ok(op);
                }
                return Err(self.mismatch(
                    expr.span,
                    "type mismatch",
                    place.ty,
                    want,
                ));
            }
            return Ok(Operand::Copy(place));
        }
        let rv = self.rvalue(expr, expected)?;
        if let Some(want) = expected {
            if rv.ty != want {
                return Err(self.mismatch(expr.span, "type mismatch", rv.ty, want));
            }
        }
        let n = self.fresh("t");
        let id = self.b.tmp(n, rv.ty, expr.span);
        let place = self.b.local_place(id, expr.span);
        self.b.assign(expr.span, place.clone(), rv);
        Ok(Operand::Copy(place))
    }

    pub(super) fn rvalue(&mut self, expr: &Expr, expected: Option<Ty>) -> Result<Rvalue, CheckError> {
        if matches!(expr.kind, ExprKind::Name(_)) {
            if let Ok(place) = self.try_place(expr) {
                if self.is_script_value(place.ty) {
                    return Err(self.script_copy(expr.span, place.ty));
                }
            }
        }
        if let Some(v) = self.fold_int(expr) {
            if v == 0 {
                if let Some(want) = expected {
                    if self.is_ptr_ty(want) {
                        return Ok(Rvalue::use_op(expr.span, Operand::null(expr.span, want)));
                    }
                }
            }
            return Ok(Rvalue::use_op(
                expr.span,
                self.num_from_int(expr.span, v, expected),
            ));
        }
        match &expr.kind {
            ExprKind::Name(n) => {
                if let Some(place) = self.aliases.get(&n.name).cloned() {
                    return Ok(Rvalue::use_op(expr.span, Operand::Copy(place)));
                }
                if let Some(&local) = self.locals.get(&n.name) {
                    let place = self.b.local_place(local, n.span);
                    return Ok(Rvalue::use_op(expr.span, Operand::Copy(place)));
                }
                if let Some(&gid) = self.globals.get(&n.name) {
                    let ty = self.cx.global(gid).ty;
                    let place = Place::global(gid, ty, self.global_access(gid), n.span);
                    return Ok(Rvalue::use_op(expr.span, Operand::Copy(place)));
                }
                if let Some(c) = self.consts.get(&n.name) {
                    return Ok(self.const_rvalue(expr.span, c));
                }
                if let Some(&id) = self.funcs.get(&n.name) {
                    let fty = self.cx.func(id).ty;
                    if let Some(want) = expected {
                        if let Some(rv) = self.fn_to_closure(expr.span, id, want)? {
                            return Ok(rv);
                        }
                        if want != fty {
                            return Err(CheckError::new(
                                self.path,
                                n.span,
                                "function type does not match expected type",
                            ));
                        }
                    }
                    return Ok(Rvalue::use_op(
                        expr.span,
                        Operand::fn_addr(expr.span, fty, id),
                    ));
                }
                Err(self.unknown(n.span, "name", &n.name, &[]))
            }
            ExprKind::Bool(v) => Ok(Rvalue::use_op(
                expr.span,
                Operand::bool_const(expr.span, self.cx.common.bool, *v),
            )),
            ExprKind::Int(v) => Ok(Rvalue::use_op(
                expr.span,
                self.num_from_int(expr.span, *v, expected),
            )),
            ExprKind::Float(v) => Ok(Rvalue::use_op(
                expr.span,
                self.num_from_float(expr.span, *v, expected),
            )),
            ExprKind::StructLit {
                name,
                type_args,
                fields,
            } => self.struct_lit(expr.span, name, type_args, fields),
            ExprKind::TypePath { name, .. } => Err(CheckError::new(
                self.path,
                name.span,
                format!("`{}` is a type, not a value", name.name),
            )),
            ExprKind::SizeOf(ty) => {
                let t = self.resolve_ty(ty)?;
                Ok(Rvalue::use_op(
                    expr.span,
                    Operand::int(
                        expr.span,
                        self.cx.common.int,
                        crate::ir::abi::size(self.cx, t),
                    ),
                ))
            }
            ExprKind::UiChoice { set, value, index } => {
                let ty = self.resolve_ty(set)?;
                let enum_id = match self.cx.ty_kind(ty) {
                    TyKind::Enum(id) if self.cx.enum_def(*id).is_c_like() => *id,
                    _ => return Err(CheckError::new(self.path, expr.span, "`@uiChoice` needs a payloadless candidate-set enum")),
                };
                let n = self.fold_int(index).filter(|n| *n >= 0 && (*n as usize) < self.cx.enum_def(enum_id).variants.len())
                    .ok_or_else(|| CheckError::new(self.path, index.span, "`@uiChoice` arm index must be a constant within the candidate set"))?;
                let op = self.operand(value, None)?;
                let expected = if op.ty() == self.cx.common.int {
                    Operand::int(expr.span, self.cx.common.int, n as u64)
                } else if op.ty() == ty {
                    let variant = VariantId { enum_id, index: n as u32 };
                    let name = self.fresh("choice");
                    let id = self.b.tmp(name, ty, expr.span);
                    let place = self.b.local_place(id, expr.span);
                    self.b.assign(expr.span, place.clone(), Rvalue::enum_new(expr.span, ty, variant, None));
                    Operand::Copy(place)
                } else {
                    let name = self.cx.name_str(self.cx.enum_def(enum_id).name);
                    return Err(CheckError::new(self.path, value.span, format!("component selector must be `int` or `{name}`")));
                };
                Ok(Rvalue::binary(expr.span, self.cx.common.bool, BinOp::Eq, op, expected))
            }
            ExprKind::SliceFrom { ptr, len } => self.slice_from(expr.span, ptr, len, expected),
            ExprKind::ArrayLit { elems } => self.array_lit(expr.span, elems, expected),
            ExprKind::SliceLit { elem, fields } => self.slice_lit(expr.span, elem, fields),
            ExprKind::Index { .. } => {
                let place = self.place(expr)?;
                Ok(Rvalue::use_op(expr.span, Operand::Copy(place)))
            }
            ExprKind::Slice { base, start, end } => {
                self.slice_rvalue(expr.span, base, start.as_deref(), end.as_deref())
            }
            ExprKind::String(bytes) => Ok(Rvalue::use_op(
                expr.span,
                Operand::bytes(expr.span, self.cx.common.string, bytes),
            )),
            ExprKind::Field { base, field } => {
                if let Some(c) = self.qualified_const(expr) {
                    return Ok(self.const_rvalue(expr.span, &c));
                }
                if let ExprKind::Name(n) = &base.kind {
                    if self.mod_ns.contains_key(&n.name) && !self.shadows(&n.name) {
                        return Err(CheckError::new(
                            self.path,
                            field.span,
                            format!("`{}` has no pub const `{}`", n.name, field.name),
                        ));
                    }
                }
                if let Some(ename) = self.enum_ctor_name(base) {
                    return self.construct_variant(expr.span, &ename, &field.name, None, expected);
                }
                if field.name == "len" {
                    if let Ok(p) = self.try_place(base) {
                        let mut p = p;
                        while self.cx.ref_inner(p.ty).is_some() {
                            p = self.b.deref(self.cx, p, field.span);
                        }
                        if let TyKind::Array { len, .. } = self.cx.ty_kind(p.ty) {
                            return Ok(Rvalue::use_op(
                                expr.span,
                                Operand::int(expr.span, self.cx.common.int, *len),
                            ));
                        }
                    }
                }
                let place = self.place(expr)?;
                Ok(Rvalue::use_op(expr.span, Operand::Copy(place)))
            }
            ExprKind::Call {
                func,
                type_args,
                args,
            } => self.call(expr.span, func, type_args, args, expected),
            ExprKind::Syscall { args } => self.syscall(expr.span, args),
            ExprKind::Atomic { op, order, args } => self.atomic(expr.span, *op, *order, args),
            ExprKind::Cast { expr: inner, ty } => {
                let to = self.resolve_ty(ty)?;
                let from = self.operand(inner, None)?;
                if self.cx.closure_sig(from.ty()).is_some() || self.cx.closure_sig(to).is_some() {
                    return Err(CheckError::new(self.path, expr.span, "a closure cannot be cast")
                        .help("a closure carries its captures; pass it to a `closure(..)` parameter instead of converting it"));
                }
                self.half_cast(expr.span, to, &from)?;
                Ok(Rvalue::cast(expr.span, to, from))
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let rv = self.binary(expr.span, *op, lhs, rhs, expected)?;
                self.half_guard(expr.span, rv)
            }
            ExprKind::If {
                cond,
                then_expr,
                else_expr,
            } => {
                let c = self.operand(cond, Some(self.cx.common.bool))?;
                let saved = self.b.begin_block();
                let then_rv = self.rvalue(then_expr, expected)?;
                let ty = then_rv.ty;
                let n = self.fresh("if");
                let id = self.b.tmp(n, ty, expr.span);
                let place = self.b.local_place(id, expr.span);
                self.b.assign(then_expr.span, place.clone(), then_rv);
                let then_block = self.b.end_block(saved);
                let saved = self.b.begin_block();
                let else_rv = self.rvalue(else_expr, Some(ty))?;
                if else_rv.ty != ty {
                    return Err(CheckError::new(
                        self.path,
                        expr.span,
                        "if-expression branches have different types",
                    ));
                }
                self.b.assign(else_expr.span, place.clone(), else_rv);
                let else_block = self.b.end_block(saved);
                self.b.push(crate::ir::stmt::Stmt {
                    ty: self.cx.common.void,
                    span: expr.span,
                    kind: crate::ir::stmt::StmtKind::If {
                        cond: c,
                        then_block,
                        else_block: Some(else_block),
                    },
                });
                Ok(Rvalue::use_op(expr.span, Operand::Copy(place)))
            }
            ExprKind::Unary { op, expr: inner } => match op {
                parse::UnaryOp::Neg => {
                    if let (ExprKind::Float(v), Some(t)) = (&inner.kind, expected) {
                        if self.is_half(t) {
                            return Ok(Rvalue::use_op(expr.span, self.half_const(expr.span, -*v)));
                        }
                    }
                    let arg = self.operand(inner, expected)?;
                    if self.is_half(arg.ty()) {
                        return Err(self.half_error(expr.span, "unary `-`"));
                    }
                    if !is_numeric(self.cx, arg.ty()) && !is_vec(self.cx, arg.ty()) {
                        return Err(CheckError::new(
                            self.path,
                            expr.span,
                            "unary `-` needs a number",
                        ));
                    }
                    self.half_guard(expr.span, Rvalue {
                        ty: arg.ty(),
                        span: expr.span,
                        kind: RvalueKind::Unary {
                            op: UnaryOp::Neg,
                            arg,
                        },
                    })
                }
                parse::UnaryOp::BitNot => {
                    let arg = self.operand(inner, expected)?;
                    if !is_int(self.cx, arg.ty()) && arg.ty() != self.cx.common.vec4i {
                        return Err(CheckError::new(
                            self.path,
                            expr.span,
                            "unary `~` takes an integer; use `!` for bools",
                        ));
                    }
                    Ok(Rvalue {
                        ty: arg.ty(),
                        span: expr.span,
                        kind: RvalueKind::Unary { op: UnaryOp::BitNot, arg },
                    })
                }
                parse::UnaryOp::Not => {
                    let arg = self.operand(inner, Some(self.cx.common.bool))?;
                    Ok(Rvalue {
                        ty: self.cx.common.bool,
                        span: expr.span,
                        kind: RvalueKind::Unary {
                            op: UnaryOp::Not,
                            arg,
                        },
                    })
                }
                parse::UnaryOp::Ref => {
                    if let ExprKind::Name(n) = &inner.kind {
                        if self.consts.contains_key(&n.name) {
                            return Err(CheckError::new(
                                self.path,
                                expr.span,
                                "cannot take `&` of a const",
                            ));
                        }
                    }
                    // ponytail: `&` of a spilled temp acts like `&local` (the language does not stop a local's address escaping either).
                    let place = self.place(inner)?;
                    reject_closure(self.cx, self.path, expr.span, place.ty, "borrowed with `&`")?;
                    if !place.access.take_ref || !place.access.assign {
                        return Err(CheckError::new(
                            self.path,
                            expr.span,
                            "cannot take `&` of this place",
                        ));
                    }
                    let ref_ty = self.cx.ty_ref(place.ty);
                    Ok(Rvalue::ref_place(expr.span, ref_ty, place))
                }
                parse::UnaryOp::Deref => {
                    let place = self.place(expr)?;
                    Ok(Rvalue::use_op(expr.span, Operand::Copy(place)))
                }
            },
            ExprKind::Interpolate { parts } => self.interpolate(expr.span, parts),
            ExprKind::Question { expr: inner } => self.question(expr.span, inner),
            ExprKind::ScriptType(_) => Err(CheckError::new(
                self.path,
                expr.span,
                "a script type is not a value",
            )),
            ExprKind::ScriptLit { path, fields } => self.script_lit(expr.span, path, fields),
            ExprKind::Closure(lit) => self.closure_lit(expr.span, lit, expected),
        }
    }

    pub(super) fn const_rvalue(&self, span: Span, c: &ConstVal) -> Rvalue {
        match &c.kind {
            ConstPayload::Int(v) => Rvalue::use_op(span, Operand::int(span, c.ty, *v as u64)),
            ConstPayload::Float(v) => {
                if c.ty == self.cx.common.f32 {
                    Rvalue::use_op(span, Operand::f32_const(span, c.ty, *v as f32))
                } else {
                    Rvalue::use_op(span, Operand::f64_const(span, c.ty, *v))
                }
            }
            ConstPayload::Bool(v) => {
                Rvalue::use_op(span, Operand::bool_const(span, c.ty, *v))
            }
            ConstPayload::String(b) => Rvalue::use_op(span, Operand::bytes(span, c.ty, b)),
        }
    }

    pub(super) fn bind_operand(&mut self, op: Operand, span: Span) -> Operand {
        match &op {
            Operand::Const(_) => op,
            Operand::Copy(p) if p.proj.is_empty() => op,
            _ => {
                let n = self.fresh("i");
                let id = self.b.tmp(n, op.ty(), span);
                let p = self.b.local_place(id, span);
                self.b.assign(span, p.clone(), Rvalue::use_op(span, op));
                Operand::Copy(p)
            }
        }
    }

    pub(super) fn materialize(&mut self, op: Operand, span: Span) -> Place {
        if let Operand::Copy(p) = op {
            return p;
        }
        let n = self.fresh("t");
        let id = self.b.tmp(n, op.ty(), span);
        let place = self.b.local_place(id, span);
        self.b.assign(span, place.clone(), Rvalue::use_op(span, op));
        place
    }

    pub(super) fn materialize_rv(&mut self, rv: Rvalue, span: Span) -> Place {
        let n = self.fresh("t");
        let id = self.b.tmp(n, rv.ty, span);
        let place = self.b.local_place(id, span);
        self.b.assign(span, place.clone(), rv);
        place
    }
    pub(super) fn bind_bool(&mut self, span: Span, rv: Rvalue) -> Operand {
        let n = self.fresh("c");
        let id = self.b.tmp(n, self.cx.common.bool, span);
        let place = self.b.local_place(id, span);
        self.b.assign(span, place.clone(), rv);
        Operand::Copy(place)
    }

    pub(super) fn eq_ops(&mut self, span: Span, lhs: Operand, rhs: Operand) -> Operand {
        self.bind_bool(
            span,
            Rvalue::binary(span, self.cx.common.bool, BinOp::Eq, lhs, rhs),
        )
    }

    pub(super) fn and_ops(&mut self, span: Span, lhs: Operand, rhs: Operand) -> Operand {
        self.bind_bool(
            span,
            Rvalue::binary(span, self.cx.common.bool, BinOp::And, lhs, rhs),
        )
    }
}
