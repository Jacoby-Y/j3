//! Package tools (a dep's `[commands]` and `[dist_outputs]`, `project/deps/tools.rs`): build one for
//! the host and run it. `j3c pkg` (`cli/pkg.rs`) and `dist` call [`run`]; the contract a tool sees
//! (env, manifest JSON, exit code) is `j3c doc packages`, versioned by [`TOOL_API`].
//!
//! A tool compiles with its **package** as the project (`use <pkg>.x;` is the package's `src/x.j3`,
//! its own deps resolve from the game's flat dep list), for the host at opt 0 with no window. The
//! game's `[emit]`, `[link]`, `[dist]` never reach it. The exe is cached under
//! `~/.cache/j3/tools/<pkg>-<hash>/<tool>` (`J3_PKG_DIR` set: `$J3_PKG_DIR/tools/`), keyed on the
//! j3c binary, the tool, and the bytes of the package's `src/` and `tools/` and of every other dep.

use crate::project::{Cc, Dep, Emit, Manifest, Tool};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

/// `J3_TOOL_API`: bump it when the env, flags or manifest JSON a tool sees change.
pub const TOOL_API: &str = "1";

/// The cached exe of `tool`, built first (stderr: `building PKG:TOOL (first run)`) when its key changed.
/// A build error carries the compiler's diagnostics.
pub fn build(project: &Manifest, dep: &Dep, tool: &Tool) -> Result<PathBuf, String> {
    let m = tool_manifest(project, dep, tool);
    let dir = cache_dir()?.join(format!("{}-{:016x}", dep.name, key(&m, tool)));
    let exe = dir.join(&tool.name);
    if exe.is_file() {
        return Ok(exe);
    }
    let label = format!("{}:{}", dep.name, tool.name);
    eprintln!("building {label} (first run)");
    let fail = |e: String| format!("building {label} ({}) failed:\n{e}", tool.file.display());
    // Tools are dev builds whoever asks (`j3c pkg`, or packaging under `--release`): `j3.build.RELEASE` is
    // false in a tool, so one cached exe serves both. Dist tools get `--release` as a flag. The
    // loader reads the global (`loader/paths.rs`); nothing else runs on this thread meanwhile.
    let release = crate::cli::RELEASE.swap(false, std::sync::atomic::Ordering::Relaxed);
    let prog = crate::compile::compile_tool(&m);
    crate::cli::RELEASE.store(release, std::sync::atomic::Ordering::Relaxed);
    let prog = prog.map_err(fail)?;
    let mut opts = crate::emit::EmitOpts::from_manifest(&m).map_err(fail)?;
    opts.backend = crate::emit::resolve_backend(opts.backend, false, &opts.clang).map_err(fail)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    // Build beside, then rename: a killed build never leaves an exe the next run trusts.
    let tmp = dir.join(format!("{}.tmp{}", tool.name, std::process::id()));
    opts.cc.out = Some(tmp.clone());
    let built = crate::emit::compile_program(&prog.cx, &prog.modules, &opts);
    // Objects are never reused: a changed source is a new `<pkg>-<hash>` dir.
    let _ = std::fs::remove_dir_all(dir.join(".j3c"));
    let built = built.inspect_err(|_| drop(std::fs::remove_file(&tmp))).map_err(fail)?;
    std::fs::rename(&built, &exe).map_err(|e| format!("rename {} to {}: {e}", built.display(), exe.display()))?;
    Ok(exe)
}

/// Build `tool` if needed and run it to the end; stdin/stdout/stderr are inherited. The exit code is
/// the tool's; death by signal N is `128 + N` (with a line on stderr saying so).
pub fn run(project: &Manifest, dep: &Dep, tool: &Tool, args: &[String], env: &[(String, String)]) -> Result<i32, String> {
    let status = command(project, dep, tool, args, env)?
        .status()
        .map_err(|e| format!("run {}:{}: {e}", dep.name, tool.name))?;
    Ok(exit_code(status, dep, tool))
}

/// The tool's exit code; death by signal N is `128 + N` (with a line on stderr saying so).
pub(crate) fn exit_code(status: std::process::ExitStatus, dep: &Dep, tool: &Tool) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        let sig = status.signal().unwrap_or(0);
        eprintln!("j3c: {}:{} killed by signal {sig}", dep.name, tool.name);
        return 128 + sig;
    }
    #[allow(unreachable_code)]
    1
}

/// The process [`run`] starts, not started yet (tests read its output): the tool built, the
/// manifest JSON written, cwd the project root, the runner's env set, then `env` on top.
pub fn command(project: &Manifest, dep: &Dep, tool: &Tool, args: &[String], env: &[(String, String)]) -> Result<Command, String> {
    let root = std::fs::canonicalize(&project.root).map_err(|e| format!("{}: {e}", project.root.display()))?;
    let exe = build(project, dep, tool)?;
    let manifest = write_manifest(&root)?;
    let j3c = std::env::current_exe().map_err(|e| format!("path of j3c: {e}"))?;
    let mut c = Command::new(exe);
    c.args(args)
        .current_dir(&root)
        .env("J3_PROJECT_ROOT", &root)
        .env("J3_PKG_ROOT", &dep.pkg_root)
        .env("J3_PKG_NAME", &dep.name)
        .env("J3C", j3c)
        .env("J3_TOOL_API", TOOL_API)
        .env("J3_MANIFEST", manifest)
        .envs(env.iter().map(|(k, v)| (k, v)));
    Ok(c)
}

/// `target/.j3c/manifest.json`: the project's whole `j3.toml` as JSON (`j3c manifest --json`).
fn write_manifest(root: &Path) -> Result<PathBuf, String> {
    let toml = root.join("j3.toml");
    let text = std::fs::read_to_string(&toml).map_err(|e| format!("read {}: {e}", toml.display()))?;
    let json = crate::project::toml_json(&text).map_err(|e| e.replacen("j3.toml", &toml.display().to_string(), 1))?;
    let dir = root.join("target/.j3c");
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    let path = dir.join("manifest.json");
    std::fs::write(&path, json + "\n").map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(path)
}

/// The package as a project whose entry is the tool: host, opt 0, nothing else of the game's.
fn tool_manifest(project: &Manifest, dep: &Dep, tool: &Tool) -> Manifest {
    Manifest {
        root: dep.pkg_root.clone(),
        name: dep.name.clone(),
        bin: dep.name.clone(),
        version: "0.0.0".into(),
        description: String::new(),
        entry: tool.file.clone(),
        cc: Cc::default(),
        emit: Emit { opt: "0".into(), ..Emit::default() },
        link: Default::default(),
        dist: Default::default(),
        android: Default::default(),
        ui: None,
        // The game's list is flat and has each name once, so the package's own deps are in it.
        deps: project.deps.iter().filter(|d| d.name != dep.name).cloned().collect(),
        lib: false,
    }
}

// ponytail: hashes every byte each run (no mtime shortcut) and keeps old `<pkg>-<hash>` dirs; add a
// stamp or a sweep if packages grow big or the cache does.
fn key(m: &Manifest, tool: &Tool) -> u64 {
    let mut h = DefaultHasher::new();
    crate::emit::incr::compiler_id().hash(&mut h);
    env!("CARGO_PKG_VERSION").hash(&mut h);
    tool.name.hash(&mut h);
    tool.file.hash(&mut h);
    std::fs::read(&tool.file).unwrap_or_default().hash(&mut h);
    for sub in ["src", "tools"] {
        crate::editor::hash_dir(&m.root.join(sub), &mut h);
    }
    for d in &m.deps {
        crate::editor::hash_dir(&d.root, &mut h);
    }
    if let Some(s) = std::env::var_os("J3_STDLIB").filter(|s| !s.is_empty()) {
        crate::editor::hash_dir(Path::new(&s), &mut h);
    }
    h.finish()
}

#[cfg(test)]
thread_local! {
    /// Tests point the cache here (per test thread) instead of racing on `J3_PKG_DIR`.
    pub static TEST_CACHE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

fn cache_dir() -> Result<PathBuf, String> {
    #[cfg(test)]
    if let Some(d) = TEST_CACHE.with(|c| c.borrow().clone()) {
        return Ok(d);
    }
    if let Some(d) = std::env::var_os("J3_PKG_DIR").filter(|s| !s.is_empty()) {
        return Ok(PathBuf::from(d).join("tools"));
    }
    crate::editor::cache_root()
        .map(|c| c.join("tools"))
        .ok_or_else(|| "no cache directory for package tools (set HOME or J3_PKG_DIR)".into())
}
