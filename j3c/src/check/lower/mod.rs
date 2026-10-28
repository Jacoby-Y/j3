use std::collections::{HashMap, HashSet};

use super::error::CheckError;
use super::{
    alloc_fn, field_path, find_operator, instantiate_struct, is_float, is_int, is_numeric,
    resolve_type_g, ConstPayload, ConstVal, GenEnv, ModData,
};
use crate::intern::Name;
use crate::ir::build::BodyBuilder;
use crate::ir::cx::IrCx;
use crate::ir::dump::dump_ty;
use crate::ir::entity::FuncKind;
use crate::ir::ids::{EnumId, FieldId, FuncId, GlobalId, LocalId, StructId, VariantId};
use crate::ir::op::{BinOp, UnaryOp};
use crate::ir::operand::Operand;
use crate::ir::place::Place;
use crate::ir::rvalue::{Rvalue, RvalueKind};
use crate::ir::ty::{Ty, TyKind};
use crate::parse::{self, Expr, ExprKind, FnItem, Ident, InterpPart, Item, MatchArm, Pat, Stmt};
use crate::span::Span;

mod binop;
mod call;
mod enums;
mod expr;
mod lit;
mod place;
mod stmt;

struct OriginNs {
    path: String,
    funcs: HashMap<String, FuncId>,
    types: HashMap<String, Ty>,
    consts: HashMap<String, ConstVal>,
    lib_ns: HashMap<String, HashMap<String, FuncId>>,
    mod_ns: HashMap<String, HashMap<String, FuncId>>,
    mod_types: HashMap<String, HashMap<String, Ty>>,
    globals: HashMap<String, GlobalId>,
    struct_temps: HashMap<String, parse::StructItem>,
    struct_temp_env: HashMap<String, HashMap<String, Ty>>,
}

/// Lowering state read by every module and written by none, built once before the lower loop.
pub(crate) struct Shared {
    origins: HashMap<String, OriginNs>,
    struct_temps: HashMap<String, parse::StructItem>,
    struct_temp_env: HashMap<String, HashMap<String, Ty>>,
}

pub(crate) fn shared(mods: &[ModData]) -> Shared {
    let mut struct_temps = HashMap::new();
    let mut struct_temp_env = HashMap::new();
    for m in mods.iter() {
        for (k, v) in &m.struct_temps {
            struct_temps.entry(k.clone()).or_insert_with(|| v.clone());
        }
        for (k, v) in &m.struct_temp_env {
            struct_temp_env.entry(k.clone()).or_insert_with(|| v.clone());
        }
    }
    Shared {
        origins: generic_origins(mods),
        struct_temps,
        struct_temp_env,
    }
}

fn generic_origins(mods: &[ModData]) -> HashMap<String, OriginNs> {
    let mut out = HashMap::new();
    for m in mods {
        for it in &m.ast.items {
            let Item::Struct(s) = it else { continue };
            if s.type_params.is_empty() {
                continue;
            }
            let mut funcs = m.funcs.clone();
            for (name, id) in &m.imports {
                funcs.insert(name.clone(), *id);
            }
            out.insert(
                s.name.name.clone(),
                OriginNs {
                    path: m.path.clone(),
                    funcs,
                    types: m.types.clone(),
                    consts: m.consts.clone(),
                    lib_ns: m.lib_ns.clone(),
                    mod_ns: m.mod_ns.clone(),
                    mod_types: m.mod_types.clone(),
                    globals: m.globals.clone(),
                    struct_temps: m.struct_temps.clone(),
                    struct_temp_env: m.struct_temp_env.clone(),
                },
            );
        }
    }
    out
}

fn merge_map<V: Clone>(base: &HashMap<String, V>, extra: &HashMap<String, V>) -> HashMap<String, V> {
    let mut out = base.clone();
    for (k, v) in extra {
        out.entry(k.clone()).or_insert_with(|| v.clone());
    }
    out
}

pub(crate) fn lower_module(
    cx: &mut IrCx,
    mods: &mut [ModData],
    i: usize,
    main_id: FuncId,
    shared: &Shared,
) -> Result<(), CheckError> {
    let origins = &shared.origins;
    let struct_temps = &shared.struct_temps;
    let struct_temp_env = &shared.struct_temp_env;
    let path = mods[i].path.clone();
    let mut funcs = mods[i].funcs.clone();
    for (name, id) in &mods[i].imports {
        funcs.insert(name.clone(), *id);
    }
    if !funcs.contains_key("main") {
        funcs.insert("main".to_string(), main_id);
    }
    let consts = mods[i].consts.clone();
    let types = mods[i].types.clone();
    let mut methods = mods[i].methods.clone();
    let lib_ns = mods[i].lib_ns.clone();
    let mod_ns = mods[i].mod_ns.clone();
    let mod_types = mods[i].mod_types.clone();
    let globals = mods[i].globals.clone();
    let enum_temps = mods[i].enum_temps.clone();
    let mut enum_insts = mods[i].insts.clone();
    let mut struct_insts = HashMap::new();
    for m in mods.iter() {
        for (k, v) in &m.struct_insts {
            struct_insts.entry(k.clone()).or_insert(*v);
        }
    }
    let mut method_temps = mods[i].method_temps.clone();
    let mut method_insts = mods[i].method_insts.clone();
    let mut fn_substs = mods[i].fn_substs.clone();
    let mut pending_mono = mods[i].pending_mono.clone();
    let module_names = mods[i].names.clone();

    let mut pending: Vec<(FuncId, FnItem)> = Vec::new();
    for it in &mods[i].ast.items {
        match it {
            Item::Fn(f) => pending.push((mods[i].funcs[&f.name.name], f.clone())),
            Item::Struct(s) => {
                if !s.type_params.is_empty() {
                    continue;
                }
                let sid = mods[i].structs[&s.name.name];
                for m in &s.methods {
                    if !m.type_params.is_empty() {
                        continue;
                    }
                    pending.push((mods[i].methods[&(sid.0, m.name.name.clone())], m.clone()));
                }
            }
            _ => {}
        }
    }

    let mut queue: Vec<(FuncId, FnItem, HashMap<String, Ty>)> = pending
        .into_iter()
        .map(|(id, f)| (id, f, HashMap::new()))
        .collect();
    queue.append(&mut pending_mono);
    // A generic struct's merged environment depends only on the struct here, so build it once.
    let mut merged_envs: HashMap<String, MergedEnv> = HashMap::new();
    // Batches keep first-in-first-out order: instances queued while lowering run after.
    while !queue.is_empty() {
        let batch = std::mem::take(&mut queue);
        for (id, f, subst) in batch {
            if matches!(cx.func(id).kind, FuncKind::J3 { .. }) {
                continue;
            }
            let body = lower_fn(
                cx,
                &path,
                id,
                &f,
                origins,
                &mut merged_envs,
                &funcs,
                &consts,
                &types,
                &mut methods,
                &lib_ns,
                &mod_ns,
                &mod_types,
                &globals,
                &enum_temps,
                &mut enum_insts,
                struct_temps,
                struct_temp_env,
                &mut struct_insts,
                &mut method_temps,
                &mut method_insts,
                &mut fn_substs,
                &mut queue,
                &module_names,
                &mut mods[i].ir.enums,
                &mut mods[i].ir.structs,
                &mut mods[i].ir.funcs,
                subst,
            )?;
            cx.func_mut(id).kind = FuncKind::J3 { body };
        }
    }
    mods[i].insts = enum_insts;
    mods[i].methods = methods;
    mods[i].struct_insts = struct_insts;
    mods[i].method_temps = method_temps;
    mods[i].method_insts = method_insts;
    mods[i].fn_substs = fn_substs;
    Ok(())
}

/// Span of the temporary at the root of a field/index chain (`f()` in `f().x[0]`).
fn temp_span(e: &Expr) -> Span {
    match &e.kind {
        ExprKind::Field { base, .. } | ExprKind::Index { base, .. } => temp_span(base),
        _ => e.span,
    }
}

struct Lower<'a> {
    cx: &'a mut IrCx,
    path: &'a str,
    b: BodyBuilder,
    locals: HashMap<String, LocalId>,
    aliases: HashMap<String, Place>,
    script_sid: Option<StructId>,
    /// Set while lowering a place that indexes through a slice (a store there hits real storage).
    store_indirect: bool,
    script_props: HashSet<String>,
    funcs: &'a HashMap<String, FuncId>,
    consts: &'a HashMap<String, ConstVal>,
    types: &'a HashMap<String, Ty>,
    methods: &'a mut HashMap<(u32, String), FuncId>,
    lib_ns: &'a HashMap<String, HashMap<String, FuncId>>,
    mod_ns: &'a HashMap<String, HashMap<String, FuncId>>,
    mod_types: &'a HashMap<String, HashMap<String, Ty>>,
    globals: &'a HashMap<String, crate::ir::ids::GlobalId>,
    enum_temps: &'a HashMap<String, parse::EnumItem>,
    enum_insts: &'a mut HashMap<(String, Vec<Ty>), EnumId>,
    struct_temps: &'a HashMap<String, parse::StructItem>,
    struct_temp_env: &'a HashMap<String, HashMap<String, Ty>>,
    struct_insts: &'a mut HashMap<(String, Vec<Ty>), StructId>,
    method_temps: &'a mut HashMap<(u32, String), parse::FnItem>,
    method_insts: &'a mut HashMap<(u32, String, Vec<Ty>), FuncId>,
    fn_substs: &'a mut HashMap<FuncId, HashMap<String, Ty>>,
    pending_mono: &'a mut Vec<(FuncId, parse::FnItem, HashMap<String, Ty>)>,
    module_names: &'a [Name],
    ir_enums: &'a mut Vec<EnumId>,
    ir_structs: &'a mut Vec<StructId>,
    ir_funcs: &'a mut Vec<FuncId>,
    type_subst: HashMap<String, Ty>,
    ret: Ty,
    tmp: u32,
}

struct MergedEnv {
    funcs: HashMap<String, FuncId>,
    consts: HashMap<String, ConstVal>,
    types: HashMap<String, Ty>,
    lib_ns: HashMap<String, HashMap<String, FuncId>>,
    mod_ns: HashMap<String, HashMap<String, FuncId>>,
    mod_types: HashMap<String, HashMap<String, Ty>>,
    globals: HashMap<String, crate::ir::ids::GlobalId>,
    struct_temps: HashMap<String, parse::StructItem>,
    struct_temp_env: HashMap<String, HashMap<String, Ty>>,
}

fn lower_fn(
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
    struct_temp_env: &HashMap<String, HashMap<String, Ty>>,
    struct_insts: &mut HashMap<(String, Vec<Ty>), StructId>,
    method_temps: &mut HashMap<(u32, String), parse::FnItem>,
    method_insts: &mut HashMap<(u32, String, Vec<Ty>), FuncId>,
    fn_substs: &mut HashMap<FuncId, HashMap<String, Ty>>,
    pending_mono: &mut Vec<(FuncId, parse::FnItem, HashMap<String, Ty>)>,
    module_names: &[Name],
    ir_enums: &mut Vec<EnumId>,
    ir_structs: &mut Vec<StructId>,
    ir_funcs: &mut Vec<FuncId>,
    type_subst: HashMap<String, Ty>,
) -> Result<crate::ir::body::Body, CheckError> {
    let mut body_path = path.to_string();
    let mut inst_note: Option<String> = None;
    let merged: Option<&MergedEnv> = if let Some(sid) = cx.func(id).method_of {
        let sname = cx.name_str(cx.struct_def(sid).name).to_string();
        if let Some(o) = origins.get(&sname) {
            if o.path != path {
                inst_note = Some(format!("while instantiating `{sname}` (from {path})"));
                body_path = o.path.clone();
            }
            Some(merged_envs.entry(sname).or_insert_with(|| MergedEnv {
                funcs: merge_map(&o.funcs, funcs),
                consts: merge_map(&o.consts, consts),
                types: merge_map(&o.types, types),
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
    let mut l = Lower {
        cx,
        path: &body_path,
        b,
        locals,
        aliases: HashMap::new(),
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
        enum_temps,
        enum_insts,
        struct_temps: use_struct_temps,
        struct_temp_env: use_struct_temp_env,
        struct_insts,
        method_temps,
        method_insts,
        fn_substs,
        pending_mono,
        module_names,
        ir_enums,
        ir_structs,
        ir_funcs,
        type_subst,
        ret,
        tmp: 0,
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
        l.stmt(stmt).map_err(|e| match &inst_note {
            Some(n) => e.note(n.clone()),
            None => e,
        })?;
    }
    Ok(l.b.finish())
}

impl<'a> Lower<'a> {
    /// A script instance used by value. Generic code (`List<Door>` growing) may move one;
    /// user code may not copy one.
    fn is_script_value(&self, ty: Ty) -> bool {
        if !self.type_subst.is_empty() {
            return false;
        }
        match self.cx.ty_kind(ty) {
            TyKind::Struct(sid) => self.cx.struct_def(*sid).script.is_some(),
            _ => false,
        }
    }

    /// `j3.scene3d.Scene`.
    fn is_scene(&self, sid: StructId) -> bool {
        let def = self.cx.struct_def(sid);
        self.cx.name_str(def.name) == "Scene"
            && def.module.len() == 2
            && self.cx.name_str(def.module[0]) == "j3"
            && self.cx.name_str(def.module[1]) == "scene3d"
    }

    /// `scene.script<Door>(node)`: the registry's `find` for that script type.
    fn scene_script_find(&self, ty: Ty, span: Span) -> Result<FuncId, CheckError> {
        let script = match self.cx.ty_kind(ty) {
            TyKind::Struct(sid) if self.cx.struct_def(*sid).script.is_some() => *sid,
            _ => {
                return Err(CheckError::new(
                    self.path,
                    span,
                    "`scene.script<T>` takes a script type",
                )
                .help("bind one with `const Door = @script(\"scripts/door.j3s\");`"));
            }
        };
        let path = crate::emit::bc::script_path(self.cx, &self.cx.struct_def(script).module);
        let want = format!("find__{}", crate::compile::scene_scripts::ident(&path));
        let registry = |f: &crate::ir::entity::Func| {
            f.module.len() == 3
                && self.cx.name_str(f.module[0]) == "j3"
                && self.cx.name_str(f.module[1]) == "scene3d"
                && self.cx.name_str(f.module[2]) == "scripts"
        };
        self.cx
            .funcs()
            .iter()
            .find(|f| registry(f) && self.cx.name_str(f.name) == want)
            .map(|f| f.id)
            .ok_or_else(|| {
                CheckError::new(
                    self.path,
                    span,
                    format!("no scene attaches {path}, so no object has one"),
                )
                .help(format!(
                    "attach it to an object in the editor (`script={path}` on its line)"
                ))
            })
    }

    fn script_copy(&self, span: Span, ty: Ty) -> CheckError {
        let name = match self.cx.ty_kind(ty) {
            TyKind::Struct(sid) => self.cx.name_str(self.cx.struct_def(*sid).name).to_string(),
            _ => "the script".to_string(),
        };
        CheckError::new(self.path, span, "script instances can't be copied")
            .help(format!("pass `&{name}` instead"))
    }

    fn script_prop_name<'b>(&self, expr: &'b Expr) -> Option<&'b str> {
        match &expr.kind {
            ExprKind::Name(n) if self.script_props.contains(&n.name) => Some(&n.name),
            ExprKind::Field { base, field } => {
                if matches!(&base.kind, ExprKind::Name(n) if n.name == "this")
                    && self.script_props.contains(&field.name)
                {
                    Some(&field.name)
                } else {
                    self.script_prop_name(base)
                }
            }
            ExprKind::Index { base, .. } => self.script_prop_name(base),
            _ => None,
        }
    }

    fn ty_str(&self, ty: Ty) -> String {
        let mut s = String::new();
        dump_ty(self.cx, ty, &mut s);
        s
    }

    fn mismatch(&self, span: Span, what: &str, have: Ty, want: Ty) -> CheckError {
        CheckError::new(
            self.path,
            span,
            format!(
                "{what}: expected `{}`, found `{}`",
                self.ty_str(want),
                self.ty_str(have)
            ),
        )
    }

    fn unknown(&self, span: Span, kind: &str, name: &str, extra: &[String]) -> CheckError {
        let mut cands: Vec<String> = self.locals.keys().cloned().collect();
        cands.extend(self.aliases.keys().cloned());
        cands.extend(self.funcs.keys().cloned());
        cands.extend(self.types.keys().cloned());
        cands.extend(self.consts.keys().filter(|k| !k.contains('.')).cloned());
        cands.extend(self.globals.keys().cloned());
        cands.extend(extra.iter().cloned());
        let mut e = CheckError::new(self.path, span, format!("unknown {kind} `{name}`"));
        if let Some(s) = super::error::suggest(name, &cands) {
            e = e.help(format!("did you mean `{s}`?"));
        }
        e
    }

    fn no_method(&self, span: Span, sid: StructId, name: &str) -> CheckError {
        let mut names: Vec<String> = self
            .methods
            .keys()
            .filter(|(id, _)| *id == sid.0)
            .map(|(_, n)| n.clone())
            .collect();
        names.extend(
            self.method_temps
                .keys()
                .filter(|(id, _)| *id == sid.0)
                .map(|(_, n)| n.clone()),
        );
        let mut e = CheckError::new(self.path, span, format!("no method `{name}`"));
        if let Some(s) = super::error::suggest(name, &names) {
            e = e.help(format!("did you mean `{s}`?"));
        }
        e
    }

    fn fresh(&mut self, hint: &str) -> Name {
        let n = self.tmp;
        self.tmp += 1;
        self.cx.intern_name(&format!("_{hint}{n}"))
    }

    fn resolve_ty(&mut self, t: &parse::Type) -> Result<Ty, CheckError> {
        let path = self.path;
        resolve_type_g(
            self.cx,
            t,
            path,
            &mut GenEnv {
                types: self.types,
                subst: &self.type_subst,
                enum_temps: self.enum_temps,
                enum_insts: self.enum_insts,
                struct_temps: self.struct_temps,
                struct_temp_env: self.struct_temp_env,
                struct_insts: self.struct_insts,
                methods: self.methods,
                method_temps: self.method_temps,
                method_insts: self.method_insts,
                fn_substs: self.fn_substs,
                pending_mono: self.pending_mono,
                module: self.module_names,
                ir_enums: self.ir_enums,
                ir_structs: self.ir_structs,
                ir_funcs: self.ir_funcs,
            },
        )
    }

    fn struct_id_named(&self, name: &str) -> Option<StructId> {
        let ty = self.types.get(name)?;
        match self.cx.ty_kind(*ty) {
            TyKind::Struct(id) => Some(*id),
            _ => None,
        }
    }

    fn resolve_type_args(&mut self, args: &[parse::Type]) -> Result<Vec<Ty>, CheckError> {
        let mut out = Vec::new();
        for a in args {
            out.push(self.resolve_ty(a)?);
        }
        Ok(out)
    }

    fn instantiate_struct_named(
        &mut self,
        name: &str,
        args: &[parse::Type],
        span: Span,
    ) -> Result<StructId, CheckError> {
        let resolved = self.resolve_type_args(args)?;
        let path = self.path;
        let ty = instantiate_struct(
            self.cx,
            path,
            span,
            name,
            resolved,
            &mut GenEnv {
                types: self.types,
                subst: &self.type_subst,
                enum_temps: self.enum_temps,
                enum_insts: self.enum_insts,
                struct_temps: self.struct_temps,
                struct_temp_env: self.struct_temp_env,
                struct_insts: self.struct_insts,
                methods: self.methods,
                method_temps: self.method_temps,
                method_insts: self.method_insts,
                fn_substs: self.fn_substs,
                pending_mono: self.pending_mono,
                module: self.module_names,
                ir_enums: self.ir_enums,
                ir_structs: self.ir_structs,
                ir_funcs: self.ir_funcs,
            },
        )?;
        match self.cx.ty_kind(ty) {
            TyKind::Struct(id) => Ok(*id),
            _ => Err(CheckError::new(self.path, span, "not a struct")),
        }
    }

    fn is_ptr_ty(&self, ty: Ty) -> bool {
        matches!(
            self.cx.ty_kind(ty),
            TyKind::Ptr | TyKind::CStr | TyKind::Ref(_)
        )
    }

}

fn embed_alias_names(cx: &IrCx, sid: StructId) -> Vec<String> {
    let mut out = Vec::new();
    for f in &cx.struct_def(sid).fields {
        out.push(cx.name_str(f.name).to_string());
        if f.embedded {
            if let TyKind::Struct(id) = cx.ty_kind(f.ty) {
                out.extend(embed_alias_names(cx, *id));
            }
        }
    }
    out
}

/// `10` / `10.0` with no name or call. Other sides of `+` can lend these a type.
fn bare_num(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Int(_) | ExprKind::Float(_) => true,
        ExprKind::Unary { op, expr: inner } if *op == parse::UnaryOp::Neg => bare_num(inner),
        _ => false,
    }
}

fn ast_binop(op: parse::BinOp) -> BinOp {
    match op {
        parse::BinOp::Add => BinOp::Add,
        parse::BinOp::Sub => BinOp::Sub,
        parse::BinOp::Mul => BinOp::Mul,
        parse::BinOp::Div => BinOp::Div,
        parse::BinOp::Rem => BinOp::Rem,
        parse::BinOp::Eq => BinOp::Eq,
        parse::BinOp::Ne => BinOp::Ne,
        parse::BinOp::Lt => BinOp::Lt,
        parse::BinOp::Le => BinOp::Le,
        parse::BinOp::Gt => BinOp::Gt,
        parse::BinOp::Ge => BinOp::Ge,
        parse::BinOp::And => BinOp::And,
        parse::BinOp::Or => BinOp::Or,
    }
}
