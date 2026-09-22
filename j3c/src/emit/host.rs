//! Host `cc` + run. No extra crates.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::project::Manifest;

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
            out: Some(m.out_path()),
        }
    }
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
    fs::write(&src_path, ll_source).map_err(|e| format!("write {}: {e}", src_path.display()))?;

    let opt = clang_opt(&cc.opt);
    let clang = if clang.is_empty() { "clang" } else { clang };

    let mut cmd = Command::new(clang);
    cmd.arg("-Wno-override-module")
        .arg(format!("-O{opt}"))
        .arg("-ffunction-sections")
        .arg("-fdata-sections")
        .arg("-Wl,--gc-sections");
    if cc.debug {
        cmd.arg("-g");
    }
    if !target.is_empty() {
        cmd.arg(format!("--target={target}"));
    }
    if !cpu.is_empty() {
        cmd.arg(format!("-mcpu={cpu}"));
    }
    for flag in extra {
        cmd.arg(flag);
    }
    cmd.arg("-o").arg(&exe_path).arg(&src_path);
    append_link_args(&mut cmd, cc, ll_source, target);
    let out = cmd.output().map_err(|e| format!("spawn {clang}: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let _ = fs::remove_file(&src_path);
        return Err(format!("{clang} failed:\n{err}"));
    }
    let _ = fs::remove_file(&src_path);
    Ok(exe_path)
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

    let compiler = if target.is_empty() {
        "cc"
    } else if clang.is_empty() {
        "clang"
    } else {
        clang
    };
    let mut cmd = Command::new(compiler);
    if !target.is_empty() {
        cmd.arg(format!("--target={target}"));
    }
    cmd.arg(format!("-std={std}"))
        .arg(format!("-O{opt}"))
        .arg("-ffunction-sections")
        .arg("-fdata-sections")
        .arg("-Wl,--gc-sections");
    if opts.debug {
        cmd.arg("-g");
    }
    cmd.arg("-o").arg(&exe_path).arg(&src_path);
    append_link_args(&mut cmd, opts, c_source, target);
    let cc = cmd.output().map_err(|e| format!("spawn {compiler}: {e}"))?;

    if !cc.status.success() {
        let err = String::from_utf8_lossy(&cc.stderr);
        let _ = fs::remove_file(&src_path);
        return Err(format!("{compiler} failed:\n{err}"));
    }

    let _ = fs::remove_file(&src_path);
    Ok(exe_path)
}

pub fn run_exe(exe: &Path) -> Result<RunResult, String> {
    run_exe_in(exe, None)
}

pub fn run_exe_in(exe: &Path, cwd: Option<&Path>) -> Result<RunResult, String> {
    let mut cmd = Command::new(exe);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
        cmd.env("J3_ROOT", dir);
    }
    let run = cmd
        .output()
        .map_err(|e| format!("run {}: {e}", exe.display()))?;
    Ok(RunResult {
        status: run.status.code().unwrap_or(1),
        stdout: run.stdout,
        stderr: run.stderr,
    })
}

fn foreign_target(target: &str) -> bool {
    let t = target.to_ascii_lowercase();
    t.contains("windows") || t.contains("apple") || t.contains("darwin")
}

fn windows_target(target: &str) -> bool {
    target.to_ascii_lowercase().contains("windows")
}

fn keep_flag(flag: &str, target: &str) -> bool {
    if !windows_target(target) {
        return true;
    }
    let f = flag.to_ascii_lowercase();
    !f.contains("rpath") && !f.contains("--disable-new-dtags")
}

fn append_link_args(cmd: &mut Command, opts: &CcOpts, source: &str, target: &str) {
    for file in &opts.files {
        cmd.arg(file);
    }
    for dir in &opts.search {
        cmd.arg(format!("-L{dir}"));
        if !foreign_target(target) {
            cmd.arg(format!("-Wl,-rpath-link,{dir}"));
        }
    }
    let mut libs = libs_from_c(source);
    for lib in &opts.libs {
        if !libs.iter().any(|l| l == lib) {
            libs.push(lib.clone());
        }
    }
    for lib in &libs {
        if let Some(flag) = link_lib(lib, &opts.search, target) {
            cmd.arg(flag);
        }
    }
    for flag in &opts.flags {
        if keep_flag(flag, target) {
            cmd.arg(flag);
        }
    }
}

/// `-lNAME` when the linker can see `libNAME.so`; otherwise a found `libNAME*.so*`.
/// Debian often ships `libSDL2-2.0.so.0` without the `-dev` symlink.
/// Windows and macOS pass `-lNAME` through so a missing library is the linker's error.
fn link_lib(name: &str, search: &[String], target: &str) -> Option<String> {
    if foreign_target(target) {
        return Some(format!("-l{name}"));
    }
    if name == "m" {
        return Some("-lm".to_string());
    }
    let mut dirs: Vec<PathBuf> = search.iter().map(PathBuf::from).collect();
    for extra in [
        "/usr/lib/x86_64-linux-gnu",
        "/lib/x86_64-linux-gnu",
        "/usr/lib",
        "/usr/local/lib",
    ] {
        let p = PathBuf::from(extra);
        if p.is_dir() && !dirs.iter().any(|d| d == &p) {
            dirs.push(p);
        }
    }
    for dir in &dirs {
        if dir.join(format!("lib{name}.so")).is_file() {
            return Some(format!("-l{name}"));
        }
    }
    for dir in &dirs {
        if let Some(found) = find_soname(dir, name) {
            return Some(found);
        }
    }
    None
}

fn find_soname(dir: &Path, name: &str) -> Option<String> {
    let prefix = format!("lib{name}");
    let entries = fs::read_dir(dir).ok()?;
    let mut best: Option<PathBuf> = None;
    for ent in entries.flatten() {
        let p = ent.path();
        let fname = match p.file_name().and_then(|n| n.to_str()) {
            Some(s) => s,
            None => continue,
        };
        let dashed = format!("{prefix}-");
        let dotted = format!("{prefix}.so");
        if (fname.starts_with(&dashed) || fname.starts_with(&dotted)) && fname.contains(".so") {
            best = Some(p);
            break;
        }
    }
    best.map(|p| p.display().to_string())
}

fn clang_opt(opt: &str) -> &str {
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
