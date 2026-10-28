use super::*;

/// `j3c new NAME --template fps`: the editor project with a walkable scene.
pub(crate) fn create_fps_project(root: &Path) -> Result<(), String> {
    create_project(root, true)
}

/// A new editor project. `fps` picks the first-person main and scene.
// ponytail: a bool for two templates; make it an enum when a third arrives.
pub(super) fn create_project(root: &Path, fps: bool) -> Result<(), String> {
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
    if root.join("AGENTS.md").exists() {
        return Err(format!("{} already exists", root.join("AGENTS.md").display()));
    }
    fs::create_dir_all(root.join("assets")).map_err(|e| e.to_string())?;
    for (relative, text) in files {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(path, text).map_err(|e| e.to_string())?;
    }
    crate::scaffold::write_agent_files(root, true)
}

/// Every file of a new editor project (`j3c doc recipes scene3d` prints them too).
pub(crate) fn project_files(name: &str, fps: bool) -> Vec<(&'static str, String)> {
    vec![
        ("j3.toml", format!("[package]\nname = \"{name}\"\nentry = \"src/main.j3\"\n\n[emit]\nbackend = \"llvm\"\nopt = 0\ndebug = true\n\n[link]\nlibs = [\"SDL2\", \"GL\"]\n")),
        ("j3.editor.toml", "[editor]\nscene = \"scenes/main.j3scene\"\n".to_string()),
        ("src/main.j3", if fps { fps_template(name) } else { game_template(name) }),
        ("scripts/spin.j3s", SPIN_SCRIPT.to_string()),
        ("scenes/main.j3scene", (if fps { FPS_SCENE } else { START_SCENE }).to_string()),
        ("assets/gen/tree.j3", TREE_GEN.to_string()),
    ]
}

pub(super) const START_SCENE: &str = "j3scene 2 next=3\n\nCamera camera pos=0,2,7 rot=-11.459,0,0 #1\nCube cube script=scripts/spin.j3s #2\n";

/// The `--template fps` scene: a camera (where the walker starts), a floor, and things to bump into.
pub(super) const FPS_SCENE: &str = "j3scene 2 next=6\n\nCamera camera pos=0,0,8 fov=70 near=0.1 #1\nGround plane scale=40,1,40 #2\nCrate cube pos=3,0.5,0 #3\nWall cube pos=0,1,-6 scale=8,2,0.5 #4\nSpinner cube pos=-3,0.5,0 script=scripts/spin.j3s #5\n";

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
pub(super) const SPIN_SCRIPT: &str = "// Attached to objects in the scene editor. The scene calls `start` once and `update`
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

pub(super) fn add_editor_files(root: &Path) -> Result<(), String> {
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
        fs::write(&scene, "j3scene 2 next=1\n")
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(super) fn module_name(s: &str) -> String {
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

pub(super) fn fps_template(name: &str) -> String {
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

pub(super) fn game_template(name: &str) -> String {
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
