//! Text for `j3c guide` and `j3c help manifest`. The editor schema in
//! `editors/vscode/toml.js` matches these keys.

pub const GUIDE: &str = "\
j3c guide — start a J3 project

The compiler is `j3c` on PATH. A new directory does not contain the
language. The standard library is inside the `j3c` binary. `$J3_STDLIB`
replaces that copy with a directory when it is set.

    j3c new mygame           hello: j3.toml + src/main.j3 + AGENTS.md
    j3c new mygame --app     SDL window (links SDL2)
    cd mygame
    j3c check                typecheck, then missing libraries and loaded files
    j3c run
    j3c build                writes target/<name>

`j3c new` with no name writes into the current directory. The module
name comes from that directory. `my-game` and `My Game` become `my_game`.

Layout
    j3.toml                  [package] name, entry (default src/main.j3)
    src/main.j3              module <name>; and fn main()
    src/other.j3             use <name>.other.Name
    AGENTS.md                how to work in this project

One file is one module. `use j3.io.printLn` brings in one name.
`use j3.memory.{ Arena, List }` brings in several.

Scripts are `.j3s` files anywhere in the project (`scripts/door.j3s`): J3
with `props { ... }`, per-instance `let` state, and `pub fn`s. Bind one
with `const Door = @script(\"scripts/door.j3s\");` and make instances with
`Door { hinge: h }`. `j3c run` runs scripts on a VM, so a script edit
re-runs without compiling the program; `j3c build` compiles them in.
`j3c doc lang` has the rules; `j3c doc recipes script` is a small program.

    j3c doc                  tasks, then every stdlib module
    j3c doc recipes          complete programs that check: 2D app, 3D scene,
                             canvas art, script, meshgen, material, URP
    j3c doc search texture   modules, names, then comments with a word
    j3c doc j3.game --short  a module's names on one screen
    j3c doc j3.app           public items and their /// comments
    j3c doc j3.app create    one item (`App.create` works too)
    j3c doc j3.gfx.canvas        headless sprite and texture drawing
    j3c doc j3.gfx.urp.raytrace  fullscreen room and sphere renderer
    j3c doc j3.gfx.urp.forest    mesh-aware light and glass composite
    j3c doc lang             language
    j3c doc j3ui             UI components
    j3c doc editor           3D scene editor, scene files, scripts
    j3c help manifest        j3.toml keys
    j3c fmt --write          rewrite src/
    j3c check --json         one JSON object (ok, or message/path/line)

`j3c check` does not run the game. It fails when a linked library is
missing, or when `tryLoadFont` / `loadFont` / `tryLoadTexture` /
`loadTexture` / `tryLoadSound` / `loadSound` / `tryLoadMusic` /
`loadMusic` / `setVolume` points at a library or file that is not there.
`j3c run` uses that same check and does not launch when it fails.

`run` and `build` with no path use ./j3.toml. They stop if it is
missing. A project run uses the package root as the working directory
and sets J3_ROOT, so assets are read from that directory.

Everything above ships inside j3c. Larger games live in the j3 source
repository (not installed with j3c), under examples/, if you have it:
    examples/bounce         2D App, paddle and ball
    examples/dungeon        tiles, UI, save, audio
    examples/island         GL mesh and a shadow map
    examples/minecraft      first person, break and place
    examples/leaves         3D yard, click, upgrades
    examples/defense        tower defense, waves and HUD
    examples/gallery        UI: widgets, controls, layout, style, inventory
    examples/canvas         procedural sprite and tile PNGs
    examples/canvas_preview SDL preview of generated PNGs and live pixels
    examples/raytrace       OpenGL room raytracer
    examples/ocean          custom-shader wave scene
    examples/forest         mesh, canopy shadows, and volumetric light
    examples/scripted       .j3s scripts: props, state, shared, instances

Programmatic art: `j3.gfx.canvas.Canvas.create(arena, w, h, Mode.Pixel)`
or `Mode.Smooth`, then draw shapes, gradients, or noise. `savePng` exports
without SDL. In a game, `j3.game.textureFromCanvas` uploads once;
`updateTextureFromCanvas` updates a changed rectangle. Keep the arena
alive while drawing; destroy uploaded textures when done.

`j3c lsp` is the language server. The editor extension spawns it.
Set j3.j3cPath when `j3c` is not on PATH.

3D scenes without the editor window (for agents and scripts):
    j3c -e scene list        objects by name path; add/set/delete/check too
    j3c -e scene ground Box  place objects: ground, ontop, scatter
    j3c -e render --view iso the scene as the game draws it, to a PNG
    j3c run --frames 120 --screenshot shot.png --headless
    j3c -e gen               run assets/gen/*.j3 mesh generators (j3.gfx.meshgen)
    j3c -e mcp               the same as MCP tools for an agent; with the
                             editor open, edits land there as undo steps
    j3c -e mcp --tools       each MCP tool's arguments and CLI twin

Check your work (`j3c doc recipes check` says what each step catches):
    j3c check  →  j3c -e scene check --all  →  j3c -e render  →
    j3c run --frames 120 --screenshot shot.png --headless

Docs that disagree with the compiler? `j3c version --verbose` prints this
build's docs id (the `j3c doc` header shows it too) and which binary ran.
A different id, or a binary you didn't expect on PATH, means other docs:
rebuild or call the j3c you meant.
";

pub const MANIFEST: &str = "\
j3.toml

[package]
    name     binary name (required). `build` writes target/<name>
    entry    file that contains main. Default src/main.j3

[emit]
    backend  llvm (default) or c
    opt      0, 1, 2, 3, s, or z. LLVM default is 2
    debug    debug info. Default true, unless [cc] fast is set
    fast     strip bounds-check aborts, same as --fast
    target   one triple, or an array. Empty means the host.
             `run` uses the host triple. `build` emits each one
    cpu      CPU passed to clang, such as native
    clang    clang executable. Default clang
    flags    extra flags for clang. An array of strings

[link]
    libs     library names without -l (SDL2 links as -lSDL2)
    search   library search directories (-L)
    files    extra object files or archives
    flags    extra linker flags, such as an rpath

[cc]
    std      C standard. Default c11. Used when the backend is C
    opt      optimization. [emit] opt inherits this when [emit] is omitted
    fast     strip bounds-check aborts

CLI flags override the file: --emit llvm|c, --opt N, --fast.
No [emit] section inherits [cc] opt.

Environment
    J3_STDLIB   stdlib directory (the parent of j3/). Unset means the
                copy compiled into this j3c binary
    J3_EDITOR_SRC  editor frontend `src/` directory for `j3c editor`, instead of the
                copy compiled into this j3c binary
    J3_ROOT     set by `run` to the package root. The process cwd is
                that directory too
    J3C_COLOR   set to color rustc-shaped diagnostics
    J3C_TIMINGS set to print how long each build phase took, to stderr
    J3_FRAMES, J3_SCREENSHOT  set by `run --frames` / `--screenshot`; an App
                stops after that many frames and saves the last one
";

/// One `j3c doc recipes <name>`: prose, then every file of a program that checks.
pub struct Recipe {
    pub name: &'static str,
    pub what: &'static str,
    pub text: &'static str,
    /// Project-relative path and contents. A test writes them out and runs `j3c check`.
    pub files: Vec<(&'static str, String)>,
}

pub fn recipes() -> Vec<Recipe> {
    let app_main = crate::fmt::format_src(&crate::scaffold::app_main("mygame")).unwrap_or_default();
    vec![
        Recipe {
            name: "app2d",
            what: "a window, a game loop, keys, and 2D drawing",
            text: RECIPE_APP2D,
            files: vec![("j3.toml", crate::scaffold::app_toml("mygame")), ("src/main.j3", app_main)],
        },
        Recipe {
            name: "scene3d",
            what: "load an editor scene, run its scripts, and draw it lit and shadowed",
            text: RECIPE_SCENE3D,
            files: crate::editor::project_files("mygame", false),
        },
        Recipe {
            name: "canvas",
            what: "draw a sprite in code and save it as a PNG, no window",
            text: RECIPE_CANVAS,
            files: vec![("j3.toml", "[package]\nname = \"sprites\"\nentry = \"src/main.j3\"\n".to_string()), ("src/main.j3", CANVAS_MAIN.to_string())],
        },
        Recipe {
            name: "script",
            what: "a `.j3s` script with props, shared and per-instance state, made from J3",
            text: RECIPE_SCRIPT,
            files: vec![
                ("j3.toml", "[package]\nname = \"counters\"\nentry = \"src/main.j3\"\n".to_string()),
                ("scripts/counter.j3s", SCRIPT_COUNTER.to_string()),
                ("src/main.j3", SCRIPT_MAIN.to_string()),
            ],
        },
        Recipe {
            name: "meshgen",
            what: "build a mesh in code for an editor project",
            text: RECIPE_MESHGEN,
            files: vec![("assets/gen/tree.j3", crate::editor::TREE_GEN.to_string())],
        },
        Recipe {
            name: "material",
            what: "a `.j3mat` material with a `.j3fx` shader effect",
            text: RECIPE_MATERIAL,
            files: Vec::new(),
        },
        Recipe {
            name: "urp",
            what: "traced mirrors, glass, and sun shadows",
            text: RECIPE_URP,
            files: Vec::new(),
        },
        Recipe {
            name: "check",
            what: "the edit, check, look, play loop and what each step catches",
            text: RECIPE_CHECK,
            files: Vec::new(),
        },
    ]
}

pub fn recipe_index() -> String {
    let mut out = String::from("recipes (`j3c doc recipes <name>`):\n\n");
    for r in recipes() {
        out.push_str(&format!("    {:<9} {}\n", r.name, r.what));
    }
    out.push_str("\nEach prints the files of a whole program; write them into an empty directory and run `j3c check`.\n");
    out
}

pub fn recipe(name: &str) -> Result<String, String> {
    let all = recipes();
    let Some(r) = all.iter().find(|r| r.name == name) else {
        let names: Vec<&str> = all.iter().map(|r| r.name).collect();
        return Err(format!("no recipe `{name}`\n  = help: one of {}", names.join(", ")));
    };
    let mut out = format!("recipe {}: {}\n\n{}", r.name, r.what, r.text);
    for (path, text) in &r.files {
        out.push_str(&format!("\n==> {path} <==\n{text}"));
        if !text.ends_with('\n') {
            out.push('\n');
        }
    }
    Ok(out)
}

const RECIPE_APP2D: &str = "\
`j3c new mygame --app` writes these files. Needs SDL2. `j3c run` opens the window.

- `App<World>.create(title, w, h, boot)` makes the window. `boot` runs once with the app's
  arena and returns the World; allocate from that arena, which lives until quit.
- Register systems before `run()`: `startup` once, `first`/`add`/`last` at a fixed 60 Hz,
  `always` every frame with dt, `draw` last (`j3c doc j3.app`).
- The window can resize; place things from `windowSize()` (`j3c doc j3.game windowSize`).
- Next: `j3c doc j3.game --short` (draw, input, audio), `j3c doc j3ui` (menus and HUDs),
  `j3c doc recipes canvas` (make the art in code).
";

const RECIPE_SCENE3D: &str = "\
`j3c editor --new mygame` writes these files and opens the editor; writing them yourself
gives the same project without a window. `j3c new mygame --template fps` is this with a
first-person walker. Needs SDL2 and GL.

- `boot`: `scene3d.load(path, mem)` reads the scene into the app arena, where it lives until
  quit. `scripts.scene()` is the scene the editor's Play picked, else j3.editor.toml's.
- `startup` runs once GL exists (`app.useGl()` asks for it): `gfx.init()`, then
  `scene3d.upload(&scene)` sends meshes and textures to the GPU, then `scripts.start`.
- Every frame: `scripts.update` in an update system. In draw, in this order:
  `environment`, `shadows`, `lights`, `beginEffects`, the camera pass, `scene3d.draw`,
  `endEffects`. Quitting frees GL through `game.destroy()`.
- `j3.scene3d.scripts` is made from the scenes' script lines: `j3c doc j3.scene3d.scripts`
  in the project shows it. Edit scenes with `j3c -e scene` (`j3c doc editor`), and look
  with `j3c -e render`.
";

const RECIPE_CANVAS: &str = "\
A headless program: no SDL, no window. `j3c run` writes coin.png in the project root
(a project runs with its root as the working directory).

- `Mode.Pixel` keeps hard pixel edges; `Mode.Smooth` antialiases circles, lines, and
  triangles. Canvas colors (`canvas.rgb`) are image pixels, not `j3.game` screen colors.
- The canvas lives in the arena you pass; keep the arena until you are done with it.
- In a game, `game.textureFromCanvas(c)` uploads once; the texture keeps its pixels after
  the arena goes. `updateTextureFromCanvas` re-uploads a changed rectangle. Destroy
  textures when done.
- Next: `j3c doc j3.gfx.canvas --short` (shapes, gradients, noise, blending).
";

const CANVAS_MAIN: &str = "\
module sprites;

use j3.gfx.canvas;
use j3.gfx.canvas.{ Canvas, Mode };
use j3.io.printLn;
use j3.memory.Arena;
use j3.result.Result;
use j3.sys.panic;

fn main() {
    let arena = Arena.create(1024 * 1024);
    let coin = Canvas.create(&arena, 32, 32, Mode.Smooth);
    coin.clear(canvas.rgba(0, 0, 0, 0));
    coin.fillCircle(16, 16, 13, canvas.rgb(214, 150, 40));
    coin.fillCircle(16, 16, 9, canvas.rgb(250, 204, 90));
    match coin.savePng(\"coin.png\", &arena) {
        Result.Ok(_) {
        }
        Result.Err(_) {
            panic(\"could not write coin.png\");
        }
    }
    printLn(\"wrote coin.png\");
    arena.destroy();
}
";

const RECIPE_SCRIPT: &str = "\
`@script` makes a script a type; each `Counter { ... }` is an instance. `j3c run` runs
scripts on a VM, so editing counter.j3s re-runs without compiling main.j3; `j3c build`
compiles them in. Prints `a: 1 of 5` and `b: 4 of 5`.

- props are set where the instance is made and are read-only in the script; one with no
  default is required.
- shared is one value for every instance. A top-level `let` is each instance's own.
- Outside code only calls `pub fn`s; it can't read an instance's fields.
- To run a script on a scene object instead, attach it in the scene (`recipes scene3d`).
- `j3c doc scripts/counter.j3s` summarizes a script. Rules: `j3c doc lang`, Scripts.
";

const SCRIPT_COUNTER: &str = "\
use j3.io.printLn;

props {
    name: string;
    step: int = 1;
}

shared {
    total: int = 0;
}

let count = 0;

pub fn tick() {
    count += step;
    total += step;
}

pub fn report() {
    printLn(`{name}: {count} of {total}`);
}
";

const SCRIPT_MAIN: &str = "\
module counters;

const Counter = @script(\"scripts/counter.j3s\");

fn main() {
    let a = Counter { name: \"a\" };
    let b = Counter { name: \"b\", step: 2 };
    a.tick();
    b.tick();
    b.tick();
    a.report();
    b.report();
}
";

const RECIPE_MESHGEN: &str = "\
In an editor project, each `assets/gen/<name>.j3` is a small program that builds a mesh
and saves `assets/gen/<name>.j3mesh`. `j3c -e gen` runs the scripts that changed, and
render, play, and build run it too. Use the result as a mesh asset:
`j3c -e scene add mesh name=Tree asset=assets/gen/tree.j3mesh`.

- A script reruns when it or j3c changed. It does not see changes in other project files
  it imports; `j3c -e gen --force` reruns everything.
- Coordinates are meters, y up; `push`/`pop` save the transform, `color` sets vertex color.
- Next: `j3c doc j3.gfx.meshgen --short`.
";

const RECIPE_MATERIAL: &str = "\
A mesh's `material` field points at a `.j3mat`: `key=value` lines, where missing keys keep
their defaults. The editor's material tab writes them; by hand, a toon-shaded material:

==> assets/materials/toon.j3mat <==
tint.r f=0.9
tint.g f=0.5
tint.b f=0.3
shine f=0.2
fx s=assets/fx/toon.j3fx
fx.bands f=3

==> assets/fx/toon.j3fx <==
kind=material
param.bands=int:3
---
vec3 effect(Surface s, vec3 lit) {
  float l = max(dot(lit, vec3(0.299, 0.587, 0.114)), 0.001);
  return lit * (ceil(l * float(uFx_bands)) / float(uFx_bands)) / l;
}

Use it: `j3c -e scene set Crate material=assets/materials/toon.j3mat`, then
`j3c -e render --focus Crate`. Other keys: glow.r/g/b, reflect, rough, ior (glass),
opacity, texture s=<png>. A `displace` function under `#ifdef J3_VERTEX` moves vertices
(and their shadows). GLSL errors print with the `.j3fx` line, and a broken effect draws
as if it weren't there. Formats and every setting: `j3c doc editor`, Materials and Effects.
";

const RECIPE_URP: &str = "\
In an editor scene URP is a setting, and `scene3d.draw` traces it; the game needs no code:

    j3c -e scene set @scene urp=true urpBounces=2 urpShadows=true
    j3c -e render

Only surfaces with a mirror (`reflect`) or glass (`ior`) material trace rays; the rest draw
as usual. Traces use material colors, not textures, and ignore `.j3fx` effects. Moving
objects are cheap: each mesh keeps its own tree, and a frame rebuilds only the tree over
object boxes. Settings: `j3c doc editor`, URP.

Outside scene3d, `j3.gfx.urp.raytrace` (spheres in a room) and `j3.gfx.urp.forest` (a
composite over `j3.gfx` meshes) are renderers you drive yourself; `j3c doc urp` has their
pass order and GL lifetime.
";

const RECIPE_CHECK: &str = "\
Each step catches something the others don't. From the project root:

    j3c check                     J3 type errors (file:line), missing libraries, and files
                                  that load calls name; wrong script props against the
                                  scene line. Does not run anything.
    j3c -e scene check --all      every scene: broken parents, missing mesh, material,
                                  texture, and scene files, unknown scripts, placement cycles
    j3c -e render --view iso --out shot.png
                                  the scene as the game draws it, without the game's code
    j3c run --frames 120 --screenshot shot.png --headless
                                  the real game for 2 seconds with no window; the last
                                  frame is saved and output ends with a J3STATS line

Paths: `--out` must be inside the project; `--screenshot` is relative to where you ran
j3c. The game's stdout and stderr come back to you. The editor's Play writes build errors
to target/j3-editor/build.log. The MCP server (`j3c -e mcp`) has the same steps as the
check, render, and play tools.
";
