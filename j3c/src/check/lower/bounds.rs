//! Does a type satisfy a trait? Checked where a bounded generic is used (`speculative/lang/traits.j3`):
//! a call of a generic fn or method, or a mention like `Map<K, V>`. Bodies are still checked per
//! instance, so a bound is the contract and the error text a caller sees.

use super::*;
use crate::check::{resolve_type, trait_short, ty_text, BoundReq, TraitDef, TraitEnv};
use crate::parse::TraitMethod;

/// What the checks look methods up in. Built by the caller from its own tables.
pub(crate) struct BoundCx<'a> {
    pub traits: &'a TraitEnv,
    pub methods: &'a HashMap<(u32, String), FuncId>,
    pub method_temps: &'a HashMap<(u32, String), FnItem>,
    /// Plain fns in scope, only to hint that they do not satisfy a bound.
    pub funcs: Option<&'a HashMap<String, FuncId>>,
}

enum Fail {
    Missing,
    /// The signature that was found.
    Wrong(String),
    Generic,
}

/// `==` and `!=` on a struct or payload enum need an `operator==`; everything else compares
/// natively. `binary` and `builtin_cmp` both ask this, so they cannot drift.
pub(crate) fn eq_needs_operator(cx: &IrCx, ty: Ty) -> bool {
    match cx.ty_kind(ty) {
        TyKind::Struct(_) => true,
        TyKind::Enum(id) => !cx.enum_def(*id).is_c_like(),
        _ => false,
    }
}

/// Is `a <op> b` built in for `ty`? Equality: numbers, bool, string, ptr, C-like enums.
/// Order: numbers (a `string` has none).
pub(crate) fn builtin_cmp(cx: &IrCx, ty: Ty, op: parse::BinOp) -> bool {
    use parse::BinOp::*;
    match op {
        Eq | Ne => !eq_needs_operator(cx, ty) && !matches!(cx.ty_kind(ty), TyKind::Slice { .. } | TyKind::Array { .. } | TyKind::Void),
        Lt | Le | Gt | Ge => is_numeric(cx, ty),
        _ => false,
    }
}

fn op_of(name: &str) -> Option<parse::BinOp> {
    use parse::BinOp::*;
    Some(match name.strip_prefix("operator")? {
        "==" => Eq,
        "!=" => Ne,
        "<" => Lt,
        "<=" => Le,
        ">" => Gt,
        ">=" => Ge,
        _ => return None,
    })
}

fn sig_text(cx: &IrCx, name: &str, ps: &[Ty], ret: Ty) -> String {
    let ps: Vec<String> = ps.iter().map(|p| ty_text(cx, *p)).collect();
    format!("fn {name}({}): {}", ps.join(", "), ty_text(cx, ret))
}

/// The signature as written in the trait, `fn hash(this: Self): u64`.
fn declared(cx: &IrCx, m: &TraitMethod) -> String {
    let src = cx.file(m.span.file).and_then(|f| f.src.get(m.span.start as usize..m.span.end as usize));
    src.unwrap_or(&m.name.name).trim().trim_end_matches(';').trim().to_string()
}

fn method_ok(cx: &mut IrCx, bc: &BoundCx, ty: Ty, def: &TraitDef, m: &TraitMethod) -> Result<Result<(), Fail>, CheckError> {
    let name = &m.name.name;
    let mut found = None;
    if let TyKind::Struct(sid) = *cx.ty_kind(ty) {
        let key = (sid.0, name.clone());
        if bc.method_temps.contains_key(&key) && !bc.methods.contains_key(&key) {
            return Ok(Err(Fail::Generic));
        }
        found = bc.methods.get(&key).copied();
    }
    if found.is_none() {
        found = bc.traits.impls.get(&(name.clone(), ty)).map(|i| i.id);
    }
    let Some(id) = found else {
        let builtin = op_of(name).is_some_and(|op| builtin_cmp(cx, ty, op));
        return Ok(if builtin { Ok(()) } else { Err(Fail::Missing) });
    };
    let (ps, ret) = match cx.fn_sig(cx.func(id).ty) {
        Some((p, r)) => (p.clone(), r),
        None => return Ok(Err(Fail::Missing)),
    };
    let mut tys = def.types.clone();
    tys.insert("Self".to_string(), ty);
    let mut want = Vec::new();
    for p in &m.params {
        want.push(resolve_type(cx, &p.ty, &def.path, &tys)?);
    }
    let want_ret = match &m.ret {
        Some(t) => resolve_type(cx, t, &def.path, &tys)?,
        None => cx.common.void,
    };
    let recv_ok = !ps.is_empty() && (ps[0] == ty || cx.ref_inner(ps[0]) == Some(ty));
    if recv_ok && ps.len() == want.len() && ps[1..] == want[1..] && ret == want_ret {
        Ok(Ok(()))
    } else {
        Ok(Err(Fail::Wrong(sig_text(cx, name, &ps, ret))))
    }
}

fn line_of(cx: &IrCx, span: Span) -> String {
    match cx.file(span.file) {
        Some(f) => {
            let (p, l) = f.loc(span.start);
            format!("{p}:{l}")
        }
        None => String::new(),
    }
}

/// Checks every bound of `reqs`; the first unmet one is the error, placed at `span`.
pub(crate) fn check_bounds(
    cx: &mut IrCx,
    bc: &BoundCx,
    path: &str,
    span: Span,
    needed_by: &str,
    reqs: &[BoundReq],
) -> Result<(), CheckError> {
    for r in reqs {
        let ty = r.arg;
        let tname = ty_text(cx, ty);
        for b in &r.bounds {
            let tname_trait = trait_short(&b.name);
            if cx.closure_sig(ty).is_some() {
                return Err(CheckError::new(path, span, format!("closure types cannot satisfy `{tname_trait}` (needed by `{}` of {needed_by})", r.param))
                    .note(format!("bound declared at {}", line_of(cx, b.span)))
                    .help("pass a plain `fn` value or a struct instead"));
            }
            let def = &bc.traits.defs[&b.name];
            let mut fails = Vec::new();
            for m in &def.item.methods {
                if let Err(f) = method_ok(cx, bc, ty, def, m)? {
                    fails.push((m, f));
                }
            }
            if fails.is_empty() {
                continue;
            }
            let mut e = CheckError::new(
                path,
                span,
                format!("`{tname}` does not satisfy `{tname_trait}` (needed by `{}` of {needed_by})", r.param),
            )
            .note(format!("bound declared at {}", line_of(cx, b.span)));
            for (m, f) in &fails {
                let want = declared(cx, m);
                e = e.note(match f {
                    Fail::Missing => format!("missing: {want}"),
                    Fail::Wrong(found) => format!("`{}` has the wrong signature: found `{found}`, the bound wants `{want}`", m.name.name),
                    Fail::Generic => format!("`{}` is a generic method; a bound needs a plain one: `{want}`", m.name.name),
                });
            }
            let (m, _) = &fails[0];
            let name = &m.name.name;
            // Operators take the receiver by value; other methods usually by `&`.
            let recv = if name.starts_with("operator") { tname.clone() } else { format!("&{tname}") };
            let want_ref = declared(cx, m).replace("this: Self", &format!("this: {recv}")).replace("Self", &tname);
            let want_val = declared(cx, m).replace("this: Self", &format!("x: {tname}")).replace("Self", &tname);
            if matches!(cx.ty_kind(ty), TyKind::Struct(_)) {
                e = e.help(format!("add `{want_ref}` to `struct {tname}`, or `#[impl] {want_val}` in a module that declares `{tname_trait}` or defines `{tname}`"));
            } else {
                e = e.help(format!("add `#[impl] {want_val}` in a module that declares `{tname_trait}`"));
            }
            if bc.funcs.is_some_and(|f| f.contains_key(name.as_str())) {
                e = e.help(format!("a plain fn `{name}` is in scope, but fns do not satisfy bounds; mark it `#[impl]`"));
            }
            return Err(e);
        }
    }
    Ok(())
}

/// Checks the instances queued by `instantiate_struct` / `instantiate_enum`.
pub(crate) fn drain_bounds(
    cx: &mut IrCx,
    traits: &TraitEnv,
    methods: &HashMap<(u32, String), FuncId>,
    method_temps: &HashMap<(u32, String), FnItem>,
    funcs: Option<&HashMap<String, FuncId>>,
) -> Result<(), CheckError> {
    while !cx.bound_queue.is_empty() {
        for site in std::mem::take(&mut cx.bound_queue) {
            let bc = BoundCx { traits, methods, method_temps, funcs };
            check_bounds(cx, &bc, &site.path, site.span, &site.needed_by, &site.reqs)?;
        }
    }
    Ok(())
}

impl<'a> Lower<'a> {
    /// The bounds of a generic fn or method being called with `targs` (`params`, `bounds` from its template).
    pub(super) fn check_call_bounds(
        &mut self,
        span: Span,
        needed_by: &str,
        params: &[parse::Ident],
        bounds: &[Vec<parse::Ident>],
        targs: &[Ty],
    ) -> Result<(), CheckError> {
        let reqs: Vec<BoundReq> = params
            .iter()
            .zip(bounds)
            .zip(targs)
            .filter(|((_, b), _)| !b.is_empty())
            .map(|((p, b), a)| BoundReq { param: p.name.clone(), arg: *a, bounds: b.clone() })
            .collect();
        if reqs.is_empty() {
            return Ok(());
        }
        let bc = BoundCx { traits: self.gen.traits, methods: &*self.methods, method_temps: &*self.method_temps, funcs: Some(self.funcs) };
        check_bounds(&mut *self.cx, &bc, self.path, span, needed_by, &reqs)
    }

    /// `x.m()` where `x` has no method `m`: the `#[impl] fn m` for its type, if there is one.
    pub(super) fn impl_target(&self, recv: Ty, name: &str) -> Option<FuncId> {
        // The VM calls imported fns by name; an `#[impl]` fn has none, so scripts cannot use them.
        if self.script_sid.is_some() {
            return None;
        }
        let mut t = recv;
        while let Some(inner) = self.cx.ref_inner(t) {
            t = inner;
        }
        let id = self.gen.traits.impls.get(&(name.to_string(), t))?.id;
        let first = self.cx.fn_sig(self.cx.func(id).ty)?.0.first().copied()?;
        self.this_adjust(recv, first).map(|_| id)
    }
}
