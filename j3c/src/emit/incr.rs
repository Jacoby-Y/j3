//! Opt-0 builds: one object per module, reused when that module's IR is unchanged.
//!
//! The whole-program source stamp still skips this entirely. A one-file edit
//! re-emits and recompiles only the modules whose IR hash changed, then links.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use crate::emit::c_expr::{self, stable_ty};
use crate::emit::host::{append_link_args, clang_opt, link_gc_flag, CcOpts};
use crate::emit::llvm;
use crate::emit::opts::EmitOpts;
use crate::ir::body::Block;
use crate::ir::cx::IrCx;
use crate::ir::entity::{ConstInit, FuncKind};
use crate::ir::module::Module;
use crate::ir::operand::{ConstKind, Operand};
use crate::ir::rvalue::{InterpPart, Rvalue, RvalueKind};
use crate::ir::stmt::{Stmt, StmtKind};

pub(crate) fn enabled(opts: &EmitOpts) -> bool {
    opts.incremental
        && opts.backend == crate::emit::Backend::Llvm
        && matches!(opts.cc.opt.as_str(), "" | "0")
        && opts.cc.out.is_some()
}

pub(crate) fn compile(cx: &IrCx, modules: &[Module], opts: &EmitOpts) -> Result<PathBuf, String> {
    let exe = opts
        .cc
        .out
        .clone()
        .ok_or("incremental build needs an output path")?;
    if let Some(parent) = exe.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    let dir = exe.parent().unwrap_or(Path::new(".")).join("j3c-obj");
    fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;

    let layouts = layout_digest(cx, modules);
    let mut pending = Vec::new();
    for (index, module) in modules.iter().enumerate() {
        let label = module_label(cx, module);
        let key = object_key(cx, module, opts, &layouts);
        let obj = dir.join(format!("{label}-{key}.o"));
        let libs = dir.join(format!("{label}-{key}.libs"));
        let hit = obj.is_file() && libs.is_file();
        pending.push(Pending {
            index,
            label,
            obj,
            libs,
            hit,
        });
    }
    let misses = pending.iter().filter(|p| !p.hit).count();
    if misses > 0 {
        let word = if misses == 1 { "module" } else { "modules" };
        crate::cli::status(&format!("compiling {misses} {word}"));
    }
    let mut units = Vec::new();
    for item in pending {
        if item.hit {
            if timings() {
                eprintln!("j3c: cached {}", item.label);
            }
            let note = fs::read_to_string(&item.libs).unwrap_or_default();
            units.push(Unit {
                index: item.index,
                label: item.label,
                obj: item.obj,
                libs: item.libs,
                miss: false,
                note,
            });
            continue;
        }
        units.push(Unit {
            index: item.index,
            label: item.label,
            obj: item.obj,
            libs: item.libs,
            miss: true,
            note: String::new(),
        });
    }
    // Biggest modules first so the slowest job doesn't start last.
    let mut jobs: Vec<&Unit> = units.iter().filter(|u| u.miss).collect();
    jobs.sort_by_key(|u| std::cmp::Reverse(modules[u.index].funcs.len()));

    let errors = Mutex::new(Vec::new());
    let queue = Mutex::new(0usize);
    let width = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(jobs.len().max(1));
    let produced: Mutex<Vec<(usize, String)>> = Mutex::new(Vec::new());
    if !jobs.is_empty() {
        std::thread::scope(|scope| {
            for _ in 0..width {
                scope.spawn(|| loop {
                    let i = {
                        let mut n = queue.lock().unwrap();
                        if *n >= jobs.len() {
                            return;
                        }
                        let i = *n;
                        *n += 1;
                        i
                    };
                    let job = jobs[i];
                    match emit_and_clang(cx, modules, job, opts) {
                        Ok(note) => produced.lock().unwrap().push((job.index, note)),
                        Err(e) => errors.lock().unwrap().push(e),
                    }
                });
            }
        });
    }
    drop(jobs);
    let errs = errors.into_inner().unwrap_or_default();
    if let Some(e) = errs.into_iter().next() {
        return Err(e);
    }
    for (index, note) in produced.into_inner().unwrap_or_default() {
        if let Some(unit) = units.iter_mut().find(|u| u.index == index) {
            unit.note = note;
        }
    }
    let notes: String = units.iter().map(|u| u.note.as_str()).collect();
    let objs: Vec<PathBuf> = units.iter().map(|u| u.obj.clone()).collect();
    link_objs(&objs, &exe, &opts.cc, &opts.clang, &opts.target, &notes)?;
    sweep_stale(&dir, modules, cx, opts, &layouts);
    Ok(exe)
}

struct Pending {
    index: usize,
    label: String,
    obj: PathBuf,
    libs: PathBuf,
    hit: bool,
}

struct Unit {
    index: usize,
    label: String,
    obj: PathBuf,
    libs: PathBuf,
    miss: bool,
    note: String,
}

fn emit_and_clang(cx: &IrCx, modules: &[Module], job: &Unit, opts: &EmitOpts) -> Result<String, String> {
    let started = std::time::Instant::now();
    let ll = llvm::emit_llvm_module(cx, modules, job.index, opts)?;
    if timings() {
        eprintln!("j3c: emit {}: {:.3}s", job.label, started.elapsed().as_secs_f64());
    }
    clang_one(job, &ll, opts)
}

fn clang_one(job: &Unit, ll: &str, opts: &EmitOpts) -> Result<String, String> {
    let ll_path = job.obj.with_extension("ll");
    fs::write(&ll_path, ll).map_err(|e| format!("write {}: {e}", ll_path.display()))?;
    let clang = if opts.clang.is_empty() {
        "clang"
    } else {
        opts.clang.as_str()
    };
    let mut cmd = Command::new(clang);
    cmd.arg("-c")
        .arg("-Wno-override-module")
        .arg(format!("-O{}", clang_opt(&opts.cc.opt)))
        .arg("-ffunction-sections")
        .arg("-fdata-sections");
    if opts.cc.debug {
        cmd.arg("-g");
    }
    if !opts.target.is_empty() {
        cmd.arg(format!("--target={}", opts.target));
    }
    if !opts.cpu.is_empty() {
        cmd.arg(format!("-mcpu={}", opts.cpu));
    }
    for flag in &opts.flags {
        cmd.arg(flag);
    }
    cmd.arg("-o").arg(&job.obj).arg(&ll_path);
    crate::cli::status(&format!("clang {}", job.label));
    let started = std::time::Instant::now();
    let out = cmd.output().map_err(|e| format!("spawn {clang}: {e}"))?;
    let _ = fs::remove_file(&ll_path);
    if timings() {
        eprintln!(
            "j3c: clang {}: {:.3}s",
            job.label,
            started.elapsed().as_secs_f64()
        );
    }
    if !out.status.success() {
        let _ = fs::remove_file(&job.obj);
        return Err(format!(
            "{clang} failed on {}:\n{}",
            job.label,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let note = lib_notes(ll);
    fs::write(&job.libs, &note).map_err(|e| format!("write {}: {e}", job.libs.display()))?;
    Ok(note)
}

fn mold_on_path() -> bool {
    static FOUND: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FOUND.get_or_init(|| {
        std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).any(|d| d.join("mold").is_file()))
            .unwrap_or(false)
    })
}

fn link_objs(
    objs: &[PathBuf],
    exe: &Path,
    cc: &CcOpts,
    clang: &str,
    target: &str,
    notes: &str,
) -> Result<(), String> {
    let clang = if clang.is_empty() { "clang" } else { clang };
    let mut link = Command::new(clang);
    link.arg(link_gc_flag(target));
    if !target.is_empty() {
        link.arg(format!("--target={target}"));
    }
    if cfg!(target_os = "linux") && target.is_empty() && mold_on_path() {
        link.arg("-fuse-ld=mold"); // a manifest `-fuse-ld=` flag comes later and wins
    }
    link.arg("-o").arg(exe);
    for obj in objs {
        link.arg(obj);
    }
    append_link_args(&mut link, cc, notes, target)?;
    crate::cli::status("linking");
    let started = std::time::Instant::now();
    let out = link.output().map_err(|e| format!("spawn {clang}: {e}"))?;
    if timings() {
        eprintln!("j3c: link: {:.3}s", started.elapsed().as_secs_f64());
    }
    if !out.status.success() {
        return Err(format!(
            "{clang} link failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

fn lib_notes(ll: &str) -> String {
    let mut s = String::new();
    for line in ll.lines() {
        if line.contains("use lib \"") {
            s.push_str(line.trim());
            s.push('\n');
        }
    }
    if ll.contains(" frem ") && !s.contains("use lib \"m\"") {
        s.push_str("; use lib \"m\"\n");
    }
    s
}

/// Drop objects whose hash is no longer the module's key.
fn sweep_stale(dir: &Path, modules: &[Module], cx: &IrCx, opts: &EmitOpts, layouts: &Layouts) {
    let mut keep = Vec::new();
    for module in modules {
        let label = module_label(cx, module);
        let key = object_key(cx, module, opts, layouts);
        keep.push(format!("{label}-{key}.o"));
        keep.push(format!("{label}-{key}.libs"));
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for ent in entries.flatten() {
        let name = ent.file_name().to_string_lossy().to_string();
        if keep.iter().any(|k| k == &name) {
            continue;
        }
        if name.ends_with(".o") || name.ends_with(".libs") || name.ends_with(".ll") {
            let _ = fs::remove_file(ent.path());
        }
    }
}

fn timings() -> bool {
    std::env::var_os("J3C_TIMINGS").is_some()
}

fn module_label(cx: &IrCx, module: &Module) -> String {
    let mut s = String::new();
    for (i, part) in module.name.iter().enumerate() {
        if i > 0 {
            s.push('.');
        }
        s.push_str(cx.name_str(*part));
    }
    if s.is_empty() {
        s.push_str("mod");
    }
    s.replace('/', "_")
}

/// Hash of this j3c's own bytes: a relink or `touch` that changes nothing keeps the objects,
/// any real emitter change drops them.
fn compiler_id() -> u64 {
    static ID: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *ID.get_or_init(|| {
        let mut h = DefaultHasher::new();
        match crate::editor::host_exe().ok().and_then(|p| fs::read(p).ok()) {
            Some(bytes) => bytes.hash(&mut h),
            None => env!("CARGO_PKG_VERSION").hash(&mut h),
        }
        h.finish()
    })
}

fn object_key(cx: &IrCx, module: &Module, opts: &EmitOpts, layouts: &Layouts) -> String {
    let mut h = DefaultHasher::new();
    let text = module_ir(cx, module);
    layouts.for_module(&text).hash(&mut h);
    text.hash(&mut h);
    opts.cc.opt.hash(&mut h);
    opts.cc.debug.hash(&mut h);
    opts.target.hash(&mut h);
    opts.cpu.hash(&mut h);
    opts.flags.hash(&mut h);
    opts.clang.hash(&mut h);
    compiler_id().hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Field and variant layout of every struct and enum in the program. A module's IR only names
/// foreign types, so the offsets and tags of the types it touches (and what those embed) go in
/// its key, or an edit elsewhere leaves a stale object. Types are matched by name token, so a
/// shared name only ever causes an extra rebuild.
struct Layouts {
    texts: Vec<String>,
    deps: Vec<Vec<usize>>,
    by_needle: HashMap<String, Vec<usize>>,
}

fn tokens(s: &str) -> impl Iterator<Item = &str> {
    s.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|t| !t.is_empty())
}

fn layout_digest(cx: &IrCx, modules: &[Module]) -> Layouts {
    let mut needles: Vec<String> = Vec::new();
    let mut texts = Vec::new();
    for module in modules {
        for id in &module.structs {
            let def = cx.struct_def(*id);
            let mut s = format!("{}.{}|{}|", module_label(cx, module), cx.name_str(def.name), def.packed);
            for f in &def.fields {
                s.push_str(&format!("{}:{}:{},", cx.name_str(f.name), f.embedded, stable_ty(cx, f.ty)));
            }
            needles.push(cx.name_str(def.name).to_string());
            texts.push(s);
        }
        for id in &module.enums {
            let def = cx.enum_def(*id);
            let mut s = format!("{}.{}|", module_label(cx, module), cx.name_str(def.name));
            for v in &def.variants {
                s.push_str(cx.name_str(v.name));
                if let Some(p) = v.payload {
                    s.push_str(&stable_ty(cx, p));
                }
                s.push(',');
            }
            needles.push(cx.name_str(def.name).to_string());
            texts.push(s);
        }
    }
    let mut by_needle: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, n) in needles.iter().enumerate() {
        by_needle.entry(n.clone()).or_default().push(i);
    }
    let deps = texts
        .iter()
        .map(|t| tokens(t).filter_map(|w| by_needle.get(w)).flatten().copied().collect())
        .collect();
    Layouts { texts, deps, by_needle }
}

impl Layouts {
    /// Layout text of the types `module_text` names, closed over embedded types.
    fn for_module(&self, module_text: &str) -> String {
        let mut seen = vec![false; self.texts.len()];
        let mut stack: Vec<usize> = tokens(module_text)
            .filter_map(|w| self.by_needle.get(w))
            .flatten()
            .copied()
            .collect();
        while let Some(i) = stack.pop() {
            if !std::mem::replace(&mut seen[i], true) {
                stack.extend(self.deps[i].iter().copied());
            }
        }
        (0..seen.len()).filter(|i| seen[*i]).map(|i| self.texts[i].as_str()).collect::<Vec<_>>().join(";")
    }
}

/// Stable text of one module: the IR dump, each function's symbol and local
/// types, callee symbols, and globals. Intern ids are not included.
fn module_ir(cx: &IrCx, module: &Module) -> String {
    let mut s = crate::ir::dump::dump_module(cx, module);
    for id in &module.structs {
        let def = cx.struct_def(*id);
        for (n, t) in &def.ty_args {
            s.push_str(n);
            s.push('=');
            s.push_str(&stable_ty(cx, *t));
            s.push(';');
        }
        for f in &def.fields {
            s.push_str(&stable_ty(cx, f.ty));
            s.push(';');
        }
    }
    for id in &module.enums {
        let def = cx.enum_def(*id);
        for v in &def.variants {
            if let Some(p) = v.payload {
                s.push_str(&stable_ty(cx, p));
                s.push(';');
            }
        }
    }
    for id in &module.globals {
        let g = cx.global(*id);
        s.push_str(cx.name_str(g.name));
        s.push(':');
        s.push_str(&stable_ty(cx, g.ty));
        if let Some(init) = &g.init {
            hash_init_into(cx, init, &mut s);
        }
        s.push(';');
    }
    for id in &module.funcs {
        let func = cx.func(*id);
        s.push_str(&c_expr::func_c_name(cx, func));
        s.push(';');
        if let FuncKind::J3 { body } = &func.kind {
            for decl in &body.locals {
                s.push_str(&stable_ty(cx, decl.ty));
                s.push(';');
            }
            hash_block_into(cx, &body.block, &mut s);
        }
    }
    s
}

fn hash_init_into(cx: &IrCx, init: &ConstInit, out: &mut String) {
    match init {
        ConstInit::Scalar(op) => hash_operand_into(cx, op, out),
        ConstInit::Fields(fs) => {
            for (n, v) in fs {
                out.push_str(n);
                hash_init_into(cx, v, out);
            }
        }
        ConstInit::Elems(es) => {
            for v in es {
                hash_init_into(cx, v, out);
            }
        }
    }
}

fn hash_block_into(cx: &IrCx, block: &Block, out: &mut String) {
    for stmt in &block.stmts {
        hash_stmt_into(cx, stmt, out);
    }
}

fn hash_stmt_into(cx: &IrCx, stmt: &Stmt, out: &mut String) {
    match &stmt.kind {
        StmtKind::Assign { rvalue, .. } => hash_rvalue_into(cx, rvalue, out),
        StmtKind::AssignOp { rhs, .. } => hash_operand_into(cx, rhs, out),
        StmtKind::Defer { body } | StmtKind::Block(body) => hash_block_into(cx, body, out),
        StmtKind::If {
            cond,
            then_block,
            else_block,
        } => {
            hash_operand_into(cx, cond, out);
            hash_block_into(cx, then_block, out);
            if let Some(e) = else_block {
                hash_block_into(cx, e, out);
            }
        }
        StmtKind::While { cond, body } => {
            hash_operand_into(cx, cond, out);
            hash_block_into(cx, body, out);
        }
        StmtKind::Return { value } => {
            if let Some(v) = value {
                hash_operand_into(cx, v, out);
            }
        }
        StmtKind::Expr(rv) => hash_rvalue_into(cx, rv, out),
        StmtKind::ForRange { end, body, .. } => {
            hash_operand_into(cx, end, out);
            hash_block_into(cx, body, out);
        }
        StmtKind::InterpFill { parts, .. } => {
            for p in parts {
                if let InterpPart::Value(op) = p {
                    hash_operand_into(cx, op, out);
                }
            }
        }
        StmtKind::Break | StmtKind::Continue | StmtKind::Abort(_) | StmtKind::Div0 => {}
    }
}

fn hash_rvalue_into(cx: &IrCx, rv: &Rvalue, out: &mut String) {
    match &rv.kind {
        RvalueKind::Use(op) | RvalueKind::Cast { from: op } => hash_operand_into(cx, op, out),
        RvalueKind::Ref { .. } => {}
        RvalueKind::Binary { lhs, rhs, .. } => {
            hash_operand_into(cx, lhs, out);
            hash_operand_into(cx, rhs, out);
        }
        RvalueKind::Unary { arg, .. } => hash_operand_into(cx, arg, out),
        RvalueKind::Call { func, args } => {
            hash_operand_into(cx, func, out);
            for a in args {
                hash_operand_into(cx, a, out);
            }
        }
        RvalueKind::Syscall { nr, args } => {
            hash_operand_into(cx, nr, out);
            for a in args {
                hash_operand_into(cx, a, out);
            }
        }
        RvalueKind::Aggregate { fields } => {
            for f in fields {
                hash_operand_into(cx, f, out);
            }
        }
        RvalueKind::Enum { payload, .. } => {
            if let Some(p) = payload {
                hash_operand_into(cx, p, out);
            }
        }
        RvalueKind::Interpolate { parts } => {
            for p in parts {
                if let InterpPart::Value(op) = p {
                    hash_operand_into(cx, op, out);
                }
            }
        }
    }
}

fn hash_operand_into(cx: &IrCx, op: &Operand, out: &mut String) {
    if let Operand::Const(c) = op {
        if let ConstKind::FnAddr(id) = &c.kind {
            out.push_str(&c_expr::func_c_name(cx, cx.func(*id)));
            out.push(';');
        }
    }
}
