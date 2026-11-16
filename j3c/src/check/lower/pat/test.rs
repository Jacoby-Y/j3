//! One pattern against one place: its test, its bindings, and the parts a compound pattern
//! (variant fields, struct fields) looks into. Generic over nesting: every part is a place and a
//! pattern, tested and bound the same way as the top. A variant's parts are tested only after its
//! tag matched (they read the payload union).

use super::Binding;
use super::*;

impl<'a> Lower<'a> {
    /// Whether the value at `place` matches `pat`; `None` when it always does.
    pub(in crate::check::lower) fn pat_test(&mut self, place: &Place, pat: &Pat) -> Result<Option<Operand>, CheckError> {
        let span = pat.span();
        if matches!(pat, Pat::Wild(_) | Pat::Bind(_)) {
            return Ok(None);
        }
        if let TyKind::Int(int_ty) = *self.cx.ty_kind(place.ty) {
            let (lo, hi) = self.int_pat_range(pat, place.ty, int_ty)?;
            let v = |v: i128| Operand::int(span, place.ty, v as u64);
            if lo == hi {
                return Ok(Some(self.eq_ops(span, Operand::Copy(place.clone()), v(lo))));
            }
            let bool_ty = self.cx.common.bool;
            let ge = self.bind_bool(span, Rvalue::binary(span, bool_ty, BinOp::Ge, Operand::Copy(place.clone()), v(lo)));
            let le = self.bind_bool(span, Rvalue::binary(span, bool_ty, BinOp::Le, Operand::Copy(place.clone()), v(hi)));
            return Ok(Some(self.and_ops(span, ge, le)));
        }
        if let (TyKind::String, Pat::Lit(e @ Expr { kind: ExprKind::String(_), .. })) = (self.cx.ty_kind(place.ty), pat) {
            let want = self.operand(e, Some(place.ty))?;
            let rv = self.string_eq(span, false, Operand::Copy(place.clone()), want);
            return Ok(Some(self.bind_bool(span, rv)));
        }
        if let (TyKind::Bool, Pat::Lit(Expr { kind: ExprKind::Bool(v), .. })) = (self.cx.ty_kind(place.ty), pat) {
            let want = Operand::bool_const(span, self.cx.common.bool, *v);
            return Ok(Some(self.eq_ops(span, Operand::Copy(place.clone()), want)));
        }
        let (vid, parts) = self.pat_parts(place, pat)?;
        let Some(vid) = vid else { return self.parts_test(span, &parts) };
        let outer = self.is_variant(place, vid, span);
        // `matched = tag; if tag { matched = parts }`
        let saved = self.b.begin_block();
        let inner = self.parts_test(span, &parts)?;
        let Some(inner) = inner else {
            self.b.end_block(saved);
            return Ok(Some(outer));
        };
        let n = self.fresh("pm");
        let id = self.b.tmp(n, self.cx.common.bool, span);
        let matched = self.b.local_place(id, span);
        self.b.assign(span, matched.clone(), Rvalue::use_op(span, inner));
        let then_block = self.b.end_block(saved);
        self.b.assign(span, matched.clone(), Rvalue::use_op(span, outer.clone()));
        self.b.push(crate::ir::stmt::Stmt {
            ty: self.cx.common.void,
            span,
            kind: crate::ir::stmt::StmtKind::If { cond: outer, then_block, else_block: None },
        });
        Ok(Some(Operand::Copy(matched)))
    }

    /// All of `parts` match; `None` when each always does.
    fn parts_test(&mut self, span: Span, parts: &[(Place, &Pat)]) -> Result<Option<Operand>, CheckError> {
        let mut all = None;
        for (place, pat) in parts {
            if let Some(t) = self.pat_test(place, pat)? {
                all = Some(match all {
                    None => t,
                    Some(a) => self.and_ops(span, a, t),
                });
            }
        }
        Ok(all)
    }

    /// The names `pat` binds, in pattern order, read from `place`.
    pub(in crate::check::lower) fn pat_binds(&mut self, place: &Place, pat: &Pat, out: &mut Vec<Binding>) -> Result<(), CheckError> {
        match pat {
            Pat::Bind(n) => out.push(Binding { name: n.clone(), place: place.clone() }),
            Pat::Wild(_) | Pat::Lit(_) | Pat::Range { .. } => {}
            // an int `const` arm
            Pat::Variant { .. } if matches!(self.cx.ty_kind(place.ty), TyKind::Int(_)) => {}
            Pat::Variant { .. } | Pat::Struct { .. } | Pat::Tuple { .. } => {
                for (p, sub) in self.pat_parts(place, pat)?.1 {
                    self.pat_binds(&p, sub, out)?;
                }
            }
        }
        Ok(())
    }

    /// A variant or struct pattern: the variant it tests for (`None` for a struct) and each
    /// sub-pattern with the place it reads.
    fn pat_parts<'p>(&self, place: &Place, pat: &'p Pat) -> Result<(Option<VariantId>, Vec<(Place, &'p Pat)>), CheckError> {
        match pat {
            Pat::Variant { path, payload, span } => {
                let enum_id = self.enum_id_of(place.ty, *span, "an enum pattern")?;
                let (vid, _) = self.variant_pat(enum_id, path, payload.len(), *span)?;
                let parts = payload.iter().enumerate().map(|(k, p)| (self.variant_field_place(place, vid, k, *span), p)).collect();
                Ok((Some(vid), parts))
            }
            Pat::Struct { span, .. } => {
                let (sid, fields) = self.struct_pat_fields(place.ty, pat)?;
                let parts = fields
                    .into_iter()
                    .map(|(k, p)| (self.b.field(self.cx, place.clone(), FieldId { struct_id: sid, index: k }, *span), p))
                    .collect();
                Ok((None, parts))
            }
            Pat::Tuple { span, elems } => {
                let sid = self.tuple_pat_sid(place.ty, elems.len(), *span)?;
                let parts = elems
                    .iter()
                    .enumerate()
                    .map(|(k, p)| (self.b.field(self.cx, place.clone(), FieldId { struct_id: sid, index: k as u32 }, *span), p))
                    .collect();
                Ok((None, parts))
            }
            _ => Err(CheckError::new(self.path, pat.span(), format!("a value pattern on a `{}`", self.ty_str(place.ty)))
                .help("value and range patterns match integers (`true`/`false` a bool); an enum arm is `Enum.Variant`")),
        }
    }

    /// The struct of the tuple type `ty`, checking the pattern's arity.
    pub(in crate::check::lower) fn tuple_pat_sid(&self, ty: Ty, given: usize, span: Span) -> Result<StructId, CheckError> {
        let n = self.cx.tuple_elems(ty).map(|e| e.len());
        match (n, self.cx.ty_kind(ty)) {
            (Some(n), TyKind::Struct(sid)) if n == given => Ok(*sid),
            (Some(n), _) => Err(CheckError::new(self.path, span, format!("`{}` has {n} elements, the pattern gives {given}", self.ty_str(ty)))),
            _ => Err(CheckError::new(self.path, span, format!("a tuple pattern on a `{}`", self.ty_str(ty)))),
        }
    }

    /// The variant `path` names and its field types, checking that the pattern gives one
    /// sub-pattern per field.
    pub(in crate::check::lower) fn variant_pat(&self, enum_id: EnumId, path: &[Ident], given: usize, span: Span) -> Result<(VariantId, Vec<Ty>), CheckError> {
        let (vid, _) = self.variant_from_path(enum_id, path, span)?;
        let ftys = self.variant_field_tys(vid);
        if ftys.len() == given {
            return Ok((vid, ftys));
        }
        let name = path.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(".");
        let msg = match (ftys.len(), given) {
            (0, _) => format!("`{name}` has no payload"),
            (1, 0) => format!("match arm is missing a payload: `{name}(_)`"),
            (n, 0) => format!("match arm is missing the payload: `{name}({})`", vec!["_"; n].join(", ")),
            (n, g) => format!("`{name}` has {n} field{}, the pattern gives {g}", if n == 1 { "" } else { "s" }),
        };
        Err(CheckError::new(self.path, span, msg))
    }

    /// `S { a, b: p, .. }` on a value of type `ty`: the struct and each listed field's index and
    /// pattern. Without `..` every field must be listed.
    pub(in crate::check::lower) fn struct_pat_fields<'p>(&self, ty: Ty, pat: &'p Pat) -> Result<(StructId, Vec<(u32, &'p Pat)>), CheckError> {
        let Pat::Struct { span, path, fields, rest } = pat else { unreachable!("a struct pattern") };
        let TyKind::Struct(sid) = *self.cx.ty_kind(ty) else {
            return Err(CheckError::new(self.path, *span, format!("a struct pattern on a `{}`", self.ty_str(ty))));
        };
        let def = self.cx.struct_def(sid);
        let sname = self.cx.name_str(def.name);
        let base = sname.split('<').next().unwrap_or(sname);
        let written = &path[path.len() - 1];
        if written.name != base {
            return Err(CheckError::new(self.path, written.span, format!("expected `{base}`, got `{}`", written.name)));
        }
        let mut out: Vec<(u32, &Pat)> = Vec::new();
        for (name, p) in fields {
            let Some(k) = def.fields.iter().position(|f| self.cx.name_str(f.name) == name.name) else {
                return Err(CheckError::new(self.path, name.span, format!("`{base}` has no field `{}`", name.name)));
            };
            if out.iter().any(|(j, _)| *j == k as u32) {
                return Err(CheckError::new(self.path, name.span, format!("field `{}` is listed twice", name.name)));
            }
            out.push((k as u32, p));
        }
        if !rest && out.len() < def.fields.len() {
            let missing: Vec<String> = def
                .fields
                .iter()
                .enumerate()
                .filter(|(k, _)| !out.iter().any(|(j, _)| *j == *k as u32))
                .map(|(_, f)| format!("`{}`", self.cx.name_str(f.name)))
                .collect();
            return Err(CheckError::new(self.path, *span, format!("struct pattern is missing {}", missing.join(", ")))
                .help("list them, or end the pattern with `..` to skip the rest"));
        }
        Ok((sid, out))
    }
}
