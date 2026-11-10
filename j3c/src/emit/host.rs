//! Host `cc` + run. No extra crates.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::project::Manifest;

mod hints;
mod link;
mod symbolize;
mod target;
#[cfg(test)]
mod tests;

pub use hints::failure_hint;
pub(crate) use hints::spawn_err;
pub use link::host_lib_present;
pub(crate) use link::*;
pub(crate) use symbolize::*;
pub(crate) use target::*;

static RUN_SEQ: AtomicU64 = AtomicU64::new(0);

pub struct RunResult {
    pub status: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct CcOpts {
    pub libs: Vec<String>,
    pub search: Vec<String>,
    pub files: Vec<String>,
    pub flags: Vec<String>,
    pub std: String,
    pub opt: String,
    pub debug: bool,
    /// `--release`: strip, trim the link, drop unwind tables (see [`release_flags`]).
    pub release: bool,
    pub out: Option<PathBuf>,
}

impl CcOpts {
    pub fn from_manifest(m: &Manifest) -> CcOpts {
        let search = m
            .link
            .search
            .iter()
            .map(|s| {
                let p = PathBuf::from(s);
                if p.is_absolute() {
                    s.clone()
                } else {
                    m.root.join(s).display().to_string()
                }
            })
            .collect();
        CcOpts {
            libs: m.link.libs.clone(),
            search,
            files: m.link.files.clone(),
            flags: m.link.flags.clone(),
            std: m.cc.std.clone(),
            opt: m.cc.opt.clone(),
            debug: !m.cc.fast,
            release: false,
            out: Some(m.out_path()),
        }
    }
}

/// Where a build output keeps its bookkeeping (stamps, objects, the dev binary):
/// `target/.j3c/<triple or host>/<exe name>/`, so `target/` holds only programs you run.
/// A per-platform folder (`target/<triple>/<name>`) shares the one `.j3c`. An output that is
/// already inside `.j3c` (the dev binary) keeps its files beside it.
pub fn cache_dir(out: &Path, target: &str) -> PathBuf {
    let parent = out.parent().unwrap_or(Path::new("."));
    if parent.components().any(|c| c.as_os_str() == ".j3c") {
        return parent.to_path_buf();
    }
    let slug = crate::project::target_slug(target);
    let base = match parent.file_name() {
        Some(n) if !target.is_empty() && n == slug.as_str() => parent.parent().unwrap_or(parent),
        _ => parent,
    };
    let name = out.file_name().map(|n| n.to_os_string()).unwrap_or_else(|| "a.out".into());
    base.join(".j3c").join(if target.is_empty() { "host" } else { slug.as_str() }).join(name)
}

pub fn compile_and_run(c_source: &str) -> Result<RunResult, String> {
    compile_and_run_with(c_source, &CcOpts::default())
}

pub fn compile_and_run_with(c_source: &str, opts: &CcOpts) -> Result<RunResult, String> {
    let exe = compile_c(c_source, opts, "", "clang")?;
    let result = run_exe(&exe);
    if opts.out.is_none() {
        let _ = fs::remove_file(&exe);
    }
    result
}

pub fn compile_llvm(
    ll_source: &str,
    cc: &CcOpts,
    clang: &str,
    target: &str,
    cpu: &str,
    extra: &[String],
) -> Result<PathBuf, String> {
    let pid = std::process::id();
    let seq = RUN_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let stamp = format!("j3c_{pid}_{seq}_{nanos}");
    let dir = std::env::temp_dir();
    let src_path = dir.join(format!("{stamp}.ll"));
    let exe_path = match &cc.out {
        Some(p) => {
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
            }
            p.clone()
        }
        None => dir.join(format!("{stamp}.bin")),
    };
    crate::phase::detail(&format!("writing IR ({} MB)", ll_source.len() >> 20));
    let t_write = std::time::Instant::now();
    fs::write(&src_path, ll_source).map_err(|e| format!("write {}: {e}", src_path.display()))?;

    let opt = clang_opt(&cc.opt);
    let clang = clang_for(clang, target);
    let clang = clang.as_str();

    let mut cmd = Command::new(clang);
    cmd.arg("-Wno-override-module")
        .arg(format!("-O{opt}"))
        .arg("-ffunction-sections")
        .arg("-fdata-sections")
        .arg(link_gc_flag(target));
    if cc.debug {
        cmd.arg("-g");
    }
    target_args(&mut cmd, target);
    if !cpu.is_empty() {
        cmd.arg(format!("-mcpu={cpu}"));
    }
    let release = cc.release.then(|| release_flags(target_os(target), lld_present()));
    if let Some(r) = &release {
        cmd.args(&r.compile);
    }
    use_mold(&mut cmd, cc.release, target);
    for flag in extra {
        cmd.arg(flag);
    }
    let time_report = std::env::var_os("J3C_TIME_REPORT").is_some();
    if time_report {
        cmd.arg("-ftime-report"); // per-pass clang timings, printed after the build
    }
    cmd.arg("-o").arg(&exe_path).arg(&src_path);
    append_link_args(&mut cmd, cc, ll_source, target)?;
    if let Some(r) = &release {
        cmd.args(&r.link);
    }
    // A named output rebuilt from the same IR and command is already up to date. Most
    // editor Plays change only scene files, which the program reads at run time.
    let stamp_path = match &cc.out {
        Some(p) => {
            let dir = cache_dir(p, target);
            fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
            Some(dir.join("clang.key"))
        }
        None => None,
    };
    crate::phase::detail("hashing IR for the build cache key");
    let key = build_key(&src_path, ll_source, &cmd);
    crate::cli::timing("  write IR + cache key", t_write);
    if let Some(stamp) = &stamp_path {
        if exe_path.exists() && fs::read_to_string(stamp).map(|k| k == key).unwrap_or(false) {
            let _ = fs::remove_file(&src_path);
            crate::cli::status("compiling");
            crate::phase::note("clang skipped: same IR and command as the last build");
            return Ok(exe_path);
        }
        let _ = fs::remove_file(stamp);
    }
    crate::cli::status("compiling");
    let what = format!("clang -O{opt} whole program (one invocation, links too)");
    crate::phase::detail(&what);
    let started = std::time::Instant::now();
    let out = crate::phase::run_ticking(&mut cmd, &what).map_err(|e| hints::spawn_err(&clang, e))?;
    if std::env::var_os("J3C_TIMINGS").is_some() {
        eprintln!("j3c: clang: {:.3}s", started.elapsed().as_secs_f64());
    }
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let _ = fs::remove_file(&src_path);
        return Err(format!("{clang} failed:\n{err}"));
    }
    let _ = fs::remove_file(&src_path);
    if time_report {
        eprintln!("{}", String::from_utf8_lossy(&out.stderr));
    }
    if let Some(stamp) = &stamp_path {
        let _ = fs::write(stamp, &key);
    }
    Ok(exe_path)
}

/// Dev builds of script programs: compile `vm_ll` (the script VM) to an object at `-O2`,
/// cached by its text, while `main_ll` compiles at the project's level; then link both.
/// The VM runs every script instruction, so it must not be `-O0` even when the game is.
pub fn compile_llvm_split(
    main_ll: &str,
    vm_ll: &str,
    cc: &CcOpts,
    clang: &str,
    target: &str,
    cpu: &str,
    extra: &[String],
) -> Result<PathBuf, String> {
    let exe_path = cc.out.clone().ok_or("split build needs an output path")?;
    if let Some(parent) = exe_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    let clang = clang_for(clang, target);
    let clang = clang.as_str();
    let dir = cache_dir(&exe_path, target);
    fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    let vm_src = dir.join("vm.ll");
    let vm_obj = dir.join("vm.o");
    let vm_stamp = dir.join("vm.key");
    let main_src = dir.join("main.ll");
    let main_obj = dir.join("main.o");
    let compile = |src: &Path, obj: &Path, opt: &str, debug: bool| {
        let mut cmd = Command::new(clang);
        cmd.arg("-c")
            .arg("-Wno-override-module")
            .arg(format!("-O{opt}"))
            .arg("-ffunction-sections")
            .arg("-fdata-sections");
        if debug {
            cmd.arg("-g");
        }
        target_args(&mut cmd, target);
        if !cpu.is_empty() {
            cmd.arg(format!("-mcpu={cpu}"));
        }
        for flag in extra {
            cmd.arg(flag);
        }
        cmd.arg("-o").arg(obj).arg(src);
        cmd
    };
    crate::cli::status("compiling");
    let started = std::time::Instant::now();
    let vm_key = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        vm_ll.hash(&mut h);
        format!("{:?}", compile(&vm_src, &vm_obj, "2", false)).hash(&mut h);
        format!("{:016x}", h.finish())
    };
    let vm_cached =
        vm_obj.exists() && fs::read_to_string(&vm_stamp).map(|k| k == vm_key).unwrap_or(false);
    let mut vm_child = None;
    if !vm_cached {
        let _ = fs::remove_file(&vm_stamp);
        fs::write(&vm_src, vm_ll).map_err(|e| format!("write {}: {e}", vm_src.display()))?;
        let child = compile(&vm_src, &vm_obj, "2", false)
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| hints::spawn_err(&clang, e))?;
        vm_child = Some(child);
    }
    fs::write(&main_src, main_ll).map_err(|e| format!("write {}: {e}", main_src.display()))?;
    let out = compile(&main_src, &main_obj, clang_opt(&cc.opt), cc.debug)
        .output()
        .map_err(|e| hints::spawn_err(&clang, e))?;
    let _ = fs::remove_file(&main_src);
    if !out.status.success() {
        return Err(format!("{clang} failed:\n{}", String::from_utf8_lossy(&out.stderr)));
    }
    if let Some(child) = vm_child {
        let vm_out = child.wait_with_output().map_err(|e| format!("wait {clang}: {e}"))?;
        let _ = fs::remove_file(&vm_src);
        if !vm_out.status.success() {
            return Err(format!(
                "{clang} failed on the script VM:\n{}",
                String::from_utf8_lossy(&vm_out.stderr)
            ));
        }
        let _ = fs::write(&vm_stamp, &vm_key);
    }
    let mut link = Command::new(clang);
    link.arg(link_gc_flag(target));
    target_args(&mut link, target);
    use_mold(&mut link, cc.release, target);
    for flag in extra {
        link.arg(flag);
    }
    link.arg("-o").arg(&exe_path).arg(&main_obj).arg(&vm_obj);
    append_link_args(&mut link, cc, main_ll, target)?;
    let linked = link.output().map_err(|e| hints::spawn_err(&clang, e))?;
    let _ = fs::remove_file(&main_obj);
    if std::env::var_os("J3C_TIMINGS").is_some() {
        eprintln!(
            "j3c: clang (split{}): {:.3}s",
            if vm_cached { ", VM cached" } else { "" },
            started.elapsed().as_secs_f64()
        );
    }
    if !linked.status.success() {
        return Err(format!("{clang} link failed:\n{}", String::from_utf8_lossy(&linked.stderr)));
    }
    Ok(exe_path)
}

/// Build-cache key for `cmd` compiling `source`, leaving out the per-run temp file name.
fn build_key(temp: &std::path::Path, source: &str, cmd: &Command) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut h);
    cmd.get_program().hash(&mut h);
    for arg in cmd.get_args() {
        if arg != temp.as_os_str() {
            arg.hash(&mut h);
        }
    }
    format!("{:016x}", h.finish())
}

pub fn compile_c(c_source: &str, opts: &CcOpts, target: &str, clang: &str) -> Result<PathBuf, String> {
    let pid = std::process::id();
    let seq = RUN_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let stamp = format!("j3c_{pid}_{seq}_{nanos}");
    let dir = std::env::temp_dir();
    let src_path = dir.join(format!("{stamp}.c"));
    let exe_path = match &opts.out {
        Some(p) => {
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
            }
            p.clone()
        }
        None => dir.join(format!("{stamp}.bin")),
    };

    fs::write(&src_path, c_source).map_err(|e| format!("write {}: {e}", src_path.display()))?;

    let std = if opts.std.is_empty() {
        "c11"
    } else {
        opts.std.as_str()
    };
    let opt = clang_opt(&opts.opt);

    let compiler = if target.is_empty() { "cc".to_string() } else { clang_for(clang, target) };
    let compiler = compiler.as_str();
    let mut cmd = Command::new(compiler);
    target_args(&mut cmd, target);
    cmd.arg(format!("-std={std}"))
        .arg(format!("-O{opt}"))
        .arg("-ffunction-sections")
        .arg("-fdata-sections")
        .arg(link_gc_flag(target));
    if opts.debug {
        cmd.arg("-g");
    }
    let release = opts.release.then(|| release_flags(target_os(target), lld_present()));
    if let Some(r) = &release {
        cmd.args(&r.compile);
    }
    cmd.arg("-o").arg(&exe_path).arg(&src_path);
    append_link_args(&mut cmd, opts, c_source, target)?;
    if let Some(r) = &release {
        cmd.args(&r.link);
    }
    crate::cli::status("compiling");
    let cc = cmd.output().map_err(|e| hints::spawn_err(compiler, e))?;

    if !cc.status.success() {
        let err = String::from_utf8_lossy(&cc.stderr);
        let _ = fs::remove_file(&src_path);
        return Err(format!("{compiler} failed:\n{err}"));
    }

    let _ = fs::remove_file(&src_path);
    Ok(exe_path)
}

pub fn run_exe(exe: &Path) -> Result<RunResult, String> {
    run_exe_in(exe, None, &[])
}

/// Exit code, or `128 + n` when a signal killed the process (the shell convention).
pub(crate) fn exit_code(status: &std::process::ExitStatus) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(n) = status.signal() {
            return 128 + n;
        }
    }
    status.code().unwrap_or(1)
}

pub fn run_exe_in(exe: &Path, cwd: Option<&Path>, args: &[String]) -> Result<RunResult, String> {
    let absolute = exe
        .canonicalize()
        .map_err(|e| format!("run {}: {e}", exe.display()))?;
    let mut cmd = Command::new(&absolute);
    cmd.args(args);
    if let Some(dir) = cwd {
        let root = dir
            .canonicalize()
            .map_err(|e| format!("project root {}: {e}", dir.display()))?;
        cmd.current_dir(&root);
        cmd.envs(crate::dist::steam_dev_env(&root));
        cmd.env("J3_ROOT", root);
        // a dev run lets F12 open the UI inspector (`j3.ui.inspect`); `J3_INSPECT=0` turns that off
        cmd.env("J3_INSPECT", std::env::var("J3_INSPECT").unwrap_or_else(|_| "1".into()));
    }
    let run = cmd
        .output()
        .map_err(|e| format!("run {}: {e}", exe.display()))?;
    let root = match cwd {
        Some(dir) => dir.canonicalize().ok(),
        None => std::env::current_dir().ok(),
    };
    let raw = String::from_utf8_lossy(&run.stderr);
    let raw: String = match &root {
        Some(root) => raw.lines().map(|l| abort_context(l, root).unwrap_or_else(|| l.to_string()) + "\n").collect(),
        None => raw.into_owned(),
    };
    let stderr = symbolize(&absolute, &raw).unwrap_or(raw).into_bytes();
    Ok(RunResult {
        status: exit_code(&run.status),
        stdout: run.stdout,
        stderr,
    })
}





pub(crate) fn clang_opt(opt: &str) -> &str {
    match opt {
        "" => "0",
        "S" => "s",
        "Z" => "z",
        other => other,
    }
}

fn libs_from_c(source: &str) -> Vec<String> {
    let mut libs = Vec::new();
    for marker in ["/* use lib \"", "; use lib \""] {
        let mut rest = source;
        while let Some(start) = rest.find(marker) {
            rest = &rest[start + marker.len()..];
            let end = match rest.find('"') {
                Some(i) => i,
                None => break,
            };
            let name = rest[..end].to_string();
            if !name.is_empty() && !libs.iter().any(|l| l == &name) {
                libs.push(name);
            }
            rest = &rest[end..];
        }
    }
    libs
}

