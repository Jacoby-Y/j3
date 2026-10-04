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

/// What `diff` found, one line per object.
pub(crate) struct Diff {
    pub against: String,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub moved: Vec<String>,
    pub changed: Vec<String>,
}

impl Diff {
    /// Counts first, then up to `cap` lines.
    pub(crate) fn text(&self, cap: usize) -> String {
        let mut out = format!(
            "{} added, {} removed, {} moved, {} changed since {}\n",
            self.added.len(),
            self.removed.len(),
            self.moved.len(),
            self.changed.len(),
            self.against
        );
        let all: Vec<String> = self.added.iter().map(|l| format!("+ {l}"))
            .chain(self.removed.iter().map(|l| format!("- {l}")))
            .chain(self.moved.iter().map(|l| format!("~ {l}")))
            .chain(self.changed.iter().map(|l| format!("* {l}")))
            .collect();
        for l in all.iter().take(cap) { out += &format!("{l}\n"); }
        if all.len() > cap { out += &format!("... and {} more\n", all.len() - cap); }
        out
    }
}

/// A scene file's objects with their `Parent/Child` path; same-path objects get `#2`, `#3`, ...
/// in file order. Reads either file version.
fn scene_objects(text: &str) -> Vec<(String, crate::scenefile::Object)> {
    use std::collections::HashMap;
    let (objects, _) = crate::scenefile::parse(text);
    let by_id: HashMap<i64, &crate::scenefile::Object> = objects.iter().map(|o| (o.id, o)).collect();
    let path_of = |o: &crate::scenefile::Object| {
        let mut parts = vec![o.name.clone()];
        let mut parent = o.parent;
        while parent != 0 && parts.len() < 64 {
            let Some(p) = by_id.get(&parent) else { break };
            parts.push(p.name.clone());
            parent = p.parent;
        }
        parts.reverse();
        parts.join("/")
    };
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::new();
    for o in &objects {
        let mut path = path_of(o);
        let n = seen.entry(path.clone()).or_insert(0);
        *n += 1;
        if *n > 1 { path = format!("{path}#{n}"); }
        out.push((path, o.clone()));
    }
    out
}

/// `diff`: `rel` now against `old` (project-relative), or against git HEAD.
// ponytail: positions are the parent's space (world for top-level objects), not world.
pub(crate) fn diff(root: &Path, rel: &str, old: Option<&str>) -> Result<Diff, String> {
    let now = fs::read_to_string(root.join(rel)).map_err(|e| format!("read {rel}: {e}"))?;
    let (before, against) = match old {
        Some(o) => {
            let o = scene_rel(root, Some(o))?;
            (fs::read_to_string(root.join(&o)).map_err(|e| format!("read {o}: {e}"))?, o)
        }
        None => {
            let out = Command::new("git")
                .current_dir(root)
                .args(["show", &format!("HEAD:./{rel}")])
                .output()
                .map_err(|e| format!("run git: {e}"))?;
            // Not in HEAD yet: everything is new.
            let text = if out.status.success() { String::from_utf8_lossy(&out.stdout).into_owned() } else { String::new() };
            (text, "git HEAD".to_string())
        }
    };
    let old = scene_objects(&before);
    let new = scene_objects(&now);
    let old_map: std::collections::HashMap<&str, &crate::scenefile::Object> =
        old.iter().map(|(p, o)| (p.as_str(), o)).collect();
    let new_paths: std::collections::HashSet<&str> = new.iter().map(|(p, _)| p.as_str()).collect();
    let show = |p: [f64; 3]| format!("{:.2},{:.2},{:.2}", p[0], p[1], p[2]);
    let mut d = Diff { against, added: Vec::new(), removed: Vec::new(), moved: Vec::new(), changed: Vec::new() };
    for (path, o) in &new {
        let Some(was) = old_map.get(path.as_str()) else {
            d.added.push(format!("{path} at {}", show(o.pos)));
            continue;
        };
        let (a, b) = (was.pos, o.pos);
        let dist = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
        if dist > 0.01 { d.moved.push(format!("{path} {} -> {} ({dist:.2} m)", show(a), show(b))); }
        let before: std::collections::BTreeMap<String, String> = was.fields().into_iter().collect();
        let now: std::collections::BTreeMap<String, String> = o.fields().into_iter().collect();
        let mut fields = Vec::new();
        for key in before.keys().chain(now.keys()).collect::<std::collections::BTreeSet<_>>() {
            let (x, y) = (before.get(key), now.get(key));
            if x != y {
                fields.push(format!("{key} {} -> {}", x.map(String::as_str).unwrap_or("(default)"), y.map(String::as_str).unwrap_or("(default)")));
            }
        }
        if !fields.is_empty() { d.changed.push(format!("{path}: {}", fields.join(", "))); }
    }
    for (path, f) in &old {
        if !new_paths.contains(path.as_str()) { d.removed.push(format!("{path} at {}", show(f.pos))); }
    }
    Ok(d)
}

const SHAPES: &[&str] = &["cube", "sphere", "cylinder", "cone", "capsule", "torus", "quad", "plane", "sky"];

/// The start of an `add` request for `what`: a shape, a mesh asset, or a scene placed as an instance.
pub(crate) fn add_what(what: &str) -> Result<Vec<String>, String> {
    let mut req = vec!["add".to_string()];
    if what.ends_with(".j3scene") {
        req.extend(["instance".into(), format!("scene={what}")]);
    } else if what.ends_with(".j3mesh") || what.starts_with("builtin:") {
        req.extend(["mesh".into(), format!("asset={what}")]);
    } else if SHAPES.contains(&what) {
        req.push(what.into());
    } else {
        return Err(format!("`{what}`: expected a .j3scene, .j3mesh, builtin: asset, or shape"));
    }
    Ok(req)
}

const PATTERNS: &[&str] = &["ring", "stairs", "spiral", "wall"];

/// `ring`, `stairs`, `spiral`, and `wall`: the `add` requests that build the shape, run as one
/// batch. `kv` are `key=value` options. Angles are degrees; rotation about +y by `a` sends
/// local +x to `(cos a, 0, -sin a)`, so a piece whose local x points along `(dx, dz)` has
/// yaw `-atan2(dz, dx)`.
// ponytail: sugar over `add` like grid; pieces go under `parent`, not a new group, since a
// batch can't name an object it hasn't made yet.
pub(crate) fn pattern_requests(op: &str, kv: &[String]) -> Result<Vec<Vec<String>>, String> {
    let mut opts = std::collections::HashMap::new();
    for a in kv {
        let (k, v) = a.split_once('=').ok_or_else(|| format!("{a}: expected key=value"))?;
        opts.insert(k.to_string(), v.to_string());
    }
    let nums = |key: &str, want: usize, default: Option<&[f64]>| -> Result<Vec<f64>, String> {
        match opts.get(key) {
            Some(v) => {
                let n: Vec<f64> = v.split(',').map(|x| x.trim().parse::<f64>()).collect::<Result<_, _>>()
                    .map_err(|_| format!("{key}: expected {want} number(s)"))?;
                if n.len() != want { return Err(format!("{key}: expected {want} number(s), got {}", n.len())); }
                Ok(n)
            }
            None => default.map(<[f64]>::to_vec).ok_or_else(|| format!("{op} needs {key}")),
        }
    };
    let one = |key: &str, default: Option<f64>| -> Result<f64, String> {
        Ok(nums(key, 1, default.as_ref().map(std::slice::from_ref))?[0])
    };
    let what = opts.get("what").cloned().unwrap_or_else(|| "cube".into());
    let name = opts.get("name").cloned();
    let parent = opts.get("parent").cloned();
    let f = |x: f64| {
        let r = (x * 1000.0).round() / 1000.0;
        if r == 0.0 { "0".to_string() } else { format!("{r}") }
    };
    let piece = |pos: [f64; 3], yaw_deg: f64, size: [f64; 3], default_name: &str| -> Result<Vec<String>, String> {
        let mut req = add_what(&what)?;
        req.push(format!("pos={},{},{}", f(pos[0]), f(pos[1]), f(pos[2])));
        req.push(format!("rot=0,{},0", f(yaw_deg)));
        req.push(format!("scale={},{},{}", f(size[0]), f(size[1]), f(size[2])));
        req.push(format!("name={}", name.clone().unwrap_or_else(|| default_name.into())));
        if let Some(p) = &parent { req.push(format!("parent={p}")); }
        Ok(req)
    };
    let known: &[&str] = match op {
        "ring" => &["center", "radius", "count", "y", "size", "face", "arc"],
        "stairs" => &["from", "dir", "steps", "rise", "run", "width", "solid"],
        "spiral" => &["center", "radius", "steps", "rise", "turn", "start", "thickness", "width", "depth", "y"],
        "wall" => &["from", "to", "height", "thickness", "door", "y"],
        _ => return Err(format!("unknown pattern {op}; expected one of {}", PATTERNS.join(", "))),
    };
    if let Some(bad) = opts.keys().find(|k| !known.contains(&k.as_str()) && !matches!(k.as_str(), "what" | "name" | "parent")) {
        return Err(format!("{op}: unknown option {bad}; options: {}, what, name, parent", known.join(", ")));
    }
    let mut reqs = Vec::new();
    match op {
        "ring" => {
            let c = nums("center", 2, Some(&[0.0, 0.0]))?;
            let r = one("radius", None)?;
            let n = one("count", None)?;
            if n < 1.0 || n > 2000.0 || n.fract() != 0.0 { return Err("count: a whole number from 1 to 2000".into()); }
            let n = n as usize;
            let y = one("y", Some(0.0))?;
            let face = opts.get("face").map(String::as_str).unwrap_or("tangent");
            let (a0, a1, full) = match opts.get("arc") {
                Some(_) => { let a = nums("arc", 2, None)?; (a[0], a[1], false) }
                None => (0.0, 360.0, true),
            };
            let step = if full { (a1 - a0) / n as f64 } else if n > 1 { (a1 - a0) / (n - 1) as f64 } else { 0.0 };
            let mut size = nums("size", 3, Some(&[1.0, 3.0, 0.0]))?;
            if face == "tangent" && size[2] == 0.0 {
                // Neighbours overlap at the outer edge, so the wall has no slits.
                size[2] = 2.0 * (r + size[0]) * (step.abs().to_radians() / 2.0).tan();
            }
            if size[2] == 0.0 { size[2] = 1.0; }
            for i in 0..n {
                let t = a0 + step * i as f64;
                let tr = t.to_radians();
                let pos = [c[0] + r * tr.cos(), y + size[1] / 2.0, c[1] + r * tr.sin()];
                // Local +x outward for tangent; local +z outward (out) or inward (in).
                let yaw = match face {
                    "tangent" => -t,
                    "out" => 90.0 - t,
                    "in" => -90.0 - t,
                    other => return Err(format!("face {other}: expected tangent, out, or in")),
                };
                reqs.push(piece(pos, yaw, [size[0], size[1], size[2]], "RingPiece")?);
            }
        }
        "stairs" => {
            let from = nums("from", 3, None)?;
            let (dx, dz) = match opts.get("dir").map(String::as_str).unwrap_or("z") {
                "x" => (1.0, 0.0),
                "-x" => (-1.0, 0.0),
                "z" => (0.0, 1.0),
                "-z" => (0.0, -1.0),
                other => return Err(format!("dir {other}: expected x, -x, z, or -z")),
            };
            let steps = one("steps", None)?;
            if steps < 1.0 || steps > 500.0 || steps.fract() != 0.0 { return Err("steps: a whole number from 1 to 500".into()); }
            let rise = one("rise", Some(0.4))?;
            let run = one("run", Some(1.0))?;
            let width = one("width", Some(2.5))?;
            let solid = opts.get("solid").map(|v| v == "true").unwrap_or(true);
            for i in 0..steps as usize {
                let top = from[1] + rise * (i + 1) as f64;
                let h = if solid { rise * (i + 1) as f64 } else { rise };
                let along = run * (i as f64 + 0.5);
                let pos = [from[0] + dx * along, top - h / 2.0, from[2] + dz * along];
                let size = if dx != 0.0 { [run, h, width] } else { [width, h, run] };
                reqs.push(piece(pos, 0.0, size, "Stair")?);
            }
        }
        "spiral" => {
            let c = nums("center", 2, Some(&[0.0, 0.0]))?;
            let r = one("radius", None)?;
            let steps = one("steps", None)?;
            if steps < 1.0 || steps > 500.0 || steps.fract() != 0.0 { return Err("steps: a whole number from 1 to 500".into()); }
            let rise = one("rise", Some(0.4))?;
            let turn = one("turn", Some(20.0))?;
            let start = one("start", Some(0.0))?;
            let thick = one("thickness", Some(0.6))?;
            let width = one("width", Some(2.0))?;
            let depth = one("depth", Some(1.2))?;
            let y = one("y", Some(0.0))?;
            for i in 0..steps as usize {
                let t = start + turn * i as f64;
                let tr = t.to_radians();
                let top = y + rise * (i + 1) as f64;
                let pos = [c[0] + r * tr.cos(), top - thick / 2.0, c[1] + r * tr.sin()];
                reqs.push(piece(pos, -t, [width, thick, depth], "SpiralStep")?);
            }
        }
        "wall" => {
            let a = nums("from", 2, None)?;
            let b = nums("to", 2, None)?;
            let h = one("height", Some(3.0))?;
            let t = one("thickness", Some(1.5))?;
            let y = one("y", Some(0.0))?;
            let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
            let len = (dx * dx + dz * dz).sqrt();
            if len < 0.01 { return Err("wall: from and to are the same point".into()); }
            let yaw = -dz.atan2(dx).to_degrees();
            let (ux, uz) = (dx / len, dz / len);
            // A run along the wall from s0 to s1, bottom b0 to top b1.
            let mut seg = |s0: f64, s1: f64, b0: f64, b1: f64| -> Result<(), String> {
                if s1 - s0 < 0.001 || b1 - b0 < 0.001 { return Ok(()); }
                let m = (s0 + s1) / 2.0;
                reqs.push(piece([a[0] + ux * m, y + (b0 + b1) / 2.0, a[1] + uz * m], yaw, [s1 - s0, b1 - b0, t], "Wall")?);
                Ok(())
            };
            match opts.get("door") {
                None => seg(0.0, len, 0.0, h)?,
                Some(_) => {
                    let d = nums("door", 3, None)?;
                    let (dw, dh, at) = (d[0], d[1], d[2]);
                    if at - dw / 2.0 < 0.0 || at + dw / 2.0 > len || dh > h {
                        return Err(format!("door {dw} wide, {dh} high at {at} does not fit a wall {len:.2} long and {h} high"));
                    }
                    seg(0.0, at - dw / 2.0, 0.0, h)?;
                    seg(at + dw / 2.0, len, 0.0, h)?;
                    seg(at - dw / 2.0, at + dw / 2.0, dh, h)?;
                }
            }
        }
        _ => unreachable!(),
    }
    if reqs.len() > 2000 { return Err(format!("{op} makes {} pieces; at most 2000", reqs.len())); }
    Ok(reqs)
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
mod pattern_tests {
    use super::*;

    fn kv(s: &str) -> Vec<String> { s.split(' ').map(str::to_string).collect() }
    fn opt<'a>(req: &'a [String], key: &str) -> &'a str {
        req.iter().find_map(|a| a.strip_prefix(&format!("{key}="))).unwrap_or("")
    }

    #[test]
    fn ring_tangent_slabs_close_the_circle() {
        let reqs = pattern_requests("ring", &kv("radius=8.5 count=12 size=1.2,6,0")).unwrap();
        assert_eq!(reqs.len(), 12);
        // The silo that needed 5.1 m slabs at r 8.5, 12 of them, 1.2 thick.
        let width: f64 = opt(&reqs[0], "scale").split(',').nth(2).unwrap().parse().unwrap();
        assert!((width - 5.198).abs() < 0.01, "{width}");
        assert_eq!(opt(&reqs[0], "pos"), "8.5,3,0");
        assert_eq!(opt(&reqs[3], "rot"), "0,-90,0");
    }

    #[test]
    fn stairs_and_door_wall() {
        let reqs = pattern_requests("stairs", &kv("from=0,0,0 dir=-x steps=3 rise=0.4 run=1 width=2")).unwrap();
        assert_eq!(opt(&reqs[2], "pos"), "-2.5,0.6,0");
        assert_eq!(opt(&reqs[2], "scale"), "1,1.2,2");
        let reqs = pattern_requests("wall", &kv("from=0,0 to=0,10 height=4 thickness=1 door=2,3,5")).unwrap();
        assert_eq!(reqs.len(), 3);
        assert_eq!(opt(&reqs[0], "rot"), "0,-90,0");
        assert_eq!(opt(&reqs[2], "pos"), "0,3.5,5");
        assert!(pattern_requests("wall", &kv("from=0,0 to=0,10 door=2,3,9.5")).is_err());
        assert!(pattern_requests("ring", &kv("radius=2 count=3 bogus=1")).is_err());
    }
}
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
            let groups: Vec<(String, Json)> = reply
                .lines
                .iter()
                .filter(|l| l[0] == "group")
                .map(|l| (at(l, 1).to_string(), Json::Num(at(l, 2).parse().unwrap_or(0.0))))
                .collect();
            if !groups.is_empty() { pairs.push(("groups", Json::Obj(groups))); }
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
        "stats" => {
            let looks = reply
                .lines
                .iter()
                .filter(|l| l[0] == "look")
                .map(|l| {
                    Json::obj(vec![
                        ("count", Json::Num(at(l, 1).parse().unwrap_or(0.0))),
                        ("asset", Json::Str(at(l, 2).into())),
                        ("material", Json::Str(at(l, 3).into())),
                        ("texture", Json::Str(at(l, 4).into())),
                        ("tint", vec3(at(l, 5))),
                        ("example", Json::Str(at(l, 6).into())),
                    ])
                })
                .collect();
            pairs.push(("looks", Json::Arr(looks)));
        }
        _ => {}
    }
    let lines: Vec<Json> = reply.lines.iter().filter(|l| l[0] == "text").map(|l| Json::Str(l[1..].join("\t"))).collect();
    if !lines.is_empty() { pairs.push(("lines", Json::Arr(lines))); }
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
            "group" => out += &format!("{:>6}  {}*\n", at(l, 2), at(l, 1)),
            "added" => out += &format!("added #{} {}\n", at(l, 1), at(l, 2)),
            "issue" => out += &format!("{}: {}\n", at(l, 1), l[3..].join(" ")),
            "look" => out += &format!("  {:>5}x {} {} {} tint {}  (e.g. {})\n", at(l, 1), at(l, 2), at(l, 3), at(l, 4), at(l, 5), at(l, 6)),
            "text" => out += &format!("{}\n", l[1..].join("\t")),
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
}
