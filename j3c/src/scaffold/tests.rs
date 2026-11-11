//! Template tests: each project type typechecks and uses the project name.

use super::*;

fn scratch() -> PathBuf {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("j3-new-{n}"));
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn hello_project_typechecks() {
    let dir = scratch();
    let root = dir.join("hello");
    let (err, module) = create(Some(root.to_str().unwrap()), false).expect("create");
    assert_eq!(module, "hello");
    assert!(err.join("j3.toml").is_file());
    let src = std::fs::read_to_string(err.join("src/main.j3")).unwrap();
    assert!(src.contains("module hello;"), "{src}");
    assert!(src.contains("printLn"), "{src}");
    let agents = std::fs::read_to_string(err.join("AGENTS.md")).unwrap();
    assert!(agents.contains("Learn J3 from `j3c` only"), "{agents}");
    assert!(agents.contains("@j3ui"), "{agents}");
    assert!(agents.contains("j3c doc j3ui"), "{agents}");
    assert!(agents.contains("j3c doc j3.gfx.canvas"), "{agents}");
    assert!(agents.contains("textureFromCanvas"), "{agents}");
    assert!(agents.contains("buttonOff"), "{agents}");
    assert!(!agents.contains(SCENES_MARKER), "plain projects skip the scene section");
    assert_eq!(std::fs::read_to_string(err.join("CLAUDE.md")).unwrap(), "@AGENTS.md\n");
    assert!(agents.contains("tryLoadFont"), "{agents}");
    assert!(agents.contains("program exited"), "{agents}");
    let code = crate::cli::run(["check".into(), err.display().to_string()]);
    assert_eq!(code, 0);
    let again = create(Some(root.to_str().unwrap()), false).unwrap_err();
    assert!(again.contains("already exists"), "{again}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn app_project_typechecks() {
    let dir = scratch();
    let root = dir.join("window");
    let (made, _) = create(Some(root.to_str().unwrap()), true).expect("create");
    let toml = std::fs::read_to_string(made.join("j3.toml")).unwrap();
    assert!(toml.contains("libs = [\"SDL2\"]"), "{toml}");
    assert!(toml.contains("does not close the window"), "{toml}");
    let code = crate::cli::run(["check".into(), made.display().to_string()]);
    assert_eq!(code, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ui_template_checks() {
    let dir = std::env::temp_dir().join(format!("j3-ui-template-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let made = create_ui(Some(dir.join("panel").to_str().unwrap())).expect("create");
    assert!(made.join("ui/Screen.j3ui").is_file());
    let agents = std::fs::read_to_string(made.join("AGENTS.md")).unwrap();
    assert!(agents.contains("j3c doc css") && agents.contains("--headless"), "{agents}");
    assert_eq!(crate::cli::run(["check".into(), made.display().to_string()]), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fpwalk_template_typechecks() {
    let dir = scratch();
    let made = create_fps(Some(dir.join("walk").to_str().unwrap())).expect("create");
    for file in ["scripts/spin.j3s", "scenes/main.j3scene", "j3.editor.toml", "AGENTS.md", "CLAUDE.md"] {
        assert!(made.join(file).is_file(), "{file}");
    }
    let agents = std::fs::read_to_string(made.join("AGENTS.md")).unwrap();
    for want in [SCENES_MARKER, "scene_stats", "place", "degrees", "claude mcp add", "hasTag", "check = "] {
        assert!(agents.contains(want), "{want}");
    }
    // An older AGENTS.md gets the section once.
    std::fs::write(made.join("AGENTS.md"), "# old\n").unwrap();
    add_scenes_section(&made);
    add_scenes_section(&made);
    let agents = std::fs::read_to_string(made.join("AGENTS.md")).unwrap();
    assert_eq!(agents.matches(SCENES_MARKER).count(), 1, "{agents}");
    assert!(agents.starts_with("# old"), "{agents}");
    let main = std::fs::read_to_string(made.join("src/main.j3")).unwrap();
    assert!(main.contains("drawFrame"), "{main}");
    let code = crate::cli::run(["check".into(), made.display().to_string()]);
    assert_eq!(code, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every template names the project after its directory, never the placeholder `mygame`, and
/// writes the canonical quoted `backend`.
#[test]
fn templates_use_the_project_name() {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() { walk(&p, out) } else { out.push(p) }
        }
    }
    let dir = scratch();
    let roots = [
        create(Some(dir.join("todo").to_str().unwrap()), false).unwrap().0,
        create(Some(dir.join("todoapp").to_str().unwrap()), true).unwrap().0,
        create_fps(Some(dir.join("todofps").to_str().unwrap())).unwrap(),
        create_ui(Some(dir.join("todoui").to_str().unwrap())).unwrap(),
    ];
    for root in roots {
        let name = root.file_name().unwrap().to_str().unwrap().to_string();
        let mut files = Vec::new();
        walk(&root, &mut files);
        for f in files {
            if f.extension().is_some_and(|x| x == "ttf") { continue; }
            let text = std::fs::read_to_string(&f).unwrap_or_default();
            if f.file_name().unwrap() != "AGENTS.md" {
                assert!(!text.contains("mygame"), "{} mentions mygame", f.display());
            }
            if f.file_name().unwrap() == "j3.toml" {
                assert!(text.contains(&format!("name = \"{name}\"")), "{text}");
                assert!(text.contains("backend = \"llvm\""), "{text}");
            }
        }
    }
    assert!(std::fs::metadata(dir.join("todoui/assets/font.ttf")).unwrap().len() > 10_000);
    assert!(std::fs::read_to_string(dir.join("todoui/assets/LICENSE-DejaVu.txt")).unwrap().contains("Bitstream"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn directory_name_becomes_a_module() {
    assert_eq!(module_name("my-game"), "my_game");
    assert_eq!(module_name("My Game"), "my_game");
    assert_eq!(module_name("bounce"), "bounce");
    assert_eq!(module_name("123go"), "_123go");
    assert_eq!(module_name("fn"), "fn_");
    assert_eq!(module_name("!!!"), "game");
    let dir = scratch();
    let root = dir.join("my-game");
    let (made, module) = create(Some(root.to_str().unwrap()), false).expect("create");
    assert_eq!(module, "my_game");
    let src = std::fs::read_to_string(made.join("src/main.j3")).unwrap();
    assert!(src.contains("module my_game;"), "{src}");
    let toml = std::fs::read_to_string(made.join("j3.toml")).unwrap();
    assert!(toml.contains("name = \"my_game\""), "{toml}");
    let code = crate::cli::run(["check".into(), made.display().to_string()]);
    assert_eq!(code, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn lib_project_checks_and_its_demo_uses_it() {
    let dir = scratch();
    let root = dir.join("hello");
    let (root, module, _) = create_lib(Some(root.to_str().unwrap())).expect("create");
    assert_eq!(module, "hello");
    let lib = crate::project::Manifest::load(&root.join("j3.toml")).expect("lib manifest");
    assert!(lib.lib);
    crate::compile::compile_manifest(&lib).unwrap_or_else(|e| panic!("{e}"));
    let demo = crate::project::Manifest::load(&root.join("examples/demo/j3.toml")).expect("demo manifest");
    crate::compile::compile_manifest(&demo).unwrap_or_else(|e| panic!("{e}"));
    // again on the same folder: adds nothing, keeps the files
    let (_, _, made) = create_lib(Some(root.to_str().unwrap())).expect("again");
    assert!(made.is_empty(), "{made:?}");
}
