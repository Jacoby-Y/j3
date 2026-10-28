//! Opt-0 builds (every opt with `incremental = "all"`): one object per module, reused when
//! that module's IR is unchanged. Above opt 0 this gives up cross-module inlining.
//!
//! The whole-program source stamp still skips this entirely. A one-file edit
//! re-emits and recompiles only the modules whose IR hash changed, then links.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::emit::c_expr::{self, stable_ty};
use crate::emit::host::{append_link_args, clang_for, clang_opt, link_gc_flag, lld_present, mold_on_path, release_flags, target_args, use_mold, CcOpts, TargetOs};
use crate::emit::llvm;
use crate::emit::opts::EmitOpts;
use crate::ir::body::Block;
use crate::ir::cx::IrCx;
use crate::ir::entity::{ConstInit, FuncKind};
use crate::ir::ids::FuncId;
use crate::ir::module::Module;
use crate::ir::operand::{ConstKind, Operand};
use crate::ir::rvalue::{InterpPart, Rvalue, RvalueKind};
use crate::ir::stmt::{Stmt, StmtKind};

pub(crate) fn enabled(opts: &EmitOpts) -> bool {
    opts.incremental
        && opts.backend == crate::emit::Backend::Llvm
        && (opts.incremental_all || matches!(opts.cc.opt.as_str(), "" | "0"))
        && opts.cc.out.is_some()
}

/// `--release` on the Linux host with lld or mold: per-module ThinLTO objects, linked (and
/// optimized across modules, in parallel, with a cache) by the linker; mold runs LLVM through
/// the gold plugin. Everywhere else release stays one clang.
/// `J3C_NO_THINLTO=1` forces the one-clang path.
pub(crate) fn thin_lto(opts: &EmitOpts) -> bool {
    opts.cc.release
        && opts.backend == crate::emit::Backend::Llvm
        && opts.target.is_empty()
        && cfg!(target_os = "linux")
        && opts.cc.out.is_some()
        && (lld_present() || mold_on_path())
        && std::env::var_os("J3C_NO_THINLTO").is_none()
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
    // One folder per output and target, or each build's sweep deletes the other's objects.
    // ThinLTO objects get their own folder, or a release build sweeps away the dev objects.
    let dir = crate::emit::host::cache_dir(&exe, &opts.target).join(if opts.cc.release { "obj-thin" } else { "obj" });
    fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    // This link replaces the output, so a whole-program build (`--release`) must not trust its old key.
    let _ = fs::remove_file(dir.with_file_name("clang.key"));

    crate::cli::status("emitting");
    crate::phase::detail("hashing modules against the object cache");
    let t_keys = std::time::Instant::now();
    let layouts = layout_digest(cx, modules);
    let mut pending = Vec::new();
    // `label-key.` of every current module, for the sweep.
    let mut keep = Vec::new();
    for (index, module) in modules.iter().enumerate() {
        let label = module_label(cx, module);
        let text = module_ir(cx, module);
        let key = object_key(&text, opts, &layouts);
        let obj = dir.join(format!("{label}-{key}.o"));
        let libs = dir.join(format!("{label}-{key}.libs"));
        let parts = parts_of(&obj);
        let hit = libs.is_file() && if parts > 0 { (0..parts).all(|n| part_path(&obj, n).is_file()) } else { obj.is_file() };
        keep.push(format!("{label}-{key}."));
        pending.push(Pending { index, label, obj, libs, parts, hit, cost: text.len() });
    }
    let misses = pending.iter().filter(|p| !p.hit).count();
    crate::cli::timing("  emit: hash modules", t_keys);
    crate::cli::status("compiling");
    crate::phase::note(&format!("{} of {} modules cached, {misses} to build", pending.len() - misses, pending.len()));
    let mut units = Vec::new();
    for item in pending {
        if item.hit && timings() {
            eprintln!("j3c: cached {}", item.label);
        }
        let note = if item.hit { fs::read_to_string(&item.libs).unwrap_or_default() } else { String::new() };
        units.push(Unit {
            index: item.index,
            label: item.label,
            obj: item.obj,
            libs: item.libs,
            parts: item.parts,
            miss: !item.hit,
            note,
            cost: item.cost,
        });
    }
    // Costliest modules first so the slowest job doesn't start last. IR text length is a rough cost.
    let mut jobs: Vec<&Unit> = units.iter().filter(|u| u.miss).collect();
    jobs.sort_by_key(|u| std::cmp::Reverse(u.cost));

    let errors = Mutex::new(Vec::new());
    let queue = Mutex::new(0usize);
    let width = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(jobs.len().max(1));
    let produced: Mutex<Vec<(usize, String, usize)>> = Mutex::new(Vec::new());
    let done_jobs = AtomicUsize::new(0);
    let timed: Mutex<Vec<JobTime>> = Mutex::new(Vec::new());
    let t_jobs = std::time::Instant::now();
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
                    let seen = JobProgress {
                        done: &done_jobs,
                        total: jobs.len(),
                    };
                    let result = emit_and_clang(cx, modules, job, opts, &seen);
                    let finished = done_jobs.fetch_add(1, Ordering::Relaxed) + 1;
                    crate::phase::detail(&format!("{finished}/{} modules compiled", jobs.len()));
                    match result {
                        Ok((note, time, parts)) => {
                            produced.lock().unwrap().push((job.index, note, parts));
                            timed.lock().unwrap().push(time);
                        }
                        Err(e) => errors.lock().unwrap().push(e),
                    }
                });
            }
        });
    }
    drop(jobs);
    jobs_done_note(&timed, t_jobs, width);
    let errs = errors.into_inner().unwrap_or_default();
    if let Some(e) = errs.into_iter().next() {
        return Err(e);
    }
    for (index, note, parts) in produced.into_inner().unwrap_or_default() {
        if let Some(unit) = units.iter_mut().find(|u| u.index == index) {
            unit.note = note;
            unit.parts = parts;
        }
    }
    let notes: String = units.iter().map(|u| u.note.as_str()).collect();
    let objs: Vec<PathBuf> = units
        .iter()
        .flat_map(|u| if u.parts > 0 { (0..u.parts).map(|n| part_path(&u.obj, n)).collect() } else { vec![u.obj.clone()] })
        .collect();
    link_objs(&objs, &exe, &opts.cc, &opts.clang, &opts.target, &notes)?;
    sweep_stale(&dir, &keep);
    Ok(exe)
}

struct Pending {
    index: usize,
    parts: usize,
    label: String,
    obj: PathBuf,
    libs: PathBuf,
    hit: bool,
    cost: usize,
}

/// `parts` > 0: the module was split, its objects are `part_path(obj, 0..parts)`.
struct Unit {
    index: usize,
    parts: usize,
    label: String,
    obj: PathBuf,
    libs: PathBuf,
    miss: bool,
    note: String,
    cost: usize,
}

/// How far the misses have got, for the inner status line.
struct JobProgress<'a> {
    done: &'a AtomicUsize,
    total: usize,
}

/// CPU seconds one module spent in each stage (jobs run in parallel, so these exceed wall time).
struct JobTime {
    label: String,
    emit: f64,
    clang: f64,
}

/// Record the build's cpu-time split and its slowest modules. Nothing to say if every module was cached.
fn jobs_done_note(timed: &Mutex<Vec<JobTime>>, since: std::time::Instant, width: usize) {
    let mut times = timed.lock().unwrap();
    if times.is_empty() {
        return;
    }
    let emit: f64 = times.iter().map(|t| t.emit).sum();
    let clang: f64 = times.iter().map(|t| t.clang).sum();
    crate::phase::note(&format!(
        "{} modules on {width} threads: {:.2}s wall, {emit:.2}s emit cpu, {clang:.2}s clang cpu",
        times.len(),
        since.elapsed().as_secs_f64()
    ));
    times.sort_by(|a, b| (b.emit + b.clang).total_cmp(&(a.emit + a.clang)));
    for t in times.iter().take(5) {
        crate::phase::step(&format!("  slow: {} (emit {:.2}s, clang {:.2}s)", t.label, t.emit, t.clang), t.emit + t.clang);
    }
}

fn emit_and_clang(
    cx: &IrCx,
    modules: &[Module],
    job: &Unit,
    opts: &EmitOpts,
    seen: &JobProgress,
) -> Result<(String, JobTime, usize), String> {
    let started = std::time::Instant::now();
    let done = || seen.done.load(Ordering::Relaxed);
    crate::phase::detail(&format!("{}/{} done, emit {}", done(), seen.total, job.label));
    let ll = llvm::emit_llvm_module(cx, modules, job.index, opts, None)?;
    let mut emit = started.elapsed().as_secs_f64();
    if timings() {
        eprintln!("j3c: emit {}: {emit:.3}s", job.label);
    }
    crate::phase::detail(&format!("{}/{} done, clang {}", done(), seen.total, job.label));
    let note = lib_notes(&ll);
    let _ = fs::remove_file(parts_file(&job.obj));
    let mut parts = 0;
    match split_plan(cx, &modules[job.index], &ll, opts) {
        None => clang_obj(&job.label, &job.obj, &ll, opts)?,
        Some(sets) => {
            parts = sets.len();
            let total = parts;
            drop(ll);
            // The giant functions are the long pole: one clang per share, side by side.
            let results: Vec<Result<f64, String>> = std::thread::scope(|scope| {
                let handles: Vec<_> = sets
                    .iter()
                    .enumerate()
                    .map(|(n, set)| {
                        scope.spawn(move || {
                            let t = std::time::Instant::now();
                            let ll = llvm::emit_llvm_module(cx, modules, job.index, opts, Some((n, set)))?;
                            let spent = t.elapsed().as_secs_f64();
                            clang_obj(&format!("{} [{}/{}]", job.label, n + 1, total), &part_path(&job.obj, n), &ll, opts)?;
                            Ok(spent)
                        })
                    })
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap_or_else(|_| Err("chunk thread panicked".into()))).collect()
            });
            for r in results {
                emit = emit.max(r?);
            }
            fs::write(parts_file(&job.obj), parts.to_string()).map_err(|e| format!("write parts file: {e}"))?;
        }
    }
    fs::write(&job.libs, &note).map_err(|e| format!("write {}: {e}", job.libs.display()))?;
    let clang = started.elapsed().as_secs_f64() - emit;
    let label = if parts > 0 { format!("{} (split in {parts})", job.label) } else { job.label.clone() };
    Ok((note, JobTime { label, emit, clang }, parts))
}

fn parts_file(obj: &Path) -> PathBuf {
    obj.with_extension("parts")
}

/// Object `n` of a split module.
fn part_path(obj: &Path, n: usize) -> PathBuf {
    obj.with_extension(format!("p{n}.o"))
}

/// How many objects a module was split into last time (0 = one object).
fn parts_of(obj: &Path) -> usize {
    fs::read_to_string(parts_file(obj)).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0)
}

/// Byte length of each `define` block in a module's LLVM text, by symbol.
fn define_sizes(ll: &str) -> HashMap<&str, usize> {
    let mut sizes = HashMap::new();
    let mut open: Option<(&str, usize)> = None;
    for line in ll.lines() {
        if let Some(rest) = line.strip_prefix("define ") {
            let name = rest.split('@').nth(1).and_then(|r| r.split('(').next()).unwrap_or("");
            open = Some((name, 0));
        }
        if let Some((name, n)) = open.as_mut() {
            *n += line.len() + 1;
            if line == "}" {
                sizes.insert(*name, *n);
                open = None;
            }
        }
    }
    sizes
}

/// A function this large makes clang's backend slow on its own (it grows faster than linearly).
const GIANT_FN: usize = 15_000;

/// Split a module with several giant functions: one set of functions per object, balanced by
/// the size of their LLVM text. None when it is not worth it (or `[emit] chunks = 0`).
fn split_plan(cx: &IrCx, module: &Module, ll: &str, opts: &EmitOpts) -> Option<Vec<HashSet<FuncId>>> {
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let cap = opts.chunks.unwrap_or(4).min((threads / 2).max(1));
    if cap < 2 {
        return None;
    }
    let sizes = define_sizes(ll);
    let funcs: Vec<(FuncId, usize)> = module
        .funcs
        .iter()
        .filter_map(|id| {
            let func = cx.func(*id);
            let size = *sizes.get(c_expr::func_c_name(cx, func).as_str())?;
            Some((*id, size))
        })
        .collect();
    let giants = funcs.iter().filter(|f| f.1 >= GIANT_FN).count();
    let (total, max) = (funcs.iter().map(|f| f.1).sum::<usize>(), funcs.iter().map(|f| f.1).max().unwrap_or(0));
    let mut n = giants.min(cap);
    if n < 2 && max >= GIANT_FN && total - max >= max / 3 {
        n = 2; // one giant: at least take everything else off its object
    }
    if n < 2 {
        return None;
    }
    let weights: Vec<usize> = funcs.iter().map(|f| f.1).collect();
    let mut sets = vec![HashSet::new(); n];
    for ((id, _), c) in funcs.iter().zip(llvm::chunk_assign(&weights, n)) {
        sets[c].insert(*id);
    }
    Some(sets)
}

fn clang_obj(label: &str, obj: &Path, ll: &str, opts: &EmitOpts) -> Result<(), String> {
    let ll_path = obj.with_extension("ll");
    fs::write(&ll_path, ll).map_err(|e| format!("write {}: {e}", ll_path.display()))?;
    let clang = clang_for(&opts.clang, &opts.target);
    let clang = clang.as_str();
    let mut cmd = Command::new(clang);
    cmd.arg("-c")
        .arg("-Wno-override-module")
        .arg(format!("-O{}", clang_opt(&opts.cc.opt)))
        .arg("-ffunction-sections")
        .arg("-fdata-sections");
    if opts.cc.debug {
        cmd.arg("-g");
    }
    if opts.cc.release {
        cmd.arg("-flto=thin").args(release_flags(TargetOs::Linux, lld_present()).compile);
    }
    target_args(&mut cmd, &opts.target);
    if !opts.cpu.is_empty() {
        cmd.arg(format!("-mcpu={}", opts.cpu));
    }
    for flag in &opts.flags {
        cmd.arg(flag);
    }
    let time_report = std::env::var_os("J3C_TIME_REPORT").is_some();
    if time_report {
        cmd.arg("-ftime-report");
    }
    cmd.arg("-o").arg(obj).arg(&ll_path);
    let started = std::time::Instant::now();
    let out = cmd.output().map_err(|e| format!("spawn {clang}: {e}"))?;
    let _ = fs::remove_file(&ll_path);
    if time_report {
        // One file per object next to it: `cat <obj dir>/*.time.txt`.
        let _ = fs::write(obj.with_extension("time.txt"), &out.stderr);
    }
    if timings() {
        eprintln!("j3c: clang {label}: {:.3}s", started.elapsed().as_secs_f64());
    }
    if !out.status.success() {
        let _ = fs::remove_file(obj);
        return Err(format!("{clang} failed on {label}:\n{}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(())
}

fn link_objs(
    objs: &[PathBuf],
    exe: &Path,
    cc: &CcOpts,
    clang: &str,
    target: &str,
    notes: &str,
) -> Result<(), String> {
    let clang = clang_for(clang, target);
    let clang = clang.as_str();
    let mut link = Command::new(clang);
    link.arg(link_gc_flag(target));
    target_args(&mut link, target);
    use_mold(&mut link, cc.release, target);
    if cc.release {
        let cache = crate::emit::host::cache_dir(exe, target).join("thinlto");
        link.arg("-flto=thin").arg(format!("-O{}", clang_opt(&cc.opt)));
        if lld_present() {
            link.arg(format!("-Wl,--thinlto-cache-dir={}", cache.display()));
        } else {
            let jobs = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
            link.arg("-fuse-ld=mold")
                .arg(format!("-Wl,-plugin-opt,cache-dir={}", cache.display()))
                .arg(format!("-Wl,-plugin-opt,jobs={jobs}"));
        }
    }
    link.arg("-o").arg(exe);
    for obj in objs {
        link.arg(obj);
    }
    append_link_args(&mut link, cc, notes, target)?;
    if cc.release {
        link.args(release_flags(TargetOs::Linux, lld_present()).link);
    }
    crate::cli::status("linking");
    let started = std::time::Instant::now();
    let out = crate::phase::run_ticking(&mut link, "linking").map_err(|e| format!("spawn {clang}: {e}"))?;
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

/// Drop objects whose hash is no longer the module's key. Everything a current module wrote
/// starts with one of `keep` (`label-key.`): its .o, .libs, .parts and p<N>.o.
fn sweep_stale(dir: &Path, keep: &[String]) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for ent in entries.flatten() {
        let name = ent.file_name().to_string_lossy().to_string();
        if keep.iter().any(|k| name.starts_with(k.as_str())) {
            continue;
        }
        if name.ends_with(".o") || name.ends_with(".libs") || name.ends_with(".ll") || name.ends_with(".parts") {
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
/// any real emitter change drops them. The hash is cached in `~/.cache/j3/compiler.id` by the
/// binary's size and mtime, so a build reads the binary only after j3c changed.
pub(crate) fn compiler_id() -> u64 {
    static ID: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *ID.get_or_init(|| {
        let Some(exe) = crate::editor::host_exe().ok() else { return version_id() };
        let Ok(meta) = fs::metadata(&exe) else { return version_id() };
        let mtime = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
        let stamp = format!("{} {mtime}", meta.len());
        let file = crate::editor::cache_root().map(|d| d.join("compiler.id"));
        let cached = file.as_ref().and_then(|f| fs::read_to_string(f).ok());
        if let Some((s, id)) = cached.as_deref().and_then(|c| c.trim().rsplit_once(' ')) {
            if s == stamp {
                if let Ok(id) = u64::from_str_radix(id, 16) {
                    return id;
                }
            }
        }
        let Ok(bytes) = fs::read(&exe) else { return version_id() };
        let mut h = DefaultHasher::new();
        bytes.hash(&mut h);
        let id = h.finish();
        if let Some(f) = &file {
            if let Some(dir) = f.parent() {
                let _ = fs::create_dir_all(dir);
            }
            let _ = fs::write(f, format!("{stamp} {id:016x}\n"));
        }
        id
    })
}

fn version_id() -> u64 {
    let mut h = DefaultHasher::new();
    env!("CARGO_PKG_VERSION").hash(&mut h);
    h.finish()
}

/// `text` is the module's [`module_ir`].
fn object_key(text: &str, opts: &EmitOpts, layouts: &Layouts) -> String {
    let mut h = DefaultHasher::new();
    opts.chunks.hash(&mut h);
    layouts.for_module(text).hash(&mut h);
    text.hash(&mut h);
    opts.cc.opt.hash(&mut h);
    opts.cc.debug.hash(&mut h);
    opts.cc.release.hash(&mut h);
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
        RvalueKind::Atomic { op, args } => {
            out.push_str(op.name());
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every function is defined by exactly one chunk, and `@main` by exactly one object.
    #[test]
    fn chunks_define_each_function_once() {
        let entry = Path::new(env!("CARGO_MANIFEST_DIR")).join("../speculative/01_basics.j3");
        let prog = crate::compile::check_path_lib(&entry).expect("check");
        let opts = EmitOpts::default();
        let names = |ll: &str| -> Vec<String> {
            ll.lines()
                .filter_map(|l| l.strip_prefix("define "))
                .map(|r| r.split('@').nth(1).unwrap_or("").split('(').next().unwrap_or("").to_string())
                .collect()
        };
        let mut mains = 0;
        for (index, module) in prog.modules.iter().enumerate() {
            let part = llvm::chunk_assign(&llvm::func_weights(&prog.cx, module), 3);
            let mut defs = Vec::new();
            for n in 0..3 {
                let set: HashSet<FuncId> = module.funcs.iter().zip(&part).filter(|(_, c)| **c == n).map(|(f, _)| *f).collect();
                let ll = llvm::emit_llvm_module(&prog.cx, &prog.modules, index, &opts, Some((n, &set))).expect("emit");
                for name in names(&ll) {
                    if name == "main" {
                        mains += 1;
                    } else {
                        defs.push(name);
                    }
                }
            }
            let whole = llvm::emit_llvm_module(&prog.cx, &prog.modules, index, &opts, None).expect("emit");
            // Only what the module uses is declared.
            for line in whole.lines().filter(|l| l.starts_with("declare ")) {
                let name = line.split('@').nth(1).unwrap_or("").split('(').next().unwrap_or("");
                let uses = whole.matches(&format!("@{name}(")).count() + whole.matches(&format!("@{name},")).count() + whole.matches(&format!("@{name} ")).count();
                assert!(uses > 1, "unused declare in module {index}: {line}");
            }
            let mut want: Vec<String> = names(&whole).into_iter().filter(|n| n != "main").collect();
            want.sort();
            defs.sort();
            assert_eq!(defs, want);
        }
        assert_eq!(mains, 1);
    }
}
