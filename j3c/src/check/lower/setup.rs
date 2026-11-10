//! Per-fn setup: the environment a body is lowered in (generic origin merged in) and `lower_fn`.

use super::*;
use crate::check::DeclEnv;

pub(super) struct MergedEnv {
    funcs: HashMap<String, FuncId>,
    consts: HashMap<String, ConstVal>,
    types: HashMap<String, Ty>,
    enum_temps: HashMap<String, parse::EnumItem>,
    lib_ns: HashMap<String, HashMap<String, FuncId>>,
    mod_ns: HashMap<String, HashMap<String, FuncId>>,
    mod_types: HashMap<String, HashMap<String, Ty>>,
    globals: HashMap<String, crate::ir::ids::GlobalId>,
    struct_temps: HashMap<String, parse::StructItem>,
    struct_temp_env: HashMap<String, DeclEnv>,
}

pub(super) fn generic_origins(mods: &[ModData]) -> HashMap<String, OriginNs> {
    let mut out = HashMap::new();
    for m in mods {
        // A generic free fn is instantiated from its caller, so its body resolves in this module.
        if !m.gen_fns.is_empty() {
            out.insert(generic::mod_origin_key(&m.key), origin_of(m));
        }
        for it in &m.ast.items {
            let Item::Struct(s) = it else { continue };
            if s.type_params.is_empty() && !has_generic_method(s) {
                continue;
            }
            out.insert(s.name.name.clone(), origin_of(m));
        }
    }
    out
}

fn origin_of(m: &ModData) -> OriginNs {
    let mut funcs = m.funcs.clone();
    for (name, id) in &m.imports {
        funcs.insert(name.clone(), *id);
    }
    OriginNs {
        path: m.path.clone(),
        names: m.names.clone(),
        enum_temps: m.enum_temps.clone(),
        funcs,
        types: m.types.clone(),
        consts: m.consts.clone(),
        lib_ns: m.lib_ns.clone(),
        mod_ns: m.mod_ns.clone(),
        mod_types: m.mod_types.clone(),
        globals: m.globals.clone(),
        struct_temps: m.struct_temps.clone(),
        struct_temp_env: m.struct_temp_env.clone(),
    }
}

pub(super) fn lower_fn(
    cx: &mut IrCx,
    path: &str,
    id: FuncId,
    f: &FnItem,
    origins: &HashMap<String, OriginNs>,
    merged_envs: &mut HashMap<String, MergedEnv>,
    funcs: &HashMap<String, FuncId>,
    consts: &HashMap<String, ConstVal>,
    types: &HashMap<String, Ty>,
    methods: &mut HashMap<(u32, String), FuncId>,
    lib_ns: &HashMap<String, HashMap<String, FuncId>>,
    mod_ns: &HashMap<String, HashMap<String, FuncId>>,
    mod_types: &HashMap<String, HashMap<String, Ty>>,
    globals: &HashMap<String, crate::ir::ids::GlobalId>,
    enum_temps: &HashMap<String, parse::EnumItem>,
    enum_insts: &mut HashMap<(String, Vec<Ty>), EnumId>,
    struct_temps: &HashMap<String, parse::StructItem>,
    struct_temp_env: &HashMap<String, DeclEnv>,
    struct_insts: &mut HashMap<(String, Vec<Ty>), StructId>,
    method_temps: &mut HashMap<(u32, String), parse::FnItem>,
    method_insts: &mut HashMap<(u32, String, Vec<Ty>), FuncId>,
    fn_substs: &mut HashMap<FuncId, HashMap<String, Ty>>,
    pending_mono: &mut Vec<(FuncId, parse::FnItem, HashMap<String, Ty>)>,
    module_names: &[Name],
    mut gen: GenCtx<'_>,
    ir_enums: &mut Vec<EnumId>,
    ir_structs: &mut Vec<StructId>,
    ir_funcs: &mut Vec<FuncId>,
    type_subst: HashMap<String, Ty>,
) -> Result<crate::ir::body::Body, CheckError> {
    let mut body_path = path.to_string();
    let mut inst_note: Option<String> = None;
    let origin_key = match cx.func(id).method_of {
        Some(sid) => Some(cx.name_str(cx.struct_def(sid).name).to_string()),
        None if cx.inst_sites.contains_key(&id) => {
            Some(generic::mod_origin_key(&join_module(cx, &cx.func(id).module)))
        }
        None => None,
    };
    gen.depth = cx.inst_sites.get(&id).map_or(0, |s| s.1);
    let merged: Option<&MergedEnv> = if let Some(sname) = origin_key {
        if let Some(o) = origins.get(&sname) {
            if let Some(note) = generic::site_note(cx, id) {
                inst_note = Some(note);
                body_path = o.path.clone();
            } else if o.path != path {
                inst_note = Some(format!("while instantiating `{sname}` (from {path})"));
                body_path = o.path.clone();
            }
            Some(merged_envs.entry(sname).or_insert_with(|| MergedEnv {
                funcs: merge_map(&o.funcs, funcs),
                consts: merge_map(&o.consts, consts),
                types: merge_map(&o.types, types),
                enum_temps: merge_map(&o.enum_temps, enum_temps),
                lib_ns: merge_map(&o.lib_ns, lib_ns),
                mod_ns: merge_map(&o.mod_ns, mod_ns),
                mod_types: merge_map(&o.mod_types, mod_types),
                globals: merge_map(&o.globals, globals),
                struct_temps: merge_map(&o.struct_temps, struct_temps),
                struct_temp_env: merge_map(&o.struct_temp_env, struct_temp_env),
            }))
        } else {
            None
        }
    } else {
        None
    };
    let use_funcs = merged.as_ref().map(|m| &m.funcs).unwrap_or(funcs);
    let use_consts = merged.as_ref().map(|m| &m.consts).unwrap_or(consts);
    let use_enum_temps = merged.as_ref().map(|m| &m.enum_temps).unwrap_or(enum_temps);
    let use_types = merged.as_ref().map(|m| &m.types).unwrap_or(types);
    let use_lib_ns = merged.as_ref().map(|m| &m.lib_ns).unwrap_or(lib_ns);
    let use_mod_ns = merged.as_ref().map(|m| &m.mod_ns).unwrap_or(mod_ns);
    let use_mod_types = merged.as_ref().map(|m| &m.mod_types).unwrap_or(mod_types);
    let use_globals = merged.as_ref().map(|m| &m.globals).unwrap_or(globals);
    let use_struct_temps = merged
        .as_ref()
        .map(|m| &m.struct_temps)
        .unwrap_or(struct_temps);
    let use_struct_temp_env = merged
        .as_ref()
        .map(|m| &m.struct_temp_env)
        .unwrap_or(struct_temp_env);
    let (param_tys, ret) = match cx.fn_sig(cx.func(id).ty) {
        Some((ps, r)) => (ps.clone(), r),
        None => (Vec::new(), cx.common.void),
    };
    let mut b = BodyBuilder::new(cx.common.void);
    let mut locals = HashMap::new();
    for (p, ty) in f.params.iter().zip(param_tys.into_iter()) {
        let n = cx.intern_name(&p.name.name);
        let pid = b.param(n, ty, p.name.span);
        if locals.contains_key(&p.name.name) {
            return Err(CheckError::new(
                path,
                p.name.span,
                format!("duplicate parameter `{}`", p.name.name),
            ));
        }
        locals.insert(p.name.name.clone(), pid);
    }
    let fn_label = fn_label(cx, id, &type_subst);
    let mut l = Lower {
        cx,
        path: &body_path,
        b,
        locals,
        aliases: HashMap::new(),
        loops: Vec::new(),
        pending_label: None,
        script_sid: None,
        store_indirect: false,
        script_props: HashSet::new(),
        funcs: use_funcs,
        consts: use_consts,
        types: use_types,
        methods,
        lib_ns: use_lib_ns,
        mod_ns: use_mod_ns,
        mod_types: use_mod_types,
        globals: use_globals,
        enum_temps: use_enum_temps,
        enum_insts,
        struct_temps: use_struct_temps,
        struct_temp_env: use_struct_temp_env,
        struct_insts,
        method_temps,
        method_insts,
        fn_substs,
        pending_mono,
        module_names,
        gen,
        ir_enums,
        ir_structs,
        ir_funcs,
        type_subst,
        ret,
        tmp: 0,
        fn_label,
        closure_n: 0,
        in_closure: false,
        closure_outer: HashSet::new(),
        errs: Vec::new(),
        poisoned: HashSet::new(),
    };
    if let Some(sid) = l.cx.func(id).method_of {
        if let Some(meta) = l.cx.struct_def(sid).script.clone() {
            l.script_sid = Some(sid);
            let this = l.locals["this"];
            let base =
                l.b.deref(l.cx, l.b.local_place(this, f.name.span), f.name.span);
            let fields: Vec<(String, usize)> =
                l.cx.struct_def(sid)
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(i, field)| (l.cx.name_str(field.name).to_string(), i))
                    .collect();
            for (name, index) in fields {
                let mut place = l.b.field(
                    l.cx,
                    base.clone(),
                    FieldId {
                        struct_id: sid,
                        index: index as u32,
                    },
                    f.name.span,
                );
                if index < meta.prop_spans.len() {
                    place.access.assign = false;
                    l.script_props.insert(name.clone());
                }
                l.aliases.insert(name, place);
            }
        }
    }
    for stmt in &f.body.stmts {
        l.stmt_recover(stmt);
    }
    let mut errs = std::mem::take(&mut l.errs).into_iter().map(|e| match &inst_note {
        Some(n) => e.note(n.clone()),
        None => e,
    });
    match errs.next() {
        Some(mut first) => {
            first.more = errs.collect();
            Err(first)
        }
        None => Ok(l.b.finish()),
    }
}
