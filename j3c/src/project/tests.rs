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
fn script_path_is_the_file_not_the_project() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/fps-shooter");
    match resolve(&root).expect("project") {
        Input::Project(m) => assert!(
            m.entry.ends_with("src/main.j3"),
            "{}",
            m.entry.display()
        ),
        Input::File(_) | Input::Hello => panic!("expected the game project"),
    }
    let script = root.join("tools/arena.j3s");
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

#[test]
fn steam_section() {
    let d = std::env::temp_dir().join(format!("j3-steam-manifest-{}", std::process::id()));
    std::fs::create_dir_all(d.join("src")).unwrap();
    std::fs::write(d.join("src/main.j3"), "").unwrap();
    let p = |extra: &str| parse_manifest(&format!("[package]\nname = \"g\"\nversion = \"2.0.0\"\n{extra}"), d.clone());
    let none = p("").unwrap().steam;
    assert_eq!((none.app_id, none.sdk, none.branch), (None, None, None));
    assert_eq!(none.desc, "g 2.0.0");
    let s = p("[steam]\napp_id = 480\nsdk = \"sdk\"\ndepot_linux = 481\ndepot_windows = \"482\"\nbranch = \"beta\"\ndesc = \"x\"\nbogus = 1\n[dist]\nbuild_outputs = [\"steam\"]\n").unwrap();
    assert_eq!(s.dist.outputs, [Output::Steam]);
    let s = s.steam;
    assert_eq!((s.app_id, s.depot_linux, s.depot_windows), (Some(480), Some(481), Some(482)));
    assert_eq!((s.sdk, s.branch.as_deref(), s.desc.as_str()), (Some(d.join("sdk")), Some("beta"), "x"));
    assert_eq!(p("[steam]\ndesc = \"say \\\"hi\\\" C:\\\\x\"\n").unwrap().steam.desc, "say \"hi\" C:\\x");
    assert_eq!(s.glibc_max, "2.31");
    assert!(p("[steam]\nglibc_max = \"x\"\n").unwrap_err().contains("steam.glibc_max"));
    assert!(p("[steam]\napp_id = abc\n").unwrap_err().contains("steam.app_id"));
    assert!(p("[steam]\ndepot_linux = 0\n").unwrap_err().contains("steam.depot_linux"));
}
