#[test]
fn game_aspect_reads_the_app_size() {
    assert_eq!(game_aspect("let app = App<World>.create(\"t\", 800, 600, boot);"), 800.0 / 600.0);
    assert_eq!(game_aspect("fn main() {}"), 16.0 / 9.0);
}

use super::*;

#[test]
fn frontend_embeds_every_source() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("editor/src");
    for e in fs::read_dir(dir).unwrap().flatten() {
        let name = e.file_name().into_string().unwrap();
        if name.ends_with(".j3") {
            assert!(FRONTEND.iter().any(|(n, _)| *n == name), "{name} is missing from FRONTEND");
        }
    }
}

#[test]
fn validates_large_scene() {
    let path = std::env::temp_dir().join(format!("j3_editor_large_{}.j3scene", std::process::id()));
    let mut file = fs::File::create(&path).unwrap();
    writeln!(file, "scene.version=1\nscene.count=150000\nscene.nextId=150001").unwrap();
    for i in 0..150000 {
        writeln!(file, "node.{i}.id={}\nnode.{i}.parent=0\nnode.{i}.kind=0", i + 1).unwrap();
    }
    drop(file);
    let result = validate_scene(&path);
    fs::remove_file(&path).unwrap();
    assert_eq!(result, Ok(()));
}

#[test]
fn catalog_lists_script_props() {
    let root = std::env::temp_dir().join(format!("j3_editor_catalog_{}", std::process::id()));
    fs::create_dir_all(root.join("scripts/doors")).unwrap();
    fs::write(root.join("j3.editor.toml"), "[editor]\nscene=\"scenes/demo.j3scene\"\n").unwrap();
    fs::write(
        root.join("scripts/doors/door.j3s"),
        "use j3.scene3d.NodeId;\nprops {\n    node: NodeId;\n    hinge: NodeId;\n    text: string = \"hi, there\";\n    speed: f32 = 2.5;\n    hp: int = -3;\n    open: bool = true;\n}\n",
    )
    .unwrap();
    assert_eq!(scene_path(&root).unwrap(), root.join("scenes/demo.j3scene"));
    generate_catalog(&root).unwrap();
    let catalog = fs::read_to_string(root.join("target/j3-editor/catalog")).unwrap();
    for line in [
        "catalog.0.name s=scripts/doors/door.j3s",
        "catalog.0.count=6",
        "catalog.0.0.required=0",
        "catalog.0.1.type s=object",
        "catalog.0.1.required=1",
        "catalog.0.2.default s=hi, there",
        "catalog.0.3.value f=2.5",
        "catalog.0.4.value=-3",
        "catalog.0.5.value=1",
    ] {
        assert!(catalog.contains(line), "{line} missing from {catalog}");
    }
    let json = script_props_json(&root, "scripts/doors/door.j3s").unwrap();
    assert!(json.contains("{\"name\":\"hinge\",\"type\":\"object\",\"default\":\"\",\"required\":true}"), "{json}");
    let made = create_script(&root, "lamp").unwrap();
    assert_eq!(made, "scripts/lamp.j3s");
    assert!(create_script(&root, "lamp").is_err());
    assert!(create_script(&root, "2lamp").is_err());
    let catalog = fs::read_to_string(root.join("target/j3-editor/catalog")).unwrap();
    assert!(catalog.contains("catalog.1.name s=scripts/lamp.j3s"), "{catalog}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rename_and_delete_assets() {
    let root = std::env::temp_dir().join(format!("j3_editor_assets_{}", std::process::id()));
    fs::create_dir_all(root.join("assets")).unwrap();
    fs::create_dir_all(root.join("scenes")).unwrap();
    fs::write(root.join("j3.editor.toml"), "[editor]\nscene = \"scenes/main.j3scene\"\n").unwrap();
    fs::write(root.join("assets/a.j3mesh"), "x").unwrap();
    fs::write(root.join("scenes/main.j3scene"), "node.0.asset s=assets/a.j3mesh\nnode.0.name s=assets/a.j3mesh.bak\nparam.1.skin s=assets/a.j3mesh\n").unwrap();
    assert_eq!(rename_asset(&root, "assets/a.j3mesh", "assets/sub/b.j3mesh").unwrap(), 1);
    assert!(root.join("assets/sub/b.j3mesh").is_file());
    let scene = fs::read_to_string(root.join("scenes/main.j3scene")).unwrap();
    assert_eq!(scene, "node.0.asset s=assets/sub/b.j3mesh\nnode.0.name s=assets/a.j3mesh.bak\nparam.1.skin s=assets/sub/b.j3mesh\n");
    assert!(rename_asset(&root, "assets/sub/b.j3mesh", "assets/../../x.j3mesh").is_err());
    assert!(rename_asset(&root, "assets/sub/b.j3mesh", "assets/b.png").is_err());
    assert!(rename_asset(&root, "scenes/main.j3scene", "scenes/other.j3scene").is_err());
    assert!(delete_asset(&root, "scenes/main.j3scene").is_err());
    assert_eq!(delete_asset(&root, "assets/sub/b.j3mesh").unwrap(), "target/j3-editor/trash/b.j3mesh");
    assert!(!root.join("assets/sub/b.j3mesh").exists());
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn robust_cache_cleanup_keeps_newest_and_locked() {
    let cache = std::env::temp_dir().join(format!("j3_editor_cache_test_{}", std::process::id()));
    fs::create_dir_all(&cache).unwrap();
    let now = std::time::SystemTime::now();
    let ago = |secs: u64| now - std::time::Duration::from_secs(secs);
    for minutes in 1..=5u64 {
        let dir = cache.join(format!("00000000000000{minutes:02}")).join("target");
        fs::create_dir_all(&dir).unwrap();
        let exe = fs::File::create(dir.join("j3_editor")).unwrap();
        exe.set_times(fs::FileTimes::new().set_modified(ago(minutes * 60))).unwrap();
    }
    let build = |n: u32| cache.join(format!("00000000000000{n:02}"));
    let lock = cache_lock(&build(5)).unwrap();
    lock.lock_shared().unwrap();
    cleanup_frontends(&cache, &build(4));
    assert!((1..=5).all(|n| build(n).exists()), "newest 3, current, and locked all stay");
    drop(lock);
    let old_tmp = cache.join("0000000000000009.tmp-1-0");
    let new_tmp = cache.join("0000000000000009.tmp-2-0");
    fs::create_dir_all(&old_tmp).unwrap();
    fs::create_dir_all(&new_tmp).unwrap();
    fs::File::open(&old_tmp).unwrap().set_times(fs::FileTimes::new().set_modified(ago(2 * 60 * 60))).unwrap();
    cleanup_frontends(&cache, &build(1));
    assert!((1..=3).all(|n| build(n).exists()));
    assert!(!build(4).exists() && !build(5).exists());
    assert!(!old_tmp.exists() && new_tmp.exists());
    fs::remove_dir_all(cache).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn robust_host_falls_back_to_running_binary() {
    let gone = host_or_self(PathBuf::from("/nonexistent/j3c (deleted)"));
    assert_eq!(gone, PathBuf::from("/proc/self/exe"));
    assert!(fs::metadata(&gone).is_ok());
    let me = std::env::current_exe().unwrap();
    assert_eq!(host_or_self(me.clone()), me);
}

#[test]
fn one_editor_owns_a_project_lock() {
    let root = std::env::temp_dir().join(format!("j3_editor_lock_test_{}", std::process::id()));
    let first = project_lock(&root).unwrap();
    assert_eq!(project_lock(&root).unwrap_err(), "editor already open for this project");
    drop(first);
    assert!(project_lock(&root).is_ok());
    fs::remove_dir_all(root).unwrap();
}
