use super::*;

/// Write `files` under a fresh temp dir; the project is `<dir>/app`.
fn tree(name: &str, files: &[(&str, &str)]) -> PathBuf {
    static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("j3c_deps_{name}_{}_{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in files {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    root
}

fn app(deps: &str) -> String {
    format!("[package]\nname = \"app\"\n\n[dependencies]\n{deps}\n")
}

const MAIN: &str = "module app;\nuse mid.calc;\nfn main() { let x = calc.next(); }\n";
const MID: &str = "module mid.calc;\nuse leaf.base;\npub fn next(): int { return base.answer() + 1; }\n";
const LEAF: &str = "module leaf.base;\npub fn answer(): int { return 41; }\n";

fn load_check(root: &Path) -> Result<Program, String> {
    let m = Manifest::load(&root.join("app/j3.toml"))?;
    compile_manifest(&m)
}

#[test]
fn path_dep_with_transitive_dep_checks() {
    let root = tree("ok", &[
        ("app/j3.toml", &app("mid = \"../mid\"")),
        ("app/src/main.j3", MAIN),
        ("mid/j3.toml", "[package]\nname = \"mid\"\n\n[dependencies]\nleaf = \"../leaf\"\n"),
        ("mid/calc.j3", MID),
        ("leaf/base.j3", LEAF),
    ]);
    let m = Manifest::load(&root.join("app/j3.toml")).unwrap();
    let names: Vec<&str> = m.deps.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, ["mid", "leaf"]);
    load_check(&root).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn dep_module_line_must_match_the_import() {
    let root = tree("modline", &[
        ("app/j3.toml", &app("mid = \"../mid\"")),
        ("app/src/main.j3", MAIN),
        ("mid/calc.j3", "module calc;\npub fn next(): int { return 1; }\n"),
    ]);
    let e = load_check(&root).err().expect("mismatch");
    assert!(e.contains("write `module mid.calc;`"), "{e}");
}

#[test]
fn missing_dep_path_names_the_key() {
    let root = tree("missing", &[("app/j3.toml", &app("mid = \"../nope\"")), ("app/src/main.j3", MAIN)]);
    let e = Manifest::load(&root.join("app/j3.toml")).unwrap_err();
    assert!(e.contains("j3.toml:5: [dependencies] mid: path `../nope` is not a directory"), "{e}");
}

#[test]
fn dependency_cycle_is_an_error() {
    let root = tree("cycle", &[
        ("app/j3.toml", &app("a = \"../a\"")),
        ("app/src/main.j3", MAIN),
        ("a/j3.toml", "[dependencies]\nb = \"../b\"\n"),
        ("b/j3.toml", "[dependencies]\na = \"../a\"\n"),
    ]);
    let e = Manifest::load(&root.join("app/j3.toml")).unwrap_err();
    assert!(e.contains("[dependencies] a: dependency cycle a -> b -> a"), "{e}");
}

#[test]
fn clashing_names_are_errors() {
    for (deps, want) in [
        ("j3 = \"../mid\"", "not a usable module name"),
        ("app = \"../mid\"", "clashes with a module of this project"),
        ("util = \"../mid\"", "clashes with a module of this project"),
        ("mid = \"../mid\"\nmid = \"../mid\"", "listed twice"),
    ] {
        let root = tree("clash", &[
            ("app/j3.toml", &app(deps)),
            ("app/src/main.j3", MAIN),
            ("app/src/util.j3", "module app.util;\n"),
            ("mid/calc.j3", MID),
        ]);
        let e = Manifest::load(&root.join("app/j3.toml")).unwrap_err();
        assert!(e.contains(want), "{deps}: {e}");
    }
}

#[test]
fn one_name_two_paths_and_wrong_package_name() {
    let root = tree("twopaths", &[
        ("app/j3.toml", &app("mid = \"../mid\"\nleaf = \"../leaf\"")),
        ("app/src/main.j3", MAIN),
        ("mid/j3.toml", "[dependencies]\nleaf = \"../other\"\n"),
        ("leaf/base.j3", LEAF),
        ("other/base.j3", LEAF),
    ]);
    let e = Manifest::load(&root.join("app/j3.toml")).unwrap_err();
    assert!(e.contains("one name needs one path"), "{e}");
    let root = tree("badname", &[
        ("app/j3.toml", &app("mid = \"../mid\"")),
        ("app/src/main.j3", MAIN),
        ("mid/j3.toml", "[package]\nname = \"other\"\n"),
    ]);
    let e = Manifest::load(&root.join("app/j3.toml")).unwrap_err();
    assert!(e.contains("the key here must match it"), "{e}");
}

#[test]
fn package_dep_in_parent_dir_loads_from_its_src() {
    // b sits inside a; only a.shared.msg is compiled, not a's main
    let root = tree("parent", &[
        ("a/j3.toml", "[package]\nname = \"a\"\n\n[link]\nlibs = [\"SDL2\"]\n"),
        ("a/src/main.j3", "module a;\nthis does not parse\n"),
        ("a/src/shared/msg.j3", "module a.shared.msg;\npub fn hello(): i32 { return 7; }\n"),
        ("a/b/j3.toml", "[package]\nname = \"b\"\n\n[dependencies]\na = { path = \"..\" }\n"),
        ("a/b/src/main.j3", "module b;\nuse a.shared.msg;\nuse a.shared.msg.{ hello };\nfn main() { let x = msg.hello() + hello(); }\n"),
    ]);
    let m = Manifest::load(&root.join("a/b/j3.toml")).unwrap_or_else(|e| panic!("{e}"));
    assert!(m.deps[0].root.ends_with("a/src"), "{:?}", m.deps);
    compile_manifest(&m).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn use_dep_alone_loads_its_root_module_and_lib_needs_no_main() {
    let root = tree("libroot", &[
        ("app/j3.toml", &app("hello = \"../hello\"")),
        ("app/src/main.j3", "module app;\nuse hello;\nfn main() { let x = hello.answer(); }\n"),
        ("hello/j3.toml", "[package]\nname = \"hello\"\nj3 = \">=0.0\"\n\n[lib]\n"),
        ("hello/src/hello.j3", "module hello;\npub fn answer(): int { return 42; }\n"),
    ]);
    load_check(&root).unwrap_or_else(|e| panic!("{e}"));
    let lib = Manifest::load(&root.join("hello/j3.toml")).unwrap_or_else(|e| panic!("{e}"));
    assert!(lib.lib && lib.entry.ends_with("src/hello.j3"));
    compile_manifest(&lib).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn dep_needing_a_newer_j3c_is_an_error() {
    let root = tree("j3bound", &[
        ("app/j3.toml", &app("hello = \"../hello\"")),
        ("app/src/main.j3", MAIN),
        ("hello/j3.toml", "[package]\nname = \"hello\"\nj3 = \">=999.0\"\n"),
    ]);
    let e = Manifest::load(&root.join("app/j3.toml")).unwrap_err();
    assert!(e.contains("needs j3c >=999.0"), "{e}");
}

/// Git deps against a local repo: clone at a tag, lock, build offline from the lock, bad ref.
#[test]
fn git_dep_clones_locks_and_reuses_the_lock() {
    let Ok(out) = std::process::Command::new("git").arg("--version").output() else { return };
    if !out.status.success() {
        return;
    }
    let root = tree("git", &[
        ("app/src/main.j3", "module app;\nuse leaf.base;\nfn main() { let x = base.answer(); }\n"),
        ("leaf/j3.toml", "[package]\nname = \"leaf\"\n"),
        ("leaf/src/base.j3", LEAF),
    ]);
    let pkgs = root.join("pkgs");
    std::env::set_var("J3_PKG_DIR", &pkgs); // the only test that reads J3_PKG_DIR
    let git = |args: &[&str]| {
        let ok = std::process::Command::new("git").current_dir(root.join("leaf")).args(["-c", "user.email=t@t", "-c", "user.name=t"]).args(args).output().unwrap();
        assert!(ok.status.success(), "{}", String::from_utf8_lossy(&ok.stderr));
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-qm", "one"]);
    git(&["tag", "v1"]);
    let toml = root.join("app/j3.toml");
    std::fs::write(&toml, app("leaf = { git = \"../leaf\", tag = \"v1\" }")).unwrap();
    let m = Manifest::load(&toml).unwrap_or_else(|e| panic!("{e}"));
    assert!(m.deps[0].root.starts_with(&pkgs) && m.deps[0].root.ends_with("src"), "{:?}", m.deps);
    let lock = std::fs::read_to_string(root.join("app/j3.lock")).unwrap();
    assert!(lock.contains("[leaf]") && lock.contains("ref = \"tag:v1\""), "{lock}");
    compile_manifest(&m).unwrap_or_else(|e| panic!("{e}"));
    // the lock and the cache are enough: no git runs once the source repo is gone
    std::fs::rename(root.join("leaf"), root.join("leaf-gone")).unwrap();
    std::fs::create_dir(root.join("leaf")).unwrap();
    let again = Manifest::load(&toml).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(again.deps, m.deps);
    std::fs::remove_dir(root.join("leaf")).unwrap();
    std::fs::rename(root.join("leaf-gone"), root.join("leaf")).unwrap();
    std::fs::write(&toml, app("leaf = { git = \"../leaf\", tag = \"v9\" }")).unwrap();
    let e = Manifest::load(&toml).unwrap_err();
    assert!(e.contains("has no tag `v9`"), "{e}");
    std::fs::write(&toml, app("leaf = { git = \"../leaf\", tag = \"v1\", rev = \"abc\" }")).unwrap();
    let e = Manifest::load(&toml).unwrap_err();
    assert!(e.contains("one of tag, rev, branch"), "{e}");
}
