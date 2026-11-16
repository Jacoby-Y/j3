use super::*;

impl<'a> Lower<'a> {
    pub(super) fn enum_ctor_name(&self, expr: &Expr) -> Option<String> {
        match &expr.kind {
            ExprKind::Name(n) => {
                if self.enum_temps.contains_key(&n.name) {
                    return Some(n.name.clone());
                }
                if let Some(&ty) = self.types.get(&n.name) {
                    if matches!(self.cx.ty_kind(ty), TyKind::Enum(_)) {
                        return Some(n.name.clone());
                    }
                }
                None
            }
            ExprKind::Field { base, field } => {
                let ExprKind::Name(n) = &base.kind else {
                    return None;
                };
                let types = self.mod_types.get(&n.name)?;
                let ty = types.get(&field.name)?;
                if matches!(self.cx.ty_kind(*ty), TyKind::Enum(_)) {
                    Some(field.name.clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub(super) fn ctor_enum_id(
        &self,
        name: &str,
        expected: Option<Ty>,
        span: Span,
    ) -> Result<EnumId, CheckError> {
        if let Some(ty) = expected {
            if let TyKind::Enum(id) = self.cx.ty_kind(ty) {
                if self.cx.name_str(self.cx.enum_def(*id).name) == name {
                    return Ok(*id);
                }
            }
        }
        if let Some(&ty) = self.types.get(name) {
            if let TyKind::Enum(id) = self.cx.ty_kind(ty) {
                return Ok(*id);
            }
        }
        if self.enum_temps.contains_key(name) {
            return Err(CheckError::new(
                self.path,
                span,
                format!("`{name}` needs an expected type"),
            ));
        }
        Err(CheckError::new(
            self.path,
            span,
            format!("`{name}` is not an enum"),
        ))
    }

    pub(super) fn find_variant(
        &self,
        id: EnumId,
        name: &str,
        span: Span,
    ) -> Result<(VariantId, Option<Ty>), CheckError> {
        let def = self.cx.enum_def(id);
        for (i, v) in def.variants.iter().enumerate() {
            if self.cx.name_str(v.name) == name {
                return Ok((
                    VariantId {
                        enum_id: id,
                        index: i as u32,
                    },
                    v.payload,
                ));
            }
        }
        Err(CheckError::new(
            self.path,
            span,
            format!("unknown variant `{name}`"),
        ))
    }

    /// `E.V`, `E.V(x)` or `E.V(a, b, c)`: `args` are the fields written in parens.
    pub(super) fn construct_variant(
        &mut self,
        span: Span,
        enum_name: &str,
        variant: &str,
        args: &[Expr],
        expected: Option<Ty>,
    ) -> Result<Rvalue, CheckError> {
        let id = self.ctor_enum_id(enum_name, expected, span)?;
        let (vid, pty) = self.find_variant(id, variant, span)?;
        let ftys = self.variant_field_tys(vid);
        let name = format!("{enum_name}.{variant}");
        if args.len() != ftys.len() {
            let msg = match (ftys.len(), args.len()) {
                (0, _) => format!("`{name}` has no payload"),
                (1, 0) => format!("`{name}` needs a payload"),
                (n, _) => format!("`{name}` takes {n} field{}, got {}", if n == 1 { "" } else { "s" }, args.len()),
            };
            return Err(CheckError::new(self.path, span, msg));
        }
        let payload_op = match (pty, self.cx.enum_def(id).variants[vid.index as usize].multi) {
            (None, _) => None,
            (Some(ty), false) => Some(self.operand(&args[0], Some(ty))?),
            (Some(ty), true) => {
                let mut ops = Vec::new();
                for (k, (a, fty)) in args.iter().zip(ftys).enumerate() {
                    let op = self.operand(a, Some(fty)).map_err(|mut e| {
                        e.message = format!("field {k} of `{name}`: {}", e.message);
                        e
                    })?;
                    if op.ty() != fty {
                        return Err(self.mismatch(a.span, &format!("field {k} of `{name}`"), op.ty(), fty));
                    }
                    ops.push(op);
                }
                Some(Operand::Copy(self.materialize_rv(Rvalue::aggregate(span, ty, ops), span)))
            }
        };
        let ty = self.cx.ty_enum(id);
        if let Some(want) = expected {
            if ty != want {
                return Err(CheckError::new(
                    self.path,
                    span,
                    "enum type does not match expected type",
                ));
            }
        }
        Ok(Rvalue::enum_new(span, ty, vid, payload_op))
    }

    /// The fields of variant `vid`, in order: none, its one payload, or the payload struct's.
    pub(super) fn variant_field_tys(&self, vid: VariantId) -> Vec<Ty> {
        let v = &self.cx.enum_def(vid.enum_id).variants[vid.index as usize];
        match (v.payload, v.multi) {
            (None, _) => Vec::new(),
            (Some(p), false) => vec![p],
            (Some(p), true) => match self.cx.ty_kind(p) {
                TyKind::Struct(sid) => self.cx.struct_def(*sid).fields.iter().map(|f| f.ty).collect(),
                _ => unreachable!("a multi-field payload is a struct"),
            },
        }
    }

    /// The place of field `k` of variant `vid` in the enum value at `place`.
    pub(super) fn variant_field_place(&self, place: &Place, vid: VariantId, k: usize, span: Span) -> Place {
        let v = &self.cx.enum_def(vid.enum_id).variants[vid.index as usize];
        let pty = v.payload.expect("a variant with fields has a payload");
        let payload = self.payload_place(place, vid, pty, span);
        if !v.multi {
            return payload;
        }
        let TyKind::Struct(sid) = *self.cx.ty_kind(pty) else { unreachable!("a multi-field payload is a struct") };
        self.b.field(self.cx, payload, crate::ir::ids::FieldId { struct_id: sid, index: k as u32 }, span)
    }

    pub(super) fn enum_id_of(&self, ty: Ty, span: Span, what: &str) -> Result<EnumId, CheckError> {
        match self.cx.ty_kind(ty) {
            TyKind::Enum(id) => Ok(*id),
            _ => Err(CheckError::new(
                self.path,
                span,
                format!("{what} on non-enum"),
            )),
        }
    }

    pub(super) fn is_variant(&mut self, place: &Place, variant: VariantId, span: Span) -> Operand {
        if self.cx.enum_def(variant.enum_id).is_c_like() {
            let ty = place.ty;
            let expected = Rvalue::enum_new(span, ty, variant, None);
            let n = self.fresh("ev");
            let id = self.b.tmp(n, ty, span);
            let tmp = self.b.local_place(id, span);
            self.b.assign(span, tmp.clone(), expected);
            self.eq_ops(span, Operand::Copy(place.clone()), Operand::Copy(tmp))
        } else {
            let tag = self.b.enum_tag(self.cx, place.clone(), span);
            self.eq_ops(
                span,
                Operand::Copy(tag),
                Operand::int(span, self.cx.common.i32, variant.index as u64),
            )
        }
    }

    pub(super) fn payload_place(&self, place: &Place, variant: VariantId, ty: Ty, span: Span) -> Place {
        self.b.enum_payload(place.clone(), variant, ty, span)
    }

    pub(super) fn variant_from_path(
        &self,
        enum_id: EnumId,
        path: &[Ident],
        span: Span,
    ) -> Result<(VariantId, Option<Ty>), CheckError> {
        let ename = self.cx.name_str(self.cx.enum_def(enum_id).name).to_string();
        let vname = match path.len() {
            2 => {
                if path[0].name != ename {
                    return Err(CheckError::new(
                        self.path,
                        path[0].span,
                        format!("expected `{ename}`, got `{}`", path[0].name),
                    ));
                }
                &path[1].name
            }
            _ => {
                return Err(CheckError::new(
                    self.path,
                    span,
                    "match arm must be `Type.Variant`",
                ));
            }
        };
        self.find_variant(enum_id, vname, span)
    }

    pub(super) fn question(&mut self, span: Span, inner: &Expr) -> Result<Rvalue, CheckError> {
        let op = self.operand(inner, None)?;
        let id = self.enum_id_of(op.ty(), span, "`?`")?;
        let tc = match self.cx.enum_def(id).try_catch {
            Some(tc) => tc,
            None => {
                return Err(CheckError::new(self.path, span, "`?` needs `#[TryCatch]`"));
            }
        };
        if self.ret == self.cx.common.void {
            return Err(CheckError::new(self.path, span, "`?` in a void function"));
        }
        let ret_id = self.enum_id_of(self.ret, span, "enclosing `?` return")?;
        let ret_tc = match self.cx.enum_def(ret_id).try_catch {
            Some(tc) => tc,
            None => {
                return Err(CheckError::new(
                    self.path,
                    span,
                    "enclosing fn must return a `#[TryCatch]` enum",
                ));
            }
        };
        let success_ty = self.cx.enum_def(id).variants[tc.success.index as usize].payload;
        let fail_ty = self.cx.enum_def(id).variants[tc.failure.index as usize].payload;
        let ret_fail_ty = self.cx.enum_def(ret_id).variants[ret_tc.failure.index as usize].payload;
        if fail_ty != ret_fail_ty {
            return Err(CheckError::new(
                self.path,
                span,
                "failure payloads must match",
            ));
        }
        let value_ty = success_ty.unwrap_or(self.cx.common.void);
        let scrut = self.materialize(op, span);
        let n = self.fresh("q");
        let rid = self.b.tmp(n, value_ty, span);
        let rplace = self.b.local_place(rid, span);
        let saved = self.b.begin_block();
        if let Some(pty) = success_ty {
            let src = self.payload_place(&scrut, tc.success, pty, span);
            self.b.assign(
                span,
                rplace.clone(),
                Rvalue::use_op(span, Operand::Copy(src)),
            );
        }
        let then_block = self.b.end_block(saved);
        let saved = self.b.begin_block();
        let ret_op = if id == ret_id {
            Operand::Copy(scrut.clone())
        } else {
            match fail_ty {
                Some(pty) => {
                    let src = self.payload_place(&scrut, tc.failure, pty, span);
                    let rv =
                        Rvalue::enum_new(span, self.ret, ret_tc.failure, Some(Operand::Copy(src)));
                    Operand::Copy(self.materialize_rv(rv, span))
                }
                None => {
                    let rv = Rvalue::enum_new(span, self.ret, ret_tc.failure, None);
                    Operand::Copy(self.materialize_rv(rv, span))
                }
            }
        };
        self.b.return_op(span, Some(ret_op));
        let else_block = self.b.end_block(saved);
        let cond = self.is_variant(&scrut, tc.success, span);
        self.b.push(crate::ir::stmt::Stmt {
            ty: self.cx.common.void,
            span,
            kind: crate::ir::stmt::StmtKind::If {
                cond,
                then_block,
                else_block: Some(else_block),
            },
        });
        Ok(Rvalue::use_op(span, Operand::Copy(rplace)))
    }
}
