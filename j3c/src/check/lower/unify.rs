//! Type-parameter inference for generic free fns: match a parameter's written type (`[]T`,
//! `&Box<T>`, `closure(T): T`) against an argument's checked type and bind each `T` it names.

use super::*;

/// `T` was bound to two different types.
pub(super) struct Conflict {
    pub name: String,
    pub first: Ty,
    pub second: Ty,
}

impl<'a> Lower<'a> {
    /// Binds the names in `tps` that `p` mentions, from `a`. Shapes that do not line up bind
    /// nothing; the argument is then checked against the instantiated parameter type.
    pub(super) fn unify(
        &self,
        p: &parse::Type,
        a: Ty,
        tps: &[String],
        bind: &mut HashMap<String, Ty>,
    ) -> Result<(), Conflict> {
        use parse::TypeKind as K;
        match &p.kind {
            K::Named(n) if tps.contains(n) => match bind.get(n) {
                Some(&first) if first != a => {
                    Err(Conflict { name: n.clone(), first, second: a })
                }
                _ => {
                    bind.insert(n.clone(), a);
                    Ok(())
                }
            },
            K::Named(_) => Ok(()),
            K::Ref(inner) => match self.cx.ref_inner(a) {
                Some(x) => self.unify(inner, x, tps, bind),
                None => Ok(()),
            },
            K::Slice(inner) => match self.cx.ty_kind(a) {
                TyKind::Slice { elem } | TyKind::Array { elem, .. } => self.unify(inner, *elem, tps, bind),
                _ => Ok(()),
            },
            K::Array { elem, .. } => match self.cx.ty_kind(a) {
                TyKind::Array { elem: e, .. } => self.unify(elem, *e, tps, bind),
                _ => Ok(()),
            },
            K::Generic { name, args } => {
                let have: Vec<Ty> = match self.cx.ty_kind(a) {
                    TyKind::Struct(id) if self.cx.name_str(self.cx.struct_def(*id).name) == name => {
                        self.cx.struct_def(*id).ty_args.iter().map(|(_, t)| *t).collect()
                    }
                    TyKind::Enum(id) if self.cx.name_str(self.cx.enum_def(*id).name) == name => {
                        self.cx.enum_def(*id).ty_args.clone()
                    }
                    _ => return Ok(()),
                };
                for (x, t) in args.iter().zip(have) {
                    self.unify(x, t, tps, bind)?;
                }
                Ok(())
            }
            K::Fn { params, ret } => {
                let (ps, r) = match self.cx.fn_sig(a) {
                    Some((ps, r)) => (ps.clone(), r),
                    None => return Ok(()),
                };
                self.unify_sig(params, ret.as_deref(), &ps, r, tps, bind)
            }
            K::Closure { params, ret } => {
                let (ps, r) = match self.cx.closure_sig(a) {
                    Some((ps, r)) => (ps.clone(), *r),
                    None => return Ok(()),
                };
                self.unify_sig(params, ret.as_deref(), &ps, r, tps, bind)
            }
        }
    }

    fn unify_sig(
        &self,
        params: &[parse::Type],
        ret: Option<&parse::Type>,
        ps: &[Ty],
        r: Ty,
        tps: &[String],
        bind: &mut HashMap<String, Ty>,
    ) -> Result<(), Conflict> {
        for (x, t) in params.iter().zip(ps) {
            self.unify(x, *t, tps, bind)?;
        }
        match ret {
            Some(x) => self.unify(x, r, tps, bind),
            None => Ok(()),
        }
    }
}
