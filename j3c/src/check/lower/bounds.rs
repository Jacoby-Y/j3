//! Does a type satisfy a trait? Checked where a bounded generic is used (`speculative/lang/traits.j3`):
//! a call of a generic fn or method, or a mention like `Map<K, V>`. Bodies are still checked per
//! instance, so a bound is the contract and the error text a caller sees.

use super::*;
use crate::check::{resolve_type, trait_short, ty_text, with_supers, BoundReq, TraitDef, TraitEnv};
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
    /// A tuple element, or a generic enum's type argument (its type), lacks the method.
    Element(String),
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
    // A tuple has `==` and `hash` when every element has them.
    if let Some(elems) = cx.tuple_elems(ty).map(|e| e.to_vec()) {
        if name == "operator==" || name == "hash" {
            for e in elems {
                if let Err(f) = method_ok(cx, bc, e, def, m)? {
                    return Ok(Err(match f {
                        Fail::Missing => Fail::Element(ty_text(cx, e)),
                        other => other,
                    }));
                }
            }
            return Ok(Ok(()));
        }
    }
    // a derived method of a generic enum instance: there when its type args meet the derive's bounds
    if let TyKind::Enum(eid) = *cx.ty_kind(ty) {
        let def = cx.enum_def(eid);
        if let Some(d) = bc.traits.enum_derives.get(&(name.clone(), def.span)) {
            for ((param, bounds), arg) in d.params.iter().zip(def.ty_args.clone()) {
                let req = BoundReq { param: param.clone(), arg, bounds: bounds.clone() };
                if !bounds.is_empty() && check_bounds(cx, bc, "", m.span, "", &[req]).is_err() {
                    return Ok(Err(Fail::Element(ty_text(cx, arg))));
                }
            }
            return Ok(Ok(()));
        }
    }
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
        // an empty `param` is a check with no type parameter (a `dyn` conversion)
        let by = if r.param.is_empty() { needed_by.to_string() } else { format!("`{}` of {needed_by}", r.param) };
        for b in &r.bounds {
            let tname_trait = trait_short(&b.name);
            if cx.closure_sig(ty).is_some() {
                return Err(CheckError::new(path, span, format!("closure types cannot satisfy `{tname_trait}` (needed by {by})"))
                    .note(format!("{} at {}", if r.param.is_empty() { "trait declared" } else { "bound declared" }, line_of(cx, b.span)))
                    .help("pass a plain `fn` value or a struct instead"));
            }
            // `T: Ord` with `trait Ord: Eq` also needs `Eq`
            let mut fails = Vec::new();
            let mut failed_key = String::new();
            for key in with_supers(bc.traits, &b.name) {
                // a trait key whose module is not loaded: only a derive writes keys by hand, and the
                // loader loads `j3.cmp` for those; were it missing, the derived body (lowered per
                // instance) still reports the missing method
                let Some(def) = bc.traits.defs.get(&key) else { continue };
                for m in &def.item.methods {
                    match method_ok(cx, bc, ty, def, m)? {
                        // a type without the method gets the default body
                        Err(Fail::Missing) if m.body.is_some() => {}
                        Err(f) => fails.push((m, f)),
                        Ok(()) => {}
                    }
                }
                if !fails.is_empty() {
                    failed_key = key;
                    break;
                }
            }
            if fails.is_empty() {
                continue;
            }
            if let Some(e) = super::defcheck::unmet_bound(cx, path, span, ty, &b.name, &by) {
                return Err(e);
            }
            let mut e = CheckError::new(
                path,
                span,
                format!("`{tname}` does not satisfy `{tname_trait}` (needed by {by})"),
            )
            .note(format!("{} at {}", if r.param.is_empty() { "trait declared" } else { "bound declared" }, line_of(cx, b.span)));
            if failed_key != b.name {
                e = e.note(format!("`{tname_trait}` requires `{}` (a supertrait), which `{tname}` does not meet", trait_short(&failed_key)));
            }
            let tname_trait = trait_short(&failed_key);
            for (m, f) in &fails {
                let want = declared(cx, m);
                e = e.note(match f {
                    Fail::Missing => format!("missing: {want}"),
                    Fail::Wrong(found) => format!("`{}` has the wrong signature: found `{found}`, the bound wants `{want}`", m.name.name),
                    Fail::Generic => format!("`{}` is a generic method; a bound needs a plain one: `{want}`", m.name.name),
                    Fail::Element(e) if cx.tuple_elems(ty).is_some() => format!("a tuple has `{}` only when every element does; `{e}` does not", m.name.name),
                    Fail::Element(e) => format!("a derived `{}` needs it on every type argument; `{e}` does not have it", m.name.name),
                });
            }
            let (m, _) = &fails[0];
            let name = &m.name.name;
            // Operators take the receiver by value; other methods usually by `&`.
            let recv = if name.starts_with("operator") { tname.clone() } else { format!("&{tname}") };
            let want_ref = declared(cx, m).replace("this: Self", &format!("this: {recv}")).replace("Self", &tname);
            let want_val = declared(cx, m).replace("this: Self", &format!("x: {tname}")).replace("Self", &tname);
            if cx.tuple_elems(ty).is_some() {
                e = e.help(format!("give each element type `{name}` (a method, an `#[impl]` fn, or `#[derive(..)]`)"));
            } else if matches!(cx.ty_kind(ty), TyKind::Struct(_)) {
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

    /// A method's `where` bounds (`traits::where_reqs`), met by the instance it is called on.
    pub(super) fn check_where(&mut self, span: Span, name: &str, reqs: &[BoundReq]) -> Result<(), CheckError> {
        if reqs.is_empty() {
            return Ok(());
        }
        let bc = BoundCx { traits: self.gen.traits, methods: &*self.methods, method_temps: &*self.method_temps, funcs: Some(self.funcs) };
        check_bounds(&mut *self.cx, &bc, self.path, span, &format!("`{name}`"), reqs)
    }

    /// `x.m(..)` where `x`'s type has no `m`, but a trait it meets has a default body for `m`:
    /// that body's instance for the type. `None` when no trait has a default `m`.
    pub(super) fn call_default(&mut self, span: Span, name: &str, recv: Place, args: &[Expr]) -> Result<Option<Rvalue>, CheckError> {
        // the VM has no generic instances (`call_generic` says so); scripts report "no method"
        if self.script_sid.is_some() {
            return Ok(None);
        }
        let Some(keys) = self.gen.traits.defaults.get(name).cloned() else { return Ok(None) };
        let mut ty = recv.ty;
        while let Some(inner) = self.cx.ref_inner(ty) {
            ty = inner;
        }
        let mut met = Vec::new();
        let mut first_err = None;
        for key in keys {
            let at = self.gen.traits.defs.get(&key).map_or(span, |d| d.item.name.span);
            let req = BoundReq { param: "Self".to_string(), arg: ty, bounds: vec![Ident { name: key.clone(), span: at }] };
            let bc = BoundCx { traits: self.gen.traits, methods: &*self.methods, method_temps: &*self.method_temps, funcs: None };
            match check_bounds(&mut *self.cx, &bc, self.path, span, &format!("the default `{name}`"), &[req]) {
                Ok(()) => met.push(key),
                Err(e) => {
                    first_err.get_or_insert(e);
                }
            }
        }
        match met.as_slice() {
            [key] => {
                let (module, tname) = key.rsplit_once("::").unwrap_or(("", key));
                let fname = crate::check::default_fn_name(tname, name);
                self.call_generic(span, module, &fname, &[], Some(recv), args).map(Some)
            }
            [] => Err(first_err.expect("a failed check per trait")),
            many => {
                let names: Vec<String> = many.iter().map(|k| format!("`{}`", trait_short(k))).collect();
                Err(CheckError::new(self.path, span, format!("`{name}` is ambiguous: {} all give `{}` a default `{name}`", names.join(", "), ty_text(self.cx, ty)))
                    .help(format!("define `{name}` on the type to choose")))
            }
        }
    }

    /// `x.m()` where `x` has no method `m`: the `#[impl] fn m` for its type (or the derived `m` of a
    /// generic enum instance), if there is one.
    pub(super) fn impl_target(&mut self, span: Span, recv: Ty, name: &str) -> Result<Option<FuncId>, CheckError> {
        // The VM calls imported fns by name; an `#[impl]` fn has none, so scripts cannot use them.
        if self.script_sid.is_some() {
            return Ok(None);
        }
        let mut t = recv;
        while let Some(inner) = self.cx.ref_inner(t) {
            t = inner;
        }
        let id = match self.gen.traits.impls.get(&(name.to_string(), t)) {
            Some(i) => i.id,
            None => match self.enum_derived(span, t, name)? {
                Some(id) => id,
                None => return Ok(None),
            },
        };
        let Some(first) = self.cx.fn_sig(self.cx.func(id).ty).and_then(|s| s.0.first().copied()) else { return Ok(None) };
        Ok(self.this_adjust(recv, first).map(|_| id))
    }

    /// `#[derive(..)]` on a generic enum (`check/derive.rs`): its instance of the derived `name` for
    /// `ty`'s type args, after their bounds. `None` when `ty` is not such an instance.
    pub(super) fn enum_derived(&mut self, span: Span, ty: Ty, name: &str) -> Result<Option<FuncId>, CheckError> {
        let TyKind::Enum(eid) = *self.cx.ty_kind(ty) else { return Ok(None) };
        let def = self.cx.enum_def(eid);
        let Some(d) = self.gen.traits.enum_derives.get(&(name.to_string(), def.span)) else { return Ok(None) };
        let (module, fname) = (d.module.clone(), d.fname.clone());
        let targs = def.ty_args.clone();
        let temps = self.gen.temps;
        let tmpl: &'a FnItem = &temps[&format!("{module}::{fname}")];
        let by = format!("the derived `{name}` of `{}`", self.ty_str(ty));
        self.check_call_bounds(span, &by, &tmpl.type_params, &tmpl.bounds, &targs)?;
        self.generic_instance(span, &module, &fname, tmpl, targs).map(Some)
    }
}
