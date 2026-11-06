//! Scene-attached scripts, script path errors and tool scripts.

use super::*;

const SCENE_TOML: &str = "[package]\nname = \"sc\"\nentry = \"src/main.j3\"\n";
const SCENE_MAIN: &str = "module sc;\nuse j3.memory.Arena;\nuse j3.option.Option;\nuse j3.scene3d;\nuse j3.scene3d.scripts;\n\
const Spin = @script(\"scripts/spin.j3s\");\n\
fn main() {\n    let mem = Arena.create(65536);\n    let scene = scene3d.load(scripts.scene(), &mem);\n    scripts.start(&scene);\n    scripts.update(&scene, 0.1);\n\
    match scene.script::<Spin>(scene.find(\"Cube\")) { Option.Some(s) { s.report(); } Option.None {} }\n}\n";
const SPIN: &str = "use j3.scene3d.{ Scene, NodeId };\nuse j3.io.printLn;\n\
props {\n    node: NodeId;\n    target: NodeId;\n    speed: f32 = 1.0;\n    label: string = \"x\";\n}\n\
pub fn update(scene: &Scene, dt: f32) {\n    let n = scene.node(node);\n    n.rotation.y += speed * dt;\n}\n\
pub fn report() { printLn(label); }\n";

fn scene_file(extra: &str) -> String {
    format!(
        "scene.version=1\nscene.count=2\nscene.nextId=3\n\
node.0.id=1\nnode.0.parent=0\nnode.0.kind=1\nnode.0.name s=Cube\nnode.0.script s=scripts/spin.j3s\n\
node.1.id=2\nnode.1.parent=0\nnode.1.kind=1\nnode.1.name s=Other\n{extra}"
    )
}

fn scene_err(scene: &str, spin: &str) -> String {
    match scene_check(scene, spin) {
        Ok(_) => panic!("expected a scene error"),
        Err(e) => e,
    }
}

fn scene_check(scene: &str, spin: &str) -> Result<Program, String> {
    let m = temp_project(
        "scene_scripts",
        &[
            ("j3.toml", SCENE_TOML),
            ("j3.editor.toml", "[editor]\nscene = \"scenes/main.j3scene\"\n"),
            ("src/main.j3", SCENE_MAIN),
            ("scripts/spin.j3s", spin),
            ("scenes/main.j3scene", scene),
        ],
    );
    let out = compile_manifest(&m);
    let _ = std::fs::remove_dir_all(&m.root);
    out
}

#[test]
fn scene_attached_scripts_check_against_the_scene() {
    let good = scene_file("param.1.target r=2\nparam.1.speed f=3\nparam.1.label s=cube\n");
    let prog = scene_check(&good, SPIN).expect("attached script checks");
    assert!(prog.cx.funcs().iter().any(|f| prog.cx.name_str(f.name) == "find__scripts__spin"));

    let missing = scene_err(&scene_file("param.1.speed f=3\n"), SPIN);
    assert!(missing.contains("scripts/spin.j3s needs prop `target`"), "{missing}");
    assert!(missing.contains("--> scenes/main.j3scene:8"), "{missing}");

    let typed = scene_err(&scene_file("param.1.target r=2\nparam.1.speed s=fast\n"), SPIN);
    assert!(typed.contains("`speed` is `f32` in scripts/spin.j3s"), "{typed}");
    assert!(typed.contains("--> scenes/main.j3scene:14"), "{typed}");
    assert!(typed.contains("param.1.speed s=fast"), "{typed}");

    let unknown = scene_err(&scene_file("param.1.target r=2\nparam.1.sped f=3\n"), SPIN);
    assert!(unknown.contains("`sped` isn't a prop of scripts/spin.j3s"), "{unknown}");

    let hook = SPIN.replace("pub fn update(scene: &Scene, dt: f32)", "pub fn update(dt: f32)");
    let bad_hook = scene_err(&scene_file("param.1.target r=2\n"), &hook);
    assert!(bad_hook.contains("must be `pub fn update(scene: &Scene, dt: f32)`"), "{bad_hook}");

    let wrong_path = scene_file("param.1.target r=2\n").replace("s=scripts/spin.j3s", "s=scripts/spn.j3s");
    let path = scene_err(&wrong_path, SPIN);
    assert!(path.contains("no script at scripts/spn.j3s"), "{path}");
    assert!(path.contains("did you mean `scripts/spin.j3s`"), "{path}");
}

#[test]
fn scene_script_lookup_needs_an_attachment() {
    let detached = scene_file("").replace("node.0.script s=scripts/spin.j3s\n", "");
    let err = scene_err(&detached, SPIN);
    assert!(err.contains("no scene attaches scripts/spin.j3s"), "{err}");
}

#[test]
fn script_path_error_suggests_existing_script() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../speculative/11_script");
    let manifest = Manifest::load(&root.join("j3.toml")).expect("script manifest");
    let mut loader = Loader {
        files: Vec::new(),
        loaded: HashSet::new(),
        next_file: 1,
        game: GameBackend::Sdl,
        arch: 0,
        os: OsKind::Linux,
        src_dir: Some(manifest.src_dir()),
        version: Some(manifest.version),
        package: Some(manifest.name),
        deps: Vec::new(),
        script_root: root,
        overlays: vec![(
            manifest.entry.clone(),
            "module script_demo; const Door = @script(\"scripts/dor.j3s\"); fn main() {}"
                .to_string(),
        )],
        scene_scan: None,
        ui_cfg: None,
        ui_gen: None,
        ui_err: None,
    };
    loader.load_path(&manifest.entry).expect("load main");
    let err = loader.load_imports().unwrap_err();
    assert!(err.contains("no script at scripts/dor.j3s"), "{err}");
    assert!(err.contains("did you mean `scripts/door.j3s`"), "{err}");
}

#[test]
fn script_references_join_import_cycles() {
    let module_a = ["script".to_string(), "scripts".to_string(), "a".to_string()];
    let module_b = ["script".to_string(), "scripts".to_string(), "b".to_string()];
    let a = "const B = @script(\"scripts/b.j3s\");";
    let b = "const A = @script(\"scripts/a.j3s\");";
    let files = vec![
        ParsedFile {
            path: "scripts/a.j3s".to_string(),
            src: a.to_string(),
            ast: parse::parse_script(a, FileId(1), &module_a).unwrap(),
            file_id: FileId(1),
        },
        ParsedFile {
            path: "scripts/b.j3s".to_string(),
            src: b.to_string(),
            ast: parse::parse_script(b, FileId(2), &module_b).unwrap(),
            file_id: FileId(2),
        },
    ];
    let err = check_import_cycles(&files).unwrap_err();
    assert!(
        err.contains("script.scripts.a -> script.scripts.b -> script.scripts.a"),
        "{err}"
    );
}

fn tool_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("j3c_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn tool_script_main_prints() {
    let dir = tool_dir("tool_main");
    let path = dir.join("greet.j3s");
    std::fs::write(
        &path,
        "struct Note { n: int; }\nuse j3.io.printLn;\npub fn main() {\n    let note = Note { n: 3 };\n    printLn(`note {note.n}`);\n}\n",
    )
    .unwrap();
    let prog = compile_path(&path).expect("tool script");
    let run = exec(&prog);
    assert_eq!(run.status, 0, "{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "note 3\n");
}

#[test]
fn tool_script_without_main_errors() {
    let dir = tool_dir("tool_nomain");
    let path = dir.join("spin.j3s");
    std::fs::write(&path, "pub fn update() {}\n").unwrap();
    let err = match compile_path(&path) {
        Ok(_) => panic!("expected a missing main"),
        Err(e) => e,
    };
    assert!(err.contains("pub fn main"), "{err}");
    assert!(err.contains("scene"), "{err}");
}

#[test]
fn tool_script_reads_args() {
    let dir = tool_dir("tool_args");
    let path = dir.join("echo.j3s");
    std::fs::write(
        &path,
        "use j3.io.printLn;\nuse j3.sys.arg;\npub fn main() { printLn(arg(1)); }\n",
    )
    .unwrap();
    let prog = compile_path(&path).expect("args script");
    let opts = llvm_opts();
    let run = emit::run_program_with(
        &prog.cx,
        &prog.modules,
        &opts,
        &["west".to_string()],
    )
    .expect("run");
    assert_eq!(run.status, 0, "{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "west\n");
}
