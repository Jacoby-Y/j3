use super::*;

#[test]
fn run_without_project_points_at_new() {
    let err = match compile_user(None) {
        Err(e) => e,
        Ok(_) => panic!("expected no project in the compiler directory"),
    };
    assert!(err.contains("j3c new"), "{err}");
    assert!(err.contains("j3c guide"), "{err}");
    assert_eq!(run(["run".into()]), 1);
    assert_eq!(run(["check".into()]), 1);
}

#[test]
fn check_json_on_bad_file() {
    let dir = std::env::temp_dir().join(format!(
        "j3-check-json-{}",
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("bad.j3");
    std::fs::write(&file, "module bad;\nfn main() { noSuch(); }\n").unwrap();
    let err = match compile_user(Some(file.to_str().unwrap())) {
        Err(e) => e,
        Ok(_) => panic!("bad.j3 should not typecheck"),
    };
    let json = json_err(&err);
    assert!(json.contains("\"ok\":false"), "{json}");
    assert!(json.contains("\"line\":2"), "{json}");
    assert!(json.contains("noSuch") || json.contains("unknown"), "{json}");
    assert!(json.contains("\"errors\":[{"), "{json}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_json_carries_i18n_warnings_and_strict_fails_on_them() {
    let w = vec!["warning: a.j3:1:1: key `x` is not in locales/en.txt [i18n-missing]".to_string()];
    let j = check_json("src/main.j3", &[], &w, &[]);
    assert_eq!(j, "{\"ok\":true,\"entry\":\"src/main.j3\",\"i18n\":[\"warning: a.j3:1:1: key `x` is not in locales/en.txt [i18n-missing]\"]}");
    assert_eq!(check_json("e", &[], &[], &[]), "{\"ok\":true,\"entry\":\"e\"}");
    let dir = std::env::temp_dir().join(format!("j3-check-i18n-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::create_dir_all(dir.join("locales")).unwrap();
    std::fs::write(dir.join("j3.toml"), "[package]\nname = \"i18nc\"\nentry = \"src/main.j3\"\n").unwrap();
    std::fs::write(dir.join("locales/en.txt"), "known = Known\n").unwrap();
    std::fs::write(dir.join("src/main.j3"), "module i18nc;\nfn t(k: string): string { return k; }\nfn main() {\n    t(\"known\");\n    t(\"unknown\");\n}\n").unwrap();
    let path = dir.display().to_string();
    assert_eq!(run(["check".into(), path.clone()]), 0);
    assert_eq!(run(["check".into(), "--strict".into(), path]), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_stops_when_check_fails() {
    let dir = std::env::temp_dir().join(format!("j3-run-gate-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("j3.toml"),
        "[package]\nname = \"gate\"\nentry = \"src/main.j3\"\n\n[link]\nlibs = [\"SDL2\"]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/main.j3"),
        "module gate;\n\nuse j3.game.tryLoadFont;\n\nfn main() {\n    let font = tryLoadFont(\"nope.ttf\", 12);\n    if font.raw == 0 {\n        return;\n    }\n}\n",
    )
    .unwrap();
    let path = dir.display().to_string();
    assert_eq!(run(["check".into(), path.clone()]), 1);
    assert_eq!(run(["run".into(), path]), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_skips_unchanged_sources() {
    let dir = std::env::temp_dir().join(format!("j3-run-skip-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("j3.toml"), "[package]\nname = \"skip\"\nentry = \"src/main.j3\"\n").unwrap();
    std::fs::write(&dir.join("src/main.j3"), "module skip;\n\nfn main() {\n}\n").unwrap();
    let path = dir.display().to_string();
    let exe = dir.join("target/skip");
    assert_eq!(run(["run".into(), path.clone()]), 0);
    let first = std::fs::metadata(&exe).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    assert_eq!(run(["run".into(), path]), 0);
    assert_eq!(
        std::fs::metadata(&exe).unwrap().modified().unwrap(),
        first,
        "unchanged run relinked"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn incremental_edit_recompiles_one_module() {
    let dir = std::env::temp_dir().join(format!("j3-incr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("j3.toml"),
        "[package]\nname = \"incr\"\nentry = \"src/main.j3\"\n\n[emit]\nbackend = llvm\nopt = 0\ndebug = true\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/main.j3"),
        "module incr;\n\nuse j3.io.printLn;\nuse helper.word;\n\nfn main() {\n    printLn(word());\n}\n",
    )
    .unwrap();
    let helper = dir.join("src/helper.j3");
    std::fs::write(&helper, "module helper;\n\npub fn word(): string {\n    return \"one\";\n}\n").unwrap();
    let path = dir.display().to_string();
    assert_eq!(build_path(&path), 0);
    let exe = dir.join("target/incr");
    let first = std::process::Command::new(&exe).output().unwrap();
    assert!(
        String::from_utf8_lossy(&first.stdout).contains("one"),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let (prog, manifest) = compile_user(Some(&path)).unwrap();
    let mut opts = emit_opts(&manifest).unwrap();
    opts.cc.opt = "2".into();
    opts.incremental = false;
    opts.cc.out = Some(dir.join("target/incr-o2"));
    let whole = emit::run_program_with(&prog.cx, &prog.modules, &opts, &[]).unwrap();
    assert_eq!(whole.stdout, first.stdout, "per-module output differed from one clang invocation");

    let before = obj_mtimes(&dir);
    std::fs::write(&helper, "module helper;\n\npub fn word(): string {\n    return \"two\";\n}\n").unwrap();
    assert_eq!(build_path(&path), 0);
    let second = std::process::Command::new(&exe).output().unwrap();
    assert!(String::from_utf8_lossy(&second.stdout).contains("two"));
    let after = obj_mtimes(&dir);
    for (name, modified) in &before {
        if name.contains("helper") {
            continue;
        }
        let same = after.iter().find(|(n, _)| n == name).map(|(_, t)| *t);
        assert_eq!(same, Some(*modified), "{name} was rebuilt");
    }
    let helper_before: Vec<_> = before.iter().filter(|(n, _)| n.contains("helper")).map(|(n, _)| n.clone()).collect();
    let helper_after: Vec<_> = after.iter().filter(|(n, _)| n.contains("helper")).map(|(n, _)| n.clone()).collect();
    assert_eq!(helper_after.len(), 1, "{helper_after:?}");
    assert_ne!(helper_before, helper_after, "helper object was reused after an edit");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn incremental_all_splits_optimized_builds() {
    let dir = std::env::temp_dir().join(format!("j3-incr-all-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("j3.toml"),
        "[package]\nname = \"incrall\"\nentry = \"src/main.j3\"\n\n[emit]\nbackend = llvm\nopt = 2\nincremental = \"all\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/main.j3"),
        "module incrall;\n\nuse j3.io.printLn;\n\nfn main() {\n    printLn(\"split\");\n}\n",
    )
    .unwrap();
    assert_eq!(build_path(&dir.display().to_string()), 0);
    let out = std::process::Command::new(dir.join("target/incrall")).output().unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("split"));
    assert!(!obj_mtimes(&dir).is_empty(), "opt 2 with incremental = all made no per-module objects");
    let _ = std::fs::remove_dir_all(&dir);
}

fn obj_mtimes(root: &std::path::Path) -> Vec<(String, std::time::SystemTime)> {
    let mut out = Vec::new();
    let host = root.join("target/.j3c/host");
    let dir = std::fs::read_dir(&host).unwrap().flatten().map(|e| e.path().join("obj")).find(|d| d.is_dir()).unwrap();
    for ent in std::fs::read_dir(&dir).unwrap().flatten() {
        let name = ent.file_name().to_string_lossy().to_string();
        if !name.ends_with(".o") {
            continue;
        }
        out.push((name, ent.metadata().unwrap().modified().unwrap()));
    }
    out.sort();
    out
}

#[test]
fn build_skips_unchanged_sources() {
    let dir = std::env::temp_dir().join(format!("j3-build-skip-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("j3.toml"), "[package]\nname = \"skip\"\nentry = \"src/main.j3\"\n").unwrap();
    let main = dir.join("src/main.j3");
    std::fs::write(&main, "module skip;\n\nfn main() {\n}\n").unwrap();
    let path = dir.display().to_string();
    let exe = dir.join("target/skip");
    assert_eq!(build_path(&path), 0);
    let first = std::fs::metadata(&exe).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    assert_eq!(build_path(&path), 0);
    assert_eq!(std::fs::metadata(&exe).unwrap().modified().unwrap(), first, "unchanged build relinked");
    std::fs::write(&main, "module skip;\n\nfn main() {\n    let x = 1;\n}\n").unwrap();
    assert_eq!(build_path(&path), 0);
    assert_ne!(std::fs::metadata(&exe).unwrap().modified().unwrap(), first, "edit didn't rebuild");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn manifest_help_lists_package_name() {
    assert!(crate::helptext::MANIFEST.contains("name"));
    assert!(crate::helptext::MANIFEST.contains("J3_STDLIB"));
    assert!(crate::helptext::GUIDE.contains("j3c new"));
}

#[test]
fn add_dep_line_places_and_replaces() {
    use super::pkg::add_dep_line;
    let t = "[package]\nname = \"a\"\n\n[dependencies]\nx = \"../x\"\n\n[emit]\nopt = 1\n";
    assert_eq!(add_dep_line(t, "y", "y = { path = \"../y\" }"), "[package]\nname = \"a\"\n\n[dependencies]\nx = \"../x\"\ny = { path = \"../y\" }\n\n[emit]\nopt = 1\n");
    assert_eq!(add_dep_line(t, "x", "x = { git = \"u\" }"), t.replace("x = \"../x\"", "x = { git = \"u\" }"));
    assert_eq!(add_dep_line("[package]\nname = \"a\"\n", "y", "y = \"../y\""), "[package]\nname = \"a\"\n\n[dependencies]\ny = \"../y\"\n");
}

/// Editing an embedded file rebuilds: one inside the project (the stat-only shortcut sees it) and
/// one outside it (no shortcut stamp is written, so the source key catches it), with per-module objects.
#[test]
fn embed_edit_rebuilds() {
    let base = std::env::temp_dir().join(format!("j3-embed-edit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let dir = base.join("proj");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("j3.toml"),
        "[package]\nname = \"emb\"\nentry = \"src/main.j3\"\n\n[emit]\nbackend = llvm\nopt = 0\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/main.j3"),
        "module emb;\n\nuse j3.io.printLn;\n\nconst IN = @embed(\"data.txt\");\nconst OUT = @embed(\"../../outside.txt\");\n\nfn main() {\n    printLn(`{IN} {OUT}`);\n}\n",
    )
    .unwrap();
    std::fs::write(dir.join("src/data.txt"), "one").unwrap();
    std::fs::write(base.join("outside.txt"), "red").unwrap();
    let path = dir.display().to_string();
    let exe = dir.join("target/emb");
    let out = || String::from_utf8_lossy(&std::process::Command::new(&exe).output().unwrap().stdout).to_string();
    assert_eq!(build_path(&path), 0);
    assert_eq!(out(), "one red\n");
    std::fs::write(dir.join("src/data.txt"), "two").unwrap();
    assert_eq!(build_path(&path), 0);
    assert_eq!(out(), "two red\n", "inside edit did not rebuild");
    std::fs::write(base.join("outside.txt"), "blu").unwrap();
    assert_eq!(build_path(&path), 0);
    assert_eq!(out(), "two blu\n", "outside edit did not rebuild");
    let _ = std::fs::remove_dir_all(&base);
}

mod tree;

fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let dst = to.join(e.file_name());
        if e.path().is_dir() {
            copy_tree(&e.path(), &dst);
        } else {
            std::fs::copy(e.path(), dst).unwrap();
        }
    }
}

#[test]
fn pkg_tool_runs_and_caches() {
    let dir = std::env::temp_dir().join(format!("j3-pkgtool-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/programs");
    copy_tree(&fixtures.join("tool_pkg"), &dir.join("tool_pkg"));
    copy_tree(&fixtures.join("tool_pkg_game"), &dir.join("tool_pkg_game"));
    let cache = dir.join("cache");
    crate::pkgtool::TEST_CACHE.with(|c| *c.borrow_mut() = Some(cache.clone()));
    let game = dir.join("tool_pkg_game");
    let m = Manifest::load(&game.join("j3.toml")).unwrap();
    let dep = m.deps.iter().find(|d| d.name == "tool_pkg").unwrap();
    let tool = &dep.commands[0];
    let args = ["a".to_string(), "b".to_string()];
    let out = crate::pkgtool::command(&m, dep, tool, &args, &[]).unwrap().output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let root = game.canonicalize().unwrap();
    assert_eq!(text, format!("tool_pkg tool: a b\nroot {}\napi 1\ngreeting hi from the game\n", root.display()), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(out.status.code(), Some(3));
    let exe = crate::pkgtool::build(&m, dep, tool).unwrap();
    let mtime = || std::fs::metadata(&exe).unwrap().modified().unwrap();
    let first = mtime();
    // the CLI path: same exe, exit code passed through
    assert_eq!(pkg::pkg_in(&game, &["tool_pkg".into(), "echo".into(), "x".into()]), 3);
    assert_eq!(crate::pkgtool::build(&m, dep, tool).unwrap(), exe);
    assert_eq!(mtime(), first, "second run rebuilt");
    let src = dir.join("tool_pkg/tools/echo.j3");
    let edited = std::fs::read_to_string(&src).unwrap().replace("sys.exit(3);", "sys.exit(4);");
    std::fs::write(&src, edited).unwrap();
    let rebuilt = crate::pkgtool::build(&m, dep, tool).unwrap();
    assert_ne!(rebuilt, exe, "edit did not rebuild");
    assert_eq!(crate::pkgtool::run(&m, dep, tool, &[], &[]).unwrap(), 4);
    assert_eq!(pkg::tools_text(dep), "tool_pkg\n  echo  print args, env and the greeting, exit 3\n  dist: fake\n");
    assert_eq!(pkg::pkg_in(&game, &[]), 0);
    assert_eq!(pkg::pkg_in(&game, &["nope".into()]), 1);
    assert_eq!(pkg::pkg_in(&game, &["tool_pkg".into(), "nope".into()]), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dist_output_runs() {
    let dir = std::env::temp_dir().join(format!("j3-pkgdist-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/programs");
    copy_tree(&fixtures.join("tool_pkg"), &dir.join("tool_pkg"));
    copy_tree(&fixtures.join("tool_pkg_game"), &dir.join("tool_pkg_game"));
    crate::pkgtool::TEST_CACHE.with(|c| *c.borrow_mut() = Some(dir.join("cache")));
    let game = std::path::absolute(dir.join("tool_pkg_game")).unwrap();
    let toml = game.join("j3.toml");
    let base = std::fs::read_to_string(&toml).unwrap();
    let outputs = |list: &str| std::fs::write(&toml, format!("{base}\n[dist]\nbuild_outputs = [{list}]\n")).unwrap();
    outputs("\"nope\"");
    let e = Manifest::load(&toml).unwrap_err();
    assert!(e.contains("dist.build_outputs: no package provides \"nope\" (add one under [dependencies])"), "{e}");
    outputs("\"zip\", \"fake\"");
    let m = Manifest::load(&toml).unwrap();
    // A stand-in program: packaging only copies it.
    let exe = game.join("target/tool_pkg_game");
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    std::fs::write(&exe, "exe").unwrap();
    let wrote = crate::dist::package(&m, &[(String::new(), exe.clone())]).unwrap();
    let dist = game.join("target/dist");
    assert!(wrote.iter().any(|p| p.extension().is_some_and(|x| x == "zip") && p.is_file()), "{wrote:?}");
    assert!(wrote.contains(&dist.join("fake")), "{wrote:?}");
    let args = std::fs::read_to_string(dist.join("fake/args.txt")).unwrap();
    let stage = game.join("target/.j3c/dist").join(crate::project::target_slug("")).join("root/opt/tool_pkg_game");
    let want = format!(
        "--dist\n{}\n--built\n={}\n--stage\n={}\n--name\ntool_pkg_game\n--version\n0.1.0\n",
        dist.display(),
        exe.display(),
        stage.display()
    );
    assert!(args.starts_with(&want), "{args}");
    assert!(!game.join("target/.j3c/dist").exists(), "stage folders left behind");
    // A failing tool fails the build with its own code.
    let src = dir.join("tool_pkg/tools/dist.j3");
    let failing = std::fs::read_to_string(&src).unwrap().replace("    printLn(dir);\n", "    printLn(dir);\n    sys.exit(4);\n");
    std::fs::write(&src, failing).unwrap();
    let (e, code) = crate::dist::package(&m, &[(String::new(), exe)]).unwrap_err();
    assert_eq!(code, 4, "{e}");
    assert!(e.contains("dist output `fake`") && e.contains("exit code 4"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}
