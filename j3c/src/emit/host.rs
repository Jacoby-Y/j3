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
        .arg(link_gc_flag(target));
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
    append_link_args(&mut cmd, cc, ll_source, target)?;
    // A named output rebuilt from the same IR and command is already up to date. Most
    // editor Plays change only scene files, which the program reads at run time.
    let stamp_path = cc.out.as_ref().map(|p| p.with_extension("j3c-stamp"));
    let key = build_key(&src_path, ll_source, &cmd);
    if let Some(stamp) = &stamp_path {
        if exe_path.exists() && fs::read_to_string(stamp).map(|k| k == key).unwrap_or(false) {
            let _ = fs::remove_file(&src_path);
            return Ok(exe_path);
        }
        let _ = fs::remove_file(stamp);
    }
    crate::cli::status("compiling");
    let started = std::time::Instant::now();
    let out = cmd.output().map_err(|e| format!("spawn {clang}: {e}"))?;
    if std::env::var_os("J3C_TIMINGS").is_some() {
        eprintln!("j3c: clang: {:.3}s", started.elapsed().as_secs_f64());
    }
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let _ = fs::remove_file(&src_path);
        return Err(format!("{clang} failed:\n{err}"));
    }
    let _ = fs::remove_file(&src_path);
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
    let clang = if clang.is_empty() { "clang" } else { clang };
    let base = exe_path.display().to_string();
    let vm_src = PathBuf::from(format!("{base}.vm.ll"));
    let vm_obj = PathBuf::from(format!("{base}.vm.o"));
    let vm_stamp = PathBuf::from(format!("{base}.vm.key"));
    let main_src = PathBuf::from(format!("{base}.main.ll"));
    let main_obj = PathBuf::from(format!("{base}.main.o"));
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
        if !target.is_empty() {
            cmd.arg(format!("--target={target}"));
        }
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
            .map_err(|e| format!("spawn {clang}: {e}"))?;
        vm_child = Some(child);
    }
    fs::write(&main_src, main_ll).map_err(|e| format!("write {}: {e}", main_src.display()))?;
    let out = compile(&main_src, &main_obj, clang_opt(&cc.opt), cc.debug)
        .output()
        .map_err(|e| format!("spawn {clang}: {e}"))?;
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
    if !target.is_empty() {
        link.arg(format!("--target={target}"));
    }
    for flag in extra {
        link.arg(flag);
    }
    link.arg("-o").arg(&exe_path).arg(&main_obj).arg(&vm_obj);
    append_link_args(&mut link, cc, main_ll, target)?;
    let linked = link.output().map_err(|e| format!("spawn {clang}: {e}"))?;
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
        .arg(link_gc_flag(target));
    if opts.debug {
        cmd.arg("-g");
    }
    cmd.arg("-o").arg(&exe_path).arg(&src_path);
    append_link_args(&mut cmd, opts, c_source, target)?;
    crate::cli::status("compiling");
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
        cmd.env("J3_ROOT", root);
    }
    let run = cmd
        .output()
        .map_err(|e| format!("run {}: {e}", exe.display()))?;
    let stderr = match symbolize(&absolute, &String::from_utf8_lossy(&run.stderr)) {
        Some(text) => text.into_bytes(),
        None => run.stderr,
    };
    Ok(RunResult {
        status: exit_code(&run.status),
        stdout: run.stdout,
        stderr,
    })
}

fn signal_name(n: i32) -> &'static str {
    match n {
        4 => "SIGILL",
        6 => "SIGABRT",
        7 => "SIGBUS",
        8 => "SIGFPE",
        11 => "SIGSEGV",
        _ => "signal",
    }
}

fn hex(s: &str) -> Option<u64> {
    u64::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok()
}

/// Rewrite the crash handler's `j3 crash:` / `j3 pc:` / `j3 at:` lines (raw addresses) into
/// `function at file:line` using `nm` and `addr2line` when installed. `None` when `err` has no
/// crash report, so ordinary stderr passes through untouched.
pub(crate) fn symbolize(exe: &Path, err: &str) -> Option<String> {
    let mut sig = None;
    let mut base = None;
    let mut addrs: Vec<u64> = Vec::new();
    let mut rest = String::new();
    for line in err.lines() {
        if let Some(v) = line.strip_prefix("j3 crash: signal ") {
            sig = v.trim().parse::<i32>().ok();
        } else if let Some(v) = line.strip_prefix("j3 base: ") {
            base = hex(v);
        } else if let Some(v) = line.strip_prefix("j3 pc: ").or_else(|| line.strip_prefix("j3 at: ")) {
            addrs.extend(hex(v));
        } else {
            rest.push_str(line);
            rest.push('\n');
        }
    }
    let sig = sig?;
    let mut out = rest;
    out.push_str(&format!("crash: {} ({sig})\n", signal_name(sig)));
    let names = base.and_then(|b| addr_names(exe, b, &addrs));
    let mut prev: Option<String> = None;
    let mut count = 0;
    let flush = |out: &mut String, prev: &mut Option<String>, count: &mut usize| {
        if let Some(p) = prev.take() {
            out.push_str(&format!("  {p}"));
            if *count > 1 {
                out.push_str(&format!("  (x{count})"));
            }
            out.push('\n');
        }
        *count = 0;
    };
    for (i, a) in addrs.iter().enumerate() {
        let text = match &names {
            Some(n) => n[i].clone(),
            None => format!("{a:#x}"),
        };
        if prev.as_deref() != Some(text.as_str()) {
            flush(&mut out, &mut prev, &mut count);
            prev = Some(text);
        }
        count += 1;
    }
    flush(&mut out, &mut prev, &mut count);
    Some(out)
}

/// `function at file:line` for each runtime address; needs `nm` (to find the load offset from
/// `main`) and `addr2line`. Return addresses (all but the first, the faulting pc) step back a byte.
fn addr_names(exe: &Path, runtime_main: u64, addrs: &[u64]) -> Option<Vec<String>> {
    let nm = Command::new("nm").arg(exe).output().ok()?;
    let nm = String::from_utf8_lossy(&nm.stdout).into_owned();
    let static_main = nm.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let (addr, kind, name) = (it.next()?, it.next()?, it.next()?);
        (name == "main" && kind.eq_ignore_ascii_case("t")).then(|| hex(addr)).flatten()
    })?;
    let offset = runtime_main.wrapping_sub(static_main);
    let mut cmd = Command::new("addr2line");
    cmd.args(["-f", "-C", "-p", "-e"]).arg(exe);
    for (i, a) in addrs.iter().enumerate() {
        let a = a.wrapping_sub(offset).wrapping_sub(u64::from(i > 0));
        cmd.arg(format!("{a:#x}"));
    }
    let out = cmd.output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let names: Vec<String> = text.lines().map(|l| l.to_string()).collect();
    (names.len() == addrs.len()).then_some(names)
}

pub(crate) fn link_gc_flag(target: &str) -> &'static str {
    let t = target.to_ascii_lowercase();
    if t.contains("apple") || t.contains("darwin") {
        "-Wl,-dead_strip"
    } else {
        "-Wl,--gc-sections"
    }
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

pub(crate) fn append_link_args(cmd: &mut Command, opts: &CcOpts, source: &str, target: &str) -> Result<(), String> {
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
        cmd.arg(link_lib(lib, &opts.search, target)?);
    }
    for flag in &opts.flags {
        if keep_flag(flag, target) {
            cmd.arg(flag);
        }
    }
    Ok(())
}

/// True when the host linker can see `lib{name}`.
pub fn host_lib_present(name: &str, search: &[String]) -> bool {
    link_lib(name, search, "").is_ok()
}

/// `-lNAME` when the linker can see `libNAME.so`; otherwise a found `libNAME*.so*`.
/// Debian often ships `libSDL2-2.0.so.0` without the `-dev` symlink.
/// Windows and macOS pass `-lNAME` through so a missing library is the linker's error.
fn link_lib(name: &str, search: &[String], target: &str) -> Result<String, String> {
    if foreign_target(target) {
        return Ok(format!("-l{name}"));
    }
    if name == "m" {
        return Ok("-lm".to_string());
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
            return Ok(format!("-l{name}"));
        }
    }
    for dir in &dirs {
        if let Some(found) = find_soname(dir, name) {
            return Ok(found);
        }
    }
    let msg = format!("cannot find library `{name}`");
    Err(match failure_hint(&msg) {
        Some(hint) => format!("{msg}\n  = help: {hint}"),
        None => format!("{msg}\n  = help: install it, or take it out of [link] libs"),
    })
}

/// Extra line for a failed `run`/`build` that is not a J3 diagnostic.
/// Names a package when the log mentions a known library or a missing compiler.
pub fn failure_hint(text: &str) -> Option<String> {
    if text.contains("-->") {
        return None;
    }
    let mut pkgs: Vec<&str> = Vec::new();
    let mut push = |pkg: &'static str| {
        if !pkgs.contains(&pkg) {
            pkgs.push(pkg);
        }
    };
    if text.contains("SDL2_image") || text.contains("libSDL2_image") || text.contains("IMG_") {
        push("libsdl2-image-2.0-0");
    }
    if text.contains("SDL2_mixer") || text.contains("libSDL2_mixer") || text.contains("Mix_") {
        push("libsdl2-mixer-2.0-0");
    }
    if text.contains("SDL2_ttf") || text.contains("libSDL2_ttf") || text.contains("TTF_") {
        push("libsdl2-ttf-2.0-0");
    }
    if sdl2_core(text) {
        push("libsdl2-2.0-0");
    }
    if text.contains("library `GL`") || text.contains("-lGL") || text.contains("libGL") {
        push("libgl1");
    }
    let mut lines: Vec<String> = Vec::new();
    if !pkgs.is_empty() {
        let list = pkgs.join(" ");
        lines.push(format!("install {list}\n         `sudo apt install {list}`"));
    }
    if text.contains("spawn clang")
        || text.contains("spawn cc")
        || text.contains("clang: not found")
        || text.contains("cc: not found")
    {
        lines.push("install clang\n         `sudo apt install clang`".to_string());
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n  = help: "))
    }
}

/// `SDL2` itself, not `SDL2_image` / `SDL2_mixer` / `SDL2_ttf`.
fn sdl2_core(text: &str) -> bool {
    let mut rest = text;
    while let Some(i) = rest.find("SDL2") {
        let after = rest[i + 4..].chars().next();
        if after != Some('_') {
            return true;
        }
        rest = &rest[i + 4..];
    }
    false
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

#[cfg(test)]
mod tests {
    #[test]
    fn link_gc_flag_matches_target() {
        assert_eq!(super::link_gc_flag(""), "-Wl,--gc-sections");
        assert_eq!(super::link_gc_flag("x86_64-pc-linux-gnu"), "-Wl,--gc-sections");
        assert_eq!(super::link_gc_flag("x86_64-pc-windows-gnu"), "-Wl,--gc-sections");
        assert_eq!(super::link_gc_flag("arm64-apple-darwin"), "-Wl,-dead_strip");
        assert_eq!(super::link_gc_flag("x86_64-apple-darwin"), "-Wl,-dead_strip");
    }

    #[test]
    fn missing_library_names_itself() {
        let err = super::link_lib("not-a-real-lib", &[], "").unwrap_err();
        assert!(err.contains("not-a-real-lib"), "{err}");
        assert!(err.contains("take it out of [link] libs"), "{err}");
    }

    #[test]
    fn missing_sdl_names_the_package() {
        let err = super::failure_hint("cannot find library `SDL2_mixer`").unwrap();
        assert!(err.contains("libsdl2-mixer-2.0-0"), "{err}");
        assert!(!err.contains("libsdl2-2.0-0"), "{err}");
        let image = super::failure_hint("libSDL2_image-2.0.so.0\n").unwrap();
        assert!(image.contains("libsdl2-image-2.0-0"), "{image}");
        let both = super::failure_hint(
            "undefined reference to `IMG_Quit'\nundefined reference to `TTF_Quit'",
        )
        .unwrap();
        assert!(both.contains("libsdl2-image-2.0-0"), "{both}");
        assert!(both.contains("libsdl2-ttf-2.0-0"), "{both}");
        assert!(super::failure_hint("error: expected `;`\n --> src/main.j3:1:1\n").is_none());
        let cc = super::failure_hint("spawn clang: No such file or directory (os error 2)").unwrap();
        assert!(cc.contains("sudo apt install clang"), "{cc}");
    }
}
