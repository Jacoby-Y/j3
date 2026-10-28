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

    pub(super) fn construct_variant(
        &mut self,
        span: Span,
        enum_name: &str,
        variant: &str,
        payload: Option<&Expr>,
        expected: Option<Ty>,
    ) -> Result<Rvalue, CheckError> {
        let id = self.ctor_enum_id(enum_name, expected, span)?;
        let (vid, pty) = self.find_variant(id, variant, span)?;
        let payload_op = match (pty, payload) {
            (None, None) => None,
            (Some(ty), Some(e)) => Some(self.operand(e, Some(ty))?),
            (Some(_), None) => {
                return Err(CheckError::new(
                    self.path,
                    span,
                    format!("`{enum_name}.{variant}` needs a payload"),
                ));
            }
            (None, Some(_)) => {
                return Err(CheckError::new(
                    self.path,
                    span,
                    format!("`{enum_name}.{variant}` has no payload"),
                ));
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

    pub(super) fn check_exhaustive(
        &self,
        enum_id: EnumId,
        arms: &[MatchArm],
        span: Span,
    ) -> Result<(), CheckError> {
        let n = self.cx.enum_def(enum_id).variants.len();
        let mut full = vec![false; n];
        let mut nested: Vec<HashSet<u32>> = vec![HashSet::new(); n];
        let mut nested_enum: Vec<Option<EnumId>> = vec![None; n];
        for (i, v) in self.cx.enum_def(enum_id).variants.iter().enumerate() {
            if let Some(pty) = v.payload {
                if let TyKind::Enum(id) = self.cx.ty_kind(pty) {
                    nested_enum[i] = Some(*id);
                }
            }
        }
        for arm in arms {
            let (vid, payload_ty) = self.variant_from_path(enum_id, &arm.path, arm.span)?;
            let i = vid.index as usize;
            match &arm.payload {
                None => {
                    if payload_ty.is_some() {
                        return Err(CheckError::new(
                            self.path,
                            arm.span,
                            "match arm is missing a payload",
                        ));
                    }
                    full[i] = true;
                }
                Some(Pat::Wild) | Some(Pat::Bind(_)) => {
                    if payload_ty.is_none() {
                        return Err(CheckError::new(
                            self.path,
                            arm.span,
                            "match arm has an unexpected payload",
                        ));
                    }
                    full[i] = true;
                }
                Some(Pat::Variant { path, .. }) => {
                    let pty = match payload_ty {
                        Some(t) => t,
                        None => {
                            return Err(CheckError::new(
                                self.path,
                                arm.span,
                                "match arm has an unexpected payload",
                            ));
                        }
                    };
                    let nid = self.enum_id_of(pty, arm.span, "nested match")?;
                    let (nv, _) = self.variant_from_path(nid, path, arm.span)?;
                    nested[i].insert(nv.index);
                }
            }
        }
        for i in 0..n {
            if full[i] {
                continue;
            }
            if let Some(eid) = nested_enum[i] {
                let need = self.cx.enum_def(eid).variants.len();
                if nested[i].len() == need {
                    continue;
                }
            }
            return Err(CheckError::new(self.path, span, "match is not exhaustive"));
        }
        Ok(())
    }

    pub(super) fn arm_cond(
        &mut self,
        scrut: &Place,
        enum_id: EnumId,
        arm: &MatchArm,
    ) -> Result<Operand, CheckError> {
        let (vid, payload_ty) = self.variant_from_path(enum_id, &arm.path, arm.span)?;
        let outer = self.is_variant(scrut, vid, arm.span);
        match &arm.payload {
            Some(Pat::Variant { path, .. }) => {
                let pty = match payload_ty {
                    Some(t) => t,
                    None => {
                        return Err(CheckError::new(
                            self.path,
                            arm.span,
                            "match arm has an unexpected payload",
                        ));
                    }
                };
                let nid = self.enum_id_of(pty, arm.span, "nested match")?;
                let (nv, _) = self.variant_from_path(nid, path, arm.span)?;
                let inner_place = self.payload_place(scrut, vid, pty, arm.span);
                let inner = self.is_variant(&inner_place, nv, arm.span);
                Ok(self.and_ops(arm.span, outer, inner))
            }
            _ => Ok(outer),
        }
    }

    pub(super) fn lower_match_arm(
        &mut self,
        scrut: &Place,
        enum_id: EnumId,
        arm: &MatchArm,
    ) -> Result<crate::ir::body::Block, CheckError> {
        let saved_locals = self.locals.clone();
        let saved_aliases = self.aliases.clone();
        let saved = self.b.begin_block();
        if let Some(Pat::Bind(n)) = &arm.payload {
            if self.locals.contains_key(&n.name) || self.aliases.contains_key(&n.name) {
                return Err(CheckError::new(
                    self.path,
                    n.span,
                    format!("shadowing `{}` is a compile error", n.name),
                ));
            }
            let (vid, payload_ty) = self.variant_from_path(enum_id, &arm.path, arm.span)?;
            let pty = match payload_ty {
                Some(t) => t,
                None => {
                    return Err(CheckError::new(
                        self.path,
                        arm.span,
                        "match arm has an unexpected payload",
                    ));
                }
            };
            let src = self.payload_place(scrut, vid, pty, n.span);
            let id = self.b.let_local(self.cx.intern_name(&n.name), pty, n.span);
            let dest = self.b.local_place(id, n.span);
            self.b
                .assign(n.span, dest, Rvalue::use_op(n.span, Operand::Copy(src)));
            self.locals.insert(n.name.clone(), id);
        }
        for stmt in &arm.body.stmts {
            self.stmt(stmt)?;
        }
        let block = self.b.end_block(saved);
        self.locals = saved_locals;
        self.aliases = saved_aliases;
        Ok(block)
    }

    pub(super) fn lower_match(&mut self, s: &parse::MatchStmt) -> Result<(), CheckError> {
        let op = self.operand(&s.expr, None)?;
        let enum_id = self.enum_id_of(op.ty(), s.span, "match")?;
        self.check_exhaustive(enum_id, &s.arms, s.span)?;
        let scrut = self.materialize(op, s.span);
        let mut else_block = None;
        for arm in s.arms.iter().rev() {
            let then_block = self.lower_match_arm(&scrut, enum_id, arm)?;
            let cond = self.arm_cond(&scrut, enum_id, arm)?;
            let saved = self.b.begin_block();
            self.b.push(crate::ir::stmt::Stmt {
                ty: self.cx.common.void,
                span: arm.span,
                kind: crate::ir::stmt::StmtKind::If {
                    cond,
                    then_block,
                    else_block,
                },
            });
            else_block = Some(self.b.end_block(saved));
        }
        if let Some(block) = else_block {
            for stmt in block.stmts {
                self.b.push(stmt);
            }
        }
        Ok(())
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
