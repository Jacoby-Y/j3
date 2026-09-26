//! `j3c run` for a project with scripts (dev mode).
//!
//! The dev binary links the VM and never knows a script's layout or code, so a script edit
//! only rewrites `target/j3s/*.j3sb`: no emit-to-clang. The binary's cache key is the
//! `.j3` sources plus the lowered native IR (`compile::dev::native_key`). While the program
//! runs, `.j3s` edits are re-checked: code-only edits swap live (the VM polls
//! `target/j3s/reload`), layout edits relaunch the same binary, and a new native need
//! rebuilds first.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant, SystemTime};

use crate::check::Program;
use crate::compile::{self, dev};
use crate::emit::{self, bc::BcModule, EmitOpts};
use crate::project::Manifest;

pub struct Built {
    pub exe: PathBuf,
    /// Script path → (layout hash, signature hash).
    pub layouts: HashMap<String, (u64, u64)>,
    /// True when this prepare ran clang.
    pub rebuilt: bool,
    pub scripts: Vec<PathBuf>,
}

pub enum Prepared {
    /// The program has no scripts; run it the normal way.
    NoScripts(Box<Program>),
    Dev(Built),
}

/// Where the dev binary lives, next to the release one.
pub fn dev_out(m: &Manifest) -> PathBuf {
    let out = m.out_path();
    let name = out
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "a.out".to_string());
    out.with_file_name(format!("{name}-dev"))
}

/// Check, print scripts to bytecode, and make sure the dev binary is current.
pub fn prepare(m: &Manifest, opts: &EmitOpts, fast: bool) -> Result<Prepared, String> {
    let started = Instant::now();
    let mut prog = compile::compile_manifest_dev(m)?;
    if fast {
        compile::apply_fast(&mut prog);
    }
    crate::cli::timing("parse + check", started);
    if !dev::has_scripts(&prog) {
        return Ok(Prepared::NoScripts(Box::new(prog)));
    }
    crate::preflight::check(&prog, Some(m))?;
    let started = Instant::now();
    let bytecode = dev::bytecode(&prog)?;
    crate::cli::timing("bytecode", started);
    let scripts: Vec<PathBuf> = prog
        .cx
        .files
        .iter()
        .filter(|f| f.path.ends_with(".j3s"))
        .map(|f| PathBuf::from(&f.path))
        .collect();
    let started = Instant::now();
    let native_sources = native_source_key(&prog);
    let thunks = dev::lower(&mut prog, &bytecode)?;
    let key = dev_key(&native_sources, &prog, opts, fast);
    crate::cli::timing(&format!("dev lower ({thunks} thunks)"), started);
    let exe = dev_out(m);
    let stamp = exe.with_extension("j3c-dev");
    let modules: Vec<String> = prog
        .modules
        .iter()
        .map(|md| {
            md.name
                .iter()
                .map(|n| prog.cx.name_str(*n))
                .collect::<Vec<_>>()
                .join(".")
        })
        .collect();
    let old = std::fs::read_to_string(&stamp).unwrap_or_default();
    let current = exe.exists() && old.lines().next() == Some(key.as_str());
    let mut rebuilt = false;
    if current {
        crate::cli::timing("dev binary up to date", Instant::now());
    } else {
        if exe.exists() {
            let before: Vec<&str> = old.lines().skip(1).collect();
            let added: Vec<&String> = modules
                .iter()
                .filter(|md| !before.contains(&md.as_str()))
                .collect();
            if let Some(first) = added.iter().find(|md| !md.starts_with("script.")) {
                eprintln!("j3c: a script uses `{first}`, which the program didn't load; rebuilding");
            }
        }
        let mut opts = opts.clone();
        opts.cc.out = Some(exe.clone());
        let started = Instant::now();
        if split_vm(&opts) {
            let vm = vm_funcs(&prog);
            let main = emit::emit_llvm_part(&prog.cx, &prog.modules, &opts, emit::Part::Except(&vm))?;
            let vm_text = emit::emit_llvm_part(&prog.cx, &prog.modules, &opts, emit::Part::Only(&vm))?;
            crate::cli::timing("emit", started);
            emit::compile_llvm_split(
                &main,
                &vm_text,
                &opts.cc,
                &opts.clang,
                &opts.target,
                &opts.cpu,
                &opts.flags,
            )?;
        } else {
            let text = emit::emit_text(&prog.cx, &prog.modules, &opts)?;
            crate::cli::timing("emit", started);
            emit::compile_text(&text, &opts)?;
        }
        let mut record = key.clone();
        for md in &modules {
            record.push('\n');
            record.push_str(md);
        }
        let _ = std::fs::write(&stamp, record);
        rebuilt = true;
    }
    for module in &bytecode {
        module.write_to(&m.root)?;
    }
    Ok(Prepared::Dev(Built {
        exe,
        layouts: layouts(&bytecode),
        rebuilt,
        scripts,
    }))
}

/// Below `-O2` the VM gets its own `-O2` object (LLVM only).
fn split_vm(opts: &EmitOpts) -> bool {
    opts.backend == emit::Backend::Llvm && matches!(opts.cc.opt.as_str(), "" | "0" | "1")
}

/// Every function the `j3.script` module defines (the VM and its instantiations).
fn vm_funcs(prog: &Program) -> std::collections::HashSet<crate::ir::ids::FuncId> {
    prog.modules
        .iter()
        .filter(|m| {
            m.name.len() == 2
                && prog.cx.name_str(m.name[0]) == "j3"
                && prog.cx.name_str(m.name[1]) == "script"
        })
        .flat_map(|m| m.funcs.iter().copied())
        .collect()
}

fn layouts(bytecode: &[BcModule]) -> HashMap<String, (u64, u64)> {
    bytecode
        .iter()
        .map(|b| (b.path.clone(), (b.layout_hash, b.signature_hash)))
        .collect()
}

fn native_source_key(prog: &Program) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for f in &prog.cx.files {
        if f.path.ends_with(".j3s") {
            continue;
        }
        f.path.hash(&mut h);
        f.src.hash(&mut h);
    }
    h.finish()
}

fn dev_key(sources: &u64, prog: &Program, opts: &EmitOpts, fast: bool) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    sources.hash(&mut h);
    dev::native_key(prog).hash(&mut h);
    format!("{opts:?}").hash(&mut h);
    fast.hash(&mut h);
    if let Some(meta) = std::env::current_exe()
        .ok()
        .and_then(|p| std::fs::metadata(p).ok())
    {
        meta.len().hash(&mut h);
        meta.modified().ok().hash(&mut h);
    }
    format!("{:016x}", h.finish())
}

fn launch(exe: &Path, root: &Path) -> Result<Child, String> {
    let root = root
        .canonicalize()
        .map_err(|e| format!("project root {}: {e}", root.display()))?;
    let exe = exe
        .canonicalize()
        .map_err(|e| format!("run {}: {e}", exe.display()))?;
    Command::new(&exe)
        .current_dir(&root)
        .env("J3_ROOT", &root)
        .spawn()
        .map_err(|e| format!("run {}: {e}", exe.display()))
}

fn mtimes(paths: &[PathBuf]) -> Vec<Option<SystemTime>> {
    paths
        .iter()
        .map(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
        .collect()
}

/// Tell a running program to swap in the bytecode just written.
pub fn bump_reload(root: &Path) {
    let path = root.join("target/j3s/reload");
    let next = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0)
        + 1;
    let _ = std::fs::write(&path, next.to_string());
}

/// Run the dev binary and watch its scripts until the program exits.
pub fn watch(m: &Manifest, opts: &EmitOpts, fast: bool, built: &mut Built) -> Result<i32, String> {
    let mut child = launch(&built.exe, &m.root)?;
    let mut seen = mtimes(&built.scripts);
    loop {
        if let Some(status) = child.try_wait().map_err(|e| format!("wait: {e}"))? {
            return Ok(status.code().unwrap_or(1));
        }
        std::thread::sleep(Duration::from_millis(100));
        let now = mtimes(&built.scripts);
        if now == seen {
            continue;
        }
        seen = now;
        match prepare(m, opts, fast) {
            Err(e) => {
                crate::cli::print_error(&e);
                eprintln!("j3c: kept the running scripts");
            }
            Ok(Prepared::NoScripts(_)) => {
                eprintln!("j3c: the program has no scripts now; rerun `j3c run`");
            }
            Ok(Prepared::Dev(next)) => {
                let same_layout = next.layouts.iter().all(|(path, (layout, _))| {
                    built
                        .layouts
                        .get(path)
                        .map(|(old, _)| old == layout)
                        .unwrap_or(true)
                });
                if next.rebuilt || !same_layout {
                    let _ = child.kill();
                    let _ = child.wait();
                    eprintln!(
                        "j3c: relaunching ({})",
                        if next.rebuilt { "rebuilt" } else { "script layout changed" }
                    );
                    child = launch(&next.exe, &m.root)?;
                } else {
                    bump_reload(&m.root);
                    eprintln!("j3c: swapped script code");
                }
                seen = mtimes(&next.scripts);
                *built = next;
            }
        }
    }
}
