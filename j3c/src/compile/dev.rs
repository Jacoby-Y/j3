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

/// A tiny function-body builder: locals, then statements.
struct Fb {
    locals: Vec<LocalDecl>,
    params: Vec<LocalId>,
    stmts: Vec<Stmt>,
    void: Ty,
    next: u32,
}

impl Fb {
    fn new(cx: &IrCx) -> Fb {
        Fb {
            locals: Vec::new(),
            params: Vec::new(),
            stmts: Vec::new(),
            void: cx.common.void,
            next: 0,
        }
    }

    fn with_locals(cx: &IrCx, body: &Body) -> Fb {
        let mut fb = Fb::new(cx);
        fb.locals = body.locals.clone();
        fb.next = body.locals.len() as u32;
        fb
    }

    fn local(&mut self, name: Name, ty: Ty, kind: LocalKind) -> Place {
        let id = LocalId(self.locals.len() as u32);
        self.locals.push(LocalDecl {
            id,
            doc: String::new(),
            name,
            ty,
            access: Access::let_binding(),
            kind,
            span: Span::dummy(),
        });
        if kind == LocalKind::Param {
            self.params.push(id);
        }
        self.next += 1;
        Place::local(id, ty, Access::let_binding(), Span::dummy())
    }

    fn tmp(&mut self, cx: &mut IrCx, ty: Ty) -> Place {
        let name = cx.intern_name(&format!("_dev{}", self.next));
        self.local(name, ty, LocalKind::Tmp)
    }

    fn assign(&mut self, place: Place, rvalue: Rvalue) {
        self.stmts.push(Stmt::assign(Span::dummy(), self.void, place, rvalue));
    }

    fn expr(&mut self, rvalue: Rvalue) {
        self.stmts.push(Stmt::expr(Span::dummy(), self.void, rvalue));
    }

    /// Store `rvalue` in a new temp and return a copy of it.
    fn value(&mut self, cx: &mut IrCx, rvalue: Rvalue) -> Operand {
        let place = self.tmp(cx, rvalue.ty);
        self.assign(place.clone(), rvalue);
        Operand::Copy(place)
    }

    /// `place as ptr`, via `&place`.
    fn address(&mut self, cx: &mut IrCx, place: Place) -> Operand {
        let ref_ty = cx.ty_ref(place.ty);
        let r = self.value(cx, Rvalue::ref_place(Span::dummy(), ref_ty, place));
        self.value(cx, Rvalue::cast(Span::dummy(), cx.common.ptr, r))
    }

    fn finish(self) -> Body {
        Body {
            locals: self.locals,
            params: self.params,
            block: Block { stmts: self.stmts },
        }
    }
}

fn deref(cx: &IrCx, place: Place) -> Place {
    let inner = cx.ref_inner(place.ty).expect("deref of a non-ref");
    place.project(Proj::Deref, inner, Access::through_ref(), Span::dummy())
}

fn handle_of(cx: &IrCx, script_place: Place, sid: StructId) -> Place {
    let ptr = cx.common.ptr;
    script_place.project(
        Proj::Field {
            field: FieldId {
                struct_id: sid,
                index: 0,
            },
            ty: ptr,
        },
        ptr,
        Access::read_only_view(),
        Span::dummy(),
    )
}

fn string_const(cx: &IrCx, text: &str) -> Operand {
    Operand::bytes(Span::dummy(), cx.common.string, text.as_bytes())
}

/// A `[]T` slice over `items` (each an operand of type `elem`), built in a stack array.
fn slice_of(cx: &mut IrCx, fb: &mut Fb, elem: Ty, items: Vec<Operand>) -> Operand {
    let slice_ty = cx.ty_slice(elem);
    let ref_ty = cx.ty_ref(elem);
    let int = cx.common.int;
    let n = items.len() as u64;
    let data = if items.is_empty() {
        Operand::null(Span::dummy(), ref_ty)
    } else {
        let array_ty = cx.ty_array(elem, n);
        let array = fb.tmp(cx, array_ty);
        fb.assign(
            array.clone(),
            Rvalue::aggregate(Span::dummy(), array_ty, items),
        );
        let first = array.project(
            Proj::Index {
                index: Operand::int(Span::dummy(), int, 0),
                ty: elem,
            },
            elem,
            Access::let_binding(),
            Span::dummy(),
        );
        fb.value(cx, Rvalue::ref_place(Span::dummy(), ref_ty, first))
    };
    fb.value(
        cx,
        Rvalue::aggregate(
            Span::dummy(),
            slice_ty,
            vec![data, Operand::int(Span::dummy(), int, n)],
        ),
    )
}

/// Pass `value` (of type `ty`) to the VM: its address, or for `&Door`, the address of the
/// VM instance pointer the handle holds.
fn vm_arg(
    cx: &mut IrCx,
    fb: &mut Fb,
    scripts: &HashMap<StructId, ScriptInfo>,
    value: Operand,
    ty: Ty,
) -> Result<(Operand, u64), String> {
    if let Some(sid) = script_ref(cx, ty, scripts) {
        let Operand::Copy(place) = value else {
            return Err("dev build: a script reference must be a place".to_string());
        };
        let handle = handle_of(cx, deref(cx, place), sid);
        let slot = fb.value(cx, Rvalue::use_op(Span::dummy(), Operand::Copy(handle)));
        let Operand::Copy(slot) = slot else { unreachable!() };
        return Ok((fb.address(cx, slot), 8));
    }
    if mentions_script(cx, ty, scripts) {
        return Err(format!(
            "dev builds pass a script instance only as `&{}` (found `{}`)",
            "Script",
            crate::ir::dump::ty_string(cx, ty)
        ));
    }
    let slot = match value {
        Operand::Copy(place) if matches!(place.root, PlaceRoot::Local(_)) && place.proj.is_empty() => {
            place
        }
        other => {
            let copied = fb.value(cx, Rvalue::use_op(Span::dummy(), other));
            let Operand::Copy(place) = copied else { unreachable!() };
            place
        }
    };
    let size = crate::ir::abi::size(cx, ty);
    Ok((fb.address(cx, slot), size))
}

/// Replace a `pub` script fn's body with a call into the VM.
fn stub(
    cx: &mut IrCx,
    rt: &Runtime,
    scripts: &HashMap<StructId, ScriptInfo>,
    id: FuncId,
) -> Result<(), String> {
    let key = bc::fn_key(cx, id);
    let func = cx.func(id).clone();
    let FuncKind::J3 { body } = &func.kind else {
        return Ok(());
    };
    let (_, ret) = cx.fn_sig(func.ty).map(|(p, r)| (p.clone(), r)).ok_or("stub signature")?;
    if mentions_script(cx, ret, scripts) {
        return Err(format!(
            "`{}` returns a script instance; dev builds can't return those to native code yet",
            bc::fn_label(cx, id)
        ));
    }
    let mut fb = Fb::new(cx);
    let mut params = Vec::new();
    for pid in &body.params {
        let decl = body.local(*pid);
        params.push(fb.local(decl.name, decl.ty, LocalKind::Param));
    }
    let arg_ty = cx.ty_struct(rt.arg);
    let int = cx.common.int;
    let mut args = Vec::new();
    for param in params.iter().skip(1) {
        let (value, size) = vm_arg(cx, &mut fb, scripts, Operand::Copy(param.clone()), param.ty)?;
        args.push(fb.value(
            cx,
            Rvalue::aggregate(
                Span::dummy(),
                arg_ty,
                vec![value, Operand::int(Span::dummy(), int, size)],
            ),
        ));
    }
    let args = slice_of(cx, &mut fb, arg_ty, args);
    let this_sid = script_ref(cx, params[0].ty, scripts).ok_or("stub without `this`")?;
    let handle = handle_of(cx, deref(cx, params[0].clone()), this_sid);
    let handle = fb.value(cx, Rvalue::use_op(Span::dummy(), Operand::Copy(handle)));
    let void = cx.common.void;
    let (result_ptr, result) = if ret == void {
        (Operand::null(Span::dummy(), cx.common.ptr), None)
    } else {
        let slot = fb.tmp(cx, ret);
        (fb.address(cx, slot.clone()), Some(slot))
    };
    let u64_ty = cx.intern_ty(TyKind::Int(IntTy::U64));
    let callee = cx.func(rt.dev_call).ty;
    fb.expr(Rvalue::call(
        Span::dummy(),
        void,
        Operand::fn_addr(Span::dummy(), callee, rt.dev_call),
        vec![
            Operand::int(Span::dummy(), u64_ty, key),
            handle,
            args,
            result_ptr,
        ],
    ));
    fb.stmts.push(Stmt::return_value(
        Span::dummy(),
        void,
        result.map(Operand::Copy),
    ));
    let body = fb.finish();
    let f = cx.func_mut(id);
    f.kind = FuncKind::J3 { body };
    f.span = Span::dummy();
    Ok(())
}

/// In a native fn, turn every script literal into `devNew(path, props)`.
fn rewrite_literals(
    cx: &mut IrCx,
    rt: &Runtime,
    scripts: &HashMap<StructId, ScriptInfo>,
    id: FuncId,
) -> Result<(), String> {
    let FuncKind::J3 { body } = &cx.func(id).kind else {
        return Ok(());
    };
    if !block_has_literal(cx, &body.block, scripts) {
        return Ok(());
    }
    let body = body.clone();
    let mut fb = Fb::with_locals(cx, &body);
    fb.params = body.params.clone();
    let block = rewrite_block(cx, rt, scripts, &mut fb, body.block)?;
    let new_body = Body {
        locals: fb.locals,
        params: fb.params,
        block,
    };
    cx.func_mut(id).kind = FuncKind::J3 { body: new_body };
    Ok(())
}

fn block_has_literal(cx: &IrCx, block: &Block, scripts: &HashMap<StructId, ScriptInfo>) -> bool {
    block.stmts.iter().any(|stmt| match &stmt.kind {
        StmtKind::Assign { rvalue, .. } => {
            matches!(rvalue.kind, RvalueKind::Aggregate { .. })
                && script_of(cx, rvalue.ty, scripts).is_some()
        }
        StmtKind::Defer { body } | StmtKind::Block(body) | StmtKind::While { body, .. } => {
            block_has_literal(cx, body, scripts)
        }
        StmtKind::ForRange { body, .. } => block_has_literal(cx, body, scripts),
        StmtKind::If {
            then_block,
            else_block,
            ..
        } => {
            block_has_literal(cx, then_block, scripts)
                || else_block
                    .as_ref()
                    .is_some_and(|b| block_has_literal(cx, b, scripts))
        }
        _ => false,
    })
}

fn rewrite_block(
    cx: &mut IrCx,
    rt: &Runtime,
    scripts: &HashMap<StructId, ScriptInfo>,
    fb: &mut Fb,
    block: Block,
) -> Result<Block, String> {
    let mut out = Vec::new();
    for mut stmt in block.stmts {
        match stmt.kind {
            StmtKind::Assign { place, rvalue } => {
                let sid = match (&rvalue.kind, script_of(cx, rvalue.ty, scripts)) {
                    (RvalueKind::Aggregate { .. }, Some(sid)) => sid,
                    _ => {
                        out.push(Stmt {
                            kind: StmtKind::Assign { place, rvalue },
                            ..stmt
                        });
                        continue;
                    }
                };
                let RvalueKind::Aggregate { fields } = rvalue.kind else { unreachable!() };
                let saved = std::mem::take(&mut fb.stmts);
                let handle = new_instance(cx, rt, scripts, fb, sid, fields)?;
                let mut made = std::mem::replace(&mut fb.stmts, saved);
                out.append(&mut made);
                out.push(Stmt::assign(
                    stmt.span,
                    stmt.ty,
                    place,
                    Rvalue::aggregate(rvalue.span, rvalue.ty, vec![handle]),
                ));
            }
            StmtKind::Defer { body } => {
                stmt.kind = StmtKind::Defer {
                    body: rewrite_block(cx, rt, scripts, fb, body)?,
                };
                out.push(stmt);
            }
            StmtKind::Block(body) => {
                stmt.kind = StmtKind::Block(rewrite_block(cx, rt, scripts, fb, body)?);
                out.push(stmt);
            }
            StmtKind::While { cond, body } => {
                stmt.kind = StmtKind::While {
                    cond,
                    body: rewrite_block(cx, rt, scripts, fb, body)?,
                };
                out.push(stmt);
            }
            StmtKind::ForRange { index, end, body } => {
                stmt.kind = StmtKind::ForRange {
                    index,
                    end,
                    body: rewrite_block(cx, rt, scripts, fb, body)?,
                };
                out.push(stmt);
            }
            StmtKind::If {
                cond,
                then_block,
                else_block,
            } => {
                let then_block = rewrite_block(cx, rt, scripts, fb, then_block)?;
                let else_block = match else_block {
                    Some(b) => Some(rewrite_block(cx, rt, scripts, fb, b)?),
                    None => None,
                };
                stmt.kind = StmtKind::If {
                    cond,
                    then_block,
                    else_block,
                };
                out.push(stmt);
            }
            kind => {
                stmt.kind = kind;
                out.push(stmt);
            }
        }
    }
    Ok(Block { stmts: out })
}

/// Emit `devNew(path, [props the literal named])` and return the handle operand.
fn new_instance(
    cx: &mut IrCx,
    rt: &Runtime,
    scripts: &HashMap<StructId, ScriptInfo>,
    fb: &mut Fb,
    sid: StructId,
    fields: Vec<Operand>,
) -> Result<Operand, String> {
    let info = &scripts[&sid];
    let path = info.path.clone();
    let script_file = info.file;
    let props = info.props.clone();
    let prop_ty = cx.ty_struct(rt.prop);
    let int = cx.common.int;
    let mut items = Vec::new();
    for (i, (name, ty)) in props.iter().enumerate() {
        let Some(value) = fields.get(i).cloned() else { continue };
        if operand_span(&value).file == script_file {
            // The script's own default; the VM fills it from the bytecode.
            continue;
        }
        let (addr, size) = vm_arg(cx, fb, scripts, value, *ty)?;
        let name = string_const(cx, name);
        items.push(fb.value(
            cx,
            Rvalue::aggregate(
                Span::dummy(),
                prop_ty,
                vec![name, addr, Operand::int(Span::dummy(), int, size)],
            ),
        ));
    }
    let props = slice_of(cx, fb, prop_ty, items);
    let callee = cx.func(rt.dev_new).ty;
    let path = string_const(cx, &path);
    Ok(fb.value(
        cx,
        Rvalue::call(
            Span::dummy(),
            cx.common.ptr,
            Operand::fn_addr(Span::dummy(), callee, rt.dev_new),
            vec![path, props],
        ),
    ))
}

fn operand_span(op: &Operand) -> Span {
    match op {
        Operand::Const(c) => c.span,
        Operand::Copy(place) => place.span,
    }
}

/// `fn(args: ptr, ret: ptr)` that reads each argument from an 8-byte-aligned slot in
/// `args`, calls `target`, and writes the result to `ret`. Same slots `Vm.callNative` packs.
fn thunk(cx: &mut IrCx, target: FuncId) -> Result<FuncId, String> {
    let func = cx.func(target).clone();
    let (params, ret) = cx
        .fn_sig(func.ty)
        .map(|(p, r)| (p.clone(), r))
        .ok_or("thunk target without a signature")?;
    let ptr = cx.common.ptr;
    let int = cx.common.int;
    let void = cx.common.void;
    let mut fb = Fb::new(cx);
    let args_name = cx.intern_name("args");
    let ret_name = cx.intern_name("ret");
    let args = fb.local(args_name, ptr, LocalKind::Param);
    let out = fb.local(ret_name, ptr, LocalKind::Param);
    let base = fb.value(cx, Rvalue::cast(Span::dummy(), int, Operand::Copy(args)));
    let mut call_args = Vec::new();
    let mut offset = 0u64;
    for ty in &params {
        let at = fb.value(
            cx,
            Rvalue::binary(
                Span::dummy(),
                int,
                BinOp::Add,
                base.clone(),
                Operand::int(Span::dummy(), int, offset),
            ),
        );
        let raw = fb.value(cx, Rvalue::cast(Span::dummy(), ptr, at));
        let ref_ty = cx.ty_ref(*ty);
        let typed = fb.value(cx, Rvalue::cast(Span::dummy(), ref_ty, raw));
        let Operand::Copy(typed) = typed else { unreachable!() };
        call_args.push(Operand::Copy(deref(cx, typed)));
        offset += crate::ir::abi::size(cx, *ty).div_ceil(8) * 8;
    }
    let call = Rvalue::call(
        Span::dummy(),
        ret,
        Operand::fn_addr(Span::dummy(), func.ty, target),
        call_args,
    );
    if ret == void {
        fb.expr(call);
    } else {
        let ref_ty = cx.ty_ref(ret);
        let typed = fb.value(cx, Rvalue::cast(Span::dummy(), ref_ty, Operand::Copy(out)));
        let Operand::Copy(typed) = typed else { unreachable!() };
        fb.assign(deref(cx, typed), call);
    }
    let body = fb.finish();
    let ty = cx.ty_fn(vec![ptr, ptr], void);
    let dev_name = vec![cx.intern_name(DEV_MODULE)];
    let name = format!("thunk_{:016x}", bc::fn_key(cx, target));
    Ok(add_func(cx, &dev_name, &name, ty, body))
}

fn add_func(cx: &mut IrCx, module: &[Name], name: &str, ty: Ty, body: Body) -> FuncId {
    let name = cx.intern_name(name);
    cx.alloc_func(Func {
        id: FuncId(0),
        doc: String::new(),
        module: module.to_vec(),
        name,
        ty,
        vis: Vis::Private,
        method_of: None,
        kind: FuncKind::J3 { body },
        span: Span::dummy(),
    })
}

/// Everything the dev binary depends on besides `.j3` sources: the lowered native IR.
pub fn native_key(prog: &Program) -> String {
    let native: Vec<Module> = prog.modules.to_vec();
    crate::ir::dump::dump_program(&prog.cx, &native)
}
