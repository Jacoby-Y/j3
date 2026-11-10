use std::collections::{HashMap, HashSet};

use super::error::CheckError;
use super::{
    alloc_fn, join_module, bare_num, field_path, find_operator, instantiate_struct, is_float, is_int, is_numeric,
    reject_closure, resolve_type_g, ConstPayload, ConstVal, DeclEnv, GenEnv, ModData, TraitEnv,
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
mod bounds;
mod call;
mod closure;
mod closure_call;
use closure::fn_label;
use generic_fn as generic;
mod enums;
mod expr;
mod interp;
mod generic_fn;
mod half;
mod lit;
mod place;
mod stmt;
mod streq;
mod setup;
mod simd;
mod ufcs;
mod unify;
pub(crate) use bounds::drain_bounds;
use bounds::eq_needs_operator;
use generic_fn::GenCtx;
use setup::{generic_origins, lower_fn, MergedEnv};
use ufcs::ThisAdjust;
use simd::is_vec;

struct OriginNs {
    path: String,
    names: Vec<Name>,
    enum_temps: HashMap<String, parse::EnumItem>,
    funcs: HashMap<String, FuncId>,
    types: HashMap<String, Ty>,
    consts: HashMap<String, ConstVal>,
    lib_ns: HashMap<String, HashMap<String, FuncId>>,
    mod_ns: HashMap<String, HashMap<String, FuncId>>,
    mod_types: HashMap<String, HashMap<String, Ty>>,
    globals: HashMap<String, GlobalId>,
    struct_temps: HashMap<String, parse::StructItem>,
    struct_temp_env: HashMap<String, DeclEnv>,
}

/// Lowering state read by every module and written by none, built once before the lower loop.
pub(crate) struct Shared {
    /// Generic free-fn templates by `module::name`.
    gen_fns: HashMap<String, parse::FnItem>,
    origins: HashMap<String, OriginNs>,
    struct_temps: HashMap<String, parse::StructItem>,
    struct_temp_env: HashMap<String, DeclEnv>,
    pub(crate) traits: TraitEnv,
}

pub(crate) fn shared(mods: &[ModData], traits: TraitEnv) -> Shared {
    let mut struct_temps = HashMap::new();
    let mut struct_temp_env = HashMap::new();
    for m in mods.iter() {
        for (k, v) in &m.struct_temps {
            struct_temps.entry(k.clone()).or_insert_with(|| v.clone());
        }
        for (k, v) in &m.struct_temp_env {
            struct_temp_env.entry(k.clone()).or_insert_with(|| v.clone());
        }
        // A plain struct's generic method is instantiated from the caller, so its signature
        // resolves in the defining module's types (`Arena.alloc::<T>` without `use j3.memory.Arena`).
        for it in &m.ast.items {
            if let Item::Struct(s) = it {
                if s.type_params.is_empty() && has_generic_method(s) {
                    struct_temp_env.entry(s.name.name.clone()).or_insert_with(|| DeclEnv::of(m));
                }
            }
        }
    }
    let mut gen_fns = HashMap::new();
    for m in mods.iter() {
        for (k, f) in &m.gen_fns {
            gen_fns.insert(format!("{}::{k}", m.key), f.clone());
        }
    }
    Shared {
        gen_fns,
        origins: generic_origins(mods),
        struct_temps,
        struct_temp_env,
        traits,
    }
}

fn has_generic_method(s: &parse::StructItem) -> bool {
    s.methods.iter().any(|m| !m.type_params.is_empty())
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
    errs: &mut Vec<CheckError>,
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
    let mut fn_insts = HashMap::new();
    for m in mods.iter() {
        for (k, v) in &m.fn_insts {
            fn_insts.entry(k.clone()).or_insert(*v);
        }
    }
    let gen_names = mods[i].gen_names.clone();
    let gen_mods = mods[i].gen_mods.clone();
    let mut method_temps = mods[i].method_temps.clone();
    let mut method_insts = mods[i].method_insts.clone();
    let mut fn_substs = mods[i].fn_substs.clone();
    let mut pending_mono = mods[i].pending_mono.clone();
    let module_names = mods[i].names.clone();

    let mut pending: Vec<(FuncId, FnItem)> = Vec::new();
    for it in &mods[i].ast.items {
        match it {
            Item::Fn(f) if !f.type_params.is_empty() => {}
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
            let body = match lower_fn(
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
                GenCtx::new(&shared.gen_fns, origins, &gen_names, &gen_mods, &mut fn_insts, &shared.traits),
                &mut mods[i].ir.enums,
                &mut mods[i].ir.structs,
                &mut mods[i].ir.funcs,
                subst,
            ) {
                Ok(body) => body,
                Err(e) => {
                    errs.push(e);
                    continue;
                }
            };
            if let Some(e) = missing_return(cx, &path, id, &f, mods[i].ast.is_script) {
                errs.push(e);
            }
            cx.func_mut(id).kind = FuncKind::J3 { body };
            // Generic struct/enum instances made by that body: check their bounds before their methods are lowered.
            drain_bounds(cx, &shared.traits, &methods, &method_temps, Some(&funcs))?;
        }
    }
    mods[i].insts = enum_insts;
    mods[i].fn_insts = fn_insts;
    mods[i].methods = methods;
    mods[i].struct_insts = struct_insts;
    mods[i].method_temps = method_temps;
    mods[i].method_insts = method_insts;
    mods[i].fn_substs = fn_substs;
    Ok(())
}

/// A non-void fn whose body can run off its end (`flow::diverges`).
fn missing_return(cx: &IrCx, path: &str, id: FuncId, f: &FnItem, is_script: bool) -> Option<CheckError> {
    if is_script || f.shader.is_some() || f.body.span.is_dummy() {
        return None;
    }
    let ret = cx.fn_sig(cx.func(id).ty)?.1;
    if ret == cx.common.void || crate::parse::flow::diverges(&f.body) {
        return None;
    }
    let end = f.body.span.end.max(f.body.span.start + 1);
    let at = Span { file: f.body.span.file, start: end - 1, end };
    Some(
        CheckError::new(
            path,
            at,
            format!(
                "function `{}` can reach its end without returning `{}`",
                f.name.name,
                crate::ir::dump::ty_string(cx, ret)
            ),
        )
        .help("end every path with `return ...;` (or `panic(..)` where it cannot happen)"),
    )
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
    /// Enclosing source loops, innermost last, with their `'label`.
    loops: Vec<Option<String>>,
    /// Set by `'label:` for the loop statement that follows it.
    pending_label: Option<String>,
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
    struct_temp_env: &'a HashMap<String, DeclEnv>,
    struct_insts: &'a mut HashMap<(String, Vec<Ty>), StructId>,
    method_temps: &'a mut HashMap<(u32, String), parse::FnItem>,
    method_insts: &'a mut HashMap<(u32, String, Vec<Ty>), FuncId>,
    fn_substs: &'a mut HashMap<FuncId, HashMap<String, Ty>>,
    pending_mono: &'a mut Vec<(FuncId, parse::FnItem, HashMap<String, Ty>)>,
    module_names: &'a [Name],
    gen: GenCtx<'a>,
    ir_enums: &'a mut Vec<EnumId>,
    ir_structs: &'a mut Vec<StructId>,
    ir_funcs: &'a mut Vec<FuncId>,
    type_subst: HashMap<String, Ty>,
    ret: Ty,
    tmp: u32,
    /// Name of the fn being lowered (`Box.push<int>`), the stem of its `$closureN` fns.
    fn_label: String,
    closure_n: u32,
    /// Lowering a closure body (its `break`/`continue` cannot reach the creator's loops).
    in_closure: bool,
    /// Names visible outside the closure being lowered; a new local may not reuse them.
    closure_outer: HashSet<String>,
    /// Errors of earlier statements in this fn (`stmt_recover`), in source order.
    errs: Vec<CheckError>,
    /// Names whose `let` failed or that were reported unknown: a later error naming one is a follow-on.
    poisoned: HashSet<String>,
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
        if let Some(e) = self.closure_mismatch(span, have, want) {
            return e;
        }
        let e = CheckError::new(
            self.path,
            span,
            format!(
                "{what}: expected `{}`, found `{}`",
                self.ty_str(want),
                self.ty_str(have)
            ),
        );
        if self.is_half(have) || self.is_half(want) {
            return e.help("`f16` is storage only: convert with `f32(x)` to compute, `f16(y)` to store");
        }
        if is_numeric(self.cx, have) && is_numeric(self.cx, want) {
            return e.help(format!("numbers never convert by themselves: write `{}(x)`", self.ty_str(want)));
        }
        e
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
        e.unknown = Some(name.to_string());
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

fn ast_binop(op: parse::BinOp) -> BinOp {
    match op {
        parse::BinOp::Add => BinOp::Add,
        parse::BinOp::Sub => BinOp::Sub,
        parse::BinOp::Mul => BinOp::Mul,
        parse::BinOp::Div => BinOp::Div,
        parse::BinOp::Rem => BinOp::Rem,
        parse::BinOp::Shl => BinOp::Shl,
        parse::BinOp::Shr => BinOp::Shr,
        parse::BinOp::BitAnd => BinOp::BitAnd,
        parse::BinOp::BitOr => BinOp::BitOr,
        parse::BinOp::BitXor => BinOp::BitXor,
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
