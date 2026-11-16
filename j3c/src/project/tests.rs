//! Manifest parsing tests.

use super::*;

#[test]
fn parse_bounce_shaped_toml() {
    let text = r#"
# comment
[package]
name = "bounce"
entry = "src/main.j3"

[cc]
std = "c11"
opt = "0"

[link]
libs = ["SDL2"]
search = []
files = []
flags = ["-lm"]
"#;
    let tmp = std::env::temp_dir().join("j3_toml_parse_test");
    let _ = std::fs::create_dir_all(tmp.join("src"));
    std::fs::write(tmp.join("src/main.j3"), "module t;\nfn main() {}\n").unwrap();
    let m = parse_manifest(text, tmp.clone()).expect("parse");
    assert_eq!(m.name, "bounce");
    assert_eq!(m.entry, tmp.join("src/main.j3"));
    assert_eq!(m.cc.std, "c11");
    assert_eq!(m.cc.opt, "0");
    assert_eq!(m.link.libs, vec!["SDL2".to_string()]);
    assert!(m.link.search.is_empty());
    assert_eq!(m.link.flags, vec!["-lm".to_string()]);
    assert_eq!(m.emit.backend, "llvm");
    assert_eq!(m.emit.opt, "0");
}

#[test]
fn parse_emit_section_bare_tokens() {
    let text = r#"
[package]
name = "emitme"
entry = "src/main.j3"

[emit]
backend = llvm
opt = 2
debug = true
fast = false
cpu = native
flags = ["-mavx2"]
"#;
    let tmp = std::env::temp_dir().join("j3_toml_emit_test");
    let _ = std::fs::create_dir_all(tmp.join("src"));
    std::fs::write(tmp.join("src/main.j3"), "module t;\nfn main() {}\n").unwrap();
    let m = parse_manifest(text, tmp).expect("parse");
    assert_eq!(m.emit.backend, "llvm");
    assert_eq!(m.emit.opt, "2");
    assert!(m.emit.debug);
    assert!(!m.emit.fast);
    assert_eq!(m.emit.cpu, "native");
    assert_eq!(m.emit.flags, vec!["-mavx2".to_string()]);
    assert!(m.emit.targets.is_empty());
}

#[test]
fn parse_target_array_and_string() {
    let tmp = std::env::temp_dir().join("j3_toml_targets_test");
    let _ = std::fs::create_dir_all(tmp.join("src"));
    std::fs::write(tmp.join("src/main.j3"), "module t;\nfn main() {}\n").unwrap();
    let many = r#"
[package]
name = "multi"
entry = "src/main.j3"
[emit]
target = ["x86_64-pc-linux-gnu", "x86_64-pc-windows-gnu", "arm64-apple-darwin"]
"#;
    let m = parse_manifest(many, tmp.clone()).expect("array");
    assert_eq!(
        m.emit.targets,
        vec![
            "x86_64-pc-linux-gnu".to_string(),
            "x86_64-pc-windows-gnu".to_string(),
            "arm64-apple-darwin".to_string(),
        ]
    );
    assert_eq!(m.emit.primary(), "x86_64-pc-linux-gnu");
    assert_eq!(
        m.out_path_for("x86_64-pc-windows-gnu"),
        tmp.join("target").join("x86_64-pc-windows-gnu").join("multi")
    );
    let one = r#"
[package]
name = "multi"
entry = "src/main.j3"
[emit]
target = "x86_64-pc-windows-gnu"
"#;
    let m = parse_manifest(one, tmp).expect("string");
    assert_eq!(m.emit.targets, vec!["x86_64-pc-windows-gnu".to_string()]);
    assert_eq!(m.emit.primary(), "x86_64-pc-windows-gnu");
    assert!(m.out_path().ends_with("multi"));
}

#[test]
fn package_bin_names_the_program() {
    let tmp = std::env::temp_dir().join(format!("j3_toml_bin_test_{}", std::process::id()));
    let _ = std::fs::create_dir_all(tmp.join("src"));
    std::fs::write(tmp.join("src/main.j3"), "module t;\nfn main() {}\n").unwrap();
    let m = parse_manifest("[package]\nname = \"j3c\"\nbin = \"j3c-boot\"\n", tmp.clone()).expect("bin");
    assert_eq!((m.name.as_str(), m.bin.as_str()), ("j3c", "j3c-boot"));
    assert_eq!(m.out_path(), tmp.join("target").join("j3c-boot"));
    let m = parse_manifest("[package]\nname = \"j3c\"\n", tmp.clone()).expect("default");
    assert_eq!(m.out_path(), tmp.join("target").join("j3c"));
    for bad in ["", "a/b", "../x"] {
        let err = parse_manifest(&format!("[package]\nname = \"j3c\"\nbin = \"{bad}\"\n"), tmp.clone()).unwrap_err();
        assert!(err.contains("package.bin"), "{err}");
    }
}

#[test]
fn script_path_is_the_file_not_the_project() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/3d-editor-scene");
    match resolve(&root).expect("project") {
        Input::Project(m) => assert!(
            m.entry.ends_with("src/main.j3"),
            "{}",
            m.entry.display()
        ),
        Input::File(_) | Input::Hello => panic!("expected the game project"),
    }
    let script = root.join("scripts/spin.j3s");
    match resolve(&script).expect("script") {
        Input::File(p) => assert_eq!(p, script),
        Input::Project(_) | Input::Hello => panic!("expected the script file"),
    }
}

#[test]
fn dist_build_outputs() {
    let dir = std::env::temp_dir().join(format!("j3-dist-toml-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/main.j3"), "").unwrap();
    let m = |t: &str| parse_manifest(&format!("[package]\nname = \"g\"\n{t}"), dir.clone());
    let a = m("[dist]\nbuild_outputs = [\"deb\", \"zip\", \"deb\"]\nicon = \"i.png\"\n").unwrap();
    assert_eq!(a.dist.outputs, [Output::Deb, Output::Zip]);
    let act = m("[dist.actions]\nnew-island = { name = \"New Island\", exec = \"--new-island\" }\nplain = { name = \"A, b\" }\n").unwrap();
    assert_eq!(act.dist.actions.len(), 2);
    assert_eq!((act.dist.actions[0].id.as_str(), act.dist.actions[0].name.as_str(), act.dist.actions[0].exec.as_str()), ("new-island", "New Island", "--new-island"));
    assert_eq!((act.dist.actions[1].name.as_str(), act.dist.actions[1].exec.as_str()), ("A, b", ""));
    let d = crate::dist::deb::desktop_for_test(&act);
    assert!(d.contains("Actions=new-island;plain;\n"), "{d}");
    assert!(d.contains("[Desktop Action new-island]\nName=New Island\nExec=/usr/bin/g --new-island\n"), "{d}");
    assert!(m("[dist.actions]\nx = \"y\"\n").unwrap_err().contains("must be"));
    assert!(m("[dist.actions]\nx = { exec = \"y\" }\n").unwrap_err().contains("needs `name`"));
    assert_eq!(a.dist.icon.as_deref(), Some("i.png"));
    assert_eq!(a.dist.categories, "Game;");
    assert_eq!((a.dist.section.as_str(), a.dist.postinst.as_deref()), ("games", None));
    let b = m("[dist]\nsection = \"x11\"\npostinst = \"p/postinst\"\nprerm = \"p/prerm\"\n").unwrap();
    assert_eq!((b.dist.section.as_str(), b.dist.postinst.as_deref(), b.dist.prerm.as_deref()), ("x11", Some("p/postinst"), Some("p/prerm")));
    assert_eq!((a.version.as_str(), a.description.as_str()), ("0.1.0", "g, made with j3"));
    assert_eq!(m("[dist]\nzip = true\n").unwrap().dist.outputs, [Output::Zip]);
    assert!(m("[dist]\nbuild_outputs = [\"msi\"]\n").unwrap_err().contains("zip, deb"));
    // `apk` without an Android triple only fails where it is built (dist/apk.rs)
    assert_eq!(m("[dist]\nbuild_outputs = [\"apk\"]\n").unwrap().dist.outputs, [Output::Apk]);
    let ok = m("[emit]\ntarget = [\"aarch64-linux-android\"]\n[dist]\nbuild_outputs = [\"apk\"]\n").unwrap();
    assert_eq!(ok.dist.outputs, [Output::Apk]);
    assert_eq!(ok.android, Android { package: "com.j3.g".into(), orientation: "landscape".into(), native_libs: None, foreground_service: None });
    let a = m("[android]\norientation = \"sensorPortrait\"\npackage = \"org.me.game\"\n").unwrap();
    assert_eq!((a.android.package.as_str(), a.android.orientation.as_str()), ("org.me.game", "sensorPortrait"));
    assert_eq!(m("[android]\nnative_libs = \"android/libs\"\n").unwrap().android.native_libs.as_deref(), Some("android/libs"));
    assert!(m("[android]\norientation = \"flip\"\n").unwrap_err().contains("android.orientation"));
    assert!(m("[android]\npackage = \"1bad.x\"\n").unwrap_err().contains("android.package"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn java_package_defaults_are_valid() {
    assert_eq!(default_java_package("My-Game"), "com.j3.my_game");
    assert_eq!(default_java_package("3d-demo"), "com.j3.app3d_demo");
    assert_eq!(default_java_package("_x"), "com.j3.app_x");
    assert_eq!(default_java_package("new"), "com.j3.new_");
    assert_eq!(default_java_package("é"), "com.j3.app");
    assert!(check_java_package(&default_java_package("desk-app")));
    assert!(!check_java_package("single"));
}

#[test]
fn missing_name_is_error() {
    let err = parse_manifest("[link]\nlibs = [\"x\"]\n", PathBuf::from("/")).unwrap_err();
    assert!(err.contains("package"), "{err}");
}

/// `[steam]` belongs to the steam package now: j3c ignores the table, and `build_outputs = ["steam"]`
/// needs a dep that provides it.
#[test]
fn steam_is_a_package_output() {
    let d = std::env::temp_dir().join(format!("j3-steam-manifest-{}", std::process::id()));
    std::fs::create_dir_all(d.join("src")).unwrap();
    std::fs::write(d.join("src/main.j3"), "").unwrap();
    let p = |extra: &str| parse_manifest(&format!("[package]\nname = \"g\"\n{extra}"), d.clone());
    assert!(p("[steam]\napp_id = abc\nbogus = 1\nglibc_max = \"x\"\n").is_ok());
    let err = p("[dist]\nbuild_outputs = [\"steam\"]\n").unwrap_err();
    assert!(err.contains("no package provides \"steam\"") && err.ends_with("built in: zip, deb, apk"), "{err}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn manifest_to_json() {
    let text = r#"
[package]
name = "g"
version = "1.0.0"

[steam]
app_id = 480
sdk = "C:\\sdk"
desc = "say \"hi\"	ünï #not a comment"
ratio = -1.5e3
odd = 007
beta = true

[dist]
build_outputs = ["zip", "steam"]

[dist.actions]
new = { name = "New, Island", exec = "--new" }

[tool_pkg]
greeting = "hello"
greeting = "hi"
"#;
    let ctrl = format!("{text}[ctl]\nbell = \"a\u{7}b\"\n[empty]\n");
    let want = concat!(
        r#"{"package":{"name":"g","version":"1.0.0"},"#,
        r#""steam":{"app_id":480,"sdk":"C:\\sdk","desc":"say \"hi\"\tünï #not a comment","ratio":-1.5e3,"odd":"007","beta":true},"#,
        r#""dist":{"build_outputs":["zip","steam"],"actions":{"new":{"name":"New, Island","exec":"--new"}}},"#,
        r#""tool_pkg":{"greeting":"hi"},"ctl":{"bell":"a\u0007b"},"empty":{}}"#
    );
    assert_eq!(toml_json(&ctrl).unwrap(), want);
    // a sub-table before its parent's keys keeps file order; a key that is also a table is an error
    assert_eq!(toml_json("[a.b]\nx = 1\n[a]\ny = \"2\"\n").unwrap(), r#"{"a":{"b":{"x":1},"y":"2"}}"#);
    assert!(toml_json("[a]\nb = 1\n[a.b]\nx = 1\n").unwrap_err().contains("j3.toml:3"));
    // an escaped quote does not end a string inside an array or inline table; `\\` before `"` does
    let esc = r#"[a]
l = ["a\"b", "c", "d\\", "e"]
t = { x = "1\", y", z = "w" }
"#;
    assert_eq!(toml_json(esc).unwrap(), r#"{"a":{"l":["a\"b","c","d\\","e"],"t":{"x":"1\", y","z":"w"}}}"#);
}

/// A game at `<dir>/game` depending on the package `<dir>/tp` whose `j3.toml` ends with `tables`.
fn tool_fixture(tag: &str, tables: &str, files: &[&str]) -> PathBuf {
    let d = std::env::temp_dir().join(format!("j3-tools-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    for f in ["game/src/main.j3", "tp/src/tp.j3"].iter().chain(files) {
        let p = d.join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "").unwrap();
    }
    std::fs::write(d.join("game/j3.toml"), "[package]\nname = \"game\"\n[dependencies]\ntp = \"../tp\"\nplain = \"../plain\"\n").unwrap();
    std::fs::create_dir_all(d.join("plain")).unwrap();
    std::fs::write(d.join("tp/j3.toml"), format!("[package]\nname = \"tp\"\n{tables}")).unwrap();
    d
}

#[test]
fn dep_tools() {
    let d = tool_fixture("ok", "[commands]\ncheck = \"tools/check.j3\"\nup-load_2 = { file = \"tools/up.j3\", help = \"build, then print\" }\n[dist_outputs]\nfake = \"tools/dist.j3\"\n", &["tp/tools/check.j3", "tp/tools/up.j3", "tp/tools/dist.j3"]);
    let m = Manifest::load(&d.join("game/j3.toml")).unwrap();
    let tp = m.deps.iter().find(|x| x.name == "tp").unwrap();
    let pkg = d.join("tp").canonicalize().unwrap();
    assert_eq!((&tp.pkg_root, &tp.root), (&pkg, &pkg.join("src")));
    let cmds: Vec<(&str, &Path, Option<&str>)> = tp.commands.iter().map(|t| (t.name.as_str(), t.file.as_path(), t.help.as_deref())).collect();
    assert_eq!(cmds, [("check", pkg.join("tools/check.j3").as_path(), None), ("up-load_2", pkg.join("tools/up.j3").as_path(), Some("build, then print"))]);
    assert_eq!(tp.dist_outputs.len(), 1);
    let (who, tool) = dist_provider(&m.deps, "fake").unwrap();
    assert_eq!((who.name.as_str(), tool.file.clone()), ("tp", pkg.join("tools/dist.j3")));
    assert!(dist_provider(&m.deps, "nope").is_none());
    // a folder of modules with no j3.toml: no tools, pkg_root is the folder
    let plain = m.deps.iter().find(|x| x.name == "plain").unwrap();
    assert!(plain.commands.is_empty() && plain.dist_outputs.is_empty() && plain.pkg_root == plain.root);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn dep_tool_errors() {
    let err = |tag: &str, tables: &str, files: &[&str]| {
        let d = tool_fixture(tag, tables, files);
        let e = Manifest::load(&d.join("game/j3.toml")).unwrap_err();
        let _ = std::fs::remove_dir_all(&d);
        e
    };
    let e = err("missing", "[commands]\ncheck = \"tools/check.j3\"\n", &[]);
    assert!(e.contains("tp/j3.toml:4: package `tp` [commands] check: no file `tools/check.j3`"), "{e}");
    let e = err("list", "[dist_outputs]\nx = [\"a.j3\"]\n", &[]);
    assert!(e.contains("package `tp` [dist_outputs] x: must be a string"), "{e}");
    let e = err("nofile", "[commands]\nx = { help = \"h\" }\n", &[]);
    assert!(e.contains("[commands] x: no `file`"), "{e}");
    let e = err("badkey", "[commands]\nx = { file = \"a.j3\", run = \"y\" }\n", &["tp/a.j3"]);
    assert!(e.contains("[commands] x: unknown key `run`"), "{e}");
    let e = err("subtable", "[commands.check]\nfile = \"a.j3\"\n", &["tp/a.j3"]);
    assert!(e.contains("tp/j3.toml:3: package `tp` [commands.check]: sub-tables are not read; under [commands] write `check = {"), "{e}");
    // two deps providing one dist output
    let d = tool_fixture("dup", "[dist_outputs]\nfake = \"a.j3\"\n", &["tp/a.j3", "tq/a.j3", "tq/src/tq.j3"]);
    std::fs::write(d.join("tq/j3.toml"), "[package]\nname = \"tq\"\n[dist_outputs]\nfake = { file = \"a.j3\" }\n").unwrap();
    std::fs::write(d.join("game/j3.toml"), "[package]\nname = \"game\"\n[dependencies]\ntp = \"../tp\"\ntq = \"../tq\"\n").unwrap();
    let e = Manifest::load(&d.join("game/j3.toml")).unwrap_err();
    assert!(e.contains("[dist_outputs] fake: provided by both `tp`") && e.contains("and `tq`"), "{e}");
    let _ = std::fs::remove_dir_all(&d);
}
