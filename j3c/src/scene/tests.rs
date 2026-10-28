use super::*;

#[test]
fn robust_part_scene_is_placed_elsewhere() {
    let root = std::env::temp_dir().join(format!("j3_part_scene_test_{}", std::process::id()));
    fs::create_dir_all(root.join("scenes")).unwrap();
    fs::write(root.join("j3.editor.toml"), "[editor]\nscene = \"scenes/main.j3scene\"\n").unwrap();
    fs::write(root.join("scenes/main.j3scene"), "node.0.kind=3\nnode.0.scene s=scenes/house.j3scene\nnode.1.kind=3\nnode.1.scene s=./scenes/shed.j3scene\n").unwrap();
    fs::write(root.join("scenes/shed.j3scene"), "").unwrap();
    fs::write(root.join("scenes/house.j3scene"), "").unwrap();
    fs::write(root.join("scenes/lonely.j3scene"), "").unwrap();
    let placed = placed_scenes(&root);
    let part = |rel: &str| check_args(&root, rel, &placed).len() == 2;
    assert!(part("scenes/house.j3scene"));
    assert!(!part("scenes/main.j3scene"));
    assert!(!part("scenes/lonely.j3scene"));
    assert!(part("scenes/shed.j3scene"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn scene_undo_round_trip_and_cap() {
    let root = std::env::temp_dir().join(format!("j3_undo_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("scenes")).unwrap();
    let rel = "scenes/main.j3scene";
    fs::write(root.join(rel), "v0").unwrap();
    backup(&root, rel).unwrap();
    fs::write(root.join(rel), "v1").unwrap();
    undo(&root, rel).unwrap();
    assert_eq!(fs::read_to_string(root.join(rel)).unwrap(), "v0");
    assert!(undo(&root, rel).is_err());
    for i in 0..25 {
        backup(&root, rel).unwrap();
        fs::write(root.join(rel), format!("v{i}")).unwrap();
    }
    assert_eq!(history(&root, rel).1.len(), KEEP);
    undo(&root, rel).unwrap();
    assert_eq!(fs::read_to_string(root.join(rel)).unwrap(), "v23");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn diff_matches_objects_by_path() {
    let root = std::env::temp_dir().join(format!("j3_diff_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("scenes")).unwrap();
    let node = |i: usize, id: u32, parent: u32, name: &str, x: f32| {
        format!("node.{i}.id={id}\nnode.{i}.parent={parent}\nnode.{i}.kind=1\nnode.{i}.name s={name}\nnode.{i}.px f={x}\nnode.{i}.tr f=1.0\n")
    };
    fs::write(root.join("scenes/a.j3scene"), [node(0, 1, 0, "House", 0.0), node(1, 2, 1, "Door", 1.0), node(2, 3, 0, "Old", 0.0)].concat()).unwrap();
    // Ids shift (as after a split); matching is by path.
    fs::write(root.join("scenes/b.j3scene"), [node(0, 7, 0, "House", 0.0), node(1, 8, 7, "Door", 3.0), node(2, 9, 0, "New", 0.0)].concat()
        + "node.0.tr f=0.5\n").unwrap();
    let d = diff(&root, "scenes/b.j3scene", Some("scenes/a.j3scene")).unwrap();
    assert_eq!(d.added, ["New at 0.00,0.00,0.00"]);
    assert_eq!(d.removed, ["Old at 0.00,0.00,0.00"]);
    assert_eq!(d.moved.len(), 1, "{:?}", d.moved);
    assert!(d.moved[0].starts_with("House/Door "), "{:?}", d.moved);
    assert_eq!(d.changed, ["House: tint (default) -> 0.5,1,1"]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn apply_sends_line_breaks_as_one_byte() {
    let args = apply_args("A cube #1\n  B cube #2\n", Some("Hub")).unwrap();
    assert_eq!(args, ["apply", "lines=A cube #1\x1f  B cube #2", "parent=Hub"]);
    assert!(request_text(&args).is_ok());
    assert!(apply_args("a\tb", None).is_err());
}

#[test]
fn reply_lines_become_json() {
    let reply = Reply::parse("node\t3\tHouse/Door\tmesh\t2\tbuiltin:cube\t1,2,3\t0,90,0\t1,1,1\t\t\t\n", true);
    let json = to_json("list", "scenes/main.j3scene", &reply).stringify();
    assert!(json.contains("\"path\":\"House/Door\""), "{json}");
    assert!(json.contains("\"worldPosition\":[1,2,3]"), "{json}");
    let bad = Reply::parse("err\tno object named Dor; did you mean House/Door\n", false);
    assert!(!bad.ok);
    let json = to_json("get", "s.j3scene", &bad).stringify();
    assert!(json.contains("did you mean House/Door"), "{json}");
}
