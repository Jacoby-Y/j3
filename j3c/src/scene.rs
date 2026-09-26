//! `j3c -e scene`: edit and check `.j3scene` files by object name. The work runs in the editor
//! frontend's headless `cli` mode (`editor/src/tool.j3`), so scene3d stays the only reader
//! and writer of the format; this side finds the project, runs it, and formats the reply.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use crate::lsp::json::Json;

const USAGE: &str = "\
usage: j3c -e scene [--project DIR] [--scene FILE] [--json] <op> [args]

Objects are named by name, by Parent/Child path, or by #id. Positions are in the parent's
space; rotations are degrees. --scene defaults to the scene in j3.editor.toml.

  list                         every object: id, path, kind, asset, world position
  get <object>|@scene          fields and script props (@scene: lighting)
  add <kind> [key=value ...]   kind: group mesh camera light instance cube sphere cylinder
                               cone capsule torus quad plane sky
  set <object> key=value ...   name pos x y z rot scale parent asset texture material scene
                               script color range fov near far alpha opacity doubleSided
                               tint r,g,b (meshes and placed scenes; 1 unchanged)
                               param.<field> prop.<name> (instances; empty = default)
                               effects=a.j3fx|b.j3fx fx.<stem>.<param> (cameras)
  set @scene key=value ...     sun sunColor sky fogStart fogEnd shadows shadowSize urp
                               urpBounces urpSamples urpShadows urpShadowNear
                               prop.<name>=<type>:<default> declares a prop (empty drops)
  delete <object> ...          delete objects and everything under them
  ground <object> [on=<obj>]   rest it on the surface below (or on one object)
  ontop <object> <target>      center it over target and rest it on target's top
  scatter <what> area=x0,z0,x1,z1 [count=10] [seed=1] [scale=lo,hi] [yaw=true]
          [spacing=d] [parent=P] [name=Base] [ground=true]
                               copies of a shape, mesh file, or .j3scene at random spots
  check [--all]                broken parents, missing files, placement cycles, unknown
                               scripts, dangling object fields; --all checks scenes/*
  undo                         put back the scene file from before the last change made
                               here or through MCP (not editor edits; keeps 20)

Saving: every change here writes the scene file at once. If `j3c editor` has the scene open,
it merges the file change into its unsaved work within a second; `undo` here puts the file
back. `j3c -e mcp` edits an open editor directly instead: one undo step there, unsaved until
the person saves. Its replies say which one happened.

Check what you built: `j3c -e scene check --all`, `j3c check`, `j3c -e render --help`, then
`j3c run --frames 120 --screenshot shot.png --headless` (`j3c doc recipes check`).";

pub fn cmd(rest: &[String]) -> i32 {
    match execute(rest) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("j3c -e scene: {e}");
            1
        }
    }
}

fn execute(rest: &[String]) -> Result<i32, String> {
    let mut project: Option<String> = None;
    let mut scene: Option<String> = None;
    let mut json = false;
    let mut all = false;
    let mut args: Vec<String> = Vec::new();
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(0);
            }
            "--project" => project = Some(it.next().ok_or("--project needs a directory")?.clone()),
            "--scene" => scene = Some(it.next().ok_or("--scene needs a file")?.clone()),
            "--json" => json = true,
            "--all" => all = true,
            _ => args.push(arg.clone()),
        }
    }
    if args.is_empty() {
        println!("{USAGE}");
        return Ok(2);
    }
    let root = find_root(Path::new(project.as_deref().unwrap_or(".")))?;
    if args == ["undo"] {
        println!("{}", undo(&root, &scene_rel(&root, scene.as_deref())?)?);
        return Ok(0);
    }
    let scenes = if all {
        if args[0] != "check" { return Err("--all only works with check".into()); }
        all_scenes(&root)?
    } else {
        vec![scene_rel(&root, scene.as_deref())?]
    };
    let mut failed = false;
    let mut outputs = Vec::new();
    let placed = if args == ["check"] { placed_scenes(&root) } else { Default::default() };
    // ponytail: one frontend spawn per scene; batch in tool.j3 if --all gets slow.
    for rel in &scenes {
        let request = if args == ["check"] { check_args(&root, rel, &placed) } else { args.clone() };
        let reply = run_tool(&root, rel, &request)?;
        failed |= !reply.ok || reply.lines.iter().any(|l| l[0] == "issue" && l.get(1).map(String::as_str) == Some("error"));
        if json {
            outputs.push(to_json(&args[0], rel, &reply));
        } else {
            if scenes.len() > 1 { println!("{rel}:"); }
            print!("{}", to_text(&reply));
        }
    }
    if json {
        let out = if outputs.len() == 1 { outputs.pop().unwrap() } else { Json::Arr(outputs) };
        println!("{}", out.stringify());
    }
    Ok(if failed { 1 } else { 0 })
}

const RENDER_USAGE: &str = "\
usage: j3c -e render [--project DIR] [--scene FILE] [--camera NAME | --view iso|front|back|side|top]
                  [--focus OBJECT] [--size WxH] [--select OBJECT] [--out FILE.png]

Draws the scene the way the game does (lights, shadows, sky, URP) to a PNG without showing a
window. With no --camera or --view it looks through the main camera, or frames everything
from --view iso when the scene has none. --focus frames one object and its children (from
--view's side). --select outlines an object. Default size 640x360,
default output target/j3-editor/render.png.";

pub fn render_cmd(rest: &[String]) -> i32 {
    let run = || -> Result<i32, String> {
        let mut project: Option<String> = None;
        let mut scene: Option<String> = None;
        let mut opts = Vec::new();
        let mut it = rest.iter();
        while let Some(arg) = it.next() {
            let mut value = |name: &str| it.next().cloned().ok_or(format!("{name} needs a value"));
            match arg.as_str() {
                "--help" | "-h" => {
                    println!("{RENDER_USAGE}");
                    return Ok(0);
                }
                "--project" => project = Some(value("--project")?),
                "--scene" => scene = Some(value("--scene")?),
                "--camera" => opts.push(format!("camera={}", value("--camera")?)),
                "--view" => opts.push(format!("view={}", value("--view")?)),
                "--size" => opts.push(format!("size={}", value("--size")?)),
                "--select" => opts.push(format!("select={}", value("--select")?)),
                "--focus" => opts.push(format!("focus={}", value("--focus")?)),
                "--out" => opts.push(format!("out={}", value("--out")?)),
                other => return Err(format!("unknown argument `{other}`")),
            }
        }
        let root = find_root(Path::new(project.as_deref().unwrap_or(".")))?;
        let rel = scene_rel(&root, scene.as_deref())?;
        let out = root.join(render(&root, &rel, &opts)?.0);
        println!("{}", out.display());
        Ok(0)
    };
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("j3c -e render: {e}");
            1
        }
    }
}

/// Render scene `rel` headless. `opts` are `key=value` (camera, view, size, select, out;
/// `out` is project-relative). Returns the image path relative to `root` and what it looked
/// through.
pub(crate) fn render(root: &Path, rel: &str, opts: &[String]) -> Result<(String, String), String> {
    if let Some(bad) = opts.iter().find(|a| a.contains(['\t', '\n', '\r'])) {
        return Err(format!("option {bad:?} has a tab or line break"));
    }
    let mut opts = opts.to_vec();
    match opts.iter().find_map(|o| o.strip_prefix("out=")) {
        Some(out) => {
            let p = Path::new(out);
            if p.is_absolute() || p.components().any(|c| matches!(c, Component::ParentDir)) {
                return Err("out must be a path inside the project".into());
            }
            if let Some(dir) = root.join(p).parent() { fs::create_dir_all(dir).map_err(|e| e.to_string())?; }
        }
        None => {
            fs::create_dir_all(root.join("target/j3-editor")).map_err(|e| e.to_string())?;
            opts.push("out=target/j3-editor/render.png".into());
        }
    }
    let host = crate::editor::host_exe()?;
    let (dir, _cache_lock) = crate::editor::frontend(&host)?;
    let mut command = Command::new(dir.join("target/j3_editor"));
    command
        .current_dir(root)
        .env("J3_EDITOR_MODE", "render")
        .env("J3_EDITOR_SCENE", rel)
        .env("J3_TOOL", opts.join("\t"));
    // No display needed: SDL's offscreen driver draws through EGL.
    if std::env::var_os("SDL_VIDEODRIVER").is_none() { command.env("SDL_VIDEODRIVER", "offscreen"); }
    let out = command.output().map_err(|e| format!("run renderer: {e}"))?;
    let reply = Reply::parse(&String::from_utf8_lossy(&out.stdout), out.status.success());
    if let Some(e) = reply.error() { return Err(e); }
    match reply.lines.iter().find(|l| l[0] == "ok") {
        Some(l) if reply.ok => Ok((at(l, 1).to_string(), at(l, 2).to_string())),
        _ => Err(format!(
            "renderer failed ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        )),
    }
}

/// The nearest directory at or above `start` with a `j3.toml`.
pub(crate) fn find_root(start: &Path) -> Result<PathBuf, String> {
    let start = if start.is_absolute() {
        start.to_path_buf()
    } else {
        std::env::current_dir().map_err(|e| e.to_string())?.join(start)
    };
    let start: PathBuf = start.components().collect();
    let mut at = start.as_path();
    loop {
        if at.join("j3.toml").is_file() { return Ok(at.to_path_buf()); }
        match at.parent() {
            Some(up) => at = up,
            None => return Err(format!("no j3.toml in {} or above it", start.display())),
        }
    }
}

/// `scene` as a clean project-relative path, or the one `j3.editor.toml` names.
pub(crate) fn scene_rel(root: &Path, scene: Option<&str>) -> Result<String, String> {
    let rel = match scene {
        Some(s) => {
            let path = Path::new(s);
            let path = if path.is_absolute() {
                path.strip_prefix(root).map_err(|_| format!("{s} is outside the project"))?.to_path_buf()
            } else {
                path.to_path_buf()
            };
            if path.components().any(|c| !matches!(c, Component::Normal(_) | Component::CurDir)) {
                return Err(format!("{s}: scene paths stay inside the project"));
            }
            path
        }
        None => crate::editor::scene_path(root)?.strip_prefix(root).map_err(|e| e.to_string())?.to_path_buf(),
    };
    let rel = rel.to_string_lossy().trim_start_matches("./").to_string();
    if !rel.ends_with(".j3scene") { return Err(format!("{rel}: expected a .j3scene file")); }
    Ok(rel)
}

pub(crate) fn all_scenes(root: &Path) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let dir = root.join("scenes");
    let mut stack = vec![dir.clone()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("j3scene") {
                out.push(path.strip_prefix(root).map_err(|e| e.to_string())?.to_string_lossy().into_owned());
            }
        }
    }
    out.sort();
    if out.is_empty() { return Err("no scenes under scenes/".into()); }
    Ok(out)
}

/// A request's reply: tab-separated lines as `tool.j3` prints them.
pub(crate) struct Reply {
    pub ok: bool,
    pub lines: Vec<Vec<String>>,
}

impl Reply {
    pub(crate) fn parse(text: &str, ok: bool) -> Reply {
        let lines: Vec<Vec<String>> = text
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.split('\t').map(str::to_string).collect())
            .collect();
        let ok = ok && !lines.iter().any(|l| l[0] == "err");
        Reply { ok, lines }
    }

    /// The first `err` message, if any.
    pub(crate) fn error(&self) -> Option<String> {
        self.lines.iter().find(|l| l[0] == "err").map(|l| l[1..].join(" "))
    }
}

/// Every scene another scene places (`.scene s=` lines), read once.
// ponytail: can't see scenes loaded from code; those still get the "no camera" warning only if unplaced.
pub(crate) fn placed_scenes(root: &Path) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for file in all_scenes(root).unwrap_or_default() {
        let Ok(text) = fs::read_to_string(root.join(&file)) else { continue };
        for line in text.lines() {
            let Some((_, placed)) = line.split_once(".scene s=") else { continue };
            let placed = placed.trim().trim_start_matches("./");
            if placed != file { out.insert(placed.to_string()); }
        }
    }
    out
}

/// The `check` request for `rel`. `placed` is `placed_scenes(root)`: a scene other scenes
/// place and the game never starts in needs no camera (`check\tpart`).
pub(crate) fn check_args(root: &Path, rel: &str, placed: &std::collections::HashSet<String>) -> Vec<String> {
    let part = placed.contains(rel.trim_start_matches("./")) && !crate::editor::is_main_scene(root, rel);
    if part { vec!["check".into(), "part".into()] } else { vec!["check".into()] }
}

/// Run one request headless on the scene file `rel` in `root`.
/// Separates the requests of a batch in `args`: `tool.apply` runs them in order as one change.
pub(crate) const BATCH: &str = "\x1e";

/// The tab-joined request text for `args`; a lone `BATCH` argument splits requests.
pub(crate) fn request_text(args: &[String]) -> Result<String, String> {
    let mut text = String::new();
    for a in args {
        if a == BATCH {
            text.push_str(BATCH);
            continue;
        }
        if a.contains(['\t', '\n', '\r', '\x1e']) {
            return Err(format!("argument {a:?} has a tab or line break"));
        }
        if !text.is_empty() && !text.ends_with(BATCH) { text.push('\t'); }
        text.push_str(a);
    }
    Ok(text)
}

pub(crate) fn run_tool(root: &Path, rel: &str, args: &[String]) -> Result<Reply, String> {
    let request = request_text(args)?;
    // Keep the script catalog current so props and checks match the scripts.
    if root.join("j3.editor.toml").is_file() {
        fs::create_dir_all(root.join("target/j3-editor")).map_err(|e| e.to_string())?;
        crate::editor::generate_catalog(root)?;
    }
    let host = crate::editor::host_exe()?;
    let (dir, _cache_lock) = crate::editor::frontend(&host)?;
    let changes = request.split(BATCH).any(|r| CHANGES.contains(&r.split('\t').next().unwrap_or("")));
    let saved = if changes { backup(root, rel) } else { None };
    let out = Command::new(dir.join("target/j3_editor"))
        .current_dir(root)
        .env("J3_EDITOR_MODE", "cli")
        .env("J3_EDITOR_SCENE", rel)
        .env("J3_TOOL", request)
        .output()
        .map_err(|e| format!("run scene tool: {e}"))?;
    // A request that changed nothing leaves no undo step.
    if let Some(b) = saved {
        if fs::read(&b).ok() == fs::read(root.join(rel)).ok() { let _ = fs::remove_file(b); }
    }
    let text = String::from_utf8_lossy(&out.stdout);
    if text.trim().is_empty() && !out.status.success() {
        return Err(format!("scene tool failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(Reply::parse(&text, out.status.success()))
}

const CHANGES: &[&str] = &["add", "set", "delete", "ground", "ontop", "scatter"];
const KEEP: usize = 20;

/// Backups of `rel`, oldest first: `target/j3-editor/history/<rel, / as __>.<unix millis>`.
fn history(root: &Path, rel: &str) -> (PathBuf, Vec<PathBuf>) {
    let dir = root.join("target/j3-editor/history");
    let prefix = format!("{}.", rel.trim_start_matches("./").replace('/', "__"));
    let mut found: Vec<(u128, PathBuf)> = fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let stamp = name.strip_prefix(&prefix)?.parse().ok()?;
            Some((stamp, e.path()))
        })
        .collect();
    found.sort();
    (dir.join(prefix), found.into_iter().map(|(_, p)| p).collect())
}

/// Copy the scene aside before a change; keeps the newest `KEEP`.
// ponytail: whole-file copies (scenes are small text), no redo.
fn backup(root: &Path, rel: &str) -> Option<PathBuf> {
    let (base, old) = history(root, rel);
    fs::create_dir_all(base.parent()?).ok()?;
    let mut millis = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_millis();
    if let Some(last) = old.last() {
        let prev: u128 = last.extension()?.to_str()?.parse().ok()?;
        millis = millis.max(prev + 1);
    }
    let path = PathBuf::from(format!("{}{millis}", base.display()));
    fs::copy(root.join(rel), &path).ok()?;
    for extra in old.iter().take((old.len() + 1).saturating_sub(KEEP)) { let _ = fs::remove_file(extra); }
    Some(path)
}

/// Put the newest backup of `rel` back over it.
pub(crate) fn undo(root: &Path, rel: &str) -> Result<String, String> {
    let (_, old) = history(root, rel);
    let last = old.last().ok_or_else(|| format!("no earlier version of {rel} to go back to"))?;
    fs::copy(last, root.join(rel)).map_err(|e| e.to_string())?;
    fs::remove_file(last).map_err(|e| e.to_string())?;
    Ok(format!("{rel} is back to how it was before the last change ({} earlier versions left)", old.len() - 1))
}

fn vec3(s: &str) -> Json {
    Json::Arr(s.split(',').map(|v| Json::Num(v.parse().unwrap_or(0.0))).collect())
}

fn at(line: &[String], i: usize) -> &str {
    line.get(i).map(String::as_str).unwrap_or("")
}

fn node_json(l: &[String]) -> Json {
    Json::obj(vec![
        ("id", Json::Num(at(l, 1).parse().unwrap_or(0.0))),
        ("path", Json::Str(at(l, 2).into())),
        ("kind", Json::Str(at(l, 3).into())),
        ("parent", Json::Num(at(l, 4).parse().unwrap_or(0.0))),
        ("asset", Json::Str(at(l, 5).into())),
        ("worldPosition", vec3(at(l, 6))),
        ("rotation", vec3(at(l, 7))),
        ("scale", vec3(at(l, 8))),
        ("script", Json::Str(at(l, 9).into())),
        ("material", Json::Str(at(l, 10).into())),
        ("scene", Json::Str(at(l, 11).into())),
    ])
}

/// The reply as JSON: `{ok, scene, nodes|node|id|issues|message|error}`.
pub(crate) fn to_json(op: &str, scene: &str, reply: &Reply) -> Json {
    let mut pairs: Vec<(&str, Json)> = vec![("ok", Json::Bool(reply.ok)), ("scene", Json::Str(scene.into()))];
    if let Some(e) = reply.error() {
        pairs.push(("error", Json::Str(e)));
        return Json::obj(pairs);
    }
    match op {
        "list" => {
            let nodes = reply.lines.iter().filter(|l| l[0] == "node").map(|l| node_json(l)).collect();
            pairs.push(("nodes", Json::Arr(nodes)));
        }
        "get" => {
            let fields: Vec<(String, Json)> = reply
                .lines
                .iter()
                .filter(|l| l[0] == "field")
                .map(|l| (at(l, 1).to_string(), Json::Str(at(l, 2).into())))
                .collect();
            pairs.push(("fields", Json::Obj(fields)));
        }
        "check" => {
            let issues = reply
                .lines
                .iter()
                .filter(|l| l[0] == "issue")
                .map(|l| {
                    Json::obj(vec![
                        ("severity", Json::Str(at(l, 1).into())),
                        ("node", Json::Num(at(l, 2).parse().unwrap_or(0.0))),
                        ("message", Json::Str(l[3..].join(" "))),
                    ])
                })
                .collect();
            pairs.push(("issues", Json::Arr(issues)));
        }
        _ => {}
    }
    if let Some(l) = reply.lines.iter().find(|l| l[0] == "added") {
        pairs.push(("id", Json::Num(at(l, 1).parse().unwrap_or(0.0))));
        pairs.push(("path", Json::Str(at(l, 2).into())));
    }
    if let Some(l) = reply.lines.iter().find(|l| l[0] == "ok") {
        pairs.push(("message", Json::Str(l[1..].join(" "))));
    }
    Json::obj(pairs)
}

/// The reply for people (and agents reading plain text).
pub(crate) fn to_text(reply: &Reply) -> String {
    let mut out = String::new();
    for l in &reply.lines {
        match l[0].as_str() {
            "node" => {
                let mut row = format!("#{:<5} {:<32} {:<8}", at(l, 1), at(l, 2), at(l, 3));
                if !at(l, 5).is_empty() { row += &format!(" {}", at(l, 5)); }
                if !at(l, 11).is_empty() { row += &format!(" {}", at(l, 11)); }
                row += &format!("  at {}", at(l, 6));
                if !at(l, 9).is_empty() { row += &format!("  [{}]", at(l, 9)); }
                out += row.trim_end();
                out.push('\n');
            }
            "field" => out += &format!("{}: {}\n", at(l, 1), at(l, 2)),
            "added" => out += &format!("added #{} {}\n", at(l, 1), at(l, 2)),
            "issue" => out += &format!("{}: {}\n", at(l, 1), l[3..].join(" ")),
            "ok" => out += &format!("{}\n", l[1..].join(" ")),
            "err" => out += &format!("error: {}\n", l[1..].join(" ")),
            _ => out += &format!("{}\n", l.join(" ")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn robust_part_scene_is_placed_elsewhere() {
        let root = std::env::temp_dir().join(format!("j3_part_scene_test_{}", std::process::id()));
        fs::create_dir_all(root.join("scenes")).unwrap();
        fs::write(root.join("j3.editor.toml"), "[editor]\nscene = \"scenes/main.j3scene\"\n").unwrap();
        fs::write(root.join("scenes/main.j3scene"), "node.0.scene s=scenes/house.j3scene\nnode.1.scene s=./scenes/shed.j3scene\n").unwrap();
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
}
