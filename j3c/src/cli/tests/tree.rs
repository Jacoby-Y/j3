//! Every program in the tree that is meant to compile still does with this compiler: each
//! `stdlib/tests/*.j3`, `speculative/*.j3`, `speculative/lang/*.j3` and `speculative/*/` project,
//! checked like `j3c check` (types, lowering checks such as the backtick-escape rule), not run.
//! The examples and tools are left to `j3c check` by hand: they need SDL libraries to resolve.

use std::path::{Path, PathBuf};

/// Files that are not programs: `09_stdlib_game.j3` has no `main` (checked as a library in
/// `compile/tests/stdlib_programs.rs`).
const SKIP: &[&str] = &["09_stdlib_game.j3"];

fn entries(dir: &Path, projects: bool, out: &mut Vec<PathBuf>) {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    v.sort();
    for p in v {
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        if SKIP.contains(&name.as_str()) {
            continue;
        }
        if p.extension().is_some_and(|x| x == "j3") || (projects && p.join("j3.toml").is_file()) {
            out.push(p);
        }
    }
}

#[test]
fn every_test_and_sample_program_checks() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut all = Vec::new();
    entries(&root.join("stdlib/tests"), false, &mut all);
    entries(&root.join("../speculative"), true, &mut all);
    entries(&root.join("../speculative/lang"), false, &mut all);
    assert!(all.len() > 200, "{}", all.len());
    // ponytail: fixed 4 workers next to cargo's own test threads; about 10 s in release
    let next = std::sync::atomic::AtomicUsize::new(0);
    let bad = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..4 {
            std::thread::Builder::new()
                .stack_size(256 << 20)
                .spawn_scoped(s, || loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(p) = all.get(i) else { break };
                    if let Err(e) = super::super::dump::compile_entry(Some(&p.to_string_lossy())) {
                        let first = e.lines().take(4).collect::<Vec<_>>().join("\n");
                        bad.lock().unwrap().push(format!("{}:\n{first}", p.display()));
                    }
                })
                .unwrap();
        }
    });
    let bad = bad.into_inner().unwrap();
    assert!(bad.is_empty(), "{} program(s) no longer check:\n{}", bad.len(), bad.join("\n"));
}
