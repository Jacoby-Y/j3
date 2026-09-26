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
    generate_dispatch(&root, &manifest.name)?;
    generate_catalog(&root)?;
    for line in crate::gen::run_all(&root, false).failed {
        eprintln!("warning: mesh generator failed: {line}");
    }
    generate_assets_catalog(&root)?;
    launch(&root, &scene)
}

/// `j3c new NAME --template fps`: the editor project with a walkable scene, plus the
/// behavior dispatch so it compiles before the editor has ever opened it.
pub(crate) fn create_fps_project(root: &Path) -> Result<(), String> {
    create_project(root, true)?;
    let manifest = Manifest::load(&root.join("j3.toml"))?;
    generate_dispatch(root, &manifest.name)
}

/// A new editor project. `fps` picks the first-person main and scene.
// ponytail: a bool for two templates; make it an enum when a third arrives.
fn create_project(root: &Path, fps: bool) -> Result<(), String> {
    for relative in [
        "j3.toml",
        "j3.editor.toml",
        "src/main.j3",
        "src/behaviors.j3",
        "src/behaviors/spin.j3",
        "scenes/main.j3scene",
    ] {
        if root.join(relative).exists() {
            return Err(format!("{} already exists", root.join(relative).display()));
        }
    }
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
    let name = module_name(&raw_name);
    fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
    fs::create_dir_all(root.join("scenes")).map_err(|e| e.to_string())?;
    fs::create_dir_all(root.join("assets")).map_err(|e| e.to_string())?;
    fs::write(
        root.join("j3.toml"),
        format!("[package]\nname = \"{name}\"\nentry = \"src/main.j3\"\n\n[emit]\nbackend = llvm\nopt = 0\ndebug = true\n\n[link]\nlibs = [\"SDL2\", \"GL\"]\n"),
    ).map_err(|e| e.to_string())?;
    let main = if fps { fps_template(&name) } else { game_template(&name) };
    fs::write(root.join("src/main.j3"), main).map_err(|e| e.to_string())?;
    add_editor_files(root)?;
    fs::create_dir_all(root.join("src/behaviors")).map_err(|e| e.to_string())?;
    fs::write(root.join("src/behaviors/spin.j3"), spin_template(&name))
        .map_err(|e| e.to_string())?;
    fs::write(root.join("j3.editor.toml"),
        "[editor]\nscene = \"scenes/main.j3scene\"\n\n[behavior.spin]\nfields = [\"speed:f32=1.0\"]\n")
        .map_err(|e| e.to_string())?;
    fs::write(root.join("scenes/main.j3scene"), if fps { FPS_SCENE } else { START_SCENE }).map_err(|e| e.to_string())?;
    fs::create_dir_all(root.join("assets/gen")).map_err(|e| e.to_string())?;
    fs::write(root.join("assets/gen/tree.j3"), TREE_GEN).map_err(|e| e.to_string())?;
    Ok(())
}

const START_SCENE: &str = "scene.version=1\nscene.count=2\nscene.nextId=3\n\
node.0.id=1\nnode.0.parent=0\nnode.0.kind=2\nnode.0.name s=Camera\nnode.0.asset s=\nnode.0.texture s=\nnode.0.behavior s=\n\
node.0.px f=0\nnode.0.py f=2\nnode.0.pz f=7\nnode.0.rx f=-0.2\nnode.0.ry f=0\nnode.0.rz f=0\n\
node.0.sx f=1\nnode.0.sy f=1\nnode.0.sz f=1\nnode.0.fov f=60\nnode.0.near f=0.15\nnode.0.far f=200\n\
node.1.id=2\nnode.1.parent=0\nnode.1.kind=1\nnode.1.name s=Cube\nnode.1.asset s=builtin:cube\nnode.1.texture s=\nnode.1.behavior s=spin\n\
node.1.px f=0\nnode.1.py f=0\nnode.1.pz f=0\nnode.1.rx f=0\nnode.1.ry f=0\nnode.1.rz f=0\n\
node.1.sx f=1\nnode.1.sy f=1\nnode.1.sz f=1\n";

/// The `--template fps` scene: a camera (where the walker starts), a floor, and things to bump into.
const FPS_SCENE: &str = "scene.version=1\nscene.count=5\nscene.nextId=6\n\
node.0.id=1\nnode.0.parent=0\nnode.0.kind=2\nnode.0.name s=Camera\nnode.0.asset s=\nnode.0.texture s=\nnode.0.behavior s=\n\
node.0.px f=0\nnode.0.py f=0\nnode.0.pz f=8\nnode.0.rx f=0\nnode.0.ry f=0\nnode.0.rz f=0\n\
node.0.sx f=1\nnode.0.sy f=1\nnode.0.sz f=1\nnode.0.fov f=70\nnode.0.near f=0.1\nnode.0.far f=200\n\
node.1.id=2\nnode.1.parent=0\nnode.1.kind=1\nnode.1.name s=Ground\nnode.1.asset s=builtin:plane\nnode.1.texture s=\nnode.1.behavior s=\n\
node.1.px f=0\nnode.1.py f=0\nnode.1.pz f=0\nnode.1.rx f=0\nnode.1.ry f=0\nnode.1.rz f=0\n\
node.1.sx f=40\nnode.1.sy f=1\nnode.1.sz f=40\n\
node.2.id=3\nnode.2.parent=0\nnode.2.kind=1\nnode.2.name s=Crate\nnode.2.asset s=builtin:cube\nnode.2.texture s=\nnode.2.behavior s=\n\
node.2.px f=3\nnode.2.py f=0.5\nnode.2.pz f=0\nnode.2.rx f=0\nnode.2.ry f=0\nnode.2.rz f=0\n\
node.2.sx f=1\nnode.2.sy f=1\nnode.2.sz f=1\n\
node.3.id=4\nnode.3.parent=0\nnode.3.kind=1\nnode.3.name s=Wall\nnode.3.asset s=builtin:cube\nnode.3.texture s=\nnode.3.behavior s=\n\
node.3.px f=0\nnode.3.py f=1\nnode.3.pz f=-6\nnode.3.rx f=0\nnode.3.ry f=0\nnode.3.rz f=0\n\
node.3.sx f=8\nnode.3.sy f=2\nnode.3.sz f=0.5\n\
node.4.id=5\nnode.4.parent=0\nnode.4.kind=1\nnode.4.name s=Spinner\nnode.4.asset s=builtin:cube\nnode.4.texture s=\nnode.4.behavior s=spin\n\
node.4.px f=-3\nnode.4.py f=0.5\nnode.4.pz f=0\nnode.4.rx f=0\nnode.4.ry f=0\nnode.4.rz f=0\n\
node.4.sx f=1\nnode.4.sy f=1\nnode.4.sz f=1\n";

/// Example mesh generator for new projects; `j3c -e gen` turns it into `assets/gen/tree.j3mesh`.
const TREE_GEN: &str = "// A low-poly tree built in code: a tapered trunk and three cones of leaves.
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

fn spin_template(name: &str) -> String {
    format!(
        r#"module {name}.behaviors.spin;

use j3.scene3d;
use j3.scene3d.Scene;

pub fn start(_scene: &Scene, _id: int) {{
}}

pub fn update(scene: &Scene, id: int, dt: f32) {{
    let at = scene3d.index(scene, id);
    if at < 0 {{ return; }}
    let speed = scene3d.number(scene, id, "speed", 1);
    scene.nodes[at].rotation.y += speed * dt;
}}
"#
    )
}

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

use {name}.behaviors;
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
    let scene = scene3d.load(behaviors.scene(), mem);
    return World {{ scene: scene, walker: fpwalk.spawn(&scene, mem), mem: mem }};
}}

fn startup(w: &World) {{
    gfx.init();
    scene3d.upload(&w.scene);
    // Colliders need mesh bounds, which upload fills in.
    w.walker = fpwalk.spawn(&w.scene, w.mem);
    game.captureMouse(true);
    let i = 0;
    while i < w.scene.len {{
        behaviors.start(&w.scene, w.scene.nodes[i].id);
        i += 1;
    }}
}}

fn update(w: &World, dt: f32) {{
    if game.keyPressed(Key.Escape) {{ game.requestQuit(); }}
    fpwalk.update(&w.walker, dt, true, 1, 4.5);
    let i = 0;
    while i < w.scene.len {{
        behaviors.update(&w.scene, w.scene.nodes[i].id, dt);
        i += 1;
    }}
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

use {name}.behaviors;
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
    return World {{ scene: scene3d.load(behaviors.scene(), mem) }};
}}

fn startup(w: &World) {{
    gfx.init();
    scene3d.upload(&w.scene);
    let i = 0;
    while i < w.scene.len {{
        behaviors.start(&w.scene, w.scene.nodes[i].id);
        i += 1;
    }}
}}

fn update(w: &World, dt: f32) {{
    if game.keyPressed(game.Key.Escape) {{ game.requestQuit(); }}
    let i = 0;
    while i < w.scene.len {{
        behaviors.update(&w.scene, w.scene.nodes[i].id, dt);
        i += 1;
    }}
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

fn behavior_names(config: &str) -> Vec<String> {
    config
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("[behavior.")
                .and_then(|s| s.strip_suffix(']'))
                .filter(|s| {
                    !s.is_empty()
                        && !s.as_bytes()[0].is_ascii_digit()
                        && crate::parse::keyword(s).is_none()
                        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                })
                .map(str::to_string)
        })
        .collect()
}

fn table_header(line: &str) -> bool {
    let line = line.trim();
    line.strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .is_some_and(|name| !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-'))
}

/// Parse the editor's TOML array of quoted field definitions. Comments and trailing commas
/// are allowed; a comma inside a string is part of that field's default value.
fn field_array(raw: &str, behavior: &str) -> Result<Vec<String>, String> {
    let error = || format!("j3.editor.toml: behavior `{behavior}` fields must be an array of strings");
    let bytes = raw.as_bytes();
    let mut i = 0;
    let skip = |i: &mut usize| {
        loop {
            while *i < bytes.len() && bytes[*i].is_ascii_whitespace() { *i += 1; }
            if *i < bytes.len() && bytes[*i] == b'#' {
                while *i < bytes.len() && bytes[*i] != b'\n' { *i += 1; }
            } else { break; }
        }
    };
    skip(&mut i);
    if bytes.get(i) != Some(&b'[') { return Err(error()); }
    i += 1;
    let mut fields = Vec::new();
    loop {
        skip(&mut i);
        if bytes.get(i) == Some(&b']') { return Ok(fields); }
        if bytes.get(i) != Some(&b'"') { return Err(error()); }
        i += 1;
        let mut value = Vec::new();
        loop {
            let Some(&b) = bytes.get(i) else { return Err(error()); };
            i += 1;
            if b == b'"' { break; }
            if b == b'\n' || b == b'\r' { return Err(error()); }
            if b == b'\\' {
                let Some(&escaped) = bytes.get(i) else { return Err(error()); };
                i += 1;
                value.push(match escaped {
                    b'"' => b'"', b'\\' => b'\\', b'n' => b'\n', b'r' => b'\r', b't' => b'\t',
                    _ => return Err(error()),
                });
            } else { value.push(b); }
        }
        fields.push(String::from_utf8(value).map_err(|_| error())?);
        skip(&mut i);
        match bytes.get(i) {
            Some(b',') => { i += 1; }
            Some(b']') => return Ok(fields),
            _ => return Err(error()),
        }
    }
}

/// Behavior field types `j3.editor.toml` may declare.
const FIELD_TYPES: [&str; 9] = ["f32", "bool", "string", "int", "vec3", "color", "choice", "asset", "object"];

pub(crate) fn generate_catalog(root: &Path) -> Result<(), String> {
    let config = fs::read_to_string(root.join("j3.editor.toml")).map_err(|e| e.to_string())?;
    for line in config.lines() {
        if let Some(raw) = line.trim().strip_prefix("[behavior.") {
            let name = raw
                .strip_suffix(']')
                .ok_or("j3.editor.toml: malformed behavior section")?;
            if name.is_empty()
                || name.as_bytes()[0].is_ascii_digit()
                || crate::parse::keyword(name).is_some()
                || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                return Err(format!("j3.editor.toml: invalid behavior name `{name}`"));
            }
        }
    }
    let names = behavior_names(&config);
    let mut unique = HashSet::new();
    for name in &names {
        if !unique.insert(name) {
            return Err(format!("j3.editor.toml: duplicate behavior `{name}`"));
        }
    }
    let mut out = format!("catalog.count={}\n", names.len());
    for (i, name) in names.iter().enumerate() {
        let header = format!("[behavior.{name}]");
        let section = config
            .split(&header)
            .nth(1)
            .unwrap_or("")
            .split('\n')
            .skip(1)
            .take_while(|line| !table_header(line))
            .collect::<Vec<_>>()
            .join("\n");
        let mut fields = Vec::new();
        let mut lines = section.lines();
        while let Some(line) = lines.next() {
            let Some((key, value)) = line.split_once('=') else { continue; };
            if key.trim() != "fields" { continue; }
            let mut raw = value.to_string();
            for continuation in lines { raw.push('\n'); raw.push_str(continuation); }
            for definition in field_array(&raw, name)? {
                let (field, typed) = definition
                    .split_once(':')
                    .ok_or_else(|| format!("j3.editor.toml: invalid field `{definition}`"))?;
                let (kind, default) = typed.split_once('=').unwrap_or((typed, ""));
                if field.is_empty()
                    || !field.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    || !FIELD_TYPES.contains(&kind)
                {
                    return Err(format!("j3.editor.toml: invalid field `{definition}`"));
                }
                fields.push((field.to_string(), kind.to_string(), default.to_string()));
            }
            break;
        }
        out.push_str(&format!(
            "catalog.{i}.name s={name}\ncatalog.{i}.count={}\n",
            fields.len()
        ));
        for (j, (field, kind, default)) in fields.iter().enumerate() {
            // A choice lists its options after `=`; the first one is the default.
            let shown = if kind == "choice" { default.split('|').next().unwrap_or("") } else { default.as_str() };
            out.push_str(&format!("catalog.{i}.{j}.name s={field}\ncatalog.{i}.{j}.type s={kind}\ncatalog.{i}.{j}.default s={shown}\n"));
            let value = match kind.as_str() {
                "int" => {
                    let parsed = if default.is_empty() { 0 } else {
                        default.parse::<i64>().map_err(|_| format!("j3.editor.toml: invalid int default for `{field}`"))?
                    };
                    format!("catalog.{i}.{j}.value={parsed}\n")
                }
                "vec3" | "color" => {
                    let fallback = if kind == "color" { "1,1,1" } else { "0,0,0" };
                    let raw = if default.is_empty() { fallback } else { default.as_str() };
                    let parts = raw
                        .split(',')
                        .map(|p| p.trim().parse::<f32>())
                        .collect::<Result<Vec<_>, _>>()
                        .ok()
                        .filter(|p| p.len() == 3)
                        .ok_or_else(|| format!("j3.editor.toml: `{field}` needs three numbers like 1,0.5,0"))?;
                    format!("catalog.{i}.{j}.x f={}\ncatalog.{i}.{j}.y f={}\ncatalog.{i}.{j}.z f={}\n", parts[0], parts[1], parts[2])
                }
                "choice" => {
                    if default.split('|').any(|o| o.trim().is_empty()) {
                        return Err(format!("j3.editor.toml: `{field}` needs options like choice=walk|run"));
                    }
                    format!("catalog.{i}.{j}.options s={default}\n")
                }
                "object" if !default.is_empty() => {
                    return Err(format!("j3.editor.toml: object field `{field}` has no default"));
                }
                "f32" => {
                    let parsed = if default.is_empty() {
                        0.0
                    } else {
                        default.parse::<f32>().map_err(|_| {
                            format!("j3.editor.toml: invalid f32 default for `{field}`")
                        })?
                    };
                    format!("catalog.{i}.{j}.value f={parsed}\n")
                }
                "bool" => {
                    if !default.is_empty() && default != "true" && default != "false" {
                        return Err(format!(
                            "j3.editor.toml: invalid bool default for `{field}`"
                        ));
                    }
                    format!(
                        "catalog.{i}.{j}.value={}\n",
                        if default == "true" { 1 } else { 0 }
                    )
                }
                _ => String::new(),
            };
            out.push_str(&value);
        }
    }
    let dir = root.join("target/j3-editor");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(dir.join("catalog"), out).map_err(|e| e.to_string())
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

pub(crate) fn generate_dispatch(root: &Path, package: &str) -> Result<(), String> {
    let path = root.join("src/behaviors.j3");
    let config = fs::read_to_string(root.join("j3.editor.toml")).map_err(|e| e.to_string())?;
    let names = behavior_names(&config);
    let entry = fs::read_to_string(root.join("src/main.j3")).unwrap_or_default();
    if !path.exists() && names.is_empty() && !entry.contains(&format!("use {package}.behaviors")) {
        return Ok(());
    }
    if let Ok(existing) = fs::read_to_string(&path) {
        if !existing.starts_with("// Generated by j3c editor") {
            if names.is_empty() {
                return Ok(());
            }
            return Err(format!(
                "{} exists and is not generated by j3c editor",
                path.display()
            ));
        }
    }
    let mut src = format!("// Generated by j3c editor. Edit behavior modules instead.\nmodule {package}.behaviors;\n\nuse lib \"c\" as libc {{\n    fn getenv(name: cstr): cstr;\n    fn strlen(s: cstr): int;\n}}\n\nuse j3.scene3d;\nuse j3.scene3d.Scene;\nuse j3.text.{{ cstr, same }};\n");
    for name in &names {
        src.push_str(&format!("use {package}.behaviors.{name};\n"));
    }
    let scene = scene_path(root)?;
    let rel = scene.strip_prefix(root).map_err(|e| e.to_string())?;
    src.push_str(&format!(
        "\n/// The scene set in j3.editor.toml.\npub const MAIN_SCENE: string = {:?};\n",
        rel.to_string_lossy()
    ));
    src.push_str("\n/// The scene to run: the one the editor's Play picked (`J3_SCENE`), else `MAIN_SCENE`.\npub fn scene(): string {\n    let raw = libc.getenv(cstr(\"J3_SCENE\"));\n    if raw == 0 { return MAIN_SCENE; }\n    let len = libc.strlen(raw);\n    if len < 1 { return MAIN_SCENE; }\n    return []u8 { ptr: raw as &u8, len: len } as string;\n}\n");
    src.push_str("\npub fn start(scene: &Scene, id: int) {\n    let at = scene3d.index(scene, id);\n    if at < 0 { return; }\n    let name = scene.nodes[at].behavior;\n");
    for name in &names {
        src.push_str(&format!(
            "    if same(name, \"{name}\") {{ {name}.start(scene, id); return; }}\n"
        ));
    }
    src.push_str("}\n\npub fn update(scene: &Scene, id: int, dt: f32) {\n    let at = scene3d.index(scene, id);\n    if at < 0 { return; }\n    let name = scene.nodes[at].behavior;\n");
    for name in &names {
        src.push_str(&format!(
            "    if same(name, \"{name}\") {{ {name}.update(scene, id, dt); return; }}\n"
        ));
    }
    src.push_str("}\n");
    if path.exists() && fs::read_to_string(&path).ok().as_deref() == Some(&src) {
        return Ok(());
    }
    fs::write(path, src).map_err(|e| e.to_string())
}

fn create_behavior(root: &Path, name: &str) -> Result<(), String> {
    if name.is_empty()
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        || name.as_bytes()[0].is_ascii_digit()
        || crate::parse::keyword(name).is_some()
    {
        return Err("behavior name must be a J3 identifier".into());
    }
    let manifest = Manifest::load(&root.join("j3.toml"))?;
    let config_path = root.join("j3.editor.toml");
    let mut config = fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
    if behavior_names(&config)
        .iter()
        .any(|existing| existing == name)
    {
        return Err(format!("behavior `{name}` already exists"));
    }
    let dir = root.join("src/behaviors");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{name}.j3"));
    if path.exists() {
        return Err(format!("{} already exists", path.display()));
    }
    let stub = format!("module {}.behaviors.{name};\n\nuse j3.scene3d.Scene;\n\npub fn start(_scene: &Scene, _id: int) {{\n}}\n\npub fn update(_scene: &Scene, _id: int, _dt: f32) {{\n}}\n", manifest.name);
    fs::write(&path, stub).map_err(|e| e.to_string())?;
    config.push_str(&format!("\n[behavior.{name}]\nfields = []\n"));
    fs::write(config_path, config).map_err(|e| e.to_string())?;
    generate_dispatch(root, &manifest.name)?;
    generate_catalog(root)?;
    println!("created {}", path.display());
    Ok(())
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

/// Build the game and (re)launch it on `scene`. The lock is the "Play is busy" flag and holds
/// the game; after launch it is released between checks so Stop and Play can reach the game.
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
    let build = match Command::new(host).arg("build").arg(root).output() {
        Ok(out) => out,
        Err(e) => return event(root, "msg", "", &format!("build game: {e}")),
    };
    let mut log = String::from_utf8_lossy(&build.stderr).to_string();
    log.push_str(&String::from_utf8_lossy(&build.stdout));
    let _ = fs::write(root.join("target/j3-editor/build.log"), &log);
    let project = format!("{}/", root.display());
    if !build.status.success() {
        // The console lists these lines; the full log stays in build.log.
        let errors = log.lines().filter(|s| s.contains("error") || s.trim_start().starts_with("-->"));
        let mut sent = 0;
        for line in errors.take(30) {
            event(root, "build", "", &line.trim().replace(&project, ""));
            sent += 1;
        }
        if sent == 0 {
            event(root, "build", "", "Build failed; see target/j3-editor/build.log");
        }
        return event(root, "msg", "", "Build failed");
    }
    for line in log.lines().filter(|s| s.contains("warning")).take(30) {
        event(root, "out", "", &line.trim().replace(&project, ""));
    }
    event(root, "out", "", &format!("Built in {:.1}s", started.elapsed().as_secs_f32()));
    let launched = Manifest::load(&root.join("j3.toml")).and_then(|m| {
        Command::new(m.out_path())
            .current_dir(root)
            .env("J3_ROOT", root)
            .env("J3_SCENE", scene)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())
    });
    let pid = match launched {
        Ok(mut process) => {
            let pid = process.id();
            if let Some(out) = process.stdout.take() { forward(root, out, "out"); }
            if let Some(err) = process.stderr.take() { forward(root, err, "err"); }
            *running = Some(process);
            event(root, "run", "", "Game launched");
            pid
        }
        Err(e) => return event(root, "msg", "", &format!("Could not launch game: {e}")),
    };
    drop(running);
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

/// Regenerate the behavior dispatcher and both catalogs, as at startup.
fn rescan(root: &Path) -> Result<(), String> {
    for line in crate::gen::run_all(root, false).failed {
        event(root, "build", "", &format!("mesh generator failed: {line}"));
    }
    let manifest = Manifest::load(&root.join("j3.toml"))?;
    generate_dispatch(root, &manifest.name)?;
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
            // ponytail: a behavior created mid-build rewrites src/behaviors.j3 under the build; Play again.
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
                Ok(()) => event(root, "refresh", "", "Assets and j3.editor.toml reloaded"),
                Err(e) => event(root, "msg", "", &e),
            }
        } else if let Some(name) = line.strip_prefix("J3EDITOR BEHAVIOR ") {
            match create_behavior(root, name) {
                Ok(()) => event(root, "refresh", "", "Behavior created. Rebuild with Play."),
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
    fn config_accepts_multiline_fields_and_comma_defaults() {
        let root = std::env::temp_dir().join(format!("j3_editor_config_{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("j3.editor.toml"), "[editor]\nscene=\"scenes/demo.j3scene\"\n\n[behavior.caption]\nfields=[\n  \"text:string=hi, there\", # comma belongs to the value\n  \"speed:f32=2.5\",\n]\n").unwrap();
        assert_eq!(scene_path(&root).unwrap(), root.join("scenes/demo.j3scene"));
        generate_catalog(&root).unwrap();
        let catalog = fs::read_to_string(root.join("target/j3-editor/catalog")).unwrap();
        assert!(catalog.contains("catalog.0.count=2"), "{catalog}");
        assert!(catalog.contains("catalog.0.0.default s=hi, there"), "{catalog}");
        assert!(catalog.contains("catalog.0.1.value f=2.5"), "{catalog}");
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

    #[test]
    fn config_field_types() {
        let root = std::env::temp_dir().join(format!("j3_editor_types_{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let write = |fields: &str| {
            fs::write(root.join("j3.editor.toml"), format!("[behavior.enemy]\nfields = [{fields}]\n")).unwrap();
            generate_catalog(&root).map(|_| fs::read_to_string(root.join("target/j3-editor/catalog")).unwrap())
        };
        let catalog = write(r#""hp:int=-3", "spot:vec3=1,2,3", "tint:color", "mode:choice=walk|run", "skin:asset=a.png", "chase:object""#).unwrap();
        for line in [
            "catalog.0.0.value=-3", "catalog.0.1.z f=3", "catalog.0.2.x f=1", "catalog.0.3.default s=walk",
            "catalog.0.3.options s=walk|run", "catalog.0.4.default s=a.png", "catalog.0.5.type s=object",
        ] {
            assert!(catalog.contains(line), "{line} missing from {catalog}");
        }
        assert!(write(r#""spot:vec3=1,2""#).is_err());
        assert!(write(r#""mode:choice=walk||run""#).is_err());
        assert!(write(r#""chase:object=3""#).is_err());
        assert!(write(r#""hp:int=1.5""#).is_err());
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

    #[test]
    fn robust_dispatch_follows_editor_toml() {
        let root = std::env::temp_dir().join(format!("j3_dispatch_test_{}", std::process::id()));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("scenes")).unwrap();
        fs::write(root.join("scenes/a.j3scene"), "").unwrap();
        fs::write(root.join("scenes/b.j3scene"), "").unwrap();
        fs::write(root.join("j3.toml"), "[package]\nname = \"t\"\n").unwrap();
        fs::write(root.join("src/main.j3"), "use t.behaviors;\n").unwrap();
        let generated = |scene: &str| {
            fs::write(root.join("j3.editor.toml"), format!("[editor]\nscene = \"{scene}\"\n")).unwrap();
            generate_dispatch(&root, "t").unwrap();
            fs::read_to_string(root.join("src/behaviors.j3")).unwrap()
        };
        assert!(generated("scenes/a.j3scene").contains("\"scenes/a.j3scene\""));
        assert!(generated("scenes/b.j3scene").contains("\"scenes/b.j3scene\""));
        fs::remove_dir_all(root).unwrap();
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
