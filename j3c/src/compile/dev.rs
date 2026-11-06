//! Dev mode: the native program never knows a script's layout or code.
//!
//! After check (and after the scripts are printed as bytecode), this pass rewrites the IR:
//! - every script struct becomes `{ handle: ptr }`, the VM instance's address;
//! - every script literal in native code becomes `j3.script.devNew(path, props)` with only the
//!   props the literal names (defaults stay in the bytecode, so adding one needs no rebuild);
//! - every `pub` script fn keeps its signature but its body calls `j3.script.devCall`;
//!   private script fns and `shared` values leave the native program;
//! - a `j3s_dev` module gets one thunk per native function a script may call, and `main`
//!   first registers them all.
//!
//! The native IR then depends only on `.j3` sources and script `pub` signatures, which is
//! what the dev build's cache key hashes.

use std::collections::{HashMap, HashSet};

use crate::check::Program;
use crate::emit::bc::{self, BcModule};
use crate::intern::Name;
use crate::ir::body::{Block, Body, LocalDecl, LocalKind};
use crate::ir::cx::IrCx;
use crate::ir::entity::{FieldDef, Func, FuncKind, Vis};
use crate::ir::ids::{FieldId, FuncId, LocalId, StructId};
use crate::ir::module::Module;
use crate::ir::op::BinOp;
use crate::ir::operand::Operand;
use crate::ir::place::{Access, Place, PlaceRoot, Proj};
use crate::ir::rvalue::{Rvalue, RvalueKind};
use crate::ir::stmt::{Stmt, StmtKind};
use crate::ir::ty::{IntTy, Ty, TyKind};
use crate::span::Span;

mod build;
mod literals;
mod thunk;

use build::*;
use literals::*;
use thunk::*;

pub const DEV_MODULE: &str = "j3s_dev";

/// Print every script module in the program as bytecode (from the checked IR).
pub fn bytecode(prog: &Program) -> Result<Vec<BcModule>, String> {
    let mut out = Vec::new();
    for module in &prog.modules {
        if !bc::is_script_module(&prog.cx, &module.name) {
            continue;
        }
        if !module
            .structs
            .iter()
            .any(|id| prog.cx.struct_def(*id).script.is_some())
        {
            continue;
        }
        out.push(bc::emit_script(&prog.cx, module)?);
    }
    Ok(out)
}

/// True when the program has at least one script module.
pub fn has_scripts(prog: &Program) -> bool {
    prog.modules
        .iter()
        .any(|m| bc::is_script_module(&prog.cx, &m.name))
}

struct Runtime {
    dev_new: FuncId,
    dev_call: FuncId,
    dev_register: FuncId,
    prop: StructId,
    arg: StructId,
}

struct ScriptInfo {
    path: String,
    file: crate::span::FileId,
    props: Vec<(String, Ty)>,
}

/// Rewrite `prog` for a dev build. `bytecode` is the scripts, printed before this runs.
/// Returns how many native thunks the program links.
pub fn lower(prog: &mut Program, bytecode: &[BcModule]) -> Result<usize, String> {
    let rt = runtime(&prog.cx)?;
    let mut scripts: HashMap<StructId, ScriptInfo> = HashMap::new();
    for (i, def) in prog.cx.structs().iter().enumerate() {
        let Some(meta) = &def.script else { continue };
        let props = def
            .fields
            .iter()
            .take(meta.prop_spans.len())
            .map(|f| (prog.cx.name_str(f.name).to_string(), f.ty))
            .collect();
        scripts.insert(
            StructId(i as u32),
            ScriptInfo {
                path: bc::script_path(&prog.cx, &def.module),
                file: def.span.file,
                props,
            },
        );
    }
    let called: HashSet<u64> = bytecode
        .iter()
        .flat_map(|m| m.natives.iter().map(|(k, _)| *k))
        .collect();

    let mut dev = Module::new(vec![prog.cx.intern_name(DEV_MODULE)]);
    let mut stubs = Vec::new();
    for module in &mut prog.modules {
        if !bc::is_script_module(&prog.cx, &module.name) {
            continue;
        }
        let own = module.name.clone();
        let mut keep = Vec::new();
        for id in &module.funcs {
            let func = prog.cx.func(*id);
            if func.module != own {
                // A native generic instantiated for this script: still native code.
                dev.funcs.push(*id);
                continue;
            }
            let script_method = func.method_of.is_some_and(|s| scripts.contains_key(&s));
            if script_method && func.vis == Vis::Pub {
                keep.push(*id);
                stubs.push(*id);
            }
        }
        module.funcs = keep;
        module.globals.clear();
    }

    // Thunks: everything a script calls now, plus every `pub` fn of a module a script
    // `use`s, so most script edits can call something new without a rebuild.
    let used = used_modules(prog);
    let mut thunk_targets = Vec::new();
    for module in prog.modules.iter().chain(std::iter::once(&dev)) {
        if bc::is_script_module(&prog.cx, &module.name) {
            continue;
        }
        let module_used = used.contains(&module_key(&prog.cx, &module.name));
        for id in &module.funcs {
            let func = prog.cx.func(*id);
            if matches!(func.kind, FuncKind::Decl) {
                continue;
            }
            let key = bc::fn_key(&prog.cx, *id);
            let wanted = called.contains(&key);
            let open = module_used && func.vis == Vis::Pub && prog.cx.func(*id).module == module.name;
            if !open && !wanted {
                continue;
            }
            if mentions_script(&prog.cx, func.ty, &scripts) {
                if wanted {
                    return Err(format!(
                        "a script calls `{}`, which takes or returns a script instance; \
                         dev builds can't pass those to native code yet",
                        bc::fn_label(&prog.cx, *id)
                    ));
                }
                continue;
            }
            thunk_targets.push(*id);
        }
    }
    for key in &called {
        if !thunk_targets
            .iter()
            .any(|id| bc::fn_key(&prog.cx, *id) == *key)
        {
            let name = bytecode
                .iter()
                .flat_map(|m| m.natives.iter())
                .find(|(k, _)| k == key)
                .map(|(_, n)| n.clone())
                .unwrap_or_default();
            return Err(format!("a script calls `{name}`, which the program can't link"));
        }
    }

    for id in stubs {
        stub(&mut prog.cx, &rt, &scripts, id)?;
    }
    let native_funcs: Vec<FuncId> = prog
        .modules
        .iter()
        .filter(|m| !bc::is_script_module(&prog.cx, &m.name))
        .flat_map(|m| m.funcs.iter().copied())
        .chain(dev.funcs.iter().copied())
        .collect();
    for id in native_funcs {
        rewrite_literals(&mut prog.cx, &rt, &scripts, id)?;
    }
    let ptr = prog.cx.common.ptr;
    for sid in scripts.keys() {
        let handle = prog.cx.intern_name("handle");
        let def = prog.cx.struct_def_mut(*sid);
        def.fields = vec![FieldDef {
            name: handle,
            doc: String::new(),
            ty: ptr,
            embedded: false,
            default: None,
        }];
        if let Some(meta) = def.script.as_mut() {
            meta.prop_spans.clear();
        }
    }

    let mut register = Fb::new(&prog.cx);
    let thunk_count = thunk_targets.len();
    for target in thunk_targets {
        let thunk = thunk(&mut prog.cx, target)?;
        dev.funcs.push(thunk);
        let key = bc::fn_key(&prog.cx, target);
        let u64_ty = prog.cx.intern_ty(TyKind::Int(IntTy::U64));
        let fn_ty = prog.cx.func(thunk).ty;
        let callee = prog.cx.func(rt.dev_register).ty;
        register.expr(Rvalue::call(
            Span::dummy(),
            prog.cx.common.void,
            Operand::fn_addr(Span::dummy(), callee, rt.dev_register),
            vec![
                Operand::int(Span::dummy(), u64_ty, key),
                Operand::fn_addr(Span::dummy(), fn_ty, thunk),
            ],
        ));
    }
    let register_ty = prog.cx.ty_fn(Vec::new(), prog.cx.common.void);
    let register_id = add_func(
        &mut prog.cx,
        &dev.name,
        "register",
        register_ty,
        register.finish(),
    );
    dev.funcs.push(register_id);
    let main = find_main(prog).ok_or("dev build: no `main` to register script thunks from")?;
    let void = prog.cx.common.void;
    if let FuncKind::J3 { body } = &mut prog.cx.func_mut(main).kind {
        body.block.stmts.insert(
            0,
            Stmt::expr(
                Span::dummy(),
                void,
                Rvalue::call(
                    Span::dummy(),
                    void,
                    Operand::fn_addr(Span::dummy(), register_ty, register_id),
                    Vec::new(),
                ),
            ),
        );
    }
    prog.modules.push(dev);
    Ok(thunk_count)
}

fn module_key(cx: &IrCx, name: &[Name]) -> String {
    name.iter().map(|n| cx.name_str(*n)).collect::<Vec<_>>().join(".")
}

/// Modules the scripts name in `use` lines (the last part may be an item, not a module).
fn used_modules(prog: &Program) -> HashSet<String> {
    let modules: HashSet<String> = prog
        .modules
        .iter()
        .map(|m| module_key(&prog.cx, &m.name))
        .collect();
    let mut out = HashSet::new();
    for file in &prog.cx.files {
        if !file.path.ends_with(".j3s") {
            continue;
        }
        let Ok(ast) = crate::parse::parse_script(&file.src, crate::span::FileId(0), &["script".to_string()]) else {
            continue;
        };
        for item in &ast.items {
            let crate::parse::Item::Use(u) = item else { continue };
            let path: Vec<String> = u.path.iter().map(|p| p.name.clone()).collect();
            let full = path.join(".");
            if modules.contains(&full) {
                out.insert(full);
            } else if path.len() > 1 {
                out.insert(path[..path.len() - 1].join("."));
            }
        }
    }
    out
}

fn runtime(cx: &IrCx) -> Result<Runtime, String> {
    let module_is = |m: &[Name]| {
        m.len() == 2 && cx.name_str(m[0]) == "j3" && cx.name_str(m[1]) == "script"
    };
    let func = |name: &str| {
        cx.funcs()
            .iter()
            .find(|f| module_is(&f.module) && cx.name_str(f.name) == name && f.method_of.is_none())
            .map(|f| f.id)
            .ok_or_else(|| format!("dev build: j3.script has no `{name}`"))
    };
    let strukt = |name: &str| {
        cx.structs()
            .iter()
            .position(|s| module_is(&s.module) && cx.name_str(s.name) == name)
            .map(|i| StructId(i as u32))
            .ok_or_else(|| format!("dev build: j3.script has no `{name}`"))
    };
    Ok(Runtime {
        dev_new: func("devNew")?,
        dev_call: func("devCall")?,
        dev_register: func("devRegister")?,
        prop: strukt("Prop")?,
        arg: strukt("Arg")?,
    })
}

fn find_main(prog: &Program) -> Option<FuncId> {
    let first = prog.modules.first()?;
    first
        .funcs
        .iter()
        .copied()
        .find(|id| prog.cx.name_str(prog.cx.func(*id).name) == "main")
}

fn script_of(cx: &IrCx, ty: Ty, scripts: &HashMap<StructId, ScriptInfo>) -> Option<StructId> {
    match cx.ty_kind(ty) {
        TyKind::Struct(sid) if scripts.contains_key(sid) => Some(*sid),
        _ => None,
    }
}

/// `&Door` for a script struct `Door`.
fn script_ref(cx: &IrCx, ty: Ty, scripts: &HashMap<StructId, ScriptInfo>) -> Option<StructId> {
    cx.ref_inner(ty).and_then(|inner| script_of(cx, inner, scripts))
}

fn mentions_script(cx: &IrCx, ty: Ty, scripts: &HashMap<StructId, ScriptInfo>) -> bool {
    let mut seen = HashSet::new();
    mentions(cx, ty, scripts, &mut seen)
}

fn mentions(
    cx: &IrCx,
    ty: Ty,
    scripts: &HashMap<StructId, ScriptInfo>,
    seen: &mut HashSet<Ty>,
) -> bool {
    if !seen.insert(ty) {
        return false;
    }
    match cx.ty_kind(ty).clone() {
        TyKind::Struct(sid) => {
            scripts.contains_key(&sid)
                || cx
                    .struct_def(sid)
                    .fields
                    .iter()
                    .any(|f| mentions(cx, f.ty, scripts, seen))
        }
        TyKind::Enum(eid) => cx
            .enum_def(eid)
            .variants
            .iter()
            .any(|v| v.payload.is_some_and(|p| mentions(cx, p, scripts, seen))),
        TyKind::Ref(inner) | TyKind::Array { elem: inner, .. } | TyKind::Slice { elem: inner } => {
            mentions(cx, inner, scripts, seen)
        }
        TyKind::Fn { params, ret } => {
            params.iter().any(|p| mentions(cx, *p, scripts, seen)) || mentions(cx, ret, scripts, seen)
        }
        _ => false,
    }
}

/// Everything the dev binary depends on besides `.j3` sources: the lowered native IR.
pub fn native_key(prog: &Program) -> String {
    let native: Vec<Module> = prog.modules.to_vec();
    crate::ir::dump::dump_program(&prog.cx, &native)
}
