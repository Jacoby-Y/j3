//! Generic bodies checked once at their definition, against their bounds (`j3c doc lang`, "Traits").
//! Each type parameter becomes a placeholder: an empty struct named after it whose only methods are
//! those of its bounds (supertraits and default methods included). The body is lowered once with
//! the placeholders, in a throwaway copy of the `IrCx`, and the IR is dropped. Only errors that a
//! placeholder caused (a method, comparison or field its bounds do not give, a bound it does not
//! meet) are reported; anything else (arithmetic on `T`, a cast) is left to the per-instance
//! lowering, which still runs as before. Templates that fail here skip their instances' errors.

use super::bounds::BoundCx;
use super::*;
use crate::check::{resolve_type, trait_short, with_supers};
use crate::ir::entity::{Func, Vis};

/// A type parameter stood in by an empty struct while its generic's body is checked.
#[derive(Clone)]
pub(crate) struct Placeholder {
    pub param: String,
    /// Trait keys of its bounds, as written (supertraits not expanded).
    pub bounds: Vec<String>,
}

/// One generic definition: the template and, for a method, the struct it belongs to.
struct Def<'m> {
    f: &'m FnItem,
    /// Generic struct template (`List`), or the plain struct of a generic method.
    owner: Option<&'m parse::StructItem>,
}

fn defs_of(m: &ModData) -> Vec<Def<'_>> {
    let mut out = Vec::new();
    for it in &m.ast.items {
        match it {
            Item::Fn(f) if !f.type_params.is_empty() && f.shader.is_none() => out.push(Def { f, owner: None }),
            Item::Trait(t) => {
                for meth in t.methods.iter().filter(|x| x.body.is_some()) {
                    if let Some(f) = m.gen_fns.get(&crate::check::default_fn_name(&t.name.name, &meth.name.name)) {
                        out.push(Def { f, owner: None });
                    }
                }
            }
            Item::Struct(s) => {
                for f in &s.methods {
                    if !s.type_params.is_empty() || !f.type_params.is_empty() {
                        out.push(Def { f, owner: Some(s) });
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Checks every generic definition once; returns the errors and records the failed templates'
/// spans in `shared.def_failed` (their instances' errors would repeat them).
pub(crate) fn check_defs(cx: &IrCx, mods: &[ModData], shared: &mut Shared) -> Vec<CheckError> {
    let mut errs = Vec::new();
    if mods.iter().all(|m| m.ast.is_script || defs_of(m).is_empty()) {
        return errs;
    }
    let mut cx = cx.clone();
    let mut struct_insts = HashMap::new();
    let mut fn_insts = HashMap::new();
    for m in mods {
        for (k, v) in &m.struct_insts {
            struct_insts.entry(k.clone()).or_insert(*v);
        }
        for (k, v) in &m.fn_insts {
            fn_insts.entry(k.clone()).or_insert(*v);
        }
    }
    for m in mods {
        if m.ast.is_script {
            continue;
        }
        let defs = defs_of(m);
        if defs.is_empty() {
            continue;
        }
        let mut t = Tables::of(m);
        for d in defs {
            let found = check_def(&mut cx, m, shared, &mut t, &mut struct_insts, &mut fn_insts, &d);
            if !found.is_empty() {
                shared.def_failed.insert(d.f.span);
                errs.extend(found);
            }
        }
    }
    errs
}

/// The module's lowering tables, copied so the check writes nothing back.
struct Tables {
    funcs: HashMap<String, FuncId>,
    methods: HashMap<(u32, String), FuncId>,
    enum_insts: HashMap<(String, Vec<Ty>), EnumId>,
    method_temps: HashMap<(u32, String), FnItem>,
    method_insts: HashMap<(u32, String, Vec<Ty>), FuncId>,
    fn_substs: HashMap<FuncId, HashMap<String, Ty>>,
}

impl Tables {
    fn of(m: &ModData) -> Tables {
        let mut funcs = m.funcs.clone();
        for (name, id) in &m.imports {
            funcs.insert(name.clone(), *id);
        }
        Tables {
            funcs,
            methods: m.methods.clone(),
            enum_insts: m.insts.clone(),
            method_temps: m.method_temps.clone(),
            method_insts: m.method_insts.clone(),
            fn_substs: m.fn_substs.clone(),
        }
    }
}

fn check_def(
    cx: &mut IrCx,
    m: &ModData,
    shared: &Shared,
    t: &mut Tables,
    struct_insts: &mut HashMap<(String, Vec<Ty>), StructId>,
    fn_insts: &mut HashMap<(String, Vec<Ty>), FuncId>,
    d: &Def,
) -> Vec<CheckError> {
    let mut queue = Vec::new();
    let (mut ir_enums, mut ir_structs, mut ir_funcs) = (Vec::new(), Vec::new(), Vec::new());
    let mut subst = HashMap::new();
    let add = |cx: &mut IrCx, t: &mut Tables, ps: &[Ident], bs: &[Vec<Ident>], subst: &mut HashMap<String, Ty>| {
        for (p, b) in ps.iter().zip(bs) {
            subst.insert(p.name.clone(), placeholder(cx, m, &shared.traits, &mut t.methods, p, b));
        }
    };
    let built = (|| -> Result<FuncId, CheckError> {
        let owner_sid = match d.owner {
            Some(s) if !s.type_params.is_empty() => {
                // a struct parameter's bounds, plus what this method's `where` adds to it
                let bs: Vec<Vec<Ident>> = s
                    .type_params
                    .iter()
                    .zip(&s.bounds)
                    .map(|(p, b)| {
                        let mut b = b.clone();
                        for (_, wb) in d.f.where_bounds.iter().filter(|(wp, _)| wp.name == p.name) {
                            for x in wb {
                                if !b.iter().any(|y| y.name == x.name) {
                                    b.push(x.clone());
                                }
                            }
                        }
                        b
                    })
                    .collect();
                add(cx, t, &s.type_params, &bs, &mut subst);
                let args: Vec<Ty> = s.type_params.iter().map(|p| subst[&p.name]).collect();
                let mut env = gen_env(m, t, struct_insts, &subst, &mut queue, &mut ir_enums, &mut ir_structs, &mut ir_funcs);
                let ty = instantiate_struct(cx, &m.path, s.name.span, &s.name.name, args, &mut env)?;
                match cx.ty_kind(ty) {
                    TyKind::Struct(sid) => Some(*sid),
                    _ => return Err(CheckError::new(&m.path, s.name.span, "not a struct")),
                }
            }
            Some(s) => m.structs.get(&s.name.name).copied(),
            None => None,
        };
        if d.f.type_params.is_empty() {
            // a plain method of a generic struct: the instance already holds its signature
            let sid = owner_sid.expect("a non-generic def is a method of a generic struct");
            return Ok(t.methods[&(sid.0, d.f.name.name.clone())]);
        }
        add(cx, t, &d.f.type_params, &d.f.bounds, &mut subst);
        let mut sig = d.f.clone();
        sig.body = parse::Block { span: d.f.body.span, stmts: Vec::new() };
        let mut env = gen_env(m, t, struct_insts, &subst, &mut queue, &mut ir_enums, &mut ir_structs, &mut ir_funcs);
        alloc_fn(cx, &m.path, &m.names, &sig, owner_sid, &HashMap::new(), &mut env)
    })();
    // A signature that does not resolve is reported by the instances (or the collector) already.
    let Ok(id) = built else { return Vec::new() };
    let mut merged_envs = HashMap::new();
    let lowered = lower_fn(
        cx,
        &m.path,
        id,
        d.f,
        &shared.origins,
        &mut merged_envs,
        &t.funcs,
        &m.consts,
        &m.types,
        &mut t.methods,
        &m.lib_ns,
        &m.mod_ns,
        &m.mod_types,
        &m.globals,
        &m.enum_temps,
        &mut t.enum_insts,
        &shared.struct_temps,
        &shared.struct_temp_env,
        struct_insts,
        &mut t.method_temps,
        &mut t.method_insts,
        &mut t.fn_substs,
        &mut queue,
        &m.names,
        GenCtx::new(&shared.gen_fns, &shared.origins, &m.gen_names, &m.gen_mods, fn_insts, &shared.traits),
        &mut ir_enums,
        &mut ir_structs,
        &mut ir_funcs,
        subst,
    );
    let mut out = Vec::new();
    if let Err(mut e) = lowered {
        let more = std::mem::take(&mut e.more);
        out.extend(std::iter::once(e).chain(more).filter(|e| e.bound_err));
    }
    // `Map<T, V>` in the body: does the placeholder meet `Map`'s bounds?
    let sites = std::mem::take(&mut cx.bound_queue);
    for site in sites {
        let bc = BoundCx { traits: &shared.traits, methods: &t.methods, method_temps: &t.method_temps, funcs: None };
        if let Err(e) = super::bounds::check_bounds(cx, &bc, &site.path, site.span, &site.needed_by, &site.reqs) {
            if e.bound_err {
                out.push(e);
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn gen_env<'a>(
    m: &'a ModData,
    t: &'a mut Tables,
    struct_insts: &'a mut HashMap<(String, Vec<Ty>), StructId>,
    subst: &'a HashMap<String, Ty>,
    queue: &'a mut Vec<(FuncId, FnItem, HashMap<String, Ty>)>,
    ir_enums: &'a mut Vec<EnumId>,
    ir_structs: &'a mut Vec<StructId>,
    ir_funcs: &'a mut Vec<FuncId>,
) -> GenEnv<'a> {
    GenEnv {
        types: &m.types,
        subst,
        enum_temps: &m.enum_temps,
        enum_insts: &mut t.enum_insts,
        struct_temps: &m.struct_temps,
        struct_temp_env: &m.struct_temp_env,
        struct_insts,
        methods: &mut t.methods,
        method_temps: &mut t.method_temps,
        method_insts: &mut t.method_insts,
        fn_substs: &mut t.fn_substs,
        pending_mono: queue,
        module: &m.names,
        ir_enums,
        ir_structs,
        ir_funcs,
    }
}

/// An empty struct standing in for `p`, with a bodiless method per method of its bounds.
fn placeholder(
    cx: &mut IrCx,
    m: &ModData,
    traits: &TraitEnv,
    methods: &mut HashMap<(u32, String), FuncId>,
    p: &Ident,
    bounds: &[Ident],
) -> Ty {
    let pname = cx.intern_name(&p.name);
    let sid = cx.alloc_struct(crate::ir::entity::StructDef {
        module: m.names.clone(),
        name: pname,
        doc: String::new(),
        fields: Vec::new(),
        packed: false,
        debug: false,
        span: p.span,
        ty_args: Vec::new(),
        script: None,
    });
    let ty = cx.ty_struct(sid);
    cx.placeholders.insert(sid, Placeholder { param: p.name.clone(), bounds: bounds.iter().map(|b| b.name.clone()).collect() });
    for b in bounds {
        for key in with_supers(traits, &b.name) {
            let Some(def) = traits.defs.get(&key) else { continue };
            let mut tys = def.types.clone();
            tys.insert("Self".to_string(), ty);
            for tm in &def.item.methods {
                let mkey = (sid.0, tm.name.name.clone());
                if methods.contains_key(&mkey) {
                    continue;
                }
                let params: Result<Vec<Ty>, _> = tm.params.iter().map(|q| resolve_type(cx, &q.ty, &def.path, &tys)).collect();
                let ret = match &tm.ret {
                    Some(r) => resolve_type(cx, r, &def.path, &tys),
                    None => Ok(cx.common.void),
                };
                // a signature that does not resolve is the trait's own error, reported elsewhere
                let (Ok(params), Ok(ret)) = (params, ret) else { continue };
                let fty = cx.ty_fn(params, ret);
                let name = cx.intern_name(&tm.name.name);
                let id = cx.alloc_func(Func {
                    id: FuncId(0),
                    doc: String::new(),
                    module: m.names.clone(),
                    name,
                    ty: fty,
                    vis: Vis::Pub,
                    method_of: Some(sid),
                    kind: FuncKind::Decl,
                    span: tm.span,
                });
                methods.insert(mkey, id);
            }
        }
    }
    ty
}

/// `T: Eq + Hash`, or `T` alone.
fn written(ph: &Placeholder) -> String {
    if ph.bounds.is_empty() {
        return ph.param.clone();
    }
    let names: Vec<&str> = ph.bounds.iter().map(|b| trait_short(b)).collect();
    format!("{}: {}", ph.param, names.join(" + "))
}

/// `T: Eq` grown by `extra`: `T: Eq + Ord`.
fn with_bound(ph: &Placeholder, extra: &str) -> String {
    if ph.bounds.is_empty() {
        format!("{}: {extra}", ph.param)
    } else {
        format!("{} + {extra}", written(ph))
    }
}

/// The placeholder behind `ty` (through `&`).
pub(crate) fn placeholder_of(cx: &IrCx, mut ty: Ty) -> Option<&Placeholder> {
    while let Some(inner) = cx.ref_inner(ty) {
        ty = inner;
    }
    match cx.ty_kind(ty) {
        TyKind::Struct(sid) => cx.placeholders.get(sid),
        _ => None,
    }
}

/// The error for a placeholder that does not meet `trait_key` (a bound of a generic it is passed to).
pub(crate) fn unmet_bound(cx: &IrCx, path: &str, span: Span, ty: Ty, trait_key: &str, needed_by: &str) -> Option<CheckError> {
    let ph = placeholder_of(cx, ty)?;
    let tr = trait_short(trait_key);
    let mut e = CheckError::new(path, span, format!("type parameter `{}` does not satisfy `{tr}` (needed by {needed_by})", ph.param))
        .note(format!("`{}` is declared `{}`; a generic body may use only what its bounds give", ph.param, written(ph)))
        .help(format!("add the bound: `{}`", with_bound(ph, tr)));
    e.bound_err = true;
    Some(e)
}

impl<'a> Lower<'a> {
    /// `what` (a method, comparison or field) used on a type parameter whose bounds do not give it.
    /// `method` is the trait method that would (`operator<` for `>`). `None` when `ty` is no placeholder.
    pub(super) fn bound_miss(&self, span: Span, ty: Ty, what: &str, method: Option<&str>) -> Option<CheckError> {
        placeholder_of(self.cx, ty).map(|ph| self.miss(span, ph, what, method))
    }

    /// `bound_miss` for a struct id.
    pub(super) fn bound_miss_sid(&self, span: Span, sid: StructId, what: &str, method: Option<&str>) -> Option<CheckError> {
        self.cx.placeholders.get(&sid).map(|ph| self.miss(span, ph, what, method))
    }

    fn miss(&self, span: Span, ph: &Placeholder, what: &str, method: Option<&str>) -> CheckError {
        let bounds = if ph.bounds.is_empty() {
            "it has no bounds".to_string()
        } else {
            format!("its bounds `{}` do not give it", written(ph).split_once(": ").map_or("", |x| x.1))
        };
        let mut e = CheckError::new(self.path, span, format!("type parameter `{}` has no {what}: {bounds}", ph.param));
        let mut givers: Vec<&str> = match method {
            Some(name) => self
                .gen
                .traits
                .defs
                .iter()
                .filter(|(_, d)| d.item.methods.iter().any(|x| x.name.name == name))
                .map(|(k, _)| trait_short(k))
                .collect(),
            None => Vec::new(),
        };
        givers.sort();
        givers.dedup();
        e = match (method, givers.as_slice()) {
            (None, _) => e.note("a bound gives a type parameter methods only, never fields")
                .help("call a trait method instead, or take a concrete type"),
            (Some(name), []) => e.help(format!("declare a trait with `fn {name}(this: Self, ..)` and add it: `{}`", with_bound(ph, "Trait"))),
            (Some(_), gs) => {
                let opts: Vec<String> = gs.iter().map(|g| format!("`{}`", with_bound(ph, g))).collect();
                e.help(format!("add the bound: {}", opts.join(" or ")))
            }
        };
        e.bound_err = true;
        e
    }
}
