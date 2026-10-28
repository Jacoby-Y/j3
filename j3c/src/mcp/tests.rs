use super::*;

fn run(server: &mut Server, input: &str) -> Vec<Json> {
    let mut out = Vec::new();
    serve(server, input.as_bytes(), &mut out);
    String::from_utf8(out).unwrap().lines().map(|l| json::parse(l).unwrap()).collect()
}

#[test]
fn coach_points_at_bulk_tools_once() {
    let none = Json::Obj(Vec::new());
    let mut c = Coach::default();
    let mut hints = Vec::new();
    for _ in 0..6 { hints.extend(c.hints("scene_add", &none, false)); }
    assert_eq!(hints.len(), 1, "{hints:?}");
    assert!(hints[0].contains("place"));
    // Eight edits without a render, then the render resets the count.
    let mut c = Coach::default();
    let n = (0..8).flat_map(|_| c.hints("scene_delete", &none, false)).count();
    assert_eq!(n, 1);
    assert!(!c.hints("render", &none, false).is_empty(), "first plain render mentions at/views");
    assert_eq!(c.edits_unseen, 0);
    // play before check warns; after check it doesn't.
    let mut c = Coach::default();
    assert_eq!(c.hints("play", &none, false).len(), 1);
    c.hints("check", &none, false);
    assert!(c.hints("play", &none, false).is_empty());
    // Failures get no hints.
    assert!(c.hints("scene_add", &none, true).is_empty());
}

#[test]
fn project_notes_join_the_instructions() {
    let root = std::env::temp_dir().join(format!("j3_notes_test_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("j3.editor.toml"), "[editor]\nscene = \"a.j3scene\"\nnotes = \"N.md\"\n").unwrap();
    fs::write(root.join("N.md"), "pads need floors").unwrap();
    assert!(instructions(Some(&root)).ends_with("pads need floors"));
    fs::write(root.join("j3.editor.toml"), "notes = \"../N.md\"\n").unwrap();
    assert_eq!(instructions(Some(&root)), INSTRUCTIONS);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn handshake_and_tool_list() {
    let mut server = Server { root: Err("no project".into()), follow: false, next: 0, was_live: false, coach: Coach::default() };
    let replies = run(
        &mut server,
        concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            "\n\n",
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"scene_list","arguments":{}}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":4,"method":"nope"}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"nope"}}"#,
            "\n",
            "{not json\n",
        ),
    );
    assert_eq!(replies.len(), 6, "the notification gets no reply");
    let init = replies[0].get("result").unwrap();
    assert_eq!(init.get("protocolVersion").and_then(Json::as_str), Some("2025-03-26"));
    let tools = match replies[1].get("result").and_then(|r| r.get("tools")) {
        Some(Json::Arr(t)) => t.clone(),
        _ => panic!("no tools"),
    };
    let names: Vec<&str> = tools.iter().filter_map(|t| t.get("name").and_then(Json::as_str)).collect();
    assert_eq!(names, TOOL_NAMES);
    let cli: Vec<&str> = CLI_SAME.iter().map(|(n, _)| *n).collect();
    assert_eq!(cli, TOOL_NAMES, "CLI_SAME must name every tool, in order");
    assert!(EDITS.iter().all(|e| TOOL_NAMES.contains(e)));
    let text = tool_reference();
    for name in TOOL_NAMES {
        assert!(text.contains(&format!("\n{name} — ")), "{name} missing\n{text}");
    }
    assert!(text.contains("    adds*: array of object"), "{text}");
    assert!(text.contains("        kind*: string"), "{text}");
    assert!(text.contains("op*: string (ground|ontop|scatter|grid|ring|stairs|spiral|wall)"), "{text}");
    for t in &tools {
        assert!(t.get("inputSchema").and_then(|s| s.get("type")).is_some());
    }
    // A tool that fails reports it in the result, so the agent can read it.
    let call = replies[2].get("result").unwrap();
    assert!(matches!(call.get("isError"), Some(Json::Bool(true))));
    assert_eq!(replies[3].get("error").and_then(|e| e.get("code")).and_then(Json::as_i64), Some(-32601));
    assert_eq!(replies[4].get("error").and_then(|e| e.get("code")).and_then(Json::as_i64), Some(-32602));
    assert_eq!(replies[5].get("error").and_then(|e| e.get("code")).and_then(Json::as_i64), Some(-32700));
}

#[test]
fn values_become_request_text() {
    let v = json::parse(r#"{"pos":[1,2.5,-3],"scale":2,"doubleSided":true,"name":"A"}"#).unwrap();
    assert_eq!(pairs(Some(&v)).unwrap(), vec!["pos=1,2.5,-3", "scale=2", "doubleSided=true", "name=A"]);
}

#[test]
fn scene_grid_and_batch_requests() {
    let args = json::parse(r#"{"cell":2,"pieces":[{"what":"builtin:cube"},{"what":"builtin:cube","x":1},
            {"what":"builtin:cube","z":1},{"what":"builtin:cube","x":1,"z":1,"turn":1,"floor":1}]}"#).unwrap();
    let reqs = grid_requests(&args).unwrap();
    let at = |r: &Vec<String>| (r[3].clone(), r[4].clone());
    assert_eq!(at(&reqs[0]), ("pos=0,0,0".into(), "rot=0,0,0".into()));
    assert_eq!(at(&reqs[1]).0, "pos=2,0,0");
    assert_eq!(at(&reqs[2]).0, "pos=0,0,2");
    assert_eq!(at(&reqs[3]), ("pos=2,2,2".into(), "rot=0,90,0".into()));
    let bad = json::parse(r#"{"pieces":[{"what":"x.png"}]}"#).unwrap();
    assert!(grid_requests(&bad).is_err());
    let text = scene::request_text(&reqs[..2].join(&scene::BATCH.to_string())).unwrap();
    assert_eq!(text, "add\tmesh\tasset=builtin:cube\tpos=0,0,0\trot=0,0,0\x1eadd\tmesh\tasset=builtin:cube\tpos=2,0,0\trot=0,0,0");
    assert!(scene::request_text(&["name=a\x1eb".into()]).is_err());
}

#[test]
fn wildcard_matches() {
    assert!(wild("wall*.glb", "wall-door.glb"));
    assert!(!wild("wall*.glb", "roof.glb"));
    assert!(wild("*", "x"));
    assert!(wild("a*b*c", "aXbYc") && !wild("a*b*c", "aXbY"));
}

#[test]
fn import_skips_existing() {
    let root = std::env::temp_dir().join(format!("j3_mcp_reimport_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("assets/imported")).unwrap();
    fs::write(root.join("rock.j3mesh"), "# j3mesh 1\n0\n").unwrap();
    let mut server = Server { root: Err("unused".into()), follow: false, next: 0, was_live: false, coach: Coach::default() };
    let args = |extra: &str| json::parse(&format!(r#"{{"path":"rock.j3mesh","place":false{extra}}}"#)).unwrap();
    let text = |a: Answer| a.content[0].get("text").and_then(Json::as_str).unwrap().to_string();
    assert!(text(server.import(&root, &args("")).unwrap()).starts_with("imported"));
    let again = text(server.import(&root, &args("")).unwrap());
    assert!(again.starts_with("skipped") && again.contains("assets/imported/rock.j3scene"), "{again}");
    assert!(!root.join("assets/imported/rock_2.j3scene").exists());
    server.import(&root, &args(r#","again":true"#)).unwrap();
    assert!(root.join("assets/imported/rock_2.j3scene").is_file());
    fs::remove_dir_all(root).unwrap();
}
