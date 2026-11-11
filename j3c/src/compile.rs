//! Load `.j3` files, parse, typecheck. Always pulls in `j3.sys` for `start`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::check::{self, ParsedFile};

pub use crate::check::Program;
use crate::parse;
use crate::project::Manifest;
use crate::span::FileId;
use crate::stdlib::stdlib_dir;

mod script_deps;
pub mod dev;
pub mod scene_scripts;
mod loader;

pub use loader::*;

#[derive(Clone, Copy)]
enum GameBackend {
    Sdl,
    Headless,
}

#[derive(Clone, Copy)]
enum OsKind {
    Linux,
    Windows,
    Macos,
    Android,
}

/// The CPU a target triple names, as the index of `j3.sys.ARCH_*` (empty: the host).
fn arch_kind(target: &str) -> usize {
    let t = if target.is_empty() { std::env::consts::ARCH } else { target };
    let t = t.split('-').next().unwrap_or("");
    match t {
        "x86_64" | "amd64" => 0,
        "aarch64" | "arm64" | "arm64e" => 1,
        "i386" | "i486" | "i586" | "i686" | "x86" => 2,
        t if t.starts_with("arm") || t.starts_with("thumb") => 3,
        t if t.starts_with("riscv") => 4,
        _ => 5,
    }
}

fn os_kind(target: &str) -> OsKind {
    use crate::emit::host::{target_os, TargetOs};
    if target.is_empty() {
        return OsKind::Linux;
    }
    match target_os(target) {
        TargetOs::Windows => OsKind::Windows,
        TargetOs::Macos => OsKind::Macos,
        TargetOs::Android => OsKind::Android,
        TargetOs::Linux | TargetOs::Other => OsKind::Linux,
    }
}

pub fn compile_path(entry: &Path) -> Result<Program, String> {
    load_and_check(entry, GameBackend::Sdl, true, None, "")
}

pub fn compile_path_target(entry: &Path, target: &str) -> Result<Program, String> {
    load_and_check(entry, GameBackend::Sdl, true, None, target)
}

pub fn compile_path_headless(entry: &Path) -> Result<Program, String> {
    load_and_check(entry, GameBackend::Headless, true, None, "")
}

pub fn compile_manifest(manifest: &Manifest) -> Result<Program, String> {
    compile_manifest_target(manifest, manifest.emit.primary())
}

pub fn compile_manifest_target(manifest: &Manifest, target: &str) -> Result<Program, String> {
    load_and_check(
        &manifest.entry,
        GameBackend::Sdl,
        !manifest.lib,
        Some(manifest),
        target,
    )
}

/// Check a project for a dev build: like `compile_manifest`, plus `j3.script` (the VM)
/// when the program has scripts.
pub fn compile_manifest_dev(manifest: &Manifest) -> Result<Program, String> {
    load_and_check_with(
        &manifest.entry,
        GameBackend::Sdl,
        true,
        Some(manifest),
        manifest.emit.primary(),
        true,
    )
}

pub fn check_path_lib(entry: &Path) -> Result<Program, String> {
    load_and_check(entry, GameBackend::Sdl, false, None, "")
}

/// A `.j3s` entry is a tool: one instance, then `pub fn main()`. Scene scripts are not this.
fn tool_host(entry: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(entry)
        .map_err(|e| format!("read {}: {e}", entry.display()))?;
    let module = vec!["script".to_string(), "tool".to_string()];
    let ast = match parse::parse_script(&text, FileId(1), &module) {
        Ok(ast) => ast,
        Err(e) => return Err(parse::format_error(&entry.display().to_string(), &text, &e)),
    };
    let ready = ast.items.iter().any(|item| match item {
        parse::Item::Fn(f) => {
            f.vis_pub && f.name.name == "main" && f.params.is_empty() && f.ret.is_none()
        }
        _ => false,
    });
    if !ready {
        return Err(format!(
            "{} has no `pub fn main()`\n  = help: this file is a tool entry; scene scripts are started by the game (`script=` on an object)",
            entry.display()
        ));
    }
    let type_name = check::script::script_name(&entry.display().to_string());
    let ident = type_name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && type_name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !ident {
        return Err(format!(
            "{} script type `{type_name}` is not a name the host can bind",
            entry.display()
        ));
    }
    let file_name = entry
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("script path has no file name: {}", entry.display()))?;
    Ok(format!(
        "module tool_host;\nconst {type_name} = @script(\"{file_name}\");\nfn main() {{\n    let tool = {type_name} {{}};\n    tool.main();\n}}\n"
    ))
}

fn load_and_check(
    entry: &Path,
    game: GameBackend,
    require_main: bool,
    project: Option<&Manifest>,
    target: &str,
) -> Result<Program, String> {
    load_and_check_with(entry, game, require_main, project, target, false)
}

fn load_and_check_with(
    entry: &Path,
    game: GameBackend,
    require_main: bool,
    project: Option<&Manifest>,
    target: &str,
    dev: bool,
) -> Result<Program, String> {
    let script_entry = entry.extension().and_then(|e| e.to_str()) == Some("j3s");
    let script_root = if script_entry {
        match entry.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
            _ => PathBuf::from("."),
        }
    } else {
        project
            .map(|m| m.root.clone())
            .unwrap_or_else(|| entry.parent().unwrap_or(Path::new(".")).to_path_buf())
    };
    let mut overlays = Vec::new();
    let entry_path = if script_entry {
        let host = tool_host(entry)?;
        let host_path = script_root.join(".j3c-tool-host.j3");
        overlays.push((host_path.clone(), host));
        host_path
    } else {
        entry.to_path_buf()
    };
    let mut loader = Loader {
        files: Vec::new(),
        loaded: HashSet::new(),
        next_file: 1,
        game,
        os: os_kind(target),
        arch: arch_kind(target),
        src_dir: project.map(|m| m.src_dir()),
        package: project.map(|m| m.name.clone()),
        version: project.map(|m| m.version.clone()),
        deps: project.map(|m| m.deps.clone()).unwrap_or_default(),
        script_root,
        overlays,
        scene_scan: None,
        ui_cfg: ui_cfg(project),
        ui_gen: None,
        ui_err: None,
    };
    crate::phase::begin("parse");
    let t_load = std::time::Instant::now();
    loader.load_path(&entry_path)?;
    loader.load_module(&["j3".to_string(), "sys".to_string()])?;
    loader.load_imports()?;
    if project.is_some() {
        loader.load_scene_scripts()?;
    }
    if dev && loader.files.iter().any(|f| f.ast.is_script) {
        loader.load_module(&["j3".to_string(), "script".to_string()])?;
        loader.load_imports()?;
    }
    check_import_cycles(&loader.files)?;
    crate::cli::timing(&format!("  load + parse ({} files)", loader.files.len()), t_load);
    crate::phase::detail("copying sources for error messages");
    let srcs: Vec<(String, String)> = loader
        .files
        .iter()
        .map(|f| (f.path.clone(), f.src.clone()))
        .collect();
    let ui_gen = loader.ui_gen.take();
    crate::phase::begin("check");
    let checked = if require_main {
        check::check_files(loader.files)
    } else {
        check::check_files_lib(loader.files)
    };
    let mut prog = checked.map_err(|e| match &ui_gen {
        Some(g) => {
            let mut all = srcs.clone();
            all.extend(g.sources());
            g.remap(e).render(&all)
        }
        None => e.render(&srcs),
    })?;
    crate::phase::detail("post-check passes");
    if let Some(g) = &ui_gen {
        if let Some(f) = prog.cx.files.iter_mut().find(|f| f.path == g.label) {
            f.locator = Some(g.locator());
        }
    }
    if project.map(|m| m.cc.fast || m.emit.fast).unwrap_or(false) {
        crate::ir::fast::strip_aborts(&mut prog.cx);
    }
    Ok(prog)
}

pub fn apply_fast(prog: &mut Program) {
    crate::ir::fast::strip_aborts(&mut prog.cx);
}

/// Typecheck for the LSP / editor. No `main` required. `overlays` replace disk.
pub fn analyze_path(
    path: &Path,
    overlays: &[(PathBuf, String)],
) -> Result<Program, String> {
    analyze_checked(path, overlays).map_err(|e| e.message)
}

/// Same as [`analyze_path`], but keeps the check span for the LSP.
pub fn analyze_checked(
    path: &Path,
    overlays: &[(PathBuf, String)],
) -> Result<Program, crate::check::CheckError> {
    analyze_ui(path, overlays).0
}

/// [`analyze_checked`] that also returns the generated `.j3ui` module (the span map), when the
/// project has one. Generation errors come back as a check error on the `.j3ui`.
pub fn analyze_ui(
    path: &Path,
    overlays: &[(PathBuf, String)],
) -> (Result<Program, crate::check::CheckError>, Option<crate::ui::Generated>) {
    let project = match crate::project::resolve(path) {
        Ok(crate::project::Input::Project(m)) => Some(m),
        _ => None,
    };
    let entry = project
        .as_ref()
        .map(|m| m.entry.clone())
        .unwrap_or_else(|| path.to_path_buf());
    let target = project
        .as_ref()
        .map(|m| m.emit.primary().to_string())
        .unwrap_or_default();
    let mut loader = Loader {
        files: Vec::new(),
        loaded: HashSet::new(),
        next_file: 1,
        game: GameBackend::Sdl,
        os: os_kind(&target),
        arch: arch_kind(&target),
        src_dir: project.as_ref().map(|m| m.src_dir()),
        package: project.as_ref().map(|m| m.name.clone()),
        version: project.as_ref().map(|m| m.version.clone()),
        deps: project.as_ref().map(|m| m.deps.clone()).unwrap_or_default(),
        script_root: project
            .as_ref()
            .map(|m| m.root.clone())
            .unwrap_or_else(|| entry.parent().unwrap_or(Path::new(".")).to_path_buf()),
        overlays: overlays.to_vec(),
        scene_scan: None,
        ui_cfg: ui_cfg(project.as_ref()),
        ui_gen: None,
        ui_err: None,
    };
    let loaded = (|| -> Result<(), String> {
        loader.load_path(&entry)?;
        if path != entry.as_path() {
            let _ = loader.load_path(path);
        }
        loader.load_module(&["j3".to_string(), "sys".to_string()])?;
        loader.load_imports()?;
        if project.is_some() {
            loader.load_scene_scripts()?;
        }
        check_import_cycles(&loader.files)
    })();
    if let Err(e) = loaded {
        let ce = match loader.ui_err.take() {
            Some(u) => {
                let span = crate::span::Span { file: crate::span::FileId(1), start: u.err.lo as u32, end: u.err.hi as u32 };
                let mut ce = crate::check::CheckError::new(u.path, span, u.err.msg);
                ce.helps.extend(u.err.help);
                ce
            }
            None => crate::check::CheckError::new(path.display().to_string(), crate::span::Span::dummy(), e),
        };
        return (Err(ce), None);
    }
    let ui_gen = loader.ui_gen.take();
    let r = check::check_files_lib(loader.files).map_err(|e| match &ui_gen {
        Some(g) => g.remap(e),
        None => e,
    });
    (r, ui_gen)
}

const HELLO_SRC: &str = include_str!("../tests/programs/hello.j3");

pub fn default_hello_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/programs/hello.j3")
}

fn builtin_hello(path: &Path) -> bool {
    path == default_hello_path()
}

#[cfg(test)]
mod tests;
