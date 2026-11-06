//! Error-message goldens: each `diag/*.j3` is checked and its rendered errors (or warnings) are
//! compared with the `.err` beside it. `J3_BLESS=1` rewrites them; read the diff before keeping it.

use super::*;

#[test]
fn diagnostics_match_goldens() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/compile/tests/diag");
    let prefix = format!("{}/", dir.display());
    let mut cases: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "j3"))
        .collect();
    cases.sort();
    assert!(!cases.is_empty());
    let mut drift = Vec::new();
    for case in cases {
        let got = match compile_path(&case) {
            Ok(prog) => prog.rendered_warnings().join("\n"),
            Err(e) => e,
        }
        .replace(&prefix, "");
        let golden = case.with_extension("err");
        if std::env::var("J3_BLESS").is_ok() {
            std::fs::write(&golden, &got).unwrap();
            continue;
        }
        let want = std::fs::read_to_string(&golden).unwrap_or_default();
        if got != want {
            drift.push(format!("--- {} ---\n{got}", golden.display()));
        }
    }
    assert!(drift.is_empty(), "error goldens drifted (J3_BLESS=1 to accept):\n{}", drift.join("\n"));
}

#[test]
fn autofix_drops_unused_imports() {
    let dir = std::env::temp_dir().join(format!("j3-autofix-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.j3");
    let src = "module main;\nuse j3.io.printLn;\nuse j3.text.Text;\nuse j3.math.{ Vec2, clamp, lerp };\nfn main() { printLn(`{lerp(1.0, 2.0, 0.5)}`); }\n";
    std::fs::write(&file, src).unwrap();
    let prog = compile_path(&file).expect("check");
    let fixes: Vec<&crate::check::lint::Fix> = prog.fixes.iter().collect();
    let got = crate::check::lint::apply_fixes(src, &fixes);
    assert_eq!(got, "module main;\nuse j3.io.printLn;\nuse j3.math.lerp;\nfn main() { printLn(`{lerp(1.0, 2.0, 0.5)}`); }\n");
    let _ = std::fs::remove_dir_all(&dir);
}
