//! `match` checks before lowering: arm order, enum exhaustiveness (`_`, guards), int values,
//! ranges and their overlaps.

use super::*;

impl<'a> Lower<'a> {
    /// One unguarded `_` at most, and it is the last arm (an arm after it never runs).
    pub(in crate::check::lower) fn check_arm_order(&self, arms: &[MatchArm]) -> Result<(), CheckError> {
        let mut wild = false;
        for arm in arms {
            if wild {
                let dup = arm.guard.is_none() && arm.pats.iter().any(|p| matches!(p, Pat::Wild(_)));
                let msg = if dup { "duplicate `_` arm" } else { "this arm never runs: the `_` arm before it matches everything" };
                return Err(CheckError::new(self.path, arm.span, msg));
            }
            if arm.guard.is_none() && arm.pats.iter().any(|p| matches!(p, Pat::Wild(_))) {
                wild = true;
            }
        }
        Ok(())
    }

    /// Every value of `ty` (an enum or a tuple) has an unguarded arm, unless there is a `_` (then
    /// warn if it covers none). Nested patterns count: `E.A(B.X)` covers only that part of `E.A`.
    pub(in crate::check::lower) fn check_exhaustive(&mut self, ty: Ty, arms: &[MatchArm], span: Span) -> Result<(), CheckError> {
        let enum_id = match self.cx.ty_kind(ty) {
            TyKind::Enum(id) => Some(*id),
            _ => None,
        };
        let mut rows = Vec::new();
        let mut wild = None;
        for arm in arms {
            for pat in &arm.pats {
                match pat {
                    Pat::Wild(s) => {
                        if arm.guard.is_none() {
                            wild = Some(*s);
                        }
                        continue;
                    }
                    Pat::Variant { path, payload, span } if enum_id.is_some() => {
                        self.variant_pat(enum_id.unwrap(), path, payload.len(), *span)?;
                    }
                    Pat::Tuple { span, elems } if enum_id.is_none() => {
                        self.tuple_pat_sid(ty, elems.len(), *span)?;
                    }
                    _ if enum_id.is_none() => {
                        return Err(CheckError::new(self.path, pat.span(), "a tuple match arm is `(a, b)` or `_`"));
                    }
                    _ => {
                        return Err(CheckError::new(self.path, pat.span(), "an enum match arm is `Enum.Variant`")
                            .help("value and range patterns match integers"))
                    }
                }
                if arm.guard.is_none() {
                    rows.push(vec![pat]);
                }
            }
        }
        let any = Pat::Wild(span);
        let mut missing = Vec::new();
        self.uncovered(&rows, &[ty], &any, &mut missing)?;
        let missing: Vec<String> = missing.into_iter().map(|mut w| format!("`{}`", w.remove(0))).collect();
        match wild {
            Some(w) if missing.is_empty() => {
                let msg = if enum_id.is_some() { "this `_` arm covers no variant: every one has its own arm" } else { "this `_` arm covers nothing: the arms before it match every value" };
                self.warn(w, msg.to_string());
                Ok(())
            }
            Some(_) => Ok(()),
            None if missing.is_empty() => Ok(()),
            None => {
                let more = if missing.len() >= MAX_MISSING { ", ..." } else { "" };
                Err(CheckError::new(self.path, span, format!("match is not exhaustive: missing {}{more}", missing.join(", ")))
                    .help("add an arm for each, or `_ { }` for the rest (a guarded arm does not count)"))
            }
        }
    }

    /// Values of types `tys` (one per column) that no row of patterns matches, pushed on `out` as
    /// one example per column (`_` where any value is missing), at most `MAX_MISSING`. A column
    /// is split by variant (or into a struct's fields) when some row looks inside it; ints and
    /// other values are covered only by `_` or a name.
    fn uncovered<'p>(&self, rows: &[Vec<&'p Pat>], tys: &[Ty], any: &'p Pat, out: &mut Vec<Vec<String>>) -> Result<(), CheckError> {
        if out.len() >= MAX_MISSING {
            return Ok(());
        }
        let Some((&ty, rest)) = tys.split_first() else {
            if rows.is_empty() {
                out.push(Vec::new());
            }
            return Ok(());
        };
        let opens = |p: &Pat| matches!(p, Pat::Variant { .. } | Pat::Struct { .. } | Pat::Tuple { .. });
        let split = rows.iter().any(|r| opens(r[0]));
        match *self.cx.ty_kind(ty) {
            TyKind::Enum(eid) if split => {
                let def = self.cx.enum_def(eid);
                let ename = self.cx.name_str(def.name).to_string();
                for k in 0..def.variants.len() {
                    let vid = VariantId { enum_id: eid, index: k as u32 };
                    let ftys = self.variant_field_tys(vid);
                    let mut spec = Vec::new();
                    for r in rows {
                        let head: Vec<&Pat> = match r[0] {
                            Pat::Variant { path, payload, span } => {
                                if self.variant_pat(eid, path, payload.len(), *span)?.0 != vid {
                                    continue;
                                }
                                payload.iter().collect()
                            }
                            p if p.irrefutable() => vec![any; ftys.len()],
                            _ => continue,
                        };
                        spec.push(head.into_iter().chain(r[1..].iter().copied()).collect());
                    }
                    let mut sub = Vec::new();
                    let cols: Vec<Ty> = ftys.iter().copied().chain(rest.iter().copied()).collect();
                    self.uncovered(&spec, &cols, any, &mut sub)?;
                    let vname = format!("{ename}.{}", self.cx.name_str(self.cx.enum_def(eid).variants[k].name));
                    for mut w in sub {
                        let tail = w.split_off(ftys.len());
                        let head = if ftys.is_empty() { vname.clone() } else { format!("{vname}({})", w.join(", ")) };
                        out.push(std::iter::once(head).chain(tail).collect());
                    }
                }
            }
            TyKind::Struct(sid) if split => {
                let def = self.cx.struct_def(sid);
                let n = def.fields.len();
                let mut spec = Vec::new();
                for r in rows {
                    let mut head = vec![any; n];
                    match r[0] {
                        p @ Pat::Struct { .. } => {
                            for (k, sub) in self.struct_pat_fields(ty, p)?.1 {
                                head[k as usize] = sub;
                            }
                        }
                        Pat::Tuple { span, elems } => {
                            self.tuple_pat_sid(ty, elems.len(), *span)?;
                            head = elems.iter().collect();
                        }
                        p if p.irrefutable() => {}
                        _ => continue,
                    }
                    spec.push(head.into_iter().chain(r[1..].iter().copied()).collect());
                }
                let cols: Vec<Ty> = def.fields.iter().map(|f| f.ty).chain(rest.iter().copied()).collect();
                let names: Vec<String> = def.fields.iter().map(|f| self.cx.name_str(f.name).to_string()).collect();
                let sname = self.cx.name_str(def.name).to_string();
                let mut sub = Vec::new();
                self.uncovered(&spec, &cols, any, &mut sub)?;
                for mut w in sub {
                    let tail = w.split_off(n);
                    let shown: Vec<String> = names.iter().zip(&w).filter(|(_, v)| *v != "_").map(|(f, v)| format!("{f}: {v}")).collect();
                    let head = if self.cx.tuple_elems(ty).is_some() {
                        format!("({})", w.join(", "))
                    } else if shown.is_empty() {
                        "_".to_string()
                    } else {
                        format!("{sname} {{ {}, .. }}", shown.join(", "))
                    };
                    out.push(std::iter::once(head).chain(tail).collect());
                }
            }
            TyKind::Bool if rows.iter().any(|r| bool_lit(r[0]).is_some()) => {
                for v in [true, false] {
                    let spec: Vec<Vec<&Pat>> =
                        rows.iter().filter(|r| r[0].irrefutable() || bool_lit(r[0]) == Some(v)).map(|r| r[1..].to_vec()).collect();
                    let mut sub = Vec::new();
                    self.uncovered(&spec, rest, any, &mut sub)?;
                    out.extend(sub.into_iter().map(|w| std::iter::once(v.to_string()).chain(w).collect()));
                }
            }
            _ => {
                let spec: Vec<Vec<&Pat>> = rows.iter().filter(|r| r[0].irrefutable()).map(|r| r[1..].to_vec()).collect();
                let mut sub = Vec::new();
                self.uncovered(&spec, rest, any, &mut sub)?;
                out.extend(sub.into_iter().map(|w| std::iter::once("_".to_string()).chain(w).collect()));
            }
        }
        out.truncate(MAX_MISSING);
        Ok(())
    }

    /// An int match: every value fits, no two unguarded arms overlap, an unguarded `_` exists.
    /// `Some(values)` (one per arm, `None` for `_`) when it can be one `Switch`: single values,
    /// no ranges, no `|`, no guards.
    pub(in crate::check::lower) fn check_int_arms(
        &mut self,
        s: &parse::MatchStmt,
        ty: Ty,
        int_ty: crate::ir::ty::IntTy,
    ) -> Result<Option<Vec<Option<i128>>>, CheckError> {
        let mut seen: Vec<(i128, i128, Span)> = Vec::new();
        let mut values = Vec::new();
        let mut switch = true;
        let mut has_wild = false;
        for arm in &s.arms {
            switch &= arm.guard.is_none() && arm.pats.len() == 1;
            let mut value = None;
            for pat in &arm.pats {
                if matches!(pat, Pat::Wild(_)) {
                    has_wild |= arm.guard.is_none();
                    continue;
                }
                let (lo, hi) = self.int_pat_range(pat, ty, int_ty)?;
                switch &= lo == hi;
                value = Some(lo);
                if arm.guard.is_none() {
                    seen.push((lo, hi, pat.span()));
                }
            }
            values.push(value);
        }
        seen.sort_by_key(|x| (x.0, x.2.start));
        for w in seen.windows(2) {
            let ((alo, ahi, aspan), (blo, bhi, bspan)) = (w[0], w[1]);
            if blo > ahi {
                continue;
            }
            let (first, second) = if aspan.start <= bspan.start { ((alo, ahi), (blo, bhi)) } else { ((blo, bhi), (alo, ahi)) };
            let later = if aspan.start <= bspan.start { bspan } else { aspan };
            let msg = if alo == ahi && blo == bhi {
                format!("duplicate match arm `{alo}`")
            } else {
                format!("match arm `{}` overlaps `{}`", range_str(second.0, second.1), range_str(first.0, first.1))
            };
            return Err(CheckError::new(self.path, later, msg));
        }
        if !has_wild {
            return Err(CheckError::new(self.path, s.span, "a `match` on an integer needs a `_` arm")
                .help("integers can't be listed exhaustively; add `_ { }`"));
        }
        Ok(if switch { Some(values) } else { None })
    }

    /// A `string` match: each arm a string literal or `_`, no literal twice (unguarded), and an
    /// unguarded `_`. Lowered as a compare chain (length, then bytes) in arm order.
    pub(in crate::check::lower) fn check_str_arms(&self, s: &parse::MatchStmt) -> Result<(), CheckError> {
        let mut seen: std::collections::HashSet<&[u8]> = std::collections::HashSet::new();
        let mut has_wild = false;
        for arm in &s.arms {
            for pat in &arm.pats {
                match pat {
                    Pat::Wild(_) => has_wild |= arm.guard.is_none(),
                    Pat::Lit(Expr { kind: ExprKind::String(b), span }) => {
                        if arm.guard.is_none() && !seen.insert(b) {
                            return Err(CheckError::new(self.path, *span, format!("duplicate match arm `{:?}`", String::from_utf8_lossy(b))));
                        }
                    }
                    _ => {
                        return Err(CheckError::new(self.path, pat.span(), "a `string` match arm is a string literal or `_`")
                            .help("bind the string with `_ { }` and test it with an `if`"))
                    }
                }
            }
        }
        if !has_wild {
            return Err(CheckError::new(self.path, s.span, "a `match` on a `string` needs a `_` arm").help("strings can't be listed exhaustively; add `_ { }`"));
        }
        Ok(())
    }

    /// The values an int pattern matches, both ends included: a literal, a `const`, `lo..=hi`.
    pub(in crate::check::lower) fn int_pat_range(&self, pat: &Pat, ty: Ty, int_ty: crate::ir::ty::IntTy) -> Result<(i128, i128), CheckError> {
        let end = |l: &Self, e: &Expr| -> Result<i128, CheckError> {
            // a `u64` const holds its bits in an i64: above `i64::MAX` it reads back negative
            let u64_const = match &e.kind {
                ExprKind::Name(n) => l.consts.get(&n.name).map(|c| c.ty),
                ExprKind::Field { .. } => l.qualified_const(e).map(|c| c.ty),
                _ => None,
            } == Some(l.cx.common.u64);
            let v = match e.kind {
                ExprKind::BigInt(v) => v as i128,
                _ => {
                    let v = l.fold_int(e).ok_or_else(|| {
                        CheckError::new(l.path, e.span, "an integer match arm is an int literal or an int `const`")
                    })?;
                    if u64_const { v as u64 as i128 } else { v as i128 }
                }
            };
            let (lo, hi) = match (int_ty.is_signed(), int_ty.bits()) {
                (true, b) => (-(1i128 << (b - 1)), (1i128 << (b - 1)) - 1),
                (false, b) => (0, (1i128 << b) - 1),
            };
            if v < lo || v > hi {
                return Err(CheckError::new(l.path, e.span, format!("match arm `{v}` does not fit `{}`", l.ty_str(ty))));
            }
            Ok(v)
        };
        match pat {
            Pat::Lit(e) => end(self, e).map(|v| (v, v)),
            Pat::Variant { path, payload, span } if payload.is_empty() => {
                let e = parse::path_expr(path, *span);
                end(self, &e).map(|v| (v, v))
            }
            Pat::Range { lo, hi } => {
                let (a, b) = (end(self, lo)?, end(self, hi)?);
                if a > b {
                    return Err(CheckError::new(self.path, pat.span(), format!("empty range `{a}..={b}`: its start is past its end")));
                }
                Ok((a, b))
            }
            Pat::Variant { span, .. } => Err(CheckError::new(self.path, *span, "an integer match arm has no payload")),
            Pat::Struct { span, .. } => Err(CheckError::new(self.path, *span, "a struct pattern on an integer")),
            Pat::Tuple { span, .. } => Err(CheckError::new(self.path, *span, "a tuple pattern on an integer")),
            Pat::Bind(n) => Err(CheckError::new(self.path, n.span, "an integer match arm is an int literal or an int `const`")),
            Pat::Wild(s) => Err(CheckError::new(self.path, *s, "`_` has no value")),
        }
    }

    /// A warning on the user's own code (not `j3.*`), shown with the lint warnings.
    pub(in crate::check::lower) fn warn(&mut self, span: Span, msg: String) {
        let std = self.module_names.first().is_some_and(|n| self.cx.name_str(*n) == "j3");
        if std || self.path.ends_with(".generated") {
            return;
        }
        self.cx.lower_warnings.push(crate::diag::Diagnostic::warning(self.path, span, msg));
    }
}

fn bool_lit(p: &Pat) -> Option<bool> {
    match p {
        Pat::Lit(Expr { kind: ExprKind::Bool(v), .. }) => Some(*v),
        _ => None,
    }
}

/// The examples a "not exhaustive" error lists.
const MAX_MISSING: usize = 10;

fn range_str(lo: i128, hi: i128) -> String {
    if lo == hi {
        lo.to_string()
    } else {
        format!("{lo}..={hi}")
    }
}
