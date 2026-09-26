//! Project-owned scene files and a bundled J3 desktop editor.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use crate::project::Manifest;

/// The editor's `src/` files, by name.
const FRONTEND: &[(&str, &str)] = &[
    ("assets.j3", include_str!("../editor/src/assets.j3")),
    ("cameras.j3", include_str!("../editor/src/cameras.j3")),
    ("commands.j3", include_str!("../editor/src/commands.j3")),
    ("console.j3", include_str!("../editor/src/console.j3")),
    ("files.j3", include_str!("../editor/src/files.j3")),
    ("gizmo.j3", include_str!("../editor/src/gizmo.j3")),
    ("history.j3", include_str!("../editor/src/history.j3")),
    ("host.j3", include_str!("../editor/src/host.j3")),
    ("input.j3", include_str!("../editor/src/input.j3")),
    ("layout.j3", include_str!("../editor/src/layout.j3")),
    ("live.j3", include_str!("../editor/src/live.j3")),
    ("main.j3", include_str!("../editor/src/main.j3")),
    ("materials.j3", include_str!("../editor/src/materials.j3")),
    ("menu.j3", include_str!("../editor/src/menu.j3")),
    ("nodes.j3", include_str!("../editor/src/nodes.j3")),
    ("overlay.j3", include_str!("../editor/src/overlay.j3")),
    ("overrides.j3", include_str!("../editor/src/overrides.j3")),
    ("picker.j3", include_str!("../editor/src/picker.j3")),
    ("prefs.j3", include_str!("../editor/src/prefs.j3")),
    ("render.j3", include_str!("../editor/src/render.j3")),
    ("scenes.j3", include_str!("../editor/src/scenes.j3")),
    ("selection.j3", include_str!("../editor/src/selection.j3")),
    ("session.j3", include_str!("../editor/src/session.j3")),
    ("shared.j3", include_str!("../editor/src/shared.j3")),
    ("tabs.j3", include_str!("../editor/src/tabs.j3")),
    ("tool.j3", include_str!("../editor/src/tool.j3")),
    ("types.j3", include_str!("../editor/src/types.j3")),
    ("ui_dialogs.j3", include_str!("../editor/src/ui_dialogs.j3")),
    ("ui_inspector.j3", include_str!("../editor/src/ui_inspector.j3")),
    ("ui_material.j3", include_str!("../editor/src/ui_material.j3")),
    ("ui_panes.j3", include_str!("../editor/src/ui_panes.j3")),
    ("ui_props.j3", include_str!("../editor/src/ui_props.j3")),
    ("ui_top.j3", include_str!("../editor/src/ui_top.j3")),
    ("ui_tree.j3", include_str!("../editor/src/ui_tree.j3")),
    ("ui_online.j3", include_str!("../editor/src/ui_online.j3")),
    ("util.j3", include_str!("../editor/src/util.j3")),
    ("viewport.j3", include_str!("../editor/src/viewport.j3")),
    ("watch.j3", include_str!("../editor/src/watch.j3")),
    ("widgets.j3", include_str!("../editor/src/widgets.j3")),
];
const FONT: &[u8] = include_bytes!("../editor/font.ttf");

pub fn cmd(rest: &[String]) -> i32 {
    match execute(rest) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("j3c editor: {e}");
            1
        }
    }
}

fn execute(rest: &[String]) -> Result<(), String> {
    if rest.iter().any(|s| s == "--help" || s == "-h") {
        println!("usage: j3c editor [--new] [project]");
        return Ok(());
    }
    let mut create = false;
    let mut path: Option<&str> = None;
    for arg in rest {
        if arg == "--new" {
            create = true;
        } else if arg.starts_with('-') {
            return Err(format!("unknown option `{arg}`"));
        } else if path.replace(arg).is_some() {
            return Err("expected one project path".into());
        }
    }
    let requested = PathBuf::from(path.unwrap_or("."));
    let root = if requested.is_absolute() {
        requested
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(requested)
    };
    if create {
        create_project(&root, false)?;
    } else {
        let fresh = !root.join("j3.editor.toml").exists();
        add_editor_files(&root)?;
        if fresh {
            println!(
                "Added j3.editor.toml and scenes/main.j3scene to {}",
                root.display()
            );
            println!("To use the scene in this game's code:");
            println!("  use j3.scene3d;");
            println!("  let scene = scene3d.load(\"scenes/main.j3scene\", mem);");
            println!("  scene3d.upload(&scene); // after gfx.init()");
            println!("  scene3d.draw(&scene);   // in shadow and camera passes");
            println!("The existing entry point was left as it was.");
        }
    }
    let manifest = Manifest::load(&root.join("j3.toml"))?;
    let scene = scene_path(&root)?;
    validate_scene(&scene)?;
    let _project_lock = project_lock(&root)?;
    note_open(&root);
    let _ = &manifest;
    generate_catalog(&root)?;
    for line in crate::gen::run_all(&root, false).failed {
        eprintln!("warning: mesh generator failed: {line}");
    }
    generate_assets_catalog(&root)?;
    launch(&root, &scene)
}

/// `j3c new NAME --template fps`: the editor project with a walkable scene.
pub(crate) fn create_fps_project(root: &Path) -> Result<(), String> {
    create_project(root, true)
}

/// A new editor project. `fps` picks the first-person main and scene.
// ponytail: a bool for two templates; make it an enum when a third arrives.
fn create_project(root: &Path, fps: bool) -> Result<(), String> {
    let raw_name = root
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| *s != ".")
        .map(str::to_string)
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .and_then(|p| p.file_name().and_then(|s| s.to_str()).map(str::to_string))
        })
        .unwrap_or_else(|| "game".into());
    let files = project_files(&module_name(&raw_name), fps);
    for (relative, _) in &files {
        if root.join(relative).exists() {
            return Err(format!("{} already exists", root.join(relative).display()));
        }
    }
    fs::create_dir_all(root.join("assets")).map_err(|e| e.to_string())?;
    for (relative, text) in files {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(path, text).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Every file of a new editor project (`j3c doc recipes scene3d` prints them too).
pub(crate) fn project_files(name: &str, fps: bool) -> Vec<(&'static str, String)> {
    vec![
        ("j3.toml", format!("[package]\nname = \"{name}\"\nentry = \"src/main.j3\"\n\n[emit]\nbackend = llvm\nopt = 0\ndebug = true\n\n[link]\nlibs = [\"SDL2\", \"GL\"]\n")),
        ("j3.editor.toml", "[editor]\nscene = \"scenes/main.j3scene\"\n".to_string()),
        ("src/main.j3", if fps { fps_template(name) } else { game_template(name) }),
        ("scripts/spin.j3s", SPIN_SCRIPT.to_string()),
        ("scenes/main.j3scene", (if fps { FPS_SCENE } else { START_SCENE }).to_string()),
        ("assets/gen/tree.j3", TREE_GEN.to_string()),
    ]
}

const START_SCENE: &str = "scene.version=1\nscene.count=2\nscene.nextId=3\n\
node.0.id=1\nnode.0.parent=0\nnode.0.kind=2\nnode.0.name s=Camera\nnode.0.asset s=\nnode.0.texture s=\nnode.0.script s=\n\
node.0.px f=0\nnode.0.py f=2\nnode.0.pz f=7\nnode.0.rx f=-0.2\nnode.0.ry f=0\nnode.0.rz f=0\n\
node.0.sx f=1\nnode.0.sy f=1\nnode.0.sz f=1\nnode.0.fov f=60\nnode.0.near f=0.15\nnode.0.far f=200\n\
node.1.id=2\nnode.1.parent=0\nnode.1.kind=1\nnode.1.name s=Cube\nnode.1.asset s=builtin:cube\nnode.1.texture s=\nnode.1.script s=scripts/spin.j3s\n\
node.1.px f=0\nnode.1.py f=0\nnode.1.pz f=0\nnode.1.rx f=0\nnode.1.ry f=0\nnode.1.rz f=0\n\
node.1.sx f=1\nnode.1.sy f=1\nnode.1.sz f=1\n";

/// The `--template fps` scene: a camera (where the walker starts), a floor, and things to bump into.
const FPS_SCENE: &str = "scene.version=1\nscene.count=5\nscene.nextId=6\n\
node.0.id=1\nnode.0.parent=0\nnode.0.kind=2\nnode.0.name s=Camera\nnode.0.asset s=\nnode.0.texture s=\nnode.0.script s=\n\
node.0.px f=0\nnode.0.py f=0\nnode.0.pz f=8\nnode.0.rx f=0\nnode.0.ry f=0\nnode.0.rz f=0\n\
node.0.sx f=1\nnode.0.sy f=1\nnode.0.sz f=1\nnode.0.fov f=70\nnode.0.near f=0.1\nnode.0.far f=200\n\
node.1.id=2\nnode.1.parent=0\nnode.1.kind=1\nnode.1.name s=Ground\nnode.1.asset s=builtin:plane\nnode.1.texture s=\nnode.1.script s=\n\
node.1.px f=0\nnode.1.py f=0\nnode.1.pz f=0\nnode.1.rx f=0\nnode.1.ry f=0\nnode.1.rz f=0\n\
node.1.sx f=40\nnode.1.sy f=1\nnode.1.sz f=40\n\
node.2.id=3\nnode.2.parent=0\nnode.2.kind=1\nnode.2.name s=Crate\nnode.2.asset s=builtin:cube\nnode.2.texture s=\nnode.2.script s=\n\
node.2.px f=3\nnode.2.py f=0.5\nnode.2.pz f=0\nnode.2.rx f=0\nnode.2.ry f=0\nnode.2.rz f=0\n\
node.2.sx f=1\nnode.2.sy f=1\nnode.2.sz f=1\n\
node.3.id=4\nnode.3.parent=0\nnode.3.kind=1\nnode.3.name s=Wall\nnode.3.asset s=builtin:cube\nnode.3.texture s=\nnode.3.script s=\n\
node.3.px f=0\nnode.3.py f=1\nnode.3.pz f=-6\nnode.3.rx f=0\nnode.3.ry f=0\nnode.3.rz f=0\n\
node.3.sx f=8\nnode.3.sy f=2\nnode.3.sz f=0.5\n\
node.4.id=5\nnode.4.parent=0\nnode.4.kind=1\nnode.4.name s=Spinner\nnode.4.asset s=builtin:cube\nnode.4.texture s=\nnode.4.script s=scripts/spin.j3s\n\
node.4.px f=-3\nnode.4.py f=0.5\nnode.4.pz f=0\nnode.4.rx f=0\nnode.4.ry f=0\nnode.4.rz f=0\n\
node.4.sx f=1\nnode.4.sy f=1\nnode.4.sz f=1\n";

/// Example mesh generator for new projects; `j3c -e gen` turns it into `assets/gen/tree.j3mesh`.
pub(crate) const TREE_GEN: &str = "// A low-poly tree built in code: a tapered trunk and three cones of leaves.
// `j3c -e gen` (and every build, render, and play) runs it when it changes and writes
// assets/gen/tree.j3mesh. `j3c doc j3.gfx.meshgen` lists the shapes.
module gen_tree;

use j3.memory.Arena;
use j3.gfx.meshgen;
use j3.gfx.meshgen.Builder;

fn main() {
    let mem = Arena.create(4 * 1024 * 1024);
    let b = Builder.new(&mem);
    b.color(0.45, 0.3, 0.18);
    b.push();
    b.at(0, 0.6, 0);
    b.frustum(0.16, 0.1, 1.2, 10);
    b.pop();
    b.color(0.2, 0.55, 0.25);
    let tier = 0;
    while tier < 3 {
        b.push();
        b.at(0, 1.3 + f32(tier) * 0.55, 0);
        b.cone(0.8 - f32(tier) * 0.2, 1.0, 12);
        b.pop();
        tier += 1;
    }
    b.save(meshgen.output(\"assets/gen/tree.j3mesh\"));
}
";

/// The script new editor projects attach to a cube.
const SPIN_SCRIPT: &str = "// Attached to objects in the scene editor. The scene calls `start` once and `update`
// every frame; `node` is the object this copy is attached to.
use j3.scene3d.{ Scene, NodeId };

props {
    node: NodeId;
    /// Turns per second, in radians.
    speed: f32 = 1.0;
}

pub fn start(scene: &Scene) {
}

pub fn update(scene: &Scene, dt: f32) {
    let n = scene.node(node);
    n.rotation.y += speed * dt;
}
";

fn add_editor_files(root: &Path) -> Result<(), String> {
    if !root.join("j3.toml").is_file() {
        return Err(format!("no j3.toml in {}", root.display()));
    }
    if !root.join("j3.editor.toml").exists() {
        fs::write(
            root.join("j3.editor.toml"),
            "[editor]\nscene = \"scenes/main.j3scene\"\n",
        )
        .map_err(|e| e.to_string())?;
    }
    let scene = scene_path(root)?;
    if !scene.exists() {
        if let Some(parent) = scene.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&scene, "scene.version=1\nscene.count=0\nscene.nextId=1\n")
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(crate) fn scene_path(root: &Path) -> Result<PathBuf, String> {
    let config = fs::read_to_string(root.join("j3.editor.toml"))
        .map_err(|e| format!("read j3.editor.toml: {e}"))?;
    let value = config
        .lines()
        .filter_map(|line| line.split_once('='))
        .find(|(key, _)| key.trim() == "scene")
        .map(|(_, value)| value.trim())
        .ok_or("j3.editor.toml: missing scene path")?;
    let value = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or("j3.editor.toml: scene path must be quoted")?;
    let rel = Path::new(value);
    if rel.is_absolute()
        || rel
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("scene must be a project-relative path".into());
    }
    Ok(root.join(rel))
}

fn scene_int(values: &HashMap<&str, &str>, key: &str) -> Result<i64, String> {
    values
        .get(key)
        .copied()
        .ok_or_else(|| format!("scene: missing `{key}`"))?
        .parse::<i64>()
        .map_err(|_| format!("scene: invalid `{key}`"))
}

fn validate_scene(path: &Path) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut values = HashMap::new();
    for line in text.lines() {
        if let Some((key, value)) = line.split_once('=') {
            values.entry(key).or_insert(value);
        }
    }
    if scene_int(&values, "scene.version")? != 1 {
        return Err("scene: unsupported version (expected 1)".into());
    }
    let count = scene_int(&values, "scene.count")?;
    if count < 0 {
        return Err("scene: invalid object count".into());
    }
    let mut parents = HashMap::new();
    for i in 0..count {
        let id = scene_int(&values, &format!("node.{i}.id"))?;
        let parent = scene_int(&values, &format!("node.{i}.parent"))?;
        let kind = scene_int(&values, &format!("node.{i}.kind"))?;
        if id <= 0 || parents.insert(id, parent).is_some() {
            return Err(format!("scene: duplicate or invalid object ID {id}"));
        }
        if !(0..=4).contains(&kind) {
            return Err(format!("scene: object {id} has invalid kind {kind}"));
        }
    }
    for (&id, &parent) in &parents {
        if parent != 0 && !parents.contains_key(&parent) {
            return Err(format!("scene: object {id} has missing parent {parent}"));
        }
    }
    let mut safe = HashSet::new();
    for &id in parents.keys() {
        let mut chain = HashSet::new();
        let mut cursor = id;
        while cursor != 0 && !safe.contains(&cursor) {
            if !chain.insert(cursor) {
                return Err(format!("scene: parenting cycle involving object {id}"));
            }
            cursor = parents[&cursor];
        }
        safe.extend(chain);
    }
    Ok(())
}

fn module_name(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    if out.is_empty() || out.as_bytes()[0].is_ascii_digit() {
        out.insert(0, 'g');
    }
    let out = out.trim_end_matches('_');
    if out.is_empty() {
        return "game".into();
    }
    if crate::parse::keyword(out).is_some() {
        format!("{out}_")
    } else {
        out.to_string()
    }
}

fn fps_template(name: &str) -> String {
    format!(
        r#"module {name};

use j3.scene3d.scripts;
use j3.app.App;
use j3.scene3d.fpwalk;
use j3.scene3d.fpwalk.Walker;
use j3.game;
use j3.game.Key;
use j3.gfx;
use j3.memory.Arena;
use j3.scene3d;
use j3.scene3d.Scene;

struct World {{
    scene: Scene;
    walker: Walker;
    mem: &Arena;
}}

fn boot(mem: &Arena): World {{
    let scene = scene3d.load(scripts.scene(), mem);
    return World {{ scene: scene, walker: fpwalk.spawn(&scene, mem), mem: mem }};
}}

fn startup(w: &World) {{
    gfx.init();
    scene3d.upload(&w.scene);
    // Colliders need mesh bounds, which upload fills in.
    w.walker = fpwalk.spawn(&w.scene, w.mem);
    game.captureMouse(true);
    scripts.start(&w.scene);
}}

fn update(w: &World, dt: f32) {{
    if game.keyPressed(Key.Escape) {{ game.requestQuit(); }}
    fpwalk.update(&w.walker, dt, true, 1, 4.5);
    scripts.update(&w.scene, dt);
}}

fn draw(w: &World) {{
    scene3d.drawFrame(&w.scene, fpwalk.look(&w.walker), scene3d.environment(&w.scene), 70);
}}

fn main() {{
    let app = App<World>.create("{name}", 960, 540, boot);
    app.useGl();
    app.startup(startup);
    app.always(update);
    app.draw(draw);
    app.run();
}}
"#
    )
}

fn game_template(name: &str) -> String {
    format!(
        r#"module {name};

use j3.scene3d.scripts;
use j3.app.App;
use j3.game;
use j3.gfx;
use j3.gfx.Look;
use j3.math.{{ Vec3, asin, atan2 }};
use j3.memory.Arena;
use j3.scene3d;
use j3.scene3d.{{ Scene, CAMERA }};

struct World {{
    scene: Scene;
}}

fn boot(mem: &Arena): World {{
    return World {{ scene: scene3d.load(scripts.scene(), mem) }};
}}

fn startup(w: &World) {{
    gfx.init();
    scene3d.upload(&w.scene);
    scripts.start(&w.scene);
}}

fn update(w: &World, dt: f32) {{
    if game.keyPressed(game.Key.Escape) {{ game.requestQuit(); }}
    scripts.update(&w.scene, dt);
}}

fn camera(w: &World): Look {{
    let i = 0;
    while i < w.scene.len {{
        let n = w.scene.nodes[i];
        if n.kind == CAMERA {{
            let model = scene3d.world(&w.scene, n.id);
            let eye = model.transformPoint(Vec3.xyz(0, 0, 0));
            let forward = (model.transformPoint(Vec3.xyz(0, 0, -1)) - eye).normalize();
            gfx.setClip(n.near, n.far);
            return Look.create(eye, atan2(forward.x, -forward.z), asin(forward.y));
        }}
        i += 1;
    }}
    return Look.create(Vec3.xyz(0, 3, 8), 0, -0.2);
}}

fn draw(w: &World) {{
    let env = scene3d.environment(&w.scene);
    scene3d.shadows(&w.scene, env);
    scene3d.lights(&w.scene);
    let fov: f32 = 60;
    let cam = 0;
    let i = 0;
    while i < w.scene.len {{
        if w.scene.nodes[i].kind == CAMERA {{ fov = w.scene.nodes[i].fov; cam = w.scene.nodes[i].id; break; }}
        i += 1;
    }}
    // The camera's `.j3fx` effects, as the editor shows them.
    let fx = scene3d.beginEffects(&w.scene, cam);
    gfx.cameraPassLookFov(camera(w), env.sunDir, env.sky, env.sunColor, fov);
    scene3d.draw(&w.scene);
    if fx {{ scene3d.endEffects(&w.scene, cam); }}
}}

fn main() {{
    let app = App<World>.create("{name}", 960, 540, boot);
    app.useGl();
    app.startup(startup);
    app.always(update);
    app.draw(draw);
    app.run();
}}
"#
    )
}

/// One script prop, as the inspector shows it.
pub(crate) struct ScriptProp {
    pub name: String,
    /// Inspector type: `f32`, `int`, `bool`, `string`, `vec3`, or `object`.
    pub kind: String,
    /// The default as written in the script ("" when the prop is required).
    pub default: String,
    pub required: bool,
}

/// The props of the script at `rel` (e.g. `scripts/door.j3s`) the editor can set. Props of
/// other types stay at their defaults.
pub(crate) fn script_props(root: &Path, rel: &str) -> Result<Vec<ScriptProp>, String> {
    let text = fs::read_to_string(root.join(rel)).map_err(|e| format!("{rel}: {e}"))?;
    let parts: Vec<String> = std::iter::once("script".to_string())
        .chain(rel.trim_end_matches(".j3s").split('/').map(str::to_string))
        .collect();
    let file = crate::parse::parse_script(&text, crate::span::FileId(0), &parts)
        .map_err(|e| crate::parse::format_error(rel, &text, &e))?;
    let mut out = Vec::new();
    for item in &file.items {
        let crate::parse::Item::Props(block) = item else { continue };
        for field in &block.fields {
            let crate::parse::TypeKind::Named(ty) = &field.ty.kind else { continue };
            let kind = match ty.rsplit('.').next().unwrap_or(ty) {
                "f32" | "f64" | "float" => "f32",
                "int" | "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" => "int",
                "bool" => "bool",
                "string" => "string",
                "Vec3" => "vec3",
                "NodeId" => "object",
                _ => continue,
            };
            let folded = field.default.as_ref().map(|e| e.folded_literal());
            let default = match folded.as_ref().map(|e| &e.kind) {
                Some(crate::parse::ExprKind::Int(v)) => v.to_string(),
                Some(crate::parse::ExprKind::Float(v)) => v.to_string(),
                Some(crate::parse::ExprKind::Bool(v)) => v.to_string(),
                Some(crate::parse::ExprKind::String(b)) => String::from_utf8_lossy(b).into_owned(),
                _ => String::new(),
            };
            out.push(ScriptProp {
                name: field.name.name.clone(),
                kind: kind.to_string(),
                default,
                // `node: NodeId` falls back to the object the script is attached to.
                required: field.default.is_none() && !(kind == "object" && field.name.name == "node"),
            });
        }
    }
    Ok(out)
}

/// `target/j3-editor/catalog`: every `.j3s` in the project and the props the inspector edits.
/// A script that doesn't parse is listed with no props (`j3c check` reports it).
pub(crate) fn generate_catalog(root: &Path) -> Result<(), String> {
    let scripts = crate::compile::script_paths(root);
    let mut out = format!("catalog.count={}\n", scripts.len());
    for (i, rel) in scripts.iter().enumerate() {
        let props = script_props(root, rel).unwrap_or_default();
        out.push_str(&format!("catalog.{i}.name s={rel}\ncatalog.{i}.count={}\n", props.len()));
        for (j, prop) in props.iter().enumerate() {
            out.push_str(&format!(
                "catalog.{i}.{j}.name s={}\ncatalog.{i}.{j}.type s={}\ncatalog.{i}.{j}.default s={}\ncatalog.{i}.{j}.required={}\n",
                prop.name,
                prop.kind,
                prop.default,
                u8::from(prop.required)
            ));
            let value = match prop.kind.as_str() {
                "int" => format!("catalog.{i}.{j}.value={}\n", prop.default.parse::<i64>().unwrap_or(0)),
                "f32" => format!("catalog.{i}.{j}.value f={}\n", prop.default.parse::<f32>().unwrap_or(0.0)),
                "bool" => format!("catalog.{i}.{j}.value={}\n", u8::from(prop.default == "true")),
                "vec3" => format!("catalog.{i}.{j}.x f=0\ncatalog.{i}.{j}.y f=0\ncatalog.{i}.{j}.z f=0\n"),
                _ => String::new(),
            };
            out.push_str(&value);
        }
    }
    let dir = root.join("target/j3-editor");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(dir.join("catalog"), out).map_err(|e| e.to_string())
}

/// `j3c -e script-props <path>`: a script's editable props as JSON.
pub(crate) fn script_props_json(root: &Path, rel: &str) -> Result<String, String> {
    let props = script_props(root, rel)?;
    let rows: Vec<String> = props
        .iter()
        .map(|p| {
            format!(
                "{{\"name\":{:?},\"type\":{:?},\"default\":{:?},\"required\":{}}}",
                p.name, p.kind, p.default, p.required
            )
        })
        .collect();
    Ok(format!("[{}]", rows.join(",")))
}

fn collect_assets(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            collect_assets(root, &path, out)?;
        } else if ["j3mesh", "png", "jpg", "jpeg", "j3scene", "j3mat", "j3fx"].contains(
            &path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str(),
        ) {
            let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
            out.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

/// `rel` as a file under the project's `assets/` or `scenes/`, refusing anything that could
/// point outside them.
fn asset_file(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let ok = (rel.starts_with("assets/") || rel.starts_with("scenes/"))
        && !rel.contains('\\')
        && rel.split('/').all(|part| !part.is_empty() && part != "." && part != "..");
    if !ok {
        return Err(format!("`{rel}` isn't a path under assets/ or scenes/"));
    }
    Ok(root.join(rel))
}

pub(crate) fn is_main_scene(root: &Path, rel: &str) -> bool {
    scene_path(root).map(|main| main == root.join(rel)).unwrap_or(false)
}

/// Rename or move asset `old` to `new`, and point every scene file's references at `new`.
/// Returns how many scene files changed.
fn rename_asset(root: &Path, old: &str, new: &str) -> Result<usize, String> {
    let from = asset_file(root, old)?;
    let to = asset_file(root, new)?;
    if !from.is_file() {
        return Err(format!("{old} doesn't exist"));
    }
    if to.exists() {
        return Err(format!("{new} already exists"));
    }
    if from.extension() != to.extension() {
        return Err("Keep the file extension when renaming".into());
    }
    if is_main_scene(root, old) {
        return Err("That's the scene in j3.editor.toml; change it there first".into());
    }
    if let Some(dir) = to.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::rename(&from, &to).map_err(|e| e.to_string())?;
    let mut scenes = Vec::new();
    collect_assets(root, &root.join("assets"), &mut scenes)?;
    collect_assets(root, &root.join("scenes"), &mut scenes)?;
    let old_value = format!(" s={old}");
    let mut changed = 0;
    for scene in scenes.iter().filter(|p| p.ends_with(".j3scene")) {
        let path = root.join(scene);
        let Ok(text) = fs::read_to_string(&path) else { continue };
        let mut hit = false;
        // Node paths and asset fields are whole values, so only exact matches change.
        let lines: Vec<String> = text
            .lines()
            .map(|line| match line.strip_suffix(&old_value) {
                Some(head) if line.starts_with("node.") || line.starts_with("param.") => {
                    hit = true;
                    format!("{head} s={new}")
                }
                _ => line.to_string(),
            })
            .collect();
        if hit {
            let temp = path.with_extension("j3tmp");
            fs::write(&temp, lines.join("\n") + "\n").map_err(|e| e.to_string())?;
            fs::rename(&temp, &path).map_err(|e| e.to_string())?;
            changed += 1;
        }
    }
    generate_assets_catalog(root)?;
    Ok(changed)
}

/// Move asset `rel` into `target/j3-editor/trash/`, where it can be recovered by hand.
/// References to it stay, and show as missing.
fn delete_asset(root: &Path, rel: &str) -> Result<String, String> {
    let from = asset_file(root, rel)?;
    if !from.is_file() {
        return Err(format!("{rel} doesn't exist"));
    }
    if is_main_scene(root, rel) {
        return Err("That's the scene in j3.editor.toml; change it there first".into());
    }
    let trash = root.join("target/j3-editor/trash");
    fs::create_dir_all(&trash).map_err(|e| e.to_string())?;
    let name = from.file_name().and_then(|n| n.to_str()).unwrap_or("asset").to_string();
    let mut to = trash.join(&name);
    let mut n = 2;
    while to.exists() {
        to = trash.join(format!("{n}-{name}"));
        n += 1;
    }
    fs::rename(&from, &to).map_err(|e| e.to_string())?;
    generate_assets_catalog(root)?;
    Ok(to.strip_prefix(root).unwrap_or(&to).to_string_lossy().into_owned())
}

pub(crate) fn generate_assets_catalog(root: &Path) -> Result<(), String> {
    let mut assets = Vec::new();
    collect_assets(root, &root.join("assets"), &mut assets)?;
    collect_assets(root, &root.join("scenes"), &mut assets)?;
    assets.sort();
    let mut out = format!("assets.count={}\n", assets.len());
    for (i, path) in assets.iter().enumerate() {
        out.push_str(&format!("assets.{i}.path s={path}\n"));
    }
    let dir = root.join("target/j3-editor");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(dir.join("assets-catalog"), out).map_err(|e| e.to_string())
}

/// Write `scripts/<name>.j3s` with a `props` block and the two hooks the scene calls.
fn create_script(root: &Path, name: &str) -> Result<String, String> {
    if name.is_empty()
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        || name.as_bytes()[0].is_ascii_digit()
        || crate::parse::keyword(name).is_some()
    {
        return Err("script name must be a J3 identifier".into());
    }
    let rel = format!("scripts/{name}.j3s");
    let path = root.join(&rel);
    if path.exists() {
        return Err(format!("{rel} already exists"));
    }
    fs::create_dir_all(root.join("scripts")).map_err(|e| e.to_string())?;
    let stub = "use j3.scene3d.{ Scene, NodeId };\n\nprops {\n    node: NodeId;\n}\n\n\
pub fn start(scene: &Scene) {\n}\n\npub fn update(scene: &Scene, dt: f32) {\n}\n";
    fs::write(&path, stub).map_err(|e| e.to_string())?;
    generate_catalog(root)?;
    Ok(rel)
}

/// Append `kind<TAB>scene<TAB>message` to the log the editor polls.
pub(crate) fn event(root: &Path, kind: &str, scene: &str, message: &str) {
    let clean = |s: &str| s.replace(['\n', '\r', '\t'], " ");
    let line = format!("{kind}\t{}\t{}\n", clean(scene), clean(message));
    append_event(root, &line);
}

/// The import event carries its own fragment name, so queued imports never share a file.
fn import_event(root: &Path, scene: &str, file: &str, message: &str) {
    let clean = |s: &str| s.replace(['\n', '\r', '\t'], " ");
    let line = format!("import\t{}\t{file}\t{}\n", clean(scene), clean(message));
    append_event(root, &line);
}

pub(crate) fn append_event(root: &Path, line: &str) {
    // One small O_APPEND write, so lines from the Play thread and the main loop don't interleave.
    if let Ok(mut f) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("target/j3-editor/events"))
    {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Move an import's scene out of `target/` into `assets/imported/<name>.j3scene`, named after
/// the source file, and return that project-relative path.
pub(crate) fn keep_import(root: &Path, fragment: &Path, source: &str) -> Result<String, String> {
    let clean = import_name(source);
    let dir = root.join("assets/imported");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut name = format!("{clean}.j3scene");
    let mut n = 2;
    while dir.join(&name).exists() {
        name = format!("{clean}_{n}.j3scene");
        n += 1;
    }
    fs::rename(fragment, dir.join(&name)).map_err(|e| e.to_string())?;
    Ok(format!("assets/imported/{name}"))
}

/// The base name `keep_import` gives `source`: `assets/imported/<this>.j3scene` when free.
pub(crate) fn import_name(source: &str) -> String {
    let stem = Path::new(source).file_stem().and_then(|s| s.to_str()).unwrap_or("model");
    stem.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect()
}

/// `bake`: see `gltf::import`; `.j3mesh` ignores it.
pub(crate) fn import_asset(root: &Path, scene: &Path, raw: &str, bake: bool) -> Result<Vec<String>, String> {
    let source = Path::new(raw.trim());
    if raw.trim().is_empty() {
        return Err("enter an asset path first".into());
    }
    let source = if source.is_absolute() {
        source.to_path_buf()
    } else {
        root.join(source)
    };
    match source.extension().and_then(|s| s.to_str()).unwrap_or("") {
        "gltf" | "glb" => crate::gltf::import(root, scene, &source, bake),
        "j3mesh" => crate::gltf::import_mesh(root, scene, &source),
        _ => Err("expected .gltf, .glb, or .j3mesh".into()),
    }
}

/// This j3c binary. After a rebuild replaces it on disk, Linux reports the old path with
/// " (deleted)"; `/proc/self/exe` still opens the running (old) binary, so a long-lived
/// `j3c -e mcp` keeps working with the toolchain it started with.
// ponytail: the server keeps using the old j3c until restarted; mcp.rs says so.
pub(crate) fn host_exe() -> Result<PathBuf, String> {
    Ok(host_or_self(std::env::current_exe().map_err(|e| e.to_string())?))
}

fn host_or_self(path: PathBuf) -> PathBuf {
    if path.exists() || !cfg!(target_os = "linux") { return path; }
    PathBuf::from("/proc/self/exe")
}

/// Build the editor frontend once per j3c binary, stdlib, and frontend source, shared by
/// every project. `J3_EDITOR_SRC` reads the frontend from disk instead of the embedded copy.
pub(crate) fn frontend(host: &Path) -> Result<(PathBuf, fs::File), String> {
    let sources: Vec<(String, String)> = match std::env::var("J3_EDITOR_SRC") {
        Ok(dir) if !dir.is_empty() => {
            let mut out = Vec::new();
            for entry in fs::read_dir(&dir).map_err(|e| format!("J3_EDITOR_SRC {dir}: {e}"))? {
                let path = entry.map_err(|e| e.to_string())?.path();
                if path.extension().and_then(|s| s.to_str()) != Some("j3") { continue; }
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                let text = fs::read_to_string(&path).map_err(|e| format!("J3_EDITOR_SRC {}: {e}", path.display()))?;
                out.push((name, text));
            }
            out.sort();
            out
        }
        _ => FRONTEND.iter().map(|(n, s)| (n.to_string(), s.to_string())).collect(),
    };
    let mut hasher = DefaultHasher::new();
    sources.hash(&mut hasher);
    let meta = fs::metadata(host).map_err(|e| e.to_string())?;
    (meta.len(), meta.modified().ok()).hash(&mut hasher);
    if crate::stdlib::using_override() {
        hash_dir(&crate::stdlib::stdlib_dir(), &mut hasher);
    }
    let cache = cache_dir().ok_or("no XDG_CACHE_HOME or HOME for the editor build")?;
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let key = format!("{:016x}", hasher.finish());
    let dir = cache.join(&key);
    let exe = dir.join("target/j3_editor");
    if !exe.exists() {
        let temp = (0..100)
            .find_map(|attempt| {
                let path = cache.join(format!("{key}.tmp-{}-{attempt}", std::process::id()));
                match fs::create_dir(&path) {
                    Ok(()) => Some(Ok(path)),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => None,
                    Err(e) => Some(Err(format!("editor build directory: {e}"))),
                }
            })
            .ok_or("could not reserve an editor build directory")??;
        let built = (|| -> Result<(), String> {
            fs::create_dir_all(temp.join("src")).map_err(|e| e.to_string())?;
            for (name, text) in &sources {
                fs::write(temp.join("src").join(name), text).map_err(|e| e.to_string())?;
            }
            fs::write(temp.join("font.ttf"), FONT).map_err(|e| e.to_string())?;
            fs::write(temp.join("j3.toml"), "[package]\nname = \"j3_editor\"\nentry = \"src/main.j3\"\n\n[link]\nlibs = [\"SDL2\", \"GL\"]\n")
                .map_err(|e| e.to_string())?;
            // Output goes to stderr: `j3c -e mcp` keeps stdout for the protocol.
            let status = Command::new(host)
                .arg("build")
                .arg(&temp)
                .stdout(Stdio::from(std::io::stderr().as_fd().try_clone_to_owned().map_err(|e| e.to_string())?))
                .status()
                .map_err(|e| format!("build editor: {e}"))?;
            if !status.success() { return Err("editor build failed".into()); }
            if !temp.join("target/j3_editor").is_file() {
                return Err("editor build did not produce target/j3_editor".into());
            }
            Ok(())
        })();
        if let Err(error) = built {
            let _ = fs::remove_dir_all(&temp);
            return Err(error);
        }
        if let Err(error) = fs::rename(&temp, &dir) {
            if !exe.is_file() {
                let _ = fs::remove_dir_all(&temp);
                return Err(format!("publish editor build: {error}"));
            }
            // Another launch published the same build first.
            let _ = fs::remove_dir_all(&temp);
        }
    }
    let lock = match cache_lock(&dir) {
        Ok(lock) => lock,
        Err(_) if !exe.exists() => return frontend(host),
        Err(error) => return Err(error),
    };
    lock.lock_shared().map_err(|e| format!("editor cache lock: {e}"))?;
    if !exe.is_file() {
        drop(lock);
        let _ = fs::remove_dir_all(&dir);
        return frontend(host);
    }
    cleanup_frontends(&cache, &dir);
    Ok((dir, lock))
}

fn cache_lock(dir: &Path) -> Result<fs::File, String> {
    fs::OpenOptions::new().create(true).read(true).write(true)
        .open(dir.join("in-use.lock"))
        .map_err(|e| format!("editor cache lock: {e}"))
}

/// `~/.cache/j3` (or `$XDG_CACHE_HOME/j3`).
pub(crate) fn cache_root() -> Option<PathBuf> {
    std::env::var_os("XDG_CACHE_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .map(|c| c.join("j3"))
}

/// `~/.cache/j3/editor`: editor builds, and which project the editor last opened.
pub(crate) fn cache_dir() -> Option<PathBuf> {
    cache_root().map(|r| r.join("editor"))
}

/// Remember `root` as the open project, for `j3c -e mcp` without `--project`.
fn note_open(root: &Path) {
    let Some(cache) = cache_dir() else { return };
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let _ = fs::create_dir_all(&cache);
    let _ = fs::write(cache.join("open-project"), root.to_string_lossy().as_bytes());
}

fn project_lock(root: &Path) -> Result<fs::File, String> {
    let dir = root.join("target/j3-editor");
    fs::create_dir_all(&dir).map_err(|e| format!("editor lock: {e}"))?;
    let lock = fs::OpenOptions::new().create(true).write(true)
        .open(dir.join("lock"))
        .map_err(|e| format!("editor lock: {e}"))?;
    lock.try_lock().map_err(|e| match e {
        fs::TryLockError::WouldBlock => "editor already open for this project".to_string(),
        fs::TryLockError::Error(e) => format!("editor lock: {e}"),
    })?;
    Ok(lock)
}

/// Keep the 3 newest builds, the current one, and any another process has locked. Every
/// rebuild of j3c makes a new key, so an age gate would keep dozens.
fn cleanup_frontends(cache: &Path, current: &Path) {
    let Ok(entries) = fs::read_dir(cache) else { return };
    let now = std::time::SystemTime::now();
    let mut builds: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let path = entry.path();
        if name.contains(".tmp-") {
            // A build killed before its own cleanup ran; live builds finish in minutes.
            let stale = fs::metadata(&path).and_then(|m| m.modified()).ok()
                .and_then(|time| now.duration_since(time).ok())
                .is_some_and(|age| age > std::time::Duration::from_secs(60 * 60));
            if stale { let _ = fs::remove_dir_all(&path); }
            continue;
        }
        if name.len() != 16 || !name.bytes().all(|c| c.is_ascii_hexdigit()) { continue; }
        if let Some(time) = fs::metadata(path.join("target/j3_editor")).ok().and_then(|m| m.modified().ok()) {
            builds.push((time, path));
        }
    }
    builds.sort_by(|a, b| b.0.cmp(&a.0));
    for (position, (_, path)) in builds.into_iter().enumerate() {
        if position < 3 || path == current { continue; }
        let Ok(lock) = cache_lock(&path) else { continue };
        if lock.try_lock().is_err() { continue; }
        // Recheck after locking: a launch that found the exe missing rebuilds it itself.
        if path.join("target/j3_editor").is_file() {
            let _ = fs::remove_dir_all(path);
        }
    }
}

/// Every file's path and bytes under `dir`, in a stable order.
fn hash_dir(dir: &Path, hasher: &mut DefaultHasher) {
    let Ok(read) = fs::read_dir(dir) else { return };
    let mut entries: Vec<PathBuf> = read.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        path.hash(hasher);
        if path.is_dir() {
            hash_dir(&path, hasher);
        } else if let Ok(bytes) = fs::read(&path) {
            bytes.hash(hasher);
        }
    }
}

/// Stop the running game, if any. Returns true when one was running.
fn kill_game(running: &mut Option<Child>) -> bool {
    let Some(mut child) = running.take() else { return false };
    let alive = matches!(child.try_wait(), Ok(None));
    let _ = child.kill();
    let _ = child.wait();
    alive
}

/// Send a failed build's error lines to the console (the full log stays in build.log).
fn report_build_errors(root: &Path, log: &str) {
    let _ = fs::write(root.join("target/j3-editor/build.log"), log);
    let project = format!("{}/", root.display());
    let errors = log.lines().filter(|s| s.contains("error") || s.trim_start().starts_with("-->"));
    let mut sent = 0;
    for line in errors.take(30) {
        event(root, "build", "", &line.trim().replace(&project, ""));
        sent += 1;
    }
    if sent == 0 {
        event(root, "build", "", "Build failed; see target/j3-editor/build.log");
    }
}

/// A game with scripts builds in dev mode: script edits never rebuild it. Anything else
/// builds with `j3c build`. Returns the program to launch and, in dev mode, its scripts.
fn build_for_play(root: &Path, host: &Path) -> Result<(PathBuf, Option<crate::devrun::Built>), ()> {
    let manifest = match Manifest::load(&root.join("j3.toml")) {
        Ok(m) => m,
        Err(e) => {
            event(root, "msg", "", &e);
            return Err(());
        }
    };
    let opts = crate::emit::EmitOpts::from_manifest(&manifest).and_then(|mut o| {
        o.backend = crate::emit::resolve_backend(o.backend, false, &o.clang)?;
        Ok(o)
    });
    let prepared = opts.and_then(|opts| crate::devrun::prepare(&manifest, &opts, false));
    match prepared {
        Ok(crate::devrun::Prepared::Dev(built)) => return Ok((built.exe.clone(), Some(built))),
        Ok(crate::devrun::Prepared::NoScripts(_)) => {}
        Err(e) => {
            report_build_errors(root, &e);
            event(root, "msg", "", "Build failed");
            return Err(());
        }
    }
    let build = match Command::new(host).arg("build").arg(root).output() {
        Ok(out) => out,
        Err(e) => {
            event(root, "msg", "", &format!("build game: {e}"));
            return Err(());
        }
    };
    let mut log = String::from_utf8_lossy(&build.stderr).to_string();
    log.push_str(&String::from_utf8_lossy(&build.stdout));
    if !build.status.success() {
        report_build_errors(root, &log);
        event(root, "msg", "", "Build failed");
        return Err(());
    }
    let _ = fs::write(root.join("target/j3-editor/build.log"), &log);
    let project = format!("{}/", root.display());
    for line in log.lines().filter(|s| s.contains("warning")).take(30) {
        event(root, "out", "", &line.trim().replace(&project, ""));
    }
    Ok((manifest.out_path(), None))
}

fn spawn_game(root: &Path, exe: &Path, scene: &str) -> Result<Child, String> {
    let mut process = Command::new(exe)
        .current_dir(root)
        .env("J3_ROOT", root)
        .env("J3_SCENE", scene)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(out) = process.stdout.take() { forward(root, out, "out"); }
    if let Some(err) = process.stderr.take() { forward(root, err, "err"); }
    Ok(process)
}

fn script_times(built: &Option<crate::devrun::Built>) -> Vec<Option<std::time::SystemTime>> {
    built
        .as_ref()
        .map(|b| {
            b.scripts
                .iter()
                .map(|p| fs::metadata(p).and_then(|m| m.modified()).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Build the game and (re)launch it on `scene`. The lock is the "Play is busy" flag and holds
/// the game; after launch it is released between checks so Stop and Play can reach the game.
/// In dev mode a script edit swaps into the running game, or relaunches it when its layout
/// changed, without compiling the game again.
fn play(root: &Path, host: &Path, game: &Mutex<Option<Child>>, scene: &str) {
    let Ok(mut running) = game.try_lock() else {
        event(root, "msg", "", "Already building");
        return;
    };
    if kill_game(&mut running) {
        event(root, "stopped", "", "Game stopped");
    }
    event(root, "msg", "", "Building...");
    event(root, "out", "", &format!("--- Play {scene} ---"));
    let started = std::time::Instant::now();
    let Ok((exe, mut built)) = build_for_play(root, host) else { return };
    event(root, "out", "", &format!("Built in {:.1}s", started.elapsed().as_secs_f32()));
    let mut pid = match spawn_game(root, &exe, scene) {
        Ok(process) => {
            let pid = process.id();
            *running = Some(process);
            event(root, "run", "", "Game launched");
            pid
        }
        Err(e) => return event(root, "msg", "", &format!("Could not launch game: {e}")),
    };
    drop(running);
    let mut seen = script_times(&built);
    let manifest = Manifest::load(&root.join("j3.toml")).ok();
    // Report the game closing by itself. Stop and restart clear or replace the child, which ends this.
    loop {
        std::thread::sleep(std::time::Duration::from_millis(250));
        let Ok(mut running) = game.lock() else { return };
        let Some(child) = running.as_mut().filter(|c| c.id() == pid) else { return };
        match child.try_wait() {
            Ok(None) => {}
            Ok(Some(status)) => {
                *running = None;
                if status.success() { return event(root, "stopped", "", "Game closed"); }
                let how = match status.code() {
                    Some(code) => format!("exit {code}"),
                    None => "killed by a signal".to_string(),
                };
                return event(root, "stopped", "crash", &format!("Game crashed ({how})"));
            }
            Err(_) => {
                *running = None;
                return event(root, "stopped", "", "Game closed");
            }
        }
        let now = script_times(&built);
        if now == seen {
            continue;
        }
        seen = now;
        let (Some(m), Some(old)) = (manifest.as_ref(), built.as_ref()) else { continue };
        let opts = crate::emit::EmitOpts::from_manifest(m).and_then(|mut o| {
            o.backend = crate::emit::resolve_backend(o.backend, false, &o.clang)?;
            Ok(o)
        });
        match opts.and_then(|opts| crate::devrun::prepare(m, &opts, false)) {
            Ok(crate::devrun::Prepared::Dev(next)) => {
                let same_layout = next.layouts.iter().all(|(path, (layout, _))| {
                    old.layouts.get(path).map(|(was, _)| was == layout).unwrap_or(true)
                });
                if next.rebuilt || !same_layout {
                    kill_game(&mut running);
                    match spawn_game(root, &next.exe, scene) {
                        Ok(process) => {
                            pid = process.id();
                            *running = Some(process);
                            event(root, "run", "", "Script layout changed; relaunched the game");
                        }
                        Err(e) => return event(root, "msg", "", &format!("Could not relaunch game: {e}")),
                    }
                } else {
                    crate::devrun::bump_reload(root);
                    event(root, "out", "", "Swapped script code");
                }
                seen = next.scripts.iter().map(|p| fs::metadata(p).and_then(|m| m.modified()).ok()).collect();
                built = Some(next);
            }
            Ok(crate::devrun::Prepared::NoScripts(_)) => {}
            Err(e) => {
                report_build_errors(root, &e);
                event(root, "msg", "", "Script error; the game keeps its old code");
            }
        }
    }
}

/// Send each line the game writes to `pipe` to the editor's console as a `kind` event.
// ponytail: 500 lines a second per stream, the rest counted and skipped, so a game
// printing every frame can't flood the events file.
fn forward(root: &Path, pipe: impl std::io::Read + Send + 'static, kind: &'static str) {
    let root = root.to_path_buf();
    std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(pipe);
        let mut line = Vec::new();
        let mut window = std::time::Instant::now();
        let (mut sent, mut skipped) = (0u32, 0u32);
        loop {
            line.clear();
            match std::io::BufRead::read_until(&mut reader, b'\n', &mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            if window.elapsed().as_secs() >= 1 {
                if skipped > 0 { event(&root, kind, "", &format!("... {skipped} lines skipped")); }
                window = std::time::Instant::now();
                sent = 0;
                skipped = 0;
            }
            if sent >= 500 {
                if skipped == 0 { event(&root, kind, "", "... too much output; skipping the rest of this second"); }
                skipped += 1;
                continue;
            }
            sent += 1;
            let text = String::from_utf8_lossy(&line);
            event(&root, kind, "", text.trim_end());
        }
        if skipped > 0 { event(&root, kind, "", &format!("... {skipped} lines skipped")); }
    });
}

/// Import `path` into the editor's tab `tab`: as objects pasted in (an `import` event), or with
/// `as_scene` as its own scene file placed once (`placed`).
fn import_into(root: &Path, path: &str, tab: &str, as_scene: bool) {
    static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let file = format!("import-{}.j3scene", COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1);
    let fragment = root.join("target/j3-editor").join(&file);
    let result = import_asset(root, &fragment, path, false).and_then(|w| {
        let placed = if as_scene { Some(keep_import(root, &fragment, path)?) } else { None };
        generate_assets_catalog(root).map(|_| (w, placed))
    });
    match result {
        Ok((w, Some(scene))) if tab.is_empty() => {
            let more = if w.is_empty() { String::new() } else { format!(" ({} import warning{})", w.len(), if w.len() == 1 { "" } else { "s" }) };
            event(root, "refresh", "", &format!("Added {scene}{more}; drag it from Assets into a scene"));
        }
        Ok((w, placed)) => {
            let note = if w.is_empty() {
                "Asset imported".to_string()
            } else {
                format!("Imported with {} warning{}: {}", w.len(), if w.len() == 1 { "" } else { "s" }, w[0])
            };
            match placed {
                Some(scene) => append_event(root, &format!("placed\t{tab}\t{scene}\t{}\n", note.replace(['\n', '\r', '\t'], " "))),
                None => import_event(root, tab, &file, &note),
            }
        }
        Err(e) => {
            eprintln!("j3c editor: {e}");
            event(root, "msg", "", &e);
        }
    }
}

/// Write `target/j3-editor/online` (rows of source, id, kind, name, author, thumbnail path;
/// `pack` names the pack they came from, else empty) in `save` format and tell the editor.
fn online_results(root: &Path, rows: &[[String; 6]], pack: &str, message: &str) {
    let mut out = format!("online.count={}\nonline.pack s={pack}\n", rows.len());
    for (i, row) in rows.iter().enumerate() {
        for (key, v) in ["source", "id", "kind", "name", "author", "thumb"].iter().zip(row) {
            out += &format!("online.{i}.{key} s={}\n", v.replace(['\n', '\r', '\t'], " "));
        }
    }
    match fs::write(root.join("target/j3-editor/online"), out) {
        Ok(()) => event(root, "online", "", message),
        Err(e) => event(root, "msg", "", &format!("online results: {e}")),
    }
}

/// Ask the desktop for a model file and hand the path back to the editor to import.
fn pick(root: &Path, tab: &str) {
    // ponytail: zenity, so Linux desktops only; drag-and-drop works everywhere.
    let out = Command::new("zenity")
        .args(["--file-selection", "--title=Import model", "--file-filter=Models | *.gltf *.glb *.j3mesh"])
        .output();
    match out {
        Ok(out) if out.status.success() => {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !path.is_empty() {
                event(root, "picked", tab, &path);
            }
        }
        Ok(_) => {}
        Err(_) => event(root, "msg", "", "No file picker (install zenity), or drop the file on the window"),
    }
}

/// Regenerate both catalogs, as at startup.
fn rescan(root: &Path) -> Result<(), String> {
    for line in crate::gen::run_all(root, false).failed {
        event(root, "build", "", &format!("mesh generator failed: {line}"));
    }
    generate_catalog(root)?;
    generate_assets_catalog(root)
}

/// Width over height of the game window, for the editor's camera preview: the numbers in
/// `.create("Title", W, H, ...)`, else 16:9.
// ponytail: a text scan of src/main.j3; games that size the window another way get 16:9.
fn game_aspect(entry: &str) -> f32 {
    let size = entry.split(".create(\"").skip(1).find_map(|rest| {
        let mut parts = rest.split_once("\",")?.1.split(',');
        let w: f32 = parts.next()?.trim().parse().ok()?;
        let h: f32 = parts.next()?.trim().parse().ok()?;
        (w > 0.0 && h > 0.0).then_some(w / h)
    });
    size.unwrap_or(16.0 / 9.0)
}

fn launch(root: &Path, scene: &Path) -> Result<(), String> {
    let host = host_exe()?;
    let (dir, _cache_lock) = frontend(&host)?;
    fs::create_dir_all(root.join("target/j3-editor")).map_err(|e| e.to_string())?;
    // A fresh log each session, so the editor never replays the last one's messages.
    fs::write(root.join("target/j3-editor/events"), "").map_err(|e| e.to_string())?;
    fs::write(root.join("target/j3-editor/replies"), "").map_err(|e| e.to_string())?;
    let _ = fs::remove_file(root.join("target/j3-editor/online"));
    let rel = scene.strip_prefix(root).map_err(|e| e.to_string())?;
    let mut child = Command::new(dir.join("target/j3_editor"))
        .current_dir(root)
        .env("J3_EDITOR_SCENE", rel)
        .env("J3_EDITOR_FONT", dir.join("font.ttf"))
        .env("J3_EDITOR_ASPECT", game_aspect(&fs::read_to_string(root.join("src/main.j3")).unwrap_or_default()).to_string())
        .env("J3_HOST", &host)
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("launch editor: {e}"))?;
    let stdout = child.stdout.take().ok_or("editor stdout unavailable")?;
    let game: Arc<Mutex<Option<Child>>> = Arc::default();
    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(|e| e.to_string())?;
        if let Some(rest) = line.strip_prefix("J3EDITOR REPLY ") {
            // An answer to a `j3c -e mcp` command; the server polls this file for its id.
            if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(root.join("target/j3-editor/replies")) {
                let _ = f.write_all(format!("{rest}\n").as_bytes());
            }
        } else if let Some(rest) = line.strip_prefix("J3EDITOR PLAY") {
            let (root, host, game) = (root.to_path_buf(), host.clone(), game.clone());
            let scene = rest.trim().to_string();
            std::thread::spawn(move || play(&root, &host, &game, &scene));
        } else if line == "J3EDITOR STOP" {
            match game.try_lock() {
                Ok(mut running) => {
                    if kill_game(&mut running) {
                        event(root, "stopped", "", "Game stopped");
                    }
                }
                Err(_) => event(root, "msg", "", "Still building; Stop once the game is up"),
            }
        } else if let Some(tab) = line.strip_prefix("J3EDITOR PICK") {
            let (root, tab) = (root.to_path_buf(), tab.trim().to_string());
            std::thread::spawn(move || pick(&root, &tab));
        } else if let Some(rest) = line.strip_prefix("J3EDITOR RENAME ") {
            let (old, new) = rest.split_once('\t').unwrap_or((rest, ""));
            match rename_asset(root, old, new) {
                Ok(n) => append_event(root, &format!("renamed\t{old}\t{new}\tRenamed; updated {n} scene file{}\n", if n == 1 { "" } else { "s" })),
                Err(e) => event(root, "msg", "", &e),
            }
        } else if let Some(rel) = line.strip_prefix("J3EDITOR DELETE ") {
            match delete_asset(root, rel) {
                Ok(trash) => append_event(root, &format!("deleted\t{rel}\tMoved to {trash}\n")),
                Err(e) => event(root, "msg", "", &e),
            }
        } else if line == "J3EDITOR RESCAN" {
            match rescan(root) {
                Ok(()) => event(root, "refresh", "", "Assets and scripts reloaded"),
                Err(e) => event(root, "msg", "", &e),
            }
        } else if let Some(name) = line.strip_prefix("J3EDITOR SCRIPT ") {
            match create_script(root, name) {
                Ok(rel) => event(root, "refresh", "", &format!("Made {rel}")),
                Err(e) => {
                    eprintln!("j3c editor: {e}");
                    event(root, "msg", "", &e);
                }
            }
        } else if let Some(rest) = line.strip_prefix("J3EDITOR IMPORT ") {
            // `<asset>\t<scene>[\tscene]`; the scene part is the editor's open tab, and a
            // trailing `scene` keeps the import as its own scene file to place.
            let mut parts = rest.split('\t');
            let path = parts.next().unwrap_or("");
            let tab = parts.next().unwrap_or("");
            import_into(root, path, tab, parts.next() == Some("scene"));
        } else if let Some(rest) = line.strip_prefix("J3EDITOR FETCH ") {
            // `<source>\t<kind>\t<query>`: search online; the results land in target/j3-editor/online.
            let mut parts = rest.splitn(3, '\t').map(str::to_string);
            let (source, kind) = (parts.next().unwrap_or_default(), parts.next().unwrap_or_default());
            let (root, query) = (root.to_path_buf(), parts.next().unwrap_or_default());
            std::thread::spawn(move || match crate::online::search(&source, &query, &kind) {
                Ok(hits) => {
                    // Thumbnails download side by side; a missing one just shows no picture.
                    let thumbs: Vec<Option<PathBuf>> = std::thread::scope(|s| {
                        let jobs: Vec<_> = hits.iter().map(|h| s.spawn(|| crate::online::thumb(h))).collect();
                        jobs.into_iter().map(|j| j.join().ok().flatten()).collect()
                    });
                    let rows: Vec<[String; 6]> = hits.iter().zip(thumbs).map(|(h, t)| {
                        let thumb = t.map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
                        [h.source.to_string(), h.id.clone(), h.kind.to_string(), h.name.clone(), h.author.clone(), thumb]
                    }).collect();
                    let n = rows.len();
                    online_results(&root, &rows, "", &format!("{n} result{}", if n == 1 { "" } else { "s" }));
                }
                Err(e) => event(&root, "msg", "", &e),
            });
        } else if let Some(rest) = line.strip_prefix("J3EDITOR GET ") {
            // `<source>\t<id>\t<tab>[\tscene]`: download an online result and import it like IMPORT;
            // an empty tab with `scene` only adds the reusable scene to the project.
            let mut parts = rest.split('\t').map(str::to_string);
            let (source, id) = (parts.next().unwrap_or_default(), parts.next().unwrap_or_default());
            let tab = parts.next().unwrap_or_default();
            let as_scene = parts.next().as_deref() == Some("scene");
            let root = root.to_path_buf();
            std::thread::spawn(move || {
                use crate::online::Fetched;
                match crate::online::fetch(&root, &source, &id, None) {
                    Ok(Fetched::Model(path)) => import_into(&root, &path.to_string_lossy(), &tab, as_scene),
                    Ok(Fetched::Material(rel) | Fetched::Sky(rel)) => {
                        let _ = generate_assets_catalog(&root);
                        event(&root, "refresh", "", &format!("Made {rel}"));
                    }
                    Ok(Fetched::Pack(files)) => {
                        let thumbs = crate::online::pack_thumbs(&source, &id, &files);
                        let rows: Vec<[String; 6]> = files.iter().zip(thumbs).map(|(f, t)| {
                            let name = f.file_name().unwrap_or_default().to_string_lossy().into_owned();
                            let thumb = t.map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
                            [source.clone(), format!("{id}/{name}"), "model".into(), name, "Kenney".into(), thumb]
                        }).collect();
                        online_results(&root, &rows, &id, &format!("{} models in {id}", rows.len()));
                    }
                    Err(e) => event(&root, "msg", "", &e),
                }
            });
        } else {
            println!("{line}");
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    // The game belongs to this session, so it closes with the editor.
    if let Ok(mut running) = game.lock() {
        kill_game(&mut running);
    }
    if !status.success() {
        return Err(format!("editor exited with {status}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
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
}
