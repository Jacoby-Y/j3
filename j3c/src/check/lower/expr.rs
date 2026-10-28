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
            return Ok(self.num_from_float(expr.span, v, expected));
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
            ExprKind::Atomic { op, args } => self.atomic(expr.span, *op, args),
            ExprKind::Cast { expr: inner, ty } => {
                let to = self.resolve_ty(ty)?;
                let from = self.operand(inner, None)?;
                Ok(Rvalue::cast(expr.span, to, from))
            }
            ExprKind::Binary { op, lhs, rhs } => self.binary(expr.span, *op, lhs, rhs, expected),
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
                    let arg = self.operand(inner, expected)?;
                    if !is_numeric(self.cx, arg.ty()) {
                        return Err(CheckError::new(
                            self.path,
                            expr.span,
                            "unary `-` needs a number",
                        ));
                    }
                    Ok(Rvalue {
                        ty: arg.ty(),
                        span: expr.span,
                        kind: RvalueKind::Unary {
                            op: UnaryOp::Neg,
                            arg,
                        },
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

    pub(super) fn interpolate(&mut self, span: Span, parts: &[InterpPart]) -> Result<Rvalue, CheckError> {
        if let Some(bytes) = self.fold_interp(parts) {
            return Ok(Rvalue::use_op(
                span,
                Operand::bytes(span, self.cx.common.string, &bytes),
            ));
        }
        let mut ir_parts = Vec::new();
        for part in parts {
            match part {
                InterpPart::Lit(b) => {
                    ir_parts.push(crate::ir::rvalue::InterpPart::Lit(b.clone()));
                }
                InterpPart::Expr(e) => {
                    let op = self.operand(e, None)?;
                    if !self.interp_ok(op.ty()) {
                        return Err(CheckError::new(
                            self.path,
                            e.span,
                            "this value cannot appear in interpolate (need primitive, string, or #[Debug])",
                        ));
                    }
                    ir_parts.push(crate::ir::rvalue::InterpPart::Value(op));
                }
            }
        }
        let cap = crate::ir::wrap::INTERP_CAP;
        let buf_ty = self.cx.ty_array(self.cx.common.u8, cap);
        let buf_n = self.fresh("ibuf");
        let buf_id = self.b.tmp(buf_n, buf_ty, span);
        let buf = self.b.local_place(buf_id, span);
        let used_n = self.fresh("in");
        let used_id = self.b.tmp(used_n, self.cx.common.int, span);
        let used = self.b.local_place(used_id, span);
        self.b.assign(
            span,
            used.clone(),
            Rvalue::use_op(span, Operand::int(span, self.cx.common.int, 0)),
        );
        self.b.push(crate::ir::stmt::Stmt {
            ty: self.cx.common.void,
            span,
            kind: crate::ir::stmt::StmtKind::InterpFill {
                buf: buf.clone(),
                used: used.clone(),
                parts: ir_parts,
            },
        });
        let start = self.b.index(
            self.cx,
            buf,
            Operand::int(span, self.cx.common.int, 0),
            span,
        );
        let ptr_ty = self.cx.ty_ref(self.cx.common.u8);
        let ptr_n = self.fresh("iptr");
        let ptr_id = self.b.tmp(ptr_n, ptr_ty, span);
        let ptr_place = self.b.local_place(ptr_id, span);
        self.b.assign(span, ptr_place.clone(), Rvalue::ref_place(span, ptr_ty, start));
        Ok(Rvalue::aggregate(
            span,
            self.cx.common.string,
            vec![Operand::Copy(ptr_place), Operand::Copy(used)],
        ))
    }

    pub(super) fn fold_interp(&self, parts: &[InterpPart]) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        for part in parts {
            match part {
                InterpPart::Lit(b) => out.extend(b),
                InterpPart::Expr(e) => match self.fold_interp_expr(e) {
                    Some(b) => out.extend(b),
                    None => return None,
                },
            }
        }
        Some(out)
    }

    pub(super) fn fold_interp_expr(&self, expr: &Expr) -> Option<Vec<u8>> {
        if let Some(v) = self.fold_int(expr) {
            return Some(v.to_string().into_bytes());
        }
        match &expr.kind {
            ExprKind::String(b) => Some(b.clone()),
            ExprKind::Bool(v) => Some(if *v {
                b"true".to_vec()
            } else {
                b"false".to_vec()
            }),
            ExprKind::Name(n) => match self.consts.get(&n.name) {
                Some(ConstVal {
                    kind: ConstPayload::String(b),
                    ..
                }) => Some(b.clone()),
                Some(ConstVal {
                    kind: ConstPayload::Int(v),
                    ..
                }) => Some(v.to_string().into_bytes()),
                Some(ConstVal {
                    kind: ConstPayload::Float(v),
                    ..
                }) => Some(v.to_string().into_bytes()),
                Some(ConstVal {
                    kind: ConstPayload::Bool(v),
                    ..
                }) => Some(if *v { b"true".to_vec() } else { b"false".to_vec() }),
                None => None,
            },
            _ => None,
        }
    }

    pub(super) fn interp_ok(&self, ty: Ty) -> bool {
        if ty == self.cx.common.string || ty == self.cx.common.bool {
            return true;
        }
        if is_int(self.cx, ty) || is_float(self.cx, ty) {
            return true;
        }
        match self.cx.ty_kind(ty) {
            TyKind::Struct(id) => self.cx.struct_def(*id).debug,
            _ => false,
        }
    }

    pub(super) fn text_to_string(
        &mut self,
        span: Span,
        place: &Place,
        want: Ty,
    ) -> Result<Option<Operand>, CheckError> {
        if want != self.cx.common.string {
            return Ok(None);
        }
        let sid = match self.cx.ty_kind(place.ty) {
            TyKind::Struct(id) => *id,
            _ => return Ok(None),
        };
        if self.cx.name_str(self.cx.struct_def(sid).name) != "Text" {
            return Ok(None);
        }
        let path = match field_path(self.cx, sid, "bytes") {
            Some(p) => p,
            None => return Ok(None),
        };
        let mut p = place.clone();
        for fid in path {
            p = self.b.field(self.cx, p, fid, span);
        }
        let rv = Rvalue::cast(span, want, Operand::Copy(p));
        let n = self.fresh("txt");
        let id = self.b.tmp(n, want, span);
        let out = self.b.local_place(id, span);
        self.b.assign(span, out.clone(), rv);
        Ok(Some(Operand::Copy(out)))
    }
}
