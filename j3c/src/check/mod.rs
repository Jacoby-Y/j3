//! Typecheck: untyped AST → typed IR.
//!
//! Desugaring happens here (`+=` / `operator+`, interpolate → `InterpFill`,
//! `&&`/`||` short-circuit, `for`, while-cond re-eval, auto-deref, auto-`&`,
//! `[N]T` → `[]T`, match, `?`), not as source text.

mod error;
pub(crate) mod gpu;
mod lower;
pub(crate) mod script;

pub use error::CheckError;

use std::collections::{HashMap, HashSet};

use crate::intern::Name;
use crate::ir::cx::IrCx;
use crate::ir::entity::{
    ConstInit, EnumDef, FieldDef, Func, FuncKind, GlobalDef, ScriptMeta, StructDef, TryCatch,
    VariantDef, Vis,
};
use crate::ir::ids::{EnumId, FuncId, GlobalId, StructId, VariantId};
use crate::ir::module::Module;
use crate::ir::operand::Operand;
use crate::ir::ty::{Ty, TyKind};
use crate::ir::validate::validate_module;
use crate::parse::{
    self, ConstItem, EnumItem, Expr, ExprKind, File, FnItem, Item, ModLetItem, StructItem, Type,
    TypeKind, UseItem,
};
use crate::span::{FileId, Span};

mod closure_ty;
mod consteval;
mod exports;
mod extend;
pub mod lint;
mod funcs;
mod generic_fn;
mod infer;
mod resolve;
mod traits;
mod types;
mod uses;

use closure_ty::*;
use consteval::*;
use exports::*;
use extend::merge_extends;
use funcs::*;
use generic_fn::*;
use infer::*;
use resolve::*;
pub(crate) use traits::{BoundReq, BoundSite, TraitEnv};
use traits::*;
use types::*;
use uses::*;

pub struct ParsedFile {
    pub path: String,
    pub src: String,
    pub ast: File,
    pub file_id: FileId,
}

pub struct Program {
    pub cx: IrCx,
    pub modules: Vec<Module>,
    /// `pub` items, for LSP import completion. Not part of the backend contract.
    pub exports: Vec<Export>,
    /// Every `const`, including private ones, so hover can show `///`.
    pub const_docs: Vec<Export>,
    pub script_aliases: Vec<ScriptAlias>,
    /// Shader programs emitted by `@shader(vs, fs)` (`j3c dump-glsl`).
    pub shaders: Vec<gpu::ShaderProgram>,
    /// Code warnings on the user's modules (`lint.rs`): unused locals and imports, unreachable code.
    pub warnings: Vec<crate::diag::Diagnostic>,
    /// Edits that remove what `warnings` names, where that is safe (unused imports): `check --autofix`.
    pub fixes: Vec<lint::Fix>,
}

impl Program {
    /// Each warning rendered against its source, one block per warning.
    pub fn rendered_warnings(&self) -> Vec<String> {
        self.warnings
            .iter()
            .map(|w| {
                let src = self.cx.file(w.span.file).map(|f| f.src.clone()).unwrap_or_default();
                w.render(&[(w.path.clone(), src)])
            })
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct ScriptAlias {
    pub file: FileId,
    pub name: String,
    pub struct_id: StructId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportKind {
    Fn,
    Struct,
    Enum,
    Const,
}

#[derive(Clone, Debug)]
pub struct Export {
    pub module: String,
    pub name: String,
    pub kind: ExportKind,
    pub doc: String,
}

#[derive(Clone, Debug)]
pub(crate) struct ConstVal {
    pub ty: Ty,
    pub kind: ConstPayload,
}

#[derive(Clone, Debug)]
pub(crate) enum ConstPayload {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(Vec<u8>),
}

pub(crate) struct ModData {
    key: String,
    path: String,
    ast: File,
    names: Vec<Name>,
    ir: Module,
    funcs: HashMap<String, FuncId>,
    consts: HashMap<String, ConstVal>,
    /// imported name → func
    imports: HashMap<String, FuncId>,
    structs: HashMap<String, StructId>,
    types: HashMap<String, Ty>,
    methods: HashMap<(u32, String), FuncId>,
    lib_ns: HashMap<String, HashMap<String, FuncId>>,
    /// `use j3.game` → prefix `game.poll()`, `game.Key`
    mod_ns: HashMap<String, HashMap<String, FuncId>>,
    mod_types: HashMap<String, HashMap<String, Ty>>,
    enum_temps: HashMap<String, parse::EnumItem>,
    insts: HashMap<(String, Vec<Ty>), crate::ir::ids::EnumId>,
    struct_temps: HashMap<String, parse::StructItem>,
    struct_temp_env: HashMap<String, DeclEnv>,
    struct_insts: HashMap<(String, Vec<Ty>), StructId>,
    method_temps: HashMap<(u32, String), parse::FnItem>,
    method_insts: HashMap<(u32, String, Vec<Ty>), FuncId>,
    fn_substs: HashMap<FuncId, HashMap<String, Ty>>,
    pending_mono: Vec<(FuncId, parse::FnItem, HashMap<String, Ty>)>,
    /// `fn name<T>(..)` declared here (see `generic_fn.rs`), by name.
    gen_fns: HashMap<String, parse::FnItem>,
    /// Generic fns callable by bare name (own and imported) → defining module key.
    gen_names: HashMap<String, String>,
    /// `use a.b` alias → module key, for `b.name(..)` calls of its generic fns.
    gen_mods: HashMap<String, String>,
    /// Instances of generic free fns by (`module::name`, type args), shared by every module.
    fn_insts: HashMap<(String, Vec<Ty>), FuncId>,
    globals: HashMap<String, GlobalId>,
    exports: HashSet<String>,
    /// Traits callable by bare name (own and imported) → `module::Name` (`traits.rs`).
    traits: HashMap<String, String>,
    /// `#[impl]` fns by their unique symbol name → the method name they implement.
    impl_names: HashMap<String, String>,
}

/// What a generic struct's declaring module can see: its types and generic templates.
/// Instance signatures resolve here, not in the module that names the instance.
#[derive(Clone)]
pub(crate) struct DeclEnv {
    pub types: HashMap<String, Ty>,
    pub enum_temps: HashMap<String, parse::EnumItem>,
    pub struct_temps: HashMap<String, parse::StructItem>,
}

impl DeclEnv {
    pub(crate) fn of(m: &ModData) -> DeclEnv {
        DeclEnv { types: m.types.clone(), enum_temps: m.enum_temps.clone(), struct_temps: m.struct_temps.clone() }
    }
}

pub(crate) struct GenEnv<'a> {
    pub types: &'a HashMap<String, Ty>,
    pub subst: &'a HashMap<String, Ty>,
    pub enum_temps: &'a HashMap<String, parse::EnumItem>,
    pub enum_insts: &'a mut HashMap<(String, Vec<Ty>), EnumId>,
    pub struct_temps: &'a HashMap<String, parse::StructItem>,
    pub struct_temp_env: &'a HashMap<String, DeclEnv>,
    pub struct_insts: &'a mut HashMap<(String, Vec<Ty>), StructId>,
    pub methods: &'a mut HashMap<(u32, String), FuncId>,
    pub method_temps: &'a mut HashMap<(u32, String), parse::FnItem>,
    pub method_insts: &'a mut HashMap<(u32, String, Vec<Ty>), FuncId>,
    pub fn_substs: &'a mut HashMap<FuncId, HashMap<String, Ty>>,
    pub pending_mono: &'a mut Vec<(FuncId, parse::FnItem, HashMap<String, Ty>)>,
    pub module: &'a [Name],
    pub ir_enums: &'a mut Vec<EnumId>,
    pub ir_structs: &'a mut Vec<StructId>,
    pub ir_funcs: &'a mut Vec<FuncId>,
}

pub fn check_files(files: Vec<ParsedFile>) -> Result<Program, CheckError> {
    check_files_cfg(files, true)
}

pub fn check_files_lib(files: Vec<ParsedFile>) -> Result<Program, CheckError> {
    check_files_cfg(files, false)
}

fn check_files_cfg(files: Vec<ParsedFile>, require_main: bool) -> Result<Program, CheckError> {
    // An error carries the path of the module being checked; its span may sit in another file
    // (a part file, or a generic method instantiated from a caller), so name the span's file.
    let file_paths: Vec<(FileId, String)> = files
        .iter()
        .filter(|f| !f.ast.is_script)
        .map(|f| (f.file_id, f.path.clone()))
        .collect();
    check_files_inner(files, require_main).map_err(|e| {
        e.map_all(&mut |mut e| {
            gpu::explain_gone(&mut e);
            if e.span.is_dummy() {
                return e;
            }
            if let Some((_, path)) = file_paths.iter().find(|(id, _)| *id == e.span.file) {
                e.path = path.clone();
            }
            e
        })
    })
}

fn check_files_inner(mut files: Vec<ParsedFile>, require_main: bool) -> Result<Program, CheckError> {
    let t_all = std::time::Instant::now();
    let (warnings, fixes) = lint::lint(&files);
    script::prepare(&mut files)?;
    let shaders = gpu::prepare(&mut files)?;
    let mut cx = IrCx::new();
    let _ = cx.ty_ref(cx.common.u8);

    let mut mods: Vec<ModData> = Vec::new();
    for file in files {
        cx.add_file(file.file_id, file.path.clone(), file.src.clone());
        // A part file (`ui/dom/*.j3` saying `module j3.ui.dom;`) adds its items to the module
        // loaded just before it; its spans keep their own file id.
        let key = file.ast.module_key();
        if !file.ast.is_script {
            if let Some(m) = mods.iter_mut().find(|m| m.key == key) {
                m.ast.items.extend(file.ast.items);
                continue;
            }
        }
        let mut names = Vec::new();
        for part in &file.ast.module {
            names.push(cx.intern_name(&part.name));
        }
        let ir = Module::new(names.clone());
        mods.push(ModData {
            key: file.ast.module_key(),
            path: file.path,
            ast: file.ast,
            names,
            ir,
            funcs: HashMap::new(),
            consts: HashMap::new(),
            imports: HashMap::new(),
            structs: HashMap::new(),
            types: HashMap::new(),
            methods: HashMap::new(),
            lib_ns: HashMap::new(),
            mod_ns: HashMap::new(),
            mod_types: HashMap::new(),
            enum_temps: HashMap::new(),
            insts: HashMap::new(),
            struct_temps: HashMap::new(),
            struct_temp_env: HashMap::new(),
            struct_insts: HashMap::new(),
            method_temps: HashMap::new(),
            method_insts: HashMap::new(),
            fn_substs: HashMap::new(),
            pending_mono: Vec::new(),
            gen_fns: HashMap::new(),
            gen_names: HashMap::new(),
            gen_mods: HashMap::new(),
            fn_insts: HashMap::new(),
            globals: HashMap::new(),
            exports: HashSet::new(),
            traits: HashMap::new(),
            impl_names: HashMap::new(),
        });
    }

    merge_extends(&mut mods)?;
    crate::cli::timing("  check: setup", t_all);
    crate::phase::detail("collecting names, types, functions");
    let t = std::time::Instant::now();
    for i in 0..mods.len() {
        collect_traits(&mut mods, i)?;
    }
    for i in 0..mods.len() {
        collect_exports(&mut mods, i);
    }
    bind_traits(&mut mods)?;
    for i in 0..mods.len() {
        collect_struct_names(&mut cx, &mut mods, i)?;
    }
    script::resolve_aliases(&mut mods)?;
    for i in 0..mods.len() {
        collect_enums(&mut cx, &mut mods, i)?;
    }
    resolve_all_uses(&mut mods, UsePhase::Types)?;
    let mut def_env: HashMap<String, DeclEnv> = HashMap::new();
    for m in &mods {
        for it in &m.ast.items {
            if let Item::Struct(s) = it {
                if !s.type_params.is_empty() {
                    def_env.insert(s.name.name.clone(), DeclEnv::of(m));
                }
            }
        }
    }
    for m in &mut mods {
        let keys: Vec<String> = m.struct_temps.keys().cloned().collect();
        for name in keys {
            if let Some(types) = def_env.get(&name) {
                m.struct_temp_env.insert(name, types.clone());
            }
        }
    }
    for i in 0..mods.len() {
        fill_struct_fields(&mut cx, &mut mods, i)?;
    }
    check_all_occurs(&cx, &mods)?;
    for i in 0..mods.len() {
        check_embed_clashes(&cx, &mods, i)?;
    }
    for i in 0..mods.len() {
        collect_consts(&mut cx, &mut mods, i)?;
    }
    for i in 0..mods.len() {
        collect_funcs(&mut cx, &mut mods, i)?;
    }

    crate::cli::timing("  check: collect", t);
    let traits = build_env(&mut cx, &mods)?;
    let main_id = match find_main(&mods) {
        Ok(id) => id,
        Err(_) if !require_main => dummy_main(&mut cx),
        Err(e) => return Err(e),
    };
    resolve_all_uses(&mut mods, UsePhase::Vals)?;
    share_methods(&mut mods);
    for i in 0..mods.len() {
        collect_globals(&mut cx, &mut mods, i)?;
    }
    let t = std::time::Instant::now();
    for i in 0..mods.len() {
        crate::phase::detail(&format!("infer returns {}/{} {}", i + 1, mods.len(), mods[i].key));
        infer_omitted_rets(&mut cx, &mut mods, i)?;
    }
    crate::cli::timing("  check: infer rets", t);

    let t = std::time::Instant::now();
    {
        let (mut all, mut temps) = (HashMap::new(), HashMap::new());
        for m in &mods {
            all.extend(m.methods.iter().map(|(k, v)| (k.clone(), *v)));
            temps.extend(m.method_temps.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        // Instances made while collecting signatures (`fn f(m: Map<K, V>)`) could not be checked before.
        lower::drain_bounds(&mut cx, &traits, &all, &temps, None)?;
    }
    let shared = lower::shared(&mods, traits);
    // Function bodies are independent: an error in one is kept and the rest are still checked.
    let mut errs: Vec<CheckError> = Vec::new();
    for i in 0..mods.len() {
        crate::phase::detail(&format!("lower {}/{} {}", i + 1, mods.len(), mods[i].key));
        lower::lower_module(&mut cx, &mut mods, i, main_id, &shared, &mut errs)?;
        if errs.len() >= MAX_ERRORS {
            break;
        }
    }
    if let Some(e) = gather_errors(errs) {
        return Err(e);
    }
    crate::cli::timing("  check: lower", t);

    crate::phase::detail("exports, docs, script aliases");
    let t = std::time::Instant::now();
    let exports = program_exports(&cx, &mods);
    let const_docs = const_docs(&cx, &mods);
    let script_aliases = mods
        .iter()
        .flat_map(|m| {
            m.ast.items.iter().filter_map(|item| {
                let Item::Const(c) = item else { return None };
                if !matches!(c.value.kind, ExprKind::ScriptType(_)) {
                    return None;
                }
                let ty = m.types.get(&c.name.name)?;
                let TyKind::Struct(sid) = cx.ty_kind(*ty) else {
                    return None;
                };
                Some(ScriptAlias {
                    file: m.ast.span.file,
                    name: c.name.name.clone(),
                    struct_id: *sid,
                })
            })
        })
        .collect();
    crate::cli::timing("  check: exports", t);

    let t = std::time::Instant::now();
    let mut modules = Vec::new();
    let total = mods.len();
    for (n, m) in mods.into_iter().enumerate() {
        crate::phase::detail(&format!("validate {}/{total} {}", n + 1, m.key));
        match validate_module(&cx, &m.ir) {
            Ok(()) => {}
            Err(e) => {
                return Err(CheckError::new(m.path, Span::dummy(), e.message));
            }
        }
        modules.push(m.ir);
    }
    crate::cli::timing("  check: validate", t);

    Ok(Program {
        cx,
        modules,
        exports,
        const_docs,
        script_aliases,
        shaders,
        warnings,
        fixes,
    })
}

/// Errors reported in one run; past this the rest are usually follow-ons.
const MAX_ERRORS: usize = 20;

/// The first error, with the others (deduplicated: generic instances repeat one) as follow-ups.
fn gather_errors(errs: Vec<CheckError>) -> Option<CheckError> {
    let mut seen = HashSet::new();
    let mut uniq: Vec<CheckError> = errs
        .into_iter()
        .flat_map(|mut e| {
            let more = std::mem::take(&mut e.more);
            std::iter::once(e).chain(more)
        })
        .filter(|e| seen.insert((e.path.clone(), e.span.file, e.span.start, e.span.end, e.message.clone())))
        .collect();
    let omitted = uniq.len().saturating_sub(MAX_ERRORS);
    uniq.truncate(MAX_ERRORS);
    let mut it = uniq.into_iter();
    let mut first = it.next()?;
    first.more = it.collect();
    first.omitted = omitted;
    Some(first)
}

/// Key for const `name` of the module `use`d as `alias`. Idents never hold `.`, so it can't collide.
pub(crate) fn qual_key(alias: &str, name: &str) -> String {
    format!("{alias}.{name}")
}

pub(crate) fn field_path(
    cx: &IrCx,
    sid: StructId,
    name: &str,
) -> Option<Vec<crate::ir::ids::FieldId>> {
    let def = cx.struct_def(sid);
    for (i, f) in def.fields.iter().enumerate() {
        if cx.name_str(f.name) == name {
            return Some(vec![crate::ir::ids::FieldId {
                struct_id: sid,
                index: i as u32,
            }]);
        }
    }
    for (i, f) in def.fields.iter().enumerate() {
        if !f.embedded {
            continue;
        }
        if let TyKind::Struct(inner) = cx.ty_kind(f.ty) {
            if let Some(mut rest) = field_path(cx, *inner, name) {
                rest.insert(
                    0,
                    crate::ir::ids::FieldId {
                        struct_id: sid,
                        index: i as u32,
                    },
                );
                return Some(rest);
            }
        }
    }
    None
}

/// An untyped number: a literal, `-` of one, or `+ - * / %` of two (`(2 + 1)`). It takes the
/// other operand's type, or the destination's when both sides are untyped.
pub(crate) fn bare_num(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Int(_) | ExprKind::Float(_) => true,
        ExprKind::Unary { op, expr: inner } if *op == parse::UnaryOp::Neg => bare_num(inner),
        ExprKind::Binary { op, lhs, rhs } => {
            matches!(
                op,
                parse::BinOp::Add | parse::BinOp::Sub | parse::BinOp::Mul | parse::BinOp::Div | parse::BinOp::Rem
            ) && bare_num(lhs)
                && bare_num(rhs)
        }
        _ => false,
    }
}

pub(crate) fn is_int(cx: &IrCx, ty: Ty) -> bool {
    matches!(cx.ty_kind(ty), TyKind::Int(_))
}

pub(crate) fn is_float(cx: &IrCx, ty: Ty) -> bool {
    matches!(cx.ty_kind(ty), TyKind::Float(_))
}

pub(crate) fn is_numeric(cx: &IrCx, ty: Ty) -> bool {
    is_int(cx, ty) || is_float(cx, ty)
}

#[cfg(test)]
mod tests;
