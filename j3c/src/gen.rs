//! `j3c -e gen`: run a project's mesh generator scripts. Each `assets/gen/<name>.j3` is a J3
//! program that builds a mesh with `j3.gfx.meshgen` and saves it to `meshgen.output(...)`, which
//! is `assets/gen/<name>.j3mesh` here. A script runs again only when it (or j3c) changed.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub fn cmd(rest: &[String]) -> i32 {
    let mut project = PathBuf::from(".");
    let mut force = false;
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("usage: j3c -e gen [--project DIR] [--force]\n\nRuns each assets/gen/<name>.j3 that changed since its last run; it writes\nassets/gen/<name>.j3mesh through j3.gfx.meshgen. --force runs them all.");
                return 0;
            }
            "--force" => force = true,
            "--project" => match it.next() {
                Some(p) => project = PathBuf::from(p),
                None => {
                    eprintln!("j3c -e gen: --project needs a directory");
                    return 1;
                }
            },
            other => {
                eprintln!("j3c -e gen: unknown argument `{other}`");
                return 1;
            }
        }
    }
    let root = match crate::scene::find_root(&project) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("j3c -e gen: {e}");
            return 1;
        }
    };
    let report = run_all(&root, force);
    for line in &report.made { println!("made {line}"); }
    for line in &report.failed { eprintln!("{line}"); }
    if report.made.is_empty() && report.failed.is_empty() { println!("{} generator(s), all up to date", report.scripts); }
    if report.failed.is_empty() { 0 } else { 1 }
}

pub(crate) struct Report {
    pub scripts: usize,
    /// Meshes written this time.
    pub made: Vec<String>,
    /// One message per script that failed, with its compiler or runtime output.
    pub failed: Vec<String>,
}

fn scripts(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root.join("assets/gen")) else { return Vec::new() };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("j3"))
        .collect();
    out.sort();
    out
}

/// What a script's output depends on: its text, and the j3c (with its stdlib) that runs it.
fn key(script: &str, host: &Path) -> String {
    let mut h = DefaultHasher::new();
    script.hash(&mut h);
    if let Ok(meta) = fs::metadata(host) {
        (meta.len(), meta.modified().ok()).hash(&mut h);
    }
    if crate::stdlib::using_override() {
        std::env::var("J3_STDLIB").unwrap_or_default().hash(&mut h);
        if let Ok(meta) = fs::metadata(crate::stdlib::stdlib_dir().join("j3/gfx/meshgen.j3")) {
            meta.modified().ok().hash(&mut h);
        }
    }
    format!("{:016x}", h.finish())
}

/// Set while a generator runs, so building it doesn't run the generators again.
const RUNNING: &str = "J3_GEN_RUNNING";

/// Run every generator whose script changed (all of them when `force`).
// ponytail: scripts that `use` other project files aren't tracked; edit the script or --force.
pub(crate) fn run_all(root: &Path, force: bool) -> Report {
    let root = &root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let list = scripts(root);
    let mut report = Report { scripts: list.len(), made: Vec::new(), failed: Vec::new() };
    if list.is_empty() { return report; }
    let host = match crate::editor::host_exe() {
        Ok(h) => h,
        Err(e) => {
            report.failed.push(format!("j3c -e gen: {e}"));
            return report;
        }
    };
    let stamps = root.join("target/j3-editor/gen");
    let _ = fs::create_dir_all(&stamps);
    for script in list {
        let name = script.file_stem().and_then(|s| s.to_str()).unwrap_or("mesh").to_string();
        let rel_script = format!("assets/gen/{name}.j3");
        let rel_out = format!("assets/gen/{name}.j3mesh");
        let text = fs::read_to_string(&script).unwrap_or_default();
        let want = key(&text, &host);
        let stamp = stamps.join(format!("{name}.stamp"));
        if !force && root.join(&rel_out).is_file() && fs::read_to_string(&stamp).ok().as_deref() == Some(want.as_str()) {
            continue;
        }
        let before = fs::metadata(root.join(&rel_out)).and_then(|m| m.modified()).ok();
        let out = Command::new(&host)
            .arg("run")
            .arg("--file")
            .arg(&script)
            .current_dir(root)
            .env("J3_GEN_OUT", root.join(&rel_out))
            .env(RUNNING, "1")
            .stdin(Stdio::null())
            .output();
        let out = match out {
            Ok(o) => o,
            Err(e) => {
                report.failed.push(format!("{rel_script}: {e}"));
                continue;
            }
        };
        let after = fs::metadata(root.join(&rel_out)).and_then(|m| m.modified()).ok();
        if out.status.success() && after.is_some() && after != before {
            let _ = fs::write(&stamp, &want);
            report.made.push(rel_out);
        } else {
            let log = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
            let why = if out.status.success() {
                format!("ran but wrote nothing to {rel_out}; end the script with b.save(meshgen.output(\"{rel_out}\"))")
            } else {
                log.trim().to_string()
            };
            report.failed.push(format!("{rel_script}: {why}"));
        }
    }
    report
}

/// Before a project build: make stale meshes, and say (on stderr) which scripts failed.
pub(crate) fn before_build(root: &Path) {
    if std::env::var_os(RUNNING).is_some() { return; }
    for line in run_all(root, false).failed {
        eprintln!("warning: mesh generator failed: {line}");
    }
}
