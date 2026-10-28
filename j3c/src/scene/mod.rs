//! `j3c -e scene`: edit and check `.j3scene` files by object name. The work runs in the editor
//! frontend's headless `cli` mode (`editor/src/tool.j3`), so scene3d stays the only reader
//! and writer of the format; this side finds the project, runs it, and formats the reply.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use crate::lsp::json::Json;

mod diff;
mod history;
mod patterns;
mod reply;

pub(crate) use diff::*;
pub(crate) use history::*;
pub(crate) use patterns::*;
pub(crate) use reply::*;

const USAGE: &str = "\
usage: j3c -e scene [--project DIR] [--scene FILE] [--json] <op> [args]

Objects are named by name, by Parent/Child path, or by #id. Positions are in the parent's
space; rotations are degrees. --scene defaults to the scene in j3.editor.toml.

  list [filter=GLOB] [group=prefix] [tree=1]
                               every object: id, path, kind, asset, world position; filter
                               keeps names or paths matching (* and ?); group=prefix counts
                               names with trailing digits dropped (DecoCorn12 -> DecoCorn);
                               tree=1 prints the scene file's own lines instead
  get <object>|@scene          the object's file lines (with its children), then its fields and
                               script props (@scene: lighting)
  add <kind> [key=value ...]   kind: group mesh camera light instance cube sphere cylinder
                               cone capsule torus quad plane sky
  set <object> key=value ...   <object> may be a glob (DecoCorn*, *Tree?): sets every match
                               (dry run: list filter=GLOB). Keys: name pos x y z rot scale parent asset texture material scene
                               script tags color range fov near far alpha opacity doubleSided
                               tags a,b,key=value (games read them with scene3d.hasTag and
                               tagValue instead of name prefixes)
                               tint r,g,b (meshes and placed scenes; 1 unchanged)
                               param.<field> prop.<name> (instances; empty = default)
                               effects=a.j3fx|b.j3fx fx.<stem>.<param> (cameras)
  set @scene key=value ...     sun sunColor sky fogStart fogEnd shadows shadowSize urp
                               urpBounces urpSamples urpShadows urpShadowNear
                               prop.<name>=<type>:<default> declares a prop (empty drops)
  delete <object> ...          delete objects and everything under them; globs too
  apply [parent=OBJECT] FILE|-  add or replace objects written in the scene file's own syntax
                               (what `get` and `list tree=1` print: `Name kind key=value #id`,
                               indent nests). A line with an existing #id replaces that object
                               (what it leaves out goes back to the default); without one it adds
  split <object> ... out=FILE [instance=1] [center=1]
                               move top-level objects (and what is under them) into a new scene
                               file; center=1 recentres it on them, instance=1 leaves a placed
                               copy where they were. Games load such files with scene3d.loadParts
  inline <instance>            replace a placed scene with real copies of its objects
  fmt                          rewrite the scene file in the canonical layout (after a hand edit)
  ground <object> [on=<obj>]   rest it on the surface below (or on one object)
  ring radius=R count=N [center=x,z] [y=0] [size=sx,sy,sz] [face=tangent|out|in] [arc=a0,a1]
                               N pieces on a circle; tangent (default) turns local x outward
                               and, with size z 0, widens slabs so the ring has no slits
  stairs from=x,y,z steps=N [dir=x|-x|z|-z] [rise=0.4] [run=1] [width=2.5] [solid=true]
  spiral radius=R steps=N [center=x,z] [rise=0.4] [turn=20] [start=0] [thickness=0.6]
         [width=2] [depth=1.2] [y=0]
                               floating slabs; keep rise*360/turn above about 2.5 m headroom
  wall from=x,z to=x,z [height=3] [thickness=1.5] [y=0] [door=width,height,at]
                               a run of wall, split round a door `at` meters along it
                               All four: what=<shape|mesh|scene> (default cube; size is scale,
                               builtin shapes are 1 m), name=Base, parent=P; angles in degrees;
                               one undo step
  ontop <object> <target>      center it over target and rest it on target's top
  scatter <what> area=x0,z0,x1,z1 [count=10] [seed=1] [scale=lo,hi] [yaw=true]
          [spacing=d] [parent=P] [name=Base] [ground=true]
                               copies of a shape, mesh file, or .j3scene at random spots
  check [--all]                broken parents, missing files, placement cycles, unknown
                               scripts, dangling object fields, rotations stored in degrees,
                               too many distinct looks; --all checks scenes/*
  stats [--all]                objects, meshes, distinct looks (one draw call each), the ten
                               biggest looks, and meshes that never batch
  diff [OLD]                   objects added, removed, moved (over 1 cm), and changed since
                               OLD (a project-relative .j3scene; default: the file at git
                               HEAD), matched by path; reads files, not the open editor
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
    if args[0] == "diff" {
        if args.len() > 2 { return Err("diff takes at most one scene to compare with".into()); }
        let rel = scene_rel(&root, scene.as_deref())?;
        let d = diff(&root, &rel, args.get(1).map(String::as_str))?;
        if json {
            let list = |v: &[String]| Json::Arr(v.iter().map(|l| Json::Str(l.clone())).collect());
            println!("{}", Json::obj(vec![
                ("ok", Json::Bool(true)),
                ("scene", Json::Str(rel)),
                ("against", Json::Str(d.against.clone())),
                ("added", list(&d.added)),
                ("removed", list(&d.removed)),
                ("moved", list(&d.moved)),
                ("changed", list(&d.changed)),
            ]).stringify());
        } else {
            print!("{}", d.text(50));
        }
        return Ok(0);
    }
    if PATTERNS.contains(&args[0].as_str()) {
        let rel = scene_rel(&root, scene.as_deref())?;
        let reqs = pattern_requests(&args[0], &args[1..])?;
        let count = reqs.len();
        let reply = run_tool(&root, &rel, &reqs.join(&BATCH.to_string()))?;
        if json {
            println!("{}", to_json("add", &rel, &reply).stringify());
        } else if reply.ok {
            println!("added {count} pieces in one step");
        } else {
            print!("{}", to_text(&reply));
        }
        return Ok(if reply.ok { 0 } else { 1 });
    }
    if args[0] == "apply" {
        // apply [parent=OBJECT] FILE|-   (the object lines, as `get` prints them)
        let parent = args.iter().find_map(|a| a.strip_prefix("parent=")).map(str::to_string);
        let source = args.iter().skip(1).find(|a| !a.starts_with("parent=")).ok_or("apply needs a file of object lines, or - for stdin")?;
        let text = if source == "-" {
            let mut t = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut t).map_err(|e| e.to_string())?;
            t
        } else {
            fs::read_to_string(source).map_err(|e| format!("read {source}: {e}"))?
        };
        let rel = scene_rel(&root, scene.as_deref())?;
        let reply = run_tool(&root, &rel, &apply_args(&text, parent.as_deref())?)?;
        if json { println!("{}", to_json("apply", &rel, &reply).stringify()); } else { print!("{}", to_text(&reply)); }
        return Ok(if reply.ok { 0 } else { 1 });
    }
    if args == ["undo"] {
        println!("{}", undo(&root, &scene_rel(&root, scene.as_deref())?)?);
        return Ok(0);
    }
    let scenes = if all {
        if args[0] != "check" && args[0] != "stats" { return Err("--all only works with check and stats".into()); }
        all_scenes(&root)?
    } else {
        vec![scene_rel(&root, scene.as_deref())?]
    };
    let mut failed = false;
    let mut outputs = Vec::new();
    let placed = if args == ["check"] { placed_scenes(&root) } else { Default::default() };
    // ponytail: one frontend spawn per scene; batch in tool.j3 if --all gets slow.
    for rel in &scenes {
        let reply = if args == ["check"] { check_scene(&root, rel, &placed)? } else { run_tool(&root, rel, &args)? };
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
                  [--at X,Y,Z [--yaw D] [--pitch D]] [--views X,Y,Z,YAW,PITCH;...] [--fov D]
                  [--hide GLOB,...] [--ambient 0..1] [--ortho]

Draws the scene the way the game does (lights, shadows, sky, URP) to a PNG without showing a
window. With no --camera or --view it looks through the main camera, or frames everything
from --view iso when the scene has none. --focus frames one object and its children (from
--view's side). --select outlines an object. Default size 640x360,
default output target/j3-editor/render.png.

--at looks from a spot, no camera object needed. --yaw and --pitch are degrees, the same as a
camera object's rot y and x: yaw 0 looks toward -z, 90 toward -x, 180 toward +z, -90 toward +x;
pitch above 0 looks up. --views draws several such views (up to 16) two per row into one
image, --size per view; the reply lists them in order. --hide leaves out objects whose name
matches (* and ?) and everything under them, e.g. --hide 'Roof*' to see into a building.
--ambient mixes white into the ambient light so enclosed rooms aren't black (renders only;
default 0.25 with --at/--views, else 0). --ortho draws --view/--focus without perspective, so
--view top --ortho is a true plan view. --fov overrides the lens (degrees).";

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
                "--at" => opts.push(format!("at={}", value("--at")?)),
                "--yaw" => opts.push(format!("yaw={}", value("--yaw")?)),
                "--pitch" => opts.push(format!("pitch={}", value("--pitch")?)),
                "--fov" => opts.push(format!("fov={}", value("--fov")?)),
                "--views" => opts.push(format!("views={}", value("--views")?)),
                "--hide" => opts.push(format!("hide={}", value("--hide")?)),
                "--ambient" => opts.push(format!("ambient={}", value("--ambient")?)),
                "--ortho" => opts.push("ortho=true".into()),
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

/// Every scene another scene places (`place` objects), read once.
// ponytail: can't see scenes loaded from code; those still get the "no camera" warning only if unplaced.
pub(crate) fn placed_scenes(root: &Path) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for file in all_scenes(root).unwrap_or_default() {
        let Ok(text) = fs::read_to_string(root.join(&file)) else { continue };
        for o in crate::scenefile::parse(&text).0 {
            let placed = o.scene.trim().trim_start_matches("./");
            if o.kind == crate::scenefile::INSTANCE && !placed.is_empty() && placed != file { out.insert(placed.to_string()); }
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

/// `check` on `rel`, plus the project's own checks: the `check = "tools/check.j3s"` script
/// in `j3.editor.toml`, run as `j3c run <script> <rel>` from the project root. Each line it
/// prints starting `error: ` or `warning: ` becomes an issue; other lines are ignored.
// ponytail: one script run per scene (about a second each); pass every scene at once if slow.
pub(crate) fn check_scene(root: &Path, rel: &str, placed: &std::collections::HashSet<String>) -> Result<Reply, String> {
    let mut reply = run_tool(root, rel, &check_args(root, rel, placed))?;
    let Some(script) = crate::editor::config_value(root, "check") else { return Ok(reply) };
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let out = Command::new(exe)
        .current_dir(root)
        .args(["run", &script, rel])
        .output()
        .map_err(|e| format!("run check script {script}: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut found = false;
    for line in text.lines() {
        let (severity, message) = if let Some(m) = line.strip_prefix("error: ") {
            ("error", m)
        } else if let Some(m) = line.strip_prefix("warning: ") {
            ("warning", m)
        } else {
            continue;
        };
        found = true;
        reply.lines.push(vec!["issue".into(), severity.into(), "0".into(), format!("{message} ({script})")]);
    }
    if !out.status.success() && !found {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail: Vec<&str> = err.lines().filter(|l| !l.starts_with("j3c: ") || l.contains("error")).rev().take(12).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        reply.lines.push(vec!["issue".into(), "error".into(), "0".into(), format!("check script {script} failed ({}): {}", out.status, tail.join(" | "))]);
    }
    Ok(reply)
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

const CHANGES: &[&str] = &["add", "set", "delete", "ground", "ontop", "scatter", "fmt", "apply", "split", "inline"];

/// The `apply` request for a block of object lines (the scene file's own syntax). Line breaks
/// travel as byte 31 because a request is one tab-separated line.
pub(crate) fn apply_args(text: &str, parent: Option<&str>) -> Result<Vec<String>, String> {
    if text.contains(['\t', '\x1e', '\x1f']) { return Err("the object lines have a tab or control character".into()); }
    let mut args = vec!["apply".to_string(), format!("lines={}", text.trim_end().replace("\r\n", "\n").replace('\n', "\x1f"))];
    if let Some(p) = parent { args.push(format!("parent={p}")); }
    Ok(args)
}

#[cfg(test)]
mod tests;
