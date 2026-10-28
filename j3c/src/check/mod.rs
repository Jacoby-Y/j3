//! Typecheck: untyped AST → typed IR.
//!
//! Desugaring happens here (`+=` / `operator+`, interpolate → `InterpFill`,
//! `&&`/`||` short-circuit, `for`, while-cond re-eval, auto-deref, auto-`&`,
//! `[N]T` → `[]T`, match, `?`), not as source text.

mod error;
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

mod consteval;
mod exports;
mod funcs;
mod infer;
mod resolve;
mod types;
mod uses;

use consteval::*;
use exports::*;
use funcs::*;
use infer::*;
use resolve::*;
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
    struct_temp_env: HashMap<String, HashMap<String, Ty>>,
    struct_insts: HashMap<(String, Vec<Ty>), StructId>,
    method_temps: HashMap<(u32, String), parse::FnItem>,
    method_insts: HashMap<(u32, String, Vec<Ty>), FuncId>,
    fn_substs: HashMap<FuncId, HashMap<String, Ty>>,
    pending_mono: Vec<(FuncId, parse::FnItem, HashMap<String, Ty>)>,
    globals: HashMap<String, GlobalId>,
    exports: HashSet<String>,
}

pub(crate) struct GenEnv<'a> {
    pub types: &'a HashMap<String, Ty>,
    pub subst: &'a HashMap<String, Ty>,
    pub enum_temps: &'a HashMap<String, parse::EnumItem>,
    pub enum_insts: &'a mut HashMap<(String, Vec<Ty>), EnumId>,
    pub struct_temps: &'a HashMap<String, parse::StructItem>,
    pub struct_temp_env: &'a HashMap<String, HashMap<String, Ty>>,
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
    // An error carries its module's path; for a span in a part file it names that file instead.
    let mut seen = HashSet::new();
    let part_paths: Vec<(FileId, String)> = files
        .iter()
        .filter(|f| !f.ast.is_script && !seen.insert(f.ast.module_key()))
        .map(|f| (f.file_id, f.path.clone()))
        .collect();
    check_files_inner(files, require_main).map_err(|mut e| {
        if let Some((_, path)) = part_paths.iter().find(|(id, _)| *id == e.span.file) {
            e.path = path.clone();
        }
        e
    })
}

fn check_files_inner(mut files: Vec<ParsedFile>, require_main: bool) -> Result<Program, CheckError> {
    let t_all = std::time::Instant::now();
    script::prepare(&mut files)?;
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
            globals: HashMap::new(),
            exports: HashSet::new(),
        });
    }

    crate::cli::timing("  check: setup", t_all);
    crate::phase::detail("collecting names, types, functions");
    let t = std::time::Instant::now();
    for i in 0..mods.len() {
        collect_exports(&mut mods, i);
    }
    for i in 0..mods.len() {
        collect_struct_names(&mut cx, &mut mods, i)?;
    }
    script::resolve_aliases(&mut mods)?;
    for i in 0..mods.len() {
        collect_enums(&mut cx, &mut mods, i)?;
    }
    resolve_all_uses(&mut mods, UsePhase::Types)?;
    let mut def_env: HashMap<String, HashMap<String, Ty>> = HashMap::new();
    for m in &mods {
        for it in &m.ast.items {
            if let Item::Struct(s) = it {
                if !s.type_params.is_empty() {
                    def_env.insert(s.name.name.clone(), m.types.clone());
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
    let shared = lower::shared(&mods);
    for i in 0..mods.len() {
        crate::phase::detail(&format!("lower {}/{} {}", i + 1, mods.len(), mods[i].key));
        lower::lower_module(&mut cx, &mut mods, i, main_id, &shared)?;
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
    })
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
mod tests {
    use super::*;

    #[test]
    fn generic_struct_instances_are_shared_across_modules() {
        let sources = [
            ("container", r#"
module container;
pub struct Box<T> {
    value: T;
    fn get(this: &Box<T>): T { return this.value; }
}
pub struct Item { value: int; }
"#),
            ("factory", r#"
module factory;
use container.{ Box, Item };
pub fn make(): Box<Item> { return Box<Item> { value: Item { value: 42 } }; }
"#),
            ("consumer", r#"
module consumer;
use container.{ Box, Item };
use factory.make;
struct World { item: Box<Item>; }
fn main() {
    let world = World { item: make() };
    let item = world.item.get();
}
"#),
        ];
        let files = sources.iter().enumerate().map(|(i, (name, src))| {
            let file_id = FileId(i as u32 + 1);
            ParsedFile {
                path: format!("{name}.j3"),
                src: src.to_string(),
                ast: parse::parse(src, file_id).expect("parse cross-module generic"),
                file_id,
            }
        }).collect();
        let prog = check_files(files).expect("share generic instances across modules");
        assert_eq!(prog.cx.structs().iter().filter(|s| !s.ty_args.is_empty()).count(), 1);
    }

    #[test]
    fn omitted_returns_use_instantiated_parameter_types() {
        let src = r#"
module test;

struct Box<T> { value: T; }

fn clear(box: &Box<int>) { box.value = 0; }
fn value(box: &Box<int>) { return box.value; }
fn identity(box: &Box<int>) { return box; }

struct Reader {
    fn read(this: &Reader, box: &Box<int>) { return box.value; }
}

fn main() {
    let box = Box<int> { value: 42 };
    let reader = Reader {};
    let n = value(&box);
    let m = reader.read(&box);
    let alias = identity(&box);
    clear(alias);
}
"#;
        let file_id = FileId(1);
        let ast = parse::parse(src, file_id).expect("parse generic parameters");
        let prog = check_files(vec![ParsedFile {
            path: "test.j3".to_string(),
            src: src.to_string(),
            ast,
            file_id,
        }])
        .expect("infer returns with generic parameters");
        for id in &prog.modules[0].funcs {
            let f = prog.cx.func(*id);
            let name = prog.cx.name_str(f.name);
            let (params, ret) = prog.cx.fn_sig(f.ty).expect("function signature");
            match name {
                "clear" => assert_eq!(ret, prog.cx.common.void),
                "value" | "read" => assert_eq!(ret, prog.cx.common.int),
                "identity" => assert_eq!(ret, params[0]),
                _ => {}
            }
        }
    }
}
