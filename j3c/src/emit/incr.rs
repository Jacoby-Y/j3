//! Opt-0 and opt-1 builds (every opt with `incremental = "all"`): one object per module, reused
//! when that module's IR is unchanged. Opt 1 gives up cross-module inlining. Opt 2/3/s/z and
//! `--release` take the same path with ThinLTO objects where [`thin_lto`] allows.
//!
//! The whole-program source stamp still skips this entirely. A one-file edit
//! re-emits and recompiles only the modules whose IR hash changed, then links.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::emit::c_expr;
use crate::emit::host::{append_link_args, clang_for, clang_opt, link_gc_flag, lld_present, mold_on_path, release_flags, target_args, use_mold, CcOpts, TargetOs};
use crate::emit::llvm;
use crate::emit::opts::EmitOpts;
use crate::ir::cx::IrCx;
use crate::ir::entity::FuncKind;
use crate::ir::ids::FuncId;
use crate::ir::module::Module;

mod key;
mod parts;

pub(crate) use key::compiler_id;
use key::{layout_digest, module_ir, object_key};
use parts::{part_path, parts_file, parts_of, split_plan};

pub(crate) fn enabled(opts: &EmitOpts) -> bool {
    opts.incremental
        && opts.backend == crate::emit::Backend::Llvm
        && (opts.incremental_all || matches!(opts.cc.opt.as_str(), "" | "0" | "1"))
        && opts.cc.out.is_some()
}

/// `--release` or opt 2/3/s/z on the Linux host with lld or mold: per-module ThinLTO objects,
/// linked (and optimized across modules, in parallel, with a cache) by the linker; mold runs
/// LLVM through the gold plugin. Everywhere else those builds stay one clang.
/// `J3C_NO_THINLTO=1` forces the one-clang path.
pub(crate) fn thin_lto(opts: &EmitOpts) -> bool {
    // `incremental = false` (one clang) and `"all"` (per-module, no LTO) opt out below release.
    (opts.cc.release
        || (opts.incremental && !opts.incremental_all && matches!(opts.cc.opt.as_str(), "2" | "3" | "s" | "z")))
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
    // ThinLTO objects get their own folder, or an optimized build sweeps away the dev objects.
    let thin = thin_lto(opts);
    let dir = crate::emit::host::cache_dir(&exe, &opts.target).join(if thin { "obj-thin" } else { "obj" });
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
    link_objs(&objs, &exe, &opts.cc, thin, &opts.clang, &opts.target, &notes)?;
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
    if thin_lto(opts) {
        cmd.arg("-flto=thin");
    }
    if opts.cc.release {
        cmd.args(release_flags(TargetOs::Linux, lld_present()).compile);
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
    thin: bool,
    clang: &str,
    target: &str,
    notes: &str,
) -> Result<(), String> {
    let clang = clang_for(clang, target);
    let clang = clang.as_str();
    let mut link = Command::new(clang);
    link.arg(link_gc_flag(target));
    target_args(&mut link, target);
    use_mold(&mut link, cc.release || thin, target);
    if thin {
        let cache = crate::emit::host::cache_dir(exe, target).join("thinlto");
        // Per-module objects lose the internal-linkage inlining bonus a whole program gets, so
        // opt 2 links at O3: a one-caller generic like `List.push` inlines again (bench: as fast).
        let lto_opt = if cc.opt == "2" { "3".to_string() } else { clang_opt(&cc.opt).to_string() };
        link.arg("-flto=thin").arg(format!("-O{lto_opt}"));
        if lld_present() {
            if !cc.release {
                link.arg("-fuse-ld=lld"); // release gets it from `release_flags`
            }
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

#[cfg(test)]
mod tests;
