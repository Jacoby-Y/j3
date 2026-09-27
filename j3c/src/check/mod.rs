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

fn check_files_cfg(mut files: Vec<ParsedFile>, require_main: bool) -> Result<Program, CheckError> {
    script::prepare(&mut files)?;
    let mut cx = IrCx::new();
    let _ = cx.ty_ref(cx.common.u8);

    let mut mods: Vec<ModData> = Vec::new();
    for file in files {
        cx.add_file(file.file_id, file.path.clone(), file.src.clone());
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
    for i in 0..mods.len() {
        infer_omitted_rets(&mut cx, &mut mods, i)?;
    }

    for i in 0..mods.len() {
        lower::lower_module(&mut cx, &mut mods, i, main_id)?;
    }

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

    let mut modules = Vec::new();
    for m in mods {
        match validate_module(&cx, &m.ir) {
            Ok(()) => {}
            Err(e) => {
                return Err(CheckError::new(m.path, Span::dummy(), e.message));
            }
        }
        modules.push(m.ir);
    }

    Ok(Program {
        cx,
        modules,
        exports,
        const_docs,
        script_aliases,
    })
}

fn const_docs(cx: &IrCx, mods: &[ModData]) -> Vec<Export> {
    let mut out = Vec::new();
    for m in mods {
        let module = join_module(cx, &m.names);
        for it in &m.ast.items {
            let Item::Const(c) = it else {
                continue;
            };
            out.push(Export {
                module: module.clone(),
                name: c.name.name.clone(),
                kind: ExportKind::Const,
                doc: c.doc.clone(),
            });
        }
    }
    out
}

fn program_exports(cx: &IrCx, mods: &[ModData]) -> Vec<Export> {
    let mut out = Vec::new();
    for m in mods {
        let module = join_module(cx, &m.names);
        let mut names: Vec<&String> = m.exports.iter().collect();
        names.sort();
        for name in names {
            let Some(kind) = export_kind(cx, m, name) else {
                continue;
            };
            out.push(Export {
                module: module.clone(),
                name: name.clone(),
                kind,
                doc: item_doc(m, name),
            });
        }
    }
    out
}

fn item_doc(m: &ModData, name: &str) -> String {
    for it in &m.ast.items {
        match it {
            Item::Fn(f) if f.name.name == name => return f.doc.clone(),
            Item::Struct(s) if s.name.name == name => return s.doc.clone(),
            Item::Enum(e) if e.name.name == name => return e.doc.clone(),
            Item::Const(c) if c.name.name == name => return c.doc.clone(),
            _ => {}
        }
    }
    String::new()
}

fn export_kind(cx: &IrCx, m: &ModData, name: &str) -> Option<ExportKind> {
    if m.funcs.contains_key(name) {
        return Some(ExportKind::Fn);
    }
    if let Some(ty) = m.types.get(name) {
        match cx.ty_kind(*ty) {
            TyKind::Struct(_) => return Some(ExportKind::Struct),
            TyKind::Enum(_) => return Some(ExportKind::Enum),
            _ => {}
        }
    }
    if m.struct_temps.contains_key(name) {
        return Some(ExportKind::Struct);
    }
    if m.enum_temps.contains_key(name) {
        return Some(ExportKind::Enum);
    }
    if m.consts.contains_key(name) {
        return Some(ExportKind::Const);
    }
    None
}

fn join_module(cx: &IrCx, names: &[Name]) -> String {
    let mut s = String::new();
    for (i, n) in names.iter().enumerate() {
        if i > 0 {
            s.push('.');
        }
        s.push_str(cx.name_str(*n));
    }
    s
}

fn share_methods(mods: &mut [ModData]) {
    let mut all = HashMap::new();
    let mut temps = HashMap::new();
    for m in mods.iter() {
        for (k, v) in &m.methods {
            all.insert(k.clone(), *v);
        }
        for (k, v) in &m.method_temps {
            temps.insert(k.clone(), v.clone());
        }
    }
    for m in mods.iter_mut() {
        for (k, v) in &all {
            m.methods.insert(k.clone(), *v);
        }
        for (k, v) in &temps {
            m.method_temps.entry(k.clone()).or_insert_with(|| v.clone());
        }
    }
}

fn collect_exports(mods: &mut [ModData], i: usize) {
    for it in &mods[i].ast.items {
        let (name, vis_pub) = match it {
            Item::Fn(f) => (&f.name.name, f.vis_pub),
            Item::Struct(s) => (&s.name.name, s.vis_pub),
            Item::Enum(e) => (&e.name.name, e.vis_pub),
            Item::Const(c) => (&c.name.name, c.vis_pub),
            _ => continue,
        };
        if vis_pub {
            mods[i].exports.insert(name.clone());
        }
    }
}

fn collect_consts(cx: &mut IrCx, mods: &mut [ModData], i: usize) -> Result<(), CheckError> {
    let path = mods[i].path.clone();
    let items: Vec<ConstItem> = mods[i]
        .ast
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Const(c) => Some(c.clone()),
            _ => None,
        })
        .collect();

    for c in items {
        if matches!(c.value.kind, ExprKind::ScriptType(_)) {
            continue;
        }
        if mods[i].consts.contains_key(&c.name.name) {
            return Err(CheckError::new(
                path.clone(),
                c.name.span,
                format!("duplicate const `{}`", c.name.name),
            ));
        }
        let mut val = eval_const(&c.value, &mods[i].consts, cx, &path)?;
        if let Some(t) = &c.ty {
            let types = mods[i].types.clone();
            let ty = resolve_type(cx, t, &path, &types)?;
            if ty != val.ty {
                if is_int(cx, ty) && matches!(val.kind, ConstPayload::Int(_)) {
                    val.ty = ty;
                } else if is_float(cx, ty)
                    && matches!(val.kind, ConstPayload::Float(_) | ConstPayload::Int(_))
                {
                    if let ConstPayload::Int(v) = val.kind {
                        val.kind = ConstPayload::Float(v as f64);
                    }
                    val.ty = ty;
                } else {
                    return Err(CheckError::new(
                        path.clone(),
                        c.span,
                        "const type does not match initializer",
                    ));
                }
            }
        }
        mods[i].consts.insert(c.name.name.clone(), val);
    }
    Ok(())
}

fn collect_struct_names(cx: &mut IrCx, mods: &mut [ModData], i: usize) -> Result<(), CheckError> {
    let path = mods[i].path.clone();
    let names = mods[i].names.clone();
    let items: Vec<StructItem> = mods[i]
        .ast
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Struct(s) => Some(s.clone()),
            _ => None,
        })
        .collect();
    for s in items {
        if mods[i].structs.contains_key(&s.name.name)
            || mods[i].types.contains_key(&s.name.name)
            || mods[i].struct_temps.contains_key(&s.name.name)
            || mods[i].enum_temps.contains_key(&s.name.name)
        {
            return Err(CheckError::new(
                path.clone(),
                s.name.span,
                format!("duplicate type `{}`", s.name.name),
            ));
        }
        if !s.type_params.is_empty() {
            mods[i].struct_temps.insert(s.name.name.clone(), s);
            continue;
        }
        let sname = cx.intern_name(&s.name.name);
        let id = cx.alloc_struct(StructDef {
            module: names.clone(),
            name: sname,
            doc: s.doc.clone(),
            fields: Vec::new(),
            packed: s.packed,
            debug: s.debug,
            span: s.span,
            ty_args: Vec::new(),
            // Only the synthesized script struct is a script value. Other structs in the
            // file are ordinary types the script uses.
            script: if mods[i].ast.is_script && s.name.name == script::script_name(&path) {
                Some(ScriptMeta {
                    path: path.clone(),
                    prop_spans: mods[i]
                        .ast
                        .items
                        .iter()
                        .filter_map(|it| match it {
                            Item::Props(b) => {
                                Some(b.fields.iter().map(|f| f.span).collect::<Vec<_>>())
                            }
                            _ => None,
                        })
                        .flatten()
                        .collect(),
                })
            } else {
                None
            },
        });
        let ty = cx.ty_struct(id);
        mods[i].structs.insert(s.name.name.clone(), id);
        mods[i].types.insert(s.name.name.clone(), ty);
        mods[i].ir.structs.push(id);
    }
    Ok(())
}

fn fill_struct_fields(cx: &mut IrCx, mods: &mut [ModData], i: usize) -> Result<(), CheckError> {
    let path = mods[i].path.clone();
    let names = mods[i].names.clone();
    let items: Vec<StructItem> = mods[i]
        .ast
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Struct(s) => Some(s.clone()),
            _ => None,
        })
        .collect();
    for s in items {
        if !s.type_params.is_empty() {
            for f in &s.fields {
                if let Some(expr) = &f.default {
                    if mentions_type_param(&f.ty, &s.type_params) {
                        return Err(CheckError::new(
                            path.clone(),
                            expr.span,
                            "default on a generic field",
                        ));
                    }
                    if !matches!(
                        expr.kind,
                        ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_)
                    ) {
                        return Err(CheckError::new(
                            path.clone(),
                            expr.span,
                            "field default must be a literal",
                        ));
                    }
                }
            }
            continue;
        }
        let id = mods[i].structs[&s.name.name];
        let mut fields = Vec::new();
        let mut seen = HashMap::new();
        for f in &s.fields {
            if seen.contains_key(&f.name.name) {
                return Err(CheckError::new(
                    path.clone(),
                    f.name.span,
                    format!("duplicate field `{}`", f.name.name),
                ));
            }
            seen.insert(f.name.name.clone(), ());
            let ty = resolve_field_ty(cx, &f.ty, &path, mods, i, &names)?;
            reject_arena_field(cx, &path, f.name.span, ty)?;
            let fname = cx.intern_name(&f.name.name);
            let default = field_default(cx, &path, ty, &f.ty, &s.type_params, &f.default)?;
            fields.push(FieldDef {
                name: fname,
                doc: f.doc.clone(),
                ty,
                embedded: f.embedded,
                default,
            });
        }
        cx.struct_def_mut(id).fields = fields;
    }
    Ok(())
}

fn is_arena_header(cx: &IrCx, ty: Ty) -> bool {
    let id = match cx.ty_kind(ty) {
        TyKind::Struct(id) => *id,
        _ => return false,
    };
    let def = cx.struct_def(id);
    if cx.name_str(def.name) != "Arena" {
        return false;
    }
    def.module.len() == 2
        && cx.name_str(def.module[0]) == "j3"
        && cx.name_str(def.module[1]) == "memory"
}

fn reject_arena_field(cx: &IrCx, path: &str, span: Span, ty: Ty) -> Result<(), CheckError> {
    if !is_arena_header(cx, ty) {
        return Ok(());
    }
    Err(CheckError::new(
        path,
        span,
        "do not store Arena by value; keep one `let` header and pass `&Arena`",
    )
    .help("after Arena.create, only `&Arena`"))
}

fn fold_const_bin(
    op: parse::BinOp,
    a: i64,
    b: i64,
    path: &str,
    span: Span,
) -> Result<Option<i64>, CheckError> {
    let ir_op = match op {
        parse::BinOp::Add => crate::ir::op::BinOp::Add,
        parse::BinOp::Sub => crate::ir::op::BinOp::Sub,
        parse::BinOp::Mul => crate::ir::op::BinOp::Mul,
        parse::BinOp::Div => crate::ir::op::BinOp::Div,
        parse::BinOp::Rem => crate::ir::op::BinOp::Rem,
        _ => return Ok(None),
    };
    match crate::ir::wrap::fold_bin(ir_op, a, b) {
        Some(v) => Ok(Some(v)),
        None if matches!(op, parse::BinOp::Div | parse::BinOp::Rem) && b == 0 => Err(CheckError::new(
            path,
            span,
            "division by zero in const initializer",
        )),
        None => Ok(None),
    }
}

fn resolve_field_ty(
    cx: &mut IrCx,
    ty: &Type,
    path: &str,
    mods: &mut [ModData],
    i: usize,
    names: &[Name],
) -> Result<Ty, CheckError> {
    let empty = HashMap::new();
    let m = &mut mods[i];
    let mut env = GenEnv {
        types: &m.types,
        subst: &empty,
        enum_temps: &m.enum_temps,
        enum_insts: &mut m.insts,
        struct_temps: &m.struct_temps,
        struct_temp_env: &m.struct_temp_env,
        struct_insts: &mut m.struct_insts,
        methods: &mut m.methods,
        method_temps: &mut m.method_temps,
        method_insts: &mut m.method_insts,
        fn_substs: &mut m.fn_substs,
        pending_mono: &mut m.pending_mono,
        module: names,
        ir_enums: &mut m.ir.enums,
        ir_structs: &mut m.ir.structs,
        ir_funcs: &mut m.ir.funcs,
    };
    resolve_type_g(cx, ty, path, &mut env)
}

fn check_embed_clashes(cx: &IrCx, mods: &[ModData], i: usize) -> Result<(), CheckError> {
    let path = mods[i].path.clone();
    for (name, sid) in &mods[i].structs {
        let _ = name;
        check_one_embed(cx, *sid, &path)?;
    }
    Ok(())
}

fn check_one_embed(cx: &IrCx, sid: StructId, path: &str) -> Result<(), CheckError> {
    let def = cx.struct_def(sid);
    let span = def.span;
    let mut seen: HashMap<String, String> = HashMap::new();
    for f in &def.fields {
        seen.insert(
            cx.name_str(f.name).to_string(),
            cx.name_str(f.name).to_string(),
        );
    }
    for f in &def.fields {
        if !f.embedded {
            continue;
        }
        let inner = match cx.ty_kind(f.ty) {
            TyKind::Struct(id) => *id,
            _ => {
                return Err(CheckError::new(
                    path,
                    span,
                    format!("`use {}` must be a struct", cx.name_str(f.name)),
                ));
            }
        };
        for promoted in embed_names(cx, inner) {
            if let Some(prev) = seen.get(&promoted) {
                return Err(CheckError::new(
                    path,
                    span,
                    format!("promoted field `{promoted}` clashes with `{prev}`"),
                ));
            }
            seen.insert(promoted, cx.name_str(f.name).to_string());
        }
    }
    Ok(())
}

fn embed_names(cx: &IrCx, sid: StructId) -> Vec<String> {
    let mut out = Vec::new();
    for f in &cx.struct_def(sid).fields {
        out.push(cx.name_str(f.name).to_string());
        if f.embedded {
            if let TyKind::Struct(id) = cx.ty_kind(f.ty) {
                out.extend(embed_names(cx, *id));
            }
        }
    }
    out
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

fn check_all_occurs(cx: &IrCx, mods: &[ModData]) -> Result<(), CheckError> {
    for m in mods {
        for (name, sid) in &m.structs {
            let _ = name;
            let def = cx.struct_def(*sid);
            let span = def.span;
            for f in &def.fields {
                if occurs_value(cx, f.ty, OccurRoot::Struct(*sid), &mut HashSet::new()) {
                    return Err(CheckError::new(
                        m.path.clone(),
                        span,
                        format!(
                            "recursive field `{}` must go through `&` or `ptr`",
                            cx.name_str(f.name)
                        ),
                    ));
                }
            }
        }
        for eid in &m.ir.enums {
            let def = cx.enum_def(*eid);
            for v in &def.variants {
                if let Some(ty) = v.payload {
                    if occurs_value(cx, ty, OccurRoot::Enum(*eid), &mut HashSet::new()) {
                        return Err(CheckError::new(
                            m.path.clone(),
                            def.span,
                            format!(
                                "recursive field `{}` must go through `&` or `ptr`",
                                cx.name_str(v.name)
                            ),
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum OccurRoot {
    Struct(StructId),
    Enum(EnumId),
}

fn occurs_value(cx: &IrCx, ty: Ty, root: OccurRoot, visiting: &mut HashSet<u32>) -> bool {
    match cx.ty_kind(ty) {
        TyKind::Struct(id) => {
            if matches!(root, OccurRoot::Struct(s) if s == *id) {
                return true;
            }
            if !visiting.insert(id.0) {
                return false;
            }
            for field in &cx.struct_def(*id).fields {
                if occurs_value(cx, field.ty, root, visiting) {
                    return true;
                }
            }
            visiting.remove(&id.0);
            false
        }
        TyKind::Array { elem, .. } => occurs_value(cx, *elem, root, visiting),
        TyKind::Enum(id) => {
            if matches!(root, OccurRoot::Enum(e) if e == *id) {
                return true;
            }
            if !visiting.insert(1_000_000 + id.0) {
                return false;
            }
            for v in &cx.enum_def(*id).variants {
                if let Some(p) = v.payload {
                    if occurs_value(cx, p, root, visiting) {
                        return true;
                    }
                }
            }
            visiting.remove(&(1_000_000 + id.0));
            false
        }
        _ => false,
    }
}

fn collect_enums(cx: &mut IrCx, mods: &mut [ModData], i: usize) -> Result<(), CheckError> {
    let path = mods[i].path.clone();
    let names = mods[i].names.clone();
    let items: Vec<EnumItem> = mods[i]
        .ast
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Enum(e) => Some(e.clone()),
            _ => None,
        })
        .collect();
    for e in items {
        if mods[i].types.contains_key(&e.name.name) || mods[i].enum_temps.contains_key(&e.name.name)
        {
            return Err(CheckError::new(
                path.clone(),
                e.name.span,
                format!("duplicate type `{}`", e.name.name),
            ));
        }
        if !e.type_params.is_empty() {
            mods[i].enum_temps.insert(e.name.name.clone(), e);
            continue;
        }
        let ename = cx.intern_name(&e.name.name);
        let id = cx.alloc_enum(EnumDef {
            module: names.clone(),
            name: ename,
            doc: e.doc.clone(),
            variants: Vec::new(),
            try_catch: None,
            debug: e.debug,
            span: e.span,
        });
        let ty = cx.ty_enum(id);
        mods[i].types.insert(e.name.name.clone(), ty);
        mods[i].ir.enums.push(id);
        let types = mods[i].types.clone();
        let mut variants = Vec::new();
        for v in &e.variants {
            let payload = match &v.payload {
                Some(t) => Some(resolve_type(cx, t, &path, &types)?),
                None => None,
            };
            variants.push(VariantDef {
                name: cx.intern_name(&v.name.name),
                doc: v.doc.clone(),
                payload,
            });
        }
        let try_catch = enum_try_catch(cx, id, &e, &path, &variants)?;
        let def = cx.enum_def_mut(id);
        def.variants = variants;
        def.try_catch = try_catch;
    }
    Ok(())
}

fn enum_try_catch(
    cx: &IrCx,
    id: EnumId,
    e: &EnumItem,
    path: &str,
    variants: &[VariantDef],
) -> Result<Option<TryCatch>, CheckError> {
    let Some((s, f)) = &e.try_catch else {
        return Ok(None);
    };
    let success = find_variant_index(cx, variants, &s.name).ok_or_else(|| {
        CheckError::new(
            path,
            s.span,
            format!("`#[{}]` unknown variant `{}`", "TryCatch", s.name),
        )
    })?;
    let failure = find_variant_index(cx, variants, &f.name).ok_or_else(|| {
        CheckError::new(
            path,
            f.span,
            format!("`#[{}]` unknown variant `{}`", "TryCatch", f.name),
        )
    })?;
    Ok(Some(TryCatch {
        success: VariantId {
            enum_id: id,
            index: success,
        },
        failure: VariantId {
            enum_id: id,
            index: failure,
        },
    }))
}

fn find_variant_index(cx: &IrCx, variants: &[VariantDef], name: &str) -> Option<u32> {
    for (i, v) in variants.iter().enumerate() {
        if cx.name_str(v.name) == name {
            return Some(i as u32);
        }
    }
    None
}

fn collect_funcs(cx: &mut IrCx, mods: &mut [ModData], i: usize) -> Result<(), CheckError> {
    let path = mods[i].path.clone();
    let names = mods[i].names.clone();
    let types = mods[i].types.clone();
    let items = mods[i].ast.items.clone();

    for it in &items {
        match it {
            Item::Fn(f) => {
                let existing = mods[i].funcs.clone();
                let id = alloc_fn_mod(cx, &mut mods[i], &path, &names, &types, f, None, &existing)?;
                mods[i].funcs.insert(f.name.name.clone(), id);
                mods[i].ir.funcs.push(id);
            }
            Item::Struct(s) => {
                if !s.type_params.is_empty() {
                    continue;
                }
                let sid = mods[i].structs[&s.name.name];
                for m in &s.methods {
                    let key = (sid.0, m.name.name.clone());
                    if mods[i].methods.contains_key(&key) || mods[i].method_temps.contains_key(&key)
                    {
                        return Err(CheckError::new(
                            path.clone(),
                            m.name.span,
                            format!("duplicate method `{}` on `{}`", m.name.name, s.name.name),
                        ));
                    }
                    if !m.type_params.is_empty() {
                        mods[i].method_temps.insert(key, m.clone());
                        continue;
                    }
                    let id = alloc_fn_mod(
                        cx,
                        &mut mods[i],
                        &path,
                        &names,
                        &types,
                        m,
                        Some(sid),
                        &HashMap::new(),
                    )?;
                    mods[i].methods.insert(key, id);
                    mods[i].ir.funcs.push(id);
                }
            }
            Item::UseLib(u) => {
                if mods[i].lib_ns.contains_key(&u.alias.name) {
                    return Err(CheckError::new(
                        path.clone(),
                        u.alias.span,
                        format!("duplicate `use lib` alias `{}`", u.alias.name),
                    ));
                }
                let lib = cx.intern_name(&u.lib);
                let mut ns = HashMap::new();
                for d in &u.decls {
                    if ns.contains_key(&d.name.name) {
                        return Err(CheckError::new(
                            path.clone(),
                            d.name.span,
                            format!("duplicate lib fn `{}`", d.name.name),
                        ));
                    }
                    let mut params = Vec::new();
                    for p in &d.params {
                        params.push(resolve_type(cx, &p.ty, &path, &types)?);
                    }
                    let ret = match &d.ret {
                        Some(t) => resolve_type(cx, t, &path, &types)?,
                        None => cx.common.void,
                    };
                    let ty = cx.ty_fn(params, ret);
                    let fname = cx.intern_name(&d.name.name);
                    let symbol = match &d.symbol {
                        Some(s) => cx.intern_name(s),
                        None => fname,
                    };
                    let id = cx.alloc_func(Func {
                        doc: String::new(),
                        id: FuncId(0),
                        module: names.clone(),
                        name: fname,
                        ty,
                        vis: Vis::Private,
                        method_of: None,
                        kind: FuncKind::Lib { lib, symbol },
                        span: d.span,
                    });
                    ns.insert(d.name.name.clone(), id);
                    mods[i].ir.funcs.push(id);
                }
                mods[i].lib_ns.insert(u.alias.name.clone(), ns);
            }
            _ => {}
        }
    }
    Ok(())
}

fn alloc_fn_mod(
    cx: &mut IrCx,
    m: &mut ModData,
    path: &str,
    names: &[Name],
    types: &HashMap<String, Ty>,
    f: &FnItem,
    method_of: Option<StructId>,
    existing: &HashMap<String, FuncId>,
) -> Result<FuncId, CheckError> {
    let empty = HashMap::new();
    let mut env = GenEnv {
        types,
        subst: &empty,
        enum_temps: &m.enum_temps,
        enum_insts: &mut m.insts,
        struct_temps: &m.struct_temps,
        struct_temp_env: &m.struct_temp_env,
        struct_insts: &mut m.struct_insts,
        methods: &mut m.methods,
        method_temps: &mut m.method_temps,
        method_insts: &mut m.method_insts,
        fn_substs: &mut m.fn_substs,
        pending_mono: &mut m.pending_mono,
        module: names,
        ir_enums: &mut m.ir.enums,
        ir_structs: &mut m.ir.structs,
        ir_funcs: &mut m.ir.funcs,
    };
    alloc_fn(cx, path, names, f, method_of, existing, &mut env)
}

pub(crate) fn alloc_fn(
    cx: &mut IrCx,
    path: &str,
    names: &[Name],
    f: &FnItem,
    method_of: Option<StructId>,
    existing: &HashMap<String, FuncId>,
    env: &mut GenEnv<'_>,
) -> Result<FuncId, CheckError> {
    if existing.contains_key(&f.name.name) {
        return Err(CheckError::new(
            path,
            f.name.span,
            format!("duplicate fn `{}`", f.name.name),
        ));
    }
    let mut params = Vec::new();
    for p in &f.params {
        params.push(resolve_type_g(cx, &p.ty, path, env)?);
    }
    let ret = match &f.ret {
        Some(t) => resolve_type_g(cx, t, path, env)?,
        None => cx.common.void,
    };
    let ty = cx.ty_fn(params, ret);
    let vis = if f.vis_pub { Vis::Pub } else { Vis::Private };
    let fname = cx.intern_name(&f.name.name);
    Ok(cx.alloc_func(Func {
        id: FuncId(0),
        doc: f.doc.clone(),
        module: names.to_vec(),
        name: fname,
        ty,
        vis,
        method_of,
        kind: FuncKind::Decl,
        span: f.span,
    }))
}

fn infer_omitted_rets(cx: &mut IrCx, mods: &mut [ModData], i: usize) -> Result<(), CheckError> {
    let path = mods[i].path.clone();
    let types = mods[i].types.clone();
    let consts = mods[i].consts.clone();
    let methods = mods[i].methods.clone();
    let lib_ns = mods[i].lib_ns.clone();
    let mut funcs = mods[i].funcs.clone();
    for (name, id) in &mods[i].imports {
        funcs.insert(name.clone(), *id);
    }
    let items = mods[i].ast.items.clone();

    let mut pending: Vec<(FuncId, FnItem)> = Vec::new();
    for it in &items {
        match it {
            Item::Fn(f) if f.ret.is_none() => {
                pending.push((mods[i].funcs[&f.name.name], f.clone()));
            }
            Item::Struct(s) => {
                if !s.type_params.is_empty() {
                    continue;
                }
                let sid = mods[i].structs[&s.name.name];
                for m in &s.methods {
                    if m.ret.is_none() && m.type_params.is_empty() {
                        pending.push((mods[i].methods[&(sid.0, m.name.name.clone())], m.clone()));
                    }
                }
            }
            _ => {}
        }
    }

    for (id, f) in pending {
        let inferred = infer_fn_ret(cx, &f, &types, &funcs, &methods, &lib_ns, &consts, &path)?;
        let (params, old_ret) = match cx.fn_sig(cx.func(id).ty) {
            Some(s) => (s.0.clone(), s.1),
            None => {
                return Err(CheckError::new(
                    path.clone(),
                    f.span,
                    "function is missing a type",
                ));
            }
        };
        if inferred == old_ret {
            continue;
        }
        let ty = cx.ty_fn(params, inferred);
        cx.func_mut(id).ty = ty;
    }
    Ok(())
}

fn infer_fn_ret(
    cx: &IrCx,
    f: &FnItem,
    types: &HashMap<String, Ty>,
    funcs: &HashMap<String, FuncId>,
    methods: &HashMap<(u32, String), FuncId>,
    lib_ns: &HashMap<String, HashMap<String, FuncId>>,
    consts: &HashMap<String, ConstVal>,
    path: &str,
) -> Result<Ty, CheckError> {
    let mut locals = HashMap::new();
    for p in &f.params {
        locals.insert(
            p.name.name.clone(),
            resolve_type_ref(cx, &p.ty, path, types)?,
        );
    }
    let mut seen: Option<Ty> = None;
    collect_return_tys(
        &f.body, cx, &locals, types, funcs, methods, lib_ns, consts, path, &mut seen,
    )?;
    Ok(seen.unwrap_or(cx.common.void))
}

fn collect_return_tys(
    block: &parse::Block,
    cx: &IrCx,
    locals: &HashMap<String, Ty>,
    types: &HashMap<String, Ty>,
    funcs: &HashMap<String, FuncId>,
    methods: &HashMap<(u32, String), FuncId>,
    lib_ns: &HashMap<String, HashMap<String, FuncId>>,
    consts: &HashMap<String, ConstVal>,
    path: &str,
    seen: &mut Option<Ty>,
) -> Result<(), CheckError> {
    for stmt in &block.stmts {
        match stmt {
            parse::Stmt::Return(s) => match &s.value {
                None => {
                    if let Some(t) = *seen {
                        if t != cx.common.void {
                            return Err(CheckError::new(path, s.span, "return type mismatch"));
                        }
                    } else {
                        *seen = Some(cx.common.void);
                    }
                }
                Some(e) => {
                    let t = peek_ty(cx, e, locals, types, funcs, methods, lib_ns, consts, path)?;
                    match *seen {
                        None => *seen = Some(t),
                        Some(old) if old != t => {
                            return Err(CheckError::new(path, s.span, "return type mismatch"));
                        }
                        _ => {}
                    }
                }
            },
            parse::Stmt::If(s) => {
                collect_return_tys(
                    &s.then_block,
                    cx,
                    locals,
                    types,
                    funcs,
                    methods,
                    lib_ns,
                    consts,
                    path,
                    seen,
                )?;
                if let Some(b) = &s.else_block {
                    collect_return_tys(
                        b, cx, locals, types, funcs, methods, lib_ns, consts, path, seen,
                    )?;
                }
            }
            parse::Stmt::While(s) => {
                collect_return_tys(
                    &s.body, cx, locals, types, funcs, methods, lib_ns, consts, path, seen,
                )?;
            }
            parse::Stmt::For(s) => {
                collect_return_tys(
                    &s.body, cx, locals, types, funcs, methods, lib_ns, consts, path, seen,
                )?;
            }
            parse::Stmt::ForEach(s) => {
                collect_return_tys(
                    &s.body, cx, locals, types, funcs, methods, lib_ns, consts, path, seen,
                )?;
            }
            parse::Stmt::Defer(s) => {
                collect_return_tys(
                    &s.body, cx, locals, types, funcs, methods, lib_ns, consts, path, seen,
                )?;
            }
            parse::Stmt::Block(b) => {
                collect_return_tys(
                    b, cx, locals, types, funcs, methods, lib_ns, consts, path, seen,
                )?;
            }
            parse::Stmt::UseLocal(_) => {}
            parse::Stmt::Match(s) => {
                for arm in &s.arms {
                    collect_return_tys(
                        &arm.body, cx, locals, types, funcs, methods, lib_ns, consts, path, seen,
                    )?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn peek_ty(
    cx: &IrCx,
    expr: &Expr,
    locals: &HashMap<String, Ty>,
    types: &HashMap<String, Ty>,
    funcs: &HashMap<String, FuncId>,
    methods: &HashMap<(u32, String), FuncId>,
    lib_ns: &HashMap<String, HashMap<String, FuncId>>,
    consts: &HashMap<String, ConstVal>,
    path: &str,
) -> Result<Ty, CheckError> {
    match &expr.kind {
        ExprKind::ScriptType(_) | ExprKind::ScriptLit { .. } => Err(CheckError::new(
            path,
            expr.span,
            "script type is not available here",
        )),
        ExprKind::Int(_) => Ok(cx.common.int),
        ExprKind::Float(_) => Ok(cx.common.float),
        ExprKind::Bool(_) => Ok(cx.common.bool),
        ExprKind::String(_) | ExprKind::Interpolate { .. } => Ok(cx.common.string),
        ExprKind::Name(n) => {
            if let Some(&t) = locals.get(&n.name) {
                return Ok(t);
            }
            if let Some(c) = consts.get(&n.name) {
                return Ok(c.ty);
            }
            Err(CheckError::new(
                path,
                n.span,
                format!("unknown name `{}`", n.name),
            ))
        }
        ExprKind::StructLit { name, .. } => match types.get(&name.name).copied() {
            Some(t) => Ok(t),
            None => Err(CheckError::new(
                path,
                name.span,
                format!("unknown type `{}`", name.name),
            )),
        },
        ExprKind::Field { base, field } => {
            if let ExprKind::Name(n) = &base.kind {
                if !locals.contains_key(&n.name) {
                    if let Some(c) = consts.get(&qual_key(&n.name, &field.name)) {
                        return Ok(c.ty);
                    }
                }
            }
            let mut base_ty = peek_ty(
                cx, base, locals, types, funcs, methods, lib_ns, consts, path,
            )?;
            while let Some(inner) = cx.ref_inner(base_ty) {
                base_ty = inner;
            }
            match cx.ty_kind(base_ty) {
                TyKind::Struct(id) => match field_path(cx, *id, &field.name) {
                    Some(p) => Ok(cx.field_def(*p.last().unwrap()).ty),
                    None => Err(CheckError::new(
                        path,
                        field.span,
                        format!("no field `{}`", field.name),
                    )),
                },
                TyKind::Array { .. } if field.name == "len" => Ok(cx.common.int),
                TyKind::String | TyKind::Slice { .. } => match field.name.as_str() {
                    "ptr" => Ok(cx
                        .lookup_ty(&TyKind::Ref(cx.common.u8))
                        .unwrap_or(cx.common.ptr)),
                    "len" => Ok(cx.common.int),
                    _ => Err(CheckError::new(
                        path,
                        field.span,
                        format!("no field `{}`", field.name),
                    )),
                },
                _ => Err(CheckError::new(
                    path,
                    field.span,
                    format!("no field `{}`", field.name),
                )),
            }
        }
        ExprKind::Call { func, args, .. } => {
            if let ExprKind::Name(n) = &func.kind {
                if args.len() == 1 {
                    if let Some(t) = conv_ty_name(cx, &n.name) {
                        return Ok(t);
                    }
                }
            }
            let id = peek_callee(
                cx, func, locals, types, funcs, methods, lib_ns, consts, path,
            )?;
            match cx.fn_sig(cx.func(id).ty) {
                Some((_, ret)) => Ok(ret),
                None => Err(CheckError::new(path, func.span, "not a function")),
            }
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let lt = peek_ty(cx, lhs, locals, types, funcs, methods, lib_ns, consts, path)?;
            match op {
                parse::BinOp::And
                | parse::BinOp::Or
                | parse::BinOp::Eq
                | parse::BinOp::Ne
                | parse::BinOp::Lt
                | parse::BinOp::Le
                | parse::BinOp::Gt
                | parse::BinOp::Ge => Ok(cx.common.bool),
                parse::BinOp::Add
                | parse::BinOp::Sub
                | parse::BinOp::Mul
                | parse::BinOp::Div
                | parse::BinOp::Rem => {
                    if let Some(id) = find_operator(cx, lt, *op, methods) {
                        return match cx.fn_sig(cx.func(id).ty) {
                            Some((_, ret)) => Ok(ret),
                            None => Err(CheckError::new(path, expr.span, "not a function")),
                        };
                    }
                    let _ = rhs;
                    if is_float(cx, lt) {
                        Ok(lt)
                    } else {
                        Ok(cx.common.int)
                    }
                }
            }
        }
        ExprKind::Unary { op, expr: inner } => match op {
            parse::UnaryOp::Neg => peek_ty(
                cx, inner, locals, types, funcs, methods, lib_ns, consts, path,
            ),
            parse::UnaryOp::Not => Ok(cx.common.bool),
            parse::UnaryOp::Ref => {
                let t = peek_ty(
                    cx, inner, locals, types, funcs, methods, lib_ns, consts, path,
                )?;
                match cx.lookup_ty(&TyKind::Ref(t)) {
                    Some(r) => Ok(r),
                    None => Err(CheckError::new(path, expr.span, "cannot name `&` type yet")),
                }
            }
            parse::UnaryOp::Deref => {
                let t = peek_ty(
                    cx, inner, locals, types, funcs, methods, lib_ns, consts, path,
                )?;
                match cx.ref_inner(t) {
                    Some(inner) => Ok(inner),
                    None => Err(CheckError::new(path, expr.span, "deref of non-ref")),
                }
            }
        },
        ExprKind::Cast { ty, .. } => resolve_type_ref(cx, ty, path, types),
        ExprKind::Question { expr } => {
            let t = peek_ty(
                cx, expr, locals, types, funcs, methods, lib_ns, consts, path,
            )?;
            match cx.ty_kind(t) {
                TyKind::Enum(id) => match &cx.enum_def(*id).try_catch {
                    Some(tc) => {
                        match cx.enum_def(*id).variants[tc.success.index as usize].payload {
                            Some(p) => Ok(p),
                            None => Ok(cx.common.void),
                        }
                    }
                    None => Err(CheckError::new(path, expr.span, "`?` needs `#[TryCatch]`")),
                },
                _ => Err(CheckError::new(path, expr.span, "`?` on non-enum")),
            }
        }
        ExprKind::Syscall { .. } => Ok(cx.common.int),
        ExprKind::SizeOf(_) => Ok(cx.common.int),
        ExprKind::SliceFrom { .. } => Err(CheckError::new(
            path,
            expr.span,
            "`@slice` needs an expected type",
        )),
        ExprKind::TypePath { name, .. } => Err(CheckError::new(
            path,
            expr.span,
            format!("`{}` is a type, not a value", name.name),
        )),
        ExprKind::SliceLit { elem, .. } => {
            let e = resolve_type_ref(cx, elem, path, types)?;
            match cx.lookup_ty(&TyKind::Slice { elem: e }) {
                Some(t) => Ok(t),
                None => Err(CheckError::new(path, expr.span, "unknown slice type")),
            }
        }
        ExprKind::ArrayLit { elems } => {
            if elems.is_empty() {
                return Err(CheckError::new(path, expr.span, "cannot infer empty array"));
            }
            let elem = peek_ty(
                cx, &elems[0], locals, types, funcs, methods, lib_ns, consts, path,
            )?;
            match cx.lookup_ty(&TyKind::Array {
                elem,
                len: elems.len() as u64,
            }) {
                Some(t) => Ok(t),
                None => Err(CheckError::new(path, expr.span, "unknown array type")),
            }
        }
        ExprKind::Index { base, .. } => {
            let bt = peek_ty(
                cx, base, locals, types, funcs, methods, lib_ns, consts, path,
            )?;
            match cx.elem_ty(bt) {
                Some(e) => Ok(e),
                None => Err(CheckError::new(path, expr.span, "index of non-array/slice")),
            }
        }
        ExprKind::Slice { base, .. } => {
            let bt = peek_ty(
                cx, base, locals, types, funcs, methods, lib_ns, consts, path,
            )?;
            let elem = match cx.elem_ty(bt) {
                Some(e) => e,
                None => {
                    return Err(CheckError::new(path, expr.span, "slice of non-array/slice"));
                }
            };
            match cx.lookup_ty(&TyKind::Slice { elem }) {
                Some(t) => Ok(t),
                None => Err(CheckError::new(path, expr.span, "unknown slice type")),
            }
        }
        ExprKind::If {
            then_expr,
            else_expr,
            ..
        } => {
            let t = peek_ty(
                cx, then_expr, locals, types, funcs, methods, lib_ns, consts, path,
            )?;
            let e = peek_ty(
                cx, else_expr, locals, types, funcs, methods, lib_ns, consts, path,
            )?;
            if t != e {
                return Err(CheckError::new(
                    path,
                    expr.span,
                    "if-expression branches have different types",
                ));
            }
            Ok(t)
        }
    }
}

fn conv_ty_name(cx: &IrCx, name: &str) -> Option<Ty> {
    Some(match name {
        "i8" => cx.common.i8,
        "i16" => cx.common.i16,
        "i32" => cx.common.i32,
        "i64" | "int" => cx.common.int,
        "u8" => cx.common.u8,
        "u16" => cx.common.u16,
        "u32" => cx.common.u32,
        "u64" => cx.common.u64,
        "f32" => cx.common.f32,
        "f64" | "float" => cx.common.float,
        _ => return None,
    })
}

fn peek_callee(
    cx: &IrCx,
    func: &Expr,
    locals: &HashMap<String, Ty>,
    types: &HashMap<String, Ty>,
    funcs: &HashMap<String, FuncId>,
    methods: &HashMap<(u32, String), FuncId>,
    lib_ns: &HashMap<String, HashMap<String, FuncId>>,
    consts: &HashMap<String, ConstVal>,
    path: &str,
) -> Result<FuncId, CheckError> {
    match &func.kind {
        ExprKind::Name(n) => match funcs.get(&n.name).copied() {
            Some(id) => Ok(id),
            None => Err(CheckError::new(
                path,
                n.span,
                format!("unknown function `{}`", n.name),
            )),
        },
        ExprKind::Field { base, field } => {
            if let ExprKind::Name(n) = &base.kind {
                if let Some(ns) = lib_ns.get(&n.name) {
                    return match ns.get(&field.name).copied() {
                        Some(id) => Ok(id),
                        None => Err(CheckError::new(
                            path,
                            field.span,
                            format!("`{}` has no `{}`", n.name, field.name),
                        )),
                    };
                }
            }
            let base_ty = peek_ty(
                cx, base, locals, types, funcs, methods, lib_ns, consts, path,
            )?;
            match find_method(cx, base_ty, &field.name, methods) {
                Some(id) => Ok(id),
                None => Err(CheckError::new(
                    path,
                    field.span,
                    format!("no method `{}`", field.name),
                )),
            }
        }
        _ => Err(CheckError::new(
            path,
            func.span,
            "cannot call this expression",
        )),
    }
}

pub(crate) fn find_method(
    cx: &IrCx,
    ty: Ty,
    name: &str,
    methods: &HashMap<(u32, String), FuncId>,
) -> Option<FuncId> {
    let mut t = ty;
    while let Some(inner) = cx.ref_inner(t) {
        t = inner;
    }
    match cx.ty_kind(t) {
        TyKind::Struct(id) => methods.get(&(id.0, name.to_string())).copied(),
        _ => None,
    }
}

pub(crate) fn find_operator(
    cx: &IrCx,
    ty: Ty,
    op: parse::BinOp,
    methods: &HashMap<(u32, String), FuncId>,
) -> Option<FuncId> {
    let name = match op {
        parse::BinOp::Add => "operator+",
        parse::BinOp::Sub => "operator-",
        parse::BinOp::Mul => "operator*",
        parse::BinOp::Div => "operator/",
        parse::BinOp::Eq => "operator==",
        parse::BinOp::Ne => "operator!=",
        parse::BinOp::Lt => "operator<",
        parse::BinOp::Le => "operator<=",
        parse::BinOp::Gt => "operator>",
        parse::BinOp::Ge => "operator>=",
        _ => return None,
    };
    find_method(cx, ty, name, methods)
}

fn collect_globals(cx: &mut IrCx, mods: &mut [ModData], i: usize) -> Result<(), CheckError> {
    let path = mods[i].path.clone();
    let names = mods[i].names.clone();
    let types = mods[i].types.clone();
    let items: Vec<ModLetItem> = mods[i]
        .ast
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Let(l) => Some(l.clone()),
            _ => None,
        })
        .collect();
    for l in items {
        if mods[i].globals.contains_key(&l.name.name)
            || mods[i].funcs.contains_key(&l.name.name)
            || mods[i].consts.contains_key(&l.name.name)
        {
            return Err(CheckError::new(
                path.clone(),
                l.name.span,
                format!("`{}` already in scope", l.name.name),
            ));
        }
        let ty = resolve_type(cx, &l.ty, &path, &types)?;
        let init = match &l.value {
            Some(v) => {
                let folded = fold_global_init(cx, v, ty, &types, &mods[i].consts, &path)?;
                if const_init_is_zero(&folded) {
                    None
                } else {
                    Some(folded)
                }
            }
            None => None,
        };
        let gname = cx.intern_name(&l.name.name);
        let id = cx.alloc_global(GlobalDef {
            id: GlobalId(0),
            doc: l.doc.clone(),
            module: names.clone(),
            name: gname,
            ty,
            span: l.span,
            init,
        });
        mods[i].globals.insert(l.name.name, id);
        mods[i].ir.globals.push(id);
    }
    Ok(())
}

fn find_main(mods: &[ModData]) -> Result<FuncId, CheckError> {
    let mut found = None;
    for m in mods {
        if m.key == "j3.sys" || m.key.starts_with("j3.") {
            continue;
        }
        if let Some(&id) = m.funcs.get("main") {
            if found.is_some() {
                return Err(CheckError::new(
                    m.path.clone(),
                    Span::dummy(),
                    "multiple `fn main` in the program",
                ));
            }
            found = Some((id, m.path.clone()));
        }
    }
    match found {
        Some((id, _)) => Ok(id),
        None => Err(CheckError::new(
            "<program>",
            Span::dummy(),
            "program needs `fn main()`",
        )),
    }
}

fn dummy_main(cx: &mut IrCx) -> FuncId {
    let ty = cx.ty_fn(Vec::new(), cx.common.void);
    let name = cx.intern_name("main");
    cx.alloc_func(Func {
        id: FuncId(0),
        doc: String::new(),
        module: Vec::new(),
        name,
        ty,
        vis: Vis::Private,
        method_of: None,
        kind: FuncKind::Decl,
        span: Span::dummy(),
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum UsePhase {
    Types,
    Vals,
}

fn resolve_all_uses(mods: &mut [ModData], phase: UsePhase) -> Result<(), CheckError> {
    let keys: Vec<String> = mods.iter().map(|m| m.key.clone()).collect();
    let mut pending: Vec<(usize, UseItem)> = Vec::new();
    for (i, m) in mods.iter().enumerate() {
        for it in &m.ast.items {
            if let Item::Use(u) = it {
                pending.push((i, u.clone()));
            }
        }
    }

    for (i, u) in pending {
        let path = mods[i].path.clone();
        let (mod_key, names) = use_target(&u, &keys, &path)?;
        let src_i = match mods.iter().position(|m| m.key == mod_key) {
            Some(j) => j,
            None => {
                return Err(CheckError::new(
                    path,
                    u.span,
                    format!("module `{mod_key}` was not loaded"),
                ));
            }
        };
        if names.is_empty() {
            let alias = match u.path.last() {
                Some(p) => p.name.clone(),
                None => {
                    return Err(CheckError::new(path, u.span, "empty use path"));
                }
            };
            match phase {
                UsePhase::Types => {
                    if mods[i].mod_types.contains_key(&alias) {
                        return Err(CheckError::new(
                            path,
                            u.span,
                            format!("`{alias}` already in scope"),
                        ));
                    }
                    let mut exported = HashMap::new();
                    for (k, v) in &mods[src_i].types {
                        if mods[src_i].exports.contains(k) {
                            exported.insert(k.clone(), *v);
                        }
                    }
                    mods[i].mod_types.insert(alias, exported);
                }
                UsePhase::Vals => {
                    if mods[i].mod_ns.contains_key(&alias) || mods[i].lib_ns.contains_key(&alias) {
                        return Err(CheckError::new(
                            path,
                            u.span,
                            format!("`{alias}` already in scope"),
                        ));
                    }
                    let mut exported = HashMap::new();
                    for (k, v) in &mods[src_i].funcs {
                        if mods[src_i].exports.contains(k) {
                            exported.insert(k.clone(), *v);
                        }
                    }
                    let consts: Vec<(String, ConstVal)> = mods[src_i]
                        .consts
                        .iter()
                        .filter(|(k, _)| mods[src_i].exports.contains(*k))
                        .map(|(k, v)| (qual_key(&alias, k), v.clone()))
                        .collect();
                    for (k, v) in consts {
                        mods[i].consts.insert(k, v);
                    }
                    mods[i].mod_ns.insert(alias.clone(), exported.clone());
                    mods[i].lib_ns.insert(alias, exported);
                }
            }
            continue;
        }
        for name in names {
            match phase {
                UsePhase::Types => {
                    if mods[i].types.contains_key(&name)
                        || mods[i].struct_temps.contains_key(&name)
                        || mods[i].enum_temps.contains_key(&name)
                    {
                        return Err(CheckError::new(
                            path,
                            u.span,
                            format!("`{name}` already in scope"),
                        ));
                    }
                    let exists = mods[src_i].types.contains_key(&name)
                        || mods[src_i].enum_temps.contains_key(&name)
                        || mods[src_i].struct_temps.contains_key(&name);
                    if exists && !mods[src_i].exports.contains(&name) {
                        return Err(CheckError::new(
                            path,
                            u.span,
                            format!("`{name}` is private in `{mod_key}`"),
                        ));
                    }
                    if let Some(&ty) = mods[src_i].types.get(&name) {
                        mods[i].types.insert(name.clone(), ty);
                        if let Some(&sid) = mods[src_i].structs.get(&name) {
                            mods[i].structs.insert(name.clone(), sid);
                        }
                        continue;
                    }
                    if let Some(t) = mods[src_i].enum_temps.get(&name).cloned() {
                        mods[i].enum_temps.insert(name, t);
                        continue;
                    }
                    if let Some(t) = mods[src_i].struct_temps.get(&name).cloned() {
                        mods[i].struct_temps.insert(name, t);
                        continue;
                    }
                }
                UsePhase::Vals => {
                    if let Some(&id) = mods[src_i].funcs.get(&name) {
                        if !mods[src_i].exports.contains(&name) {
                            return Err(CheckError::new(
                                path,
                                u.span,
                                format!("`{name}` is private in `{mod_key}`"),
                            ));
                        }
                        if mods[i].imports.contains_key(&name) || mods[i].funcs.contains_key(&name)
                        {
                            return Err(CheckError::new(
                                path,
                                u.span,
                                format!("`{name}` already in scope"),
                            ));
                        }
                        mods[i].imports.insert(name, id);
                        continue;
                    }
                    if mods[i].types.get(&name) == mods[src_i].types.get(&name)
                        && mods[src_i].types.contains_key(&name)
                    {
                        if let Some(&sid) = mods[src_i].structs.get(&name) {
                            let methods: Vec<((u32, String), FuncId)> = mods[src_i]
                                .methods
                                .iter()
                                .filter(|((s, _), _)| *s == sid.0)
                                .map(|(k, v)| (k.clone(), *v))
                                .collect();
                            for (k, v) in methods {
                                mods[i].methods.insert(k, v);
                            }
                            let temps: Vec<((u32, String), parse::FnItem)> = mods[src_i]
                                .method_temps
                                .iter()
                                .filter(|((s, _), _)| *s == sid.0)
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect();
                            for (k, v) in temps {
                                mods[i].method_temps.insert(k, v);
                            }
                        }
                        continue;
                    }
                    if mods[i].struct_temps.contains_key(&name)
                        && mods[src_i].struct_temps.contains_key(&name)
                    {
                        continue;
                    }
                    if mods[i].enum_temps.contains_key(&name)
                        && mods[src_i].enum_temps.contains_key(&name)
                    {
                        continue;
                    }
                    if let Some(c) = mods[src_i].consts.get(&name).cloned() {
                        if !mods[src_i].exports.contains(&name) {
                            return Err(CheckError::new(
                                path,
                                u.span,
                                format!("`{name}` is private in `{mod_key}`"),
                            ));
                        }
                        if mods[i].consts.contains_key(&name) {
                            return Err(CheckError::new(
                                path,
                                u.span,
                                format!("`{name}` already in scope"),
                            ));
                        }
                        mods[i].consts.insert(name, c);
                        continue;
                    }
                    return Err(CheckError::new(
                        path,
                        u.span,
                        format!("`{mod_key}` has no `{name}`"),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn use_target(
    u: &UseItem,
    loaded: &[String],
    path: &str,
) -> Result<(String, Vec<String>), CheckError> {
    let parts: Vec<String> = u.path.iter().map(|p| p.name.clone()).collect();
    if let Some(names) = &u.names {
        return Ok((
            parts.join("."),
            names.iter().map(|n| n.name.clone()).collect(),
        ));
    }
    let as_mod = parts.join(".");
    if loaded.iter().any(|k| k == &as_mod) {
        return Ok((as_mod, Vec::new()));
    }
    if parts.len() >= 2 {
        let mod_key = parts[..parts.len() - 1].join(".");
        let name = parts[parts.len() - 1].clone();
        if loaded.iter().any(|k| k == &mod_key) {
            return Ok((mod_key, vec![name]));
        }
    }
    Err(CheckError::new(
        path,
        u.span,
        format!("cannot resolve `use {}`", parts.join(".")),
    ))
}

pub(crate) fn resolve_type(
    cx: &mut IrCx,
    ty: &Type,
    path: &str,
    types: &HashMap<String, Ty>,
) -> Result<Ty, CheckError> {
    match &ty.kind {
        TypeKind::Ref(inner) => {
            let inner = resolve_type(cx, inner, path, types)?;
            Ok(cx.ty_ref(inner))
        }
        TypeKind::Named(name) => resolve_named(cx, name, ty.span, path, types),
        TypeKind::Generic { name, .. } => Err(CheckError::new(
            path,
            ty.span,
            format!("generic type `{name}` needs instantiation"),
        )),
        TypeKind::Array { len, elem } => {
            let elem = resolve_type(cx, elem, path, types)?;
            Ok(cx.ty_array(elem, *len))
        }
        TypeKind::Slice(elem) => {
            let elem = resolve_type(cx, elem, path, types)?;
            Ok(cx.ty_slice(elem))
        }
        TypeKind::Fn { params, ret } => {
            let mut ps = Vec::new();
            for p in params {
                ps.push(resolve_type(cx, p, path, types)?);
            }
            let r = match ret {
                Some(t) => resolve_type(cx, t, path, types)?,
                None => cx.common.void,
            };
            Ok(cx.ty_fn(ps, r))
        }
    }
}

pub(crate) fn resolve_type_g(
    cx: &mut IrCx,
    ty: &Type,
    path: &str,
    env: &mut GenEnv<'_>,
) -> Result<Ty, CheckError> {
    match &ty.kind {
        TypeKind::Ref(inner) => {
            let inner = resolve_type_g(cx, inner, path, env)?;
            Ok(cx.ty_ref(inner))
        }
        TypeKind::Named(name) => {
            if let Some(&t) = env.subst.get(name) {
                return Ok(t);
            }
            if let Some(&t) = env.types.get(name) {
                return Ok(t);
            }
            if env.enum_temps.contains_key(name) || env.struct_temps.contains_key(name) {
                return Err(CheckError::new(
                    path,
                    ty.span,
                    format!("`{name}` needs type arguments"),
                ));
            }
            resolve_named(cx, name, ty.span, path, env.types)
        }
        TypeKind::Generic { name, args } => {
            let mut resolved = Vec::new();
            for a in args {
                resolved.push(resolve_type_g(cx, a, path, env)?);
            }
            if env.enum_temps.contains_key(name) {
                instantiate_enum(cx, path, ty.span, name, resolved, env)
            } else if env.struct_temps.contains_key(name) {
                instantiate_struct(cx, path, ty.span, name, resolved, env)
            } else {
                Err(CheckError::new(
                    path,
                    ty.span,
                    format!("unknown generic type `{name}`"),
                ))
            }
        }
        TypeKind::Array { len, elem } => {
            let elem = resolve_type_g(cx, elem, path, env)?;
            Ok(cx.ty_array(elem, *len))
        }
        TypeKind::Slice(elem) => {
            let elem = resolve_type_g(cx, elem, path, env)?;
            Ok(cx.ty_slice(elem))
        }
        TypeKind::Fn { params, ret } => {
            let mut ps = Vec::new();
            for p in params {
                ps.push(resolve_type_g(cx, p, path, env)?);
            }
            let r = match ret {
                Some(t) => resolve_type_g(cx, t, path, env)?,
                None => cx.common.void,
            };
            Ok(cx.ty_fn(ps, r))
        }
    }
}

pub(crate) fn instantiate_enum(
    cx: &mut IrCx,
    path: &str,
    span: Span,
    name: &str,
    args: Vec<Ty>,
    env: &mut GenEnv<'_>,
) -> Result<Ty, CheckError> {
    let key = (name.to_string(), args.clone());
    if let Some(&id) = env.enum_insts.get(&key) {
        return Ok(cx.ty_enum(id));
    }
    let temp = match env.enum_temps.get(name) {
        Some(t) => t.clone(),
        None => {
            return Err(CheckError::new(
                path,
                span,
                format!("unknown generic type `{name}`"),
            ));
        }
    };
    if temp.type_params.len() != args.len() {
        return Err(CheckError::new(
            path,
            span,
            format!(
                "`{name}` takes {} type args, got {}",
                temp.type_params.len(),
                args.len()
            ),
        ));
    }
    let mut subst = HashMap::new();
    for (p, a) in temp.type_params.iter().zip(args.iter()) {
        subst.insert(p.name.clone(), *a);
    }
    let mut merged = env.subst.clone();
    merged.extend(subst);
    let mut variants = Vec::new();
    for v in &temp.variants {
        let payload = match &v.payload {
            Some(t) => {
                let mut nested = GenEnv {
                    types: env.types,
                    subst: &merged,
                    enum_temps: env.enum_temps,
                    enum_insts: env.enum_insts,
                    struct_temps: env.struct_temps,
                    struct_temp_env: env.struct_temp_env,
                    struct_insts: env.struct_insts,
                    methods: env.methods,
                    method_temps: env.method_temps,
                    method_insts: env.method_insts,
                    fn_substs: env.fn_substs,
                    pending_mono: env.pending_mono,
                    module: env.module,
                    ir_enums: env.ir_enums,
                    ir_structs: env.ir_structs,
                    ir_funcs: env.ir_funcs,
                };
                Some(resolve_type_g(cx, t, path, &mut nested)?)
            }
            None => None,
        };
        variants.push(VariantDef {
            name: cx.intern_name(&v.name.name),
            doc: v.doc.clone(),
            payload,
        });
    }
    let ename = cx.intern_name(&temp.name.name);
    let id = cx.alloc_enum(EnumDef {
        module: env.module.to_vec(),
        name: ename,
        doc: temp.doc.clone(),
        variants: Vec::new(),
        try_catch: None,
        debug: temp.debug,
        span: temp.span,
    });
    let try_catch = enum_try_catch(cx, id, &temp, path, &variants)?;
    let def = cx.enum_def_mut(id);
    def.variants = variants;
    def.try_catch = try_catch;
    for v in &cx.enum_def(id).variants {
        if let Some(ty) = v.payload {
            if occurs_value(cx, ty, OccurRoot::Enum(id), &mut HashSet::new()) {
                return Err(CheckError::new(
                    path,
                    temp.span,
                    format!(
                        "recursive field `{}` must go through `&` or `ptr`",
                        cx.name_str(v.name)
                    ),
                ));
            }
        }
    }
    env.enum_insts.insert(key, id);
    env.ir_enums.push(id);
    Ok(cx.ty_enum(id))
}

pub(crate) fn instantiate_struct(
    cx: &mut IrCx,
    path: &str,
    span: Span,
    name: &str,
    args: Vec<Ty>,
    env: &mut GenEnv<'_>,
) -> Result<Ty, CheckError> {
    let key = (name.to_string(), args.clone());
    if let Some(&id) = env.struct_insts.get(&key) {
        return Ok(cx.ty_struct(id));
    }
    let temp = match env.struct_temps.get(name) {
        Some(t) => t.clone(),
        None => {
            return Err(CheckError::new(
                path,
                span,
                format!("unknown generic type `{name}`"),
            ));
        }
    };
    if temp.type_params.len() != args.len() {
        return Err(CheckError::new(
            path,
            span,
            format!(
                "`{name}` takes {} type args, got {}",
                temp.type_params.len(),
                args.len()
            ),
        ));
    }
    let mut subst = HashMap::new();
    for (p, a) in temp.type_params.iter().zip(args.iter()) {
        subst.insert(p.name.clone(), *a);
    }
    // The generic's own module wins on a shared name (`Arena` in `arena.j3s`
    // must not replace `j3.memory.Arena` inside `List`).
    let mut merged_types = env
        .struct_temp_env
        .get(name)
        .cloned()
        .unwrap_or_else(|| env.types.clone());
    for (k, v) in env.types.iter() {
        merged_types.entry(k.clone()).or_insert(*v);
    }
    let sname = cx.intern_name(&temp.name.name);
    let id = cx.alloc_struct(StructDef {
        module: env.module.to_vec(),
        name: sname,
        doc: temp.doc.clone(),
        fields: Vec::new(),
        packed: temp.packed,
        debug: temp.debug,
        span: temp.span,
        ty_args: temp
            .type_params
            .iter()
            .zip(args.iter())
            .map(|(p, a)| (p.name.clone(), *a))
            .collect(),
        script: None,
    });
    env.struct_insts.insert(key, id);
    env.ir_structs.push(id);
    let mut fields = Vec::new();
    let mut seen = HashMap::new();
    for f in &temp.fields {
        if seen.contains_key(&f.name.name) {
            return Err(CheckError::new(
                path,
                f.name.span,
                format!("duplicate field `{}`", f.name.name),
            ));
        }
        seen.insert(f.name.name.clone(), ());
        let mut nested = env_reborrow(env, &subst, &merged_types);
        let fty = resolve_type_g(cx, &f.ty, path, &mut nested)?;
        reject_arena_field(cx, path, f.name.span, fty)?;
        let default = field_default(cx, path, fty, &f.ty, &temp.type_params, &f.default)?;
        fields.push(FieldDef {
            name: cx.intern_name(&f.name.name),
            doc: f.doc.clone(),
            ty: fty,
            embedded: f.embedded,
            default,
        });
    }
    cx.struct_def_mut(id).fields = fields;
    for f in &cx.struct_def(id).fields {
        if occurs_value(cx, f.ty, OccurRoot::Struct(id), &mut HashSet::new()) {
            return Err(CheckError::new(
                path,
                temp.span,
                format!(
                    "recursive field `{}` must go through `&` or `ptr`",
                    cx.name_str(f.name)
                ),
            ));
        }
    }
    for m in &temp.methods {
        let mkey = (id.0, m.name.name.clone());
        if !m.type_params.is_empty() {
            env.method_temps.insert(mkey, m.clone());
            continue;
        }
        let module = env.module;
        let mut nested = env_reborrow(env, &subst, &merged_types);
        let fid = alloc_fn(cx, path, module, m, Some(id), &HashMap::new(), &mut nested)?;
        env.methods.insert(mkey, fid);
        env.ir_funcs.push(fid);
        env.fn_substs.insert(fid, subst.clone());
        env.pending_mono.push((fid, m.clone(), subst.clone()));
    }
    Ok(cx.ty_struct(id))
}

fn env_reborrow<'a, 'b>(
    env: &'b mut GenEnv<'a>,
    subst: &'b HashMap<String, Ty>,
    types: &'b HashMap<String, Ty>,
) -> GenEnv<'b>
where
    'a: 'b,
{
    GenEnv {
        types,
        subst,
        enum_temps: env.enum_temps,
        enum_insts: env.enum_insts,
        struct_temps: env.struct_temps,
        struct_temp_env: env.struct_temp_env,
        struct_insts: env.struct_insts,
        methods: env.methods,
        method_temps: env.method_temps,
        method_insts: env.method_insts,
        fn_substs: env.fn_substs,
        pending_mono: env.pending_mono,
        module: env.module,
        ir_enums: env.ir_enums,
        ir_structs: env.ir_structs,
        ir_funcs: env.ir_funcs,
    }
}

fn resolve_type_ref(
    cx: &IrCx,
    ty: &Type,
    path: &str,
    types: &HashMap<String, Ty>,
) -> Result<Ty, CheckError> {
    match &ty.kind {
        TypeKind::Ref(inner) => {
            let inner = resolve_type_ref(cx, inner, path, types)?;
            match cx.lookup_ty(&TyKind::Ref(inner)) {
                Some(t) => Ok(t),
                None => Err(CheckError::new(path, ty.span, "unknown `&` type")),
            }
        }
        TypeKind::Named(name) => resolve_named(cx, name, ty.span, path, types),
        TypeKind::Generic { name, .. } => Err(CheckError::new(
            path,
            ty.span,
            format!("generic type `{name}` needs instantiation"),
        )),
        TypeKind::Array { len, elem } => {
            let elem = resolve_type_ref(cx, elem, path, types)?;
            match cx.lookup_ty(&TyKind::Array { elem, len: *len }) {
                Some(t) => Ok(t),
                None => Err(CheckError::new(path, ty.span, "unknown array type")),
            }
        }
        TypeKind::Slice(elem) => {
            let elem = resolve_type_ref(cx, elem, path, types)?;
            match cx.lookup_ty(&TyKind::Slice { elem }) {
                Some(t) => Ok(t),
                None => Err(CheckError::new(path, ty.span, "unknown slice type")),
            }
        }
        TypeKind::Fn { params, ret } => {
            let mut ps = Vec::new();
            for p in params {
                ps.push(resolve_type_ref(cx, p, path, types)?);
            }
            let r = match ret {
                Some(t) => resolve_type_ref(cx, t, path, types)?,
                None => cx.common.void,
            };
            match cx.lookup_ty(&TyKind::Fn { params: ps, ret: r }) {
                Some(t) => Ok(t),
                None => Err(CheckError::new(path, ty.span, "unknown fn type")),
            }
        }
    }
}

fn resolve_named(
    cx: &IrCx,
    name: &str,
    span: Span,
    path: &str,
    types: &HashMap<String, Ty>,
) -> Result<Ty, CheckError> {
    if let Some(t) = cx.builtin_named(name) {
        return Ok(t);
    }
    match types.get(name).copied() {
        Some(t) => Ok(t),
        None => Err(CheckError::new(
            path,
            span,
            format!("unknown type `{name}`"),
        )),
    }
}

fn field_default(
    cx: &IrCx,
    path: &str,
    ty: Ty,
    field_ty: &Type,
    params: &[crate::parse::Ident],
    default: &Option<Expr>,
) -> Result<Option<Operand>, CheckError> {
    let Some(expr) = default else {
        return Ok(None);
    };
    if mentions_type_param(field_ty, params) {
        return Err(CheckError::new(
            path,
            expr.span,
            "default on a generic field",
        ));
    }
    Ok(Some(literal_default(cx, path, ty, expr)?))
}

fn mentions_type_param(ty: &Type, params: &[crate::parse::Ident]) -> bool {
    match &ty.kind {
        TypeKind::Named(n) => params.iter().any(|p| p.name == *n),
        TypeKind::Generic { name, args } => {
            params.iter().any(|p| p.name == *name)
                || args.iter().any(|a| mentions_type_param(a, params))
        }
        TypeKind::Ref(inner) | TypeKind::Slice(inner) => mentions_type_param(inner, params),
        TypeKind::Array { elem, .. } => mentions_type_param(elem, params),
        TypeKind::Fn { params: ps, ret } => {
            ps.iter().any(|p| mentions_type_param(p, params))
                || ret.as_ref().is_some_and(|r| mentions_type_param(r, params))
        }
    }
}

fn literal_default(cx: &IrCx, path: &str, ty: Ty, expr: &Expr) -> Result<Operand, CheckError> {
    let mismatch = |span| CheckError::new(path, span, "field default type mismatch");
    let folded = expr.folded_literal();
    let expr = &folded;
    match &expr.kind {
        ExprKind::Bool(v) => {
            if ty != cx.common.bool {
                return Err(mismatch(expr.span));
            }
            Ok(Operand::bool_const(expr.span, ty, *v))
        }
        ExprKind::Int(v) => {
            if *v == 0 && matches!(cx.ty_kind(ty), TyKind::Ptr | TyKind::CStr | TyKind::Ref(_)) {
                return Ok(Operand::null(expr.span, ty));
            }
            if is_int(cx, ty) {
                return Ok(Operand::int(expr.span, ty, *v as u64));
            }
            if ty == cx.common.f32 {
                return Ok(Operand::f32_const(expr.span, ty, *v as f32));
            }
            if is_float(cx, ty) {
                return Ok(Operand::f64_const(expr.span, ty, *v as f64));
            }
            Err(mismatch(expr.span))
        }
        ExprKind::Float(v) => {
            if ty == cx.common.f32 {
                return Ok(Operand::f32_const(expr.span, ty, *v as f32));
            }
            if is_float(cx, ty) {
                return Ok(Operand::f64_const(expr.span, ty, *v));
            }
            Err(mismatch(expr.span))
        }
        ExprKind::String(bytes) if ty == cx.common.string => {
            Ok(Operand::bytes(expr.span, ty, bytes))
        }
        _ => Err(CheckError::new(
            path,
            expr.span,
            "field default must be a literal",
        )),
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

fn fold_global_init(
    cx: &IrCx,
    expr: &Expr,
    expected: Ty,
    types: &HashMap<String, Ty>,
    consts: &HashMap<String, ConstVal>,
    path: &str,
) -> Result<ConstInit, CheckError> {
    match &expr.kind {
        ExprKind::Int(v) => {
            if matches!(cx.ty_kind(expected), TyKind::Ptr | TyKind::CStr) && *v == 0 {
                return Ok(ConstInit::Scalar(Operand::null(expr.span, expected)));
            }
            if is_int(cx, expected) || expected == cx.common.int {
                return Ok(ConstInit::Scalar(Operand::int(
                    expr.span, expected, *v as u64,
                )));
            }
            if expected == cx.common.f32 {
                return Ok(ConstInit::Scalar(Operand::f32_const(
                    expr.span, expected, *v as f32,
                )));
            }
            if is_float(cx, expected) {
                return Ok(ConstInit::Scalar(Operand::f64_const(
                    expr.span, expected, *v as f64,
                )));
            }
            Err(CheckError::new(
                path,
                expr.span,
                "singleton init type mismatch",
            ))
        }
        ExprKind::Float(v) => {
            if expected == cx.common.f32 {
                Ok(ConstInit::Scalar(Operand::f32_const(
                    expr.span, expected, *v as f32,
                )))
            } else if is_float(cx, expected) {
                Ok(ConstInit::Scalar(Operand::f64_const(
                    expr.span, expected, *v,
                )))
            } else {
                Err(CheckError::new(
                    path,
                    expr.span,
                    "singleton init type mismatch",
                ))
            }
        }
        ExprKind::Bool(v) => {
            if expected != cx.common.bool {
                return Err(CheckError::new(
                    path,
                    expr.span,
                    "singleton init type mismatch",
                ));
            }
            Ok(ConstInit::Scalar(Operand::bool_const(
                expr.span, expected, *v,
            )))
        }
        ExprKind::Name(n) => match consts.get(&n.name) {
            Some(ConstVal {
                kind: ConstPayload::Int(v),
                ..
            }) => fold_global_init(
                cx,
                &Expr {
                    span: n.span,
                    kind: ExprKind::Int(*v),
                },
                expected,
                types,
                consts,
                path,
            ),
            _ => Err(CheckError::new(
                path,
                n.span,
                "singleton init must be a constant",
            )),
        },
        ExprKind::Unary {
            op: parse::UnaryOp::Neg,
            expr: inner,
        } => match fold_global_init(cx, inner, expected, types, consts, path)? {
            ConstInit::Scalar(Operand::Const(c)) => match c.kind {
                crate::ir::operand::ConstKind::Int(bits) => {
                    let neg = (bits as i64).wrapping_neg();
                    Ok(ConstInit::Scalar(Operand::int(
                        expr.span, expected, neg as u64,
                    )))
                }
                _ => Err(CheckError::new(
                    path,
                    expr.span,
                    "singleton init must be a constant",
                )),
            },
            _ => Err(CheckError::new(
                path,
                expr.span,
                "singleton init must be a constant",
            )),
        },
        ExprKind::StructLit { name, fields, .. } => {
            let sid = match cx.ty_kind(expected) {
                TyKind::Struct(id) => *id,
                _ => {
                    return Err(CheckError::new(
                        path,
                        name.span,
                        "singleton init type mismatch",
                    ));
                }
            };
            let def = cx.struct_def(sid);
            let mut out = Vec::new();
            for f in &def.fields {
                let fname = cx.name_str(f.name).to_string();
                let found = fields.iter().find(|(n, _)| n.name == fname);
                let val = match found {
                    Some((_, e)) => fold_global_init(cx, e, f.ty, types, consts, path)?,
                    None => zero_init_for(cx, f.ty, expr.span),
                };
                out.push((fname, val));
            }
            Ok(ConstInit::Fields(out))
        }
        ExprKind::ArrayLit { elems } => {
            let (elem_ty, len) = match cx.ty_kind(expected) {
                TyKind::Array { elem, len } => (*elem, *len),
                _ => {
                    return Err(CheckError::new(
                        path,
                        expr.span,
                        "singleton init type mismatch",
                    ));
                }
            };
            if elems.len() as u64 != len {
                return Err(CheckError::new(
                    path,
                    expr.span,
                    "singleton array length mismatch",
                ));
            }
            let mut out = Vec::new();
            for e in elems {
                out.push(fold_global_init(cx, e, elem_ty, types, consts, path)?);
            }
            Ok(ConstInit::Elems(out))
        }
        ExprKind::Call { .. } | ExprKind::Syscall { .. } => Err(CheckError::new(
            path,
            expr.span,
            "singleton init cannot call",
        )),
        _ => Err(CheckError::new(
            path,
            expr.span,
            "singleton init must be a constant",
        )),
    }
}

fn zero_init_for(cx: &IrCx, ty: Ty, span: Span) -> ConstInit {
    if matches!(cx.ty_kind(ty), TyKind::Ptr | TyKind::CStr | TyKind::Ref(_)) {
        return ConstInit::Scalar(Operand::null(span, ty));
    }
    if ty == cx.common.bool {
        return ConstInit::Scalar(Operand::bool_const(span, ty, false));
    }
    if is_int(cx, ty) {
        return ConstInit::Scalar(Operand::int(span, ty, 0));
    }
    if ty == cx.common.f32 {
        return ConstInit::Scalar(Operand::f32_const(span, ty, 0.0));
    }
    if is_float(cx, ty) {
        return ConstInit::Scalar(Operand::f64_const(span, ty, 0.0));
    }
    if let TyKind::Struct(id) = cx.ty_kind(ty) {
        let mut fields = Vec::new();
        for f in &cx.struct_def(*id).fields {
            fields.push((
                cx.name_str(f.name).to_string(),
                zero_init_for(cx, f.ty, span),
            ));
        }
        return ConstInit::Fields(fields);
    }
    if let TyKind::Array { elem, len } = cx.ty_kind(ty) {
        let z = zero_init_for(cx, *elem, span);
        return ConstInit::Elems(vec![z; *len as usize]);
    }
    ConstInit::Scalar(Operand::int(span, ty, 0))
}

fn const_init_is_zero(init: &ConstInit) -> bool {
    match init {
        ConstInit::Scalar(Operand::Const(c)) => match &c.kind {
            crate::ir::operand::ConstKind::Int(0) => true,
            crate::ir::operand::ConstKind::Bool(false) => true,
            crate::ir::operand::ConstKind::Null => true,
            crate::ir::operand::ConstKind::F32Bits(0) => true,
            crate::ir::operand::ConstKind::F64Bits(0) => true,
            _ => false,
        },
        ConstInit::Scalar(_) => false,
        ConstInit::Fields(fs) => fs.iter().all(|(_, v)| const_init_is_zero(v)),
        ConstInit::Elems(es) => es.iter().all(const_init_is_zero),
    }
}

fn eval_const(
    expr: &parse::Expr,
    consts: &HashMap<String, ConstVal>,
    cx: &IrCx,
    path: &str,
) -> Result<ConstVal, CheckError> {
    match &expr.kind {
        parse::ExprKind::String(b) => Ok(ConstVal {
            ty: cx.common.string,
            kind: ConstPayload::String(b.clone()),
        }),
        parse::ExprKind::Int(v) => Ok(ConstVal {
            ty: cx.common.int,
            kind: ConstPayload::Int(*v),
        }),
        parse::ExprKind::Float(v) => Ok(ConstVal {
            ty: cx.common.float,
            kind: ConstPayload::Float(*v),
        }),
        parse::ExprKind::Name(n) => match consts.get(&n.name) {
            Some(c) => Ok(c.clone()),
            None => Err(CheckError::new(
                path,
                n.span,
                format!("unknown const `{}`", n.name),
            )),
        },
        parse::ExprKind::Binary { op, lhs, rhs } => {
            let a = eval_int(lhs, consts, path)?;
            let b = eval_int(rhs, consts, path)?;
            let v = match fold_const_bin(*op, a, b, path, expr.span)? {
                Some(v) => v,
                None => {
                    return Err(CheckError::new(
                        path,
                        expr.span,
                        "const initializer must be an integer expression",
                    ));
                }
            };
            Ok(ConstVal {
                ty: cx.common.int,
                kind: ConstPayload::Int(v),
            })
        }
        parse::ExprKind::Unary {
            op: parse::UnaryOp::Neg,
            expr,
        } => Ok(ConstVal {
            ty: cx.common.int,
            kind: ConstPayload::Int(eval_int(expr, consts, path)?.wrapping_neg()),
        }),
        _ => Err(CheckError::new(
            path,
            expr.span,
            "const initializer must be an integer or string",
        )),
    }
}

fn eval_int(
    expr: &parse::Expr,
    consts: &HashMap<String, ConstVal>,
    path: &str,
) -> Result<i64, CheckError> {
    match &expr.kind {
        parse::ExprKind::Int(v) => Ok(*v),
        parse::ExprKind::Name(n) => match consts.get(&n.name) {
            Some(ConstVal {
                kind: ConstPayload::Int(v),
                ..
            }) => Ok(*v),
            Some(_) => Err(CheckError::new(
                path,
                n.span,
                format!("`{}` is not an int const", n.name),
            )),
            None => Err(CheckError::new(
                path,
                n.span,
                format!("unknown const `{}`", n.name),
            )),
        },
        parse::ExprKind::Binary { op, lhs, rhs } => {
            let a = eval_int(lhs, consts, path)?;
            let b = eval_int(rhs, consts, path)?;
            match fold_const_bin(*op, a, b, path, expr.span)? {
                Some(v) => Ok(v),
                None => Err(CheckError::new(
                    path,
                    expr.span,
                    "const initializer must be an integer expression",
                )),
            }
        }
        parse::ExprKind::Unary {
            op: parse::UnaryOp::Neg,
            expr,
        } => Ok(eval_int(expr, consts, path)?.wrapping_neg()),
        _ => Err(CheckError::new(
            path,
            expr.span,
            "const initializer must be an integer expression",
        )),
    }
}
