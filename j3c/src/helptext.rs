//! Text for `j3c guide` and `j3c help manifest`. The editor schema in
//! `editors/vscode/toml.js` matches these keys.

pub const GUIDE: &str = "\
j3c guide — start a J3 project

The compiler is `j3c` on PATH. A new directory does not contain the
language. The standard library is inside the `j3c` binary. `$J3_STDLIB`
replaces that copy with a directory when it is set.

    j3c new mygame           hello: j3.toml + src/main.j3 + AGENTS.md
    j3c new mygame --app     SDL window (links SDL2)
    j3c new mygame --template ui   .j3ui components window (`j3c doc j3ui`)
    cd mygame
    j3c check                typecheck, then missing libraries and loaded files
    j3c run --frames 60 --screenshot shot.png --headless   run with no window, save a frame
    j3c build                writes target/<name>
    j3c build --release      small binary, plus [dist] build_outputs: zip, .deb (`j3c doc ship`)

Agents: never open a window. A bare `j3c run` of an --app, ui, or 3D project opens one.
Always add `--headless` (or `SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy` on the
same command for a built binary), read the PNG, and wrap long runs in `timeout`.
`j3c doc testing` has captures, goldens, and `j3c -e imgdiff`.

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
`j3c run path/to/tool.j3s` runs that file's `pub fn main` once.
`j3c doc lang` has the rules; `j3c doc recipes script` is a small program.

    j3c doc                  tasks, then every stdlib module
    j3c doc recipes          complete programs that check: 2D app, 3D scene,
                             components (.j3ui), canvas art, script, meshgen, material, URP
    j3c doc search texture   modules, names, then comments with a word
    j3c doc j3.game --short  a module's names on one screen
    j3c doc j3.app           public items and their /// comments
    j3c doc j3.app create    one item (`App.create` works too)
    j3c doc j3.gfx.canvas        headless sprite and texture drawing
    j3c doc j3.gfx.urp.raytrace  fullscreen room and sphere renderer
    j3c doc j3.gfx.urp.forest    mesh-aware light and glass composite
    j3c doc lang             language
    j3c doc j3ui             .j3ui components (markup + CSS); j3ui-immediate is the older @j3ui
    j3c doc css              the CSS subset .j3ui supports, and differences from a browser
    j3c doc testing          headless run, screenshot, goldens, imgdiff, HTML snapshot
    j3c doc editor           3D scene editor, scene files, scripts
    j3c doc ship             release builds, zips, .deb with menu entry and icon
    j3c help manifest        j3.toml keys
    j3c fmt --write          rewrite src/
    j3c check --json         one JSON object (ok, or message/path/line)
    j3c check --strict       exit 1 on CSS lint warnings too (`j3c doc css`, Lint)

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
    examples/fps-shooter    first-person shooter: biome scenes, bosses, HUD, save, perks
    examples/3d-editor-scene smallest editor project: two scenes, a .j3s script
    examples/editor_village scenes placing scenes (houses in a village)
    examples/gallery        UI: widgets, controls, layout, style, inventory
    examples/scripted       .j3s scripts: props, state, shared, instances
    examples/idle-breakout  .j3ui UI + canvas game, goldens (golden.toml), bench
    examples/industry       first-person voxel automation: mine, belts, furnaces
    examples/blocks-adventure  network-first voxel adventure (DESIGN.md)
    examples/arcane-bash    3D scene game with editor scenes and tools
    examples/mini-voxel     small voxel world (PERF.md)
    tools/bench2d           2D draw benchmark and golden (README.md)

Programmatic art: `j3.gfx.canvas.Canvas.create(arena, w, h, Mode.Pixel)`
or `Mode.Smooth`, then draw shapes, gradients, or noise. `savePng` exports
without SDL. In a game, `j3.game.textureFromCanvas` uploads once;
`updateTextureFromCanvas` updates a changed rectangle. Keep the arena
alive while drawing; destroy uploaded textures when done.

`j3c lsp` is the language server. The editor extension spawns it.
Set j3.j3cPath when `j3c` is not on PATH.

3D scenes without the editor window (for agents and scripts):
    j3c -e scene list        objects by name path; add/set/delete/check/stats/diff too
    j3c -e scene ring|wall|stairs|spiral ...  build shapes; ground, ontop, scatter place
    j3c -e render --view iso the scene as the game draws it, to a PNG
    j3c -e render --at 0,2,5 --yaw 90   look from any spot (--views for several, --hide to see in)
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
    version  package version (the .deb version). Default 0.1.0
    description  one line for the .deb and the menu entry

[emit]
    backend  llvm (default) or c
    opt      0, 1, 2, 3, s, or z. LLVM default is 2
    incremental  at opt 0, reuse one object per module. Default true.
             \"all\" does it at every opt (faster rebuilds, no
             cross-module inlining). --release ignores it
    debug    debug info. Default true, unless [cc] fast is set
    fast     strip bounds-check aborts, same as --fast
    target   one triple, or an array. Empty means the host.
             `run` uses the host triple. `build` emits each one,
             to target/<triple>/<name> when there are several
    cpu      CPU passed to clang, such as native
    clang    clang executable. Default clang
    flags    extra flags for clang. An array of strings

[link]
    libs     library names without -l (SDL2 links as -lSDL2)
    search   library search directories (-L)
    files    extra object files or archives
    flags    extra linker flags, such as an rpath

[dist]
    build_outputs  what `build --release` writes to target/dist/, per target:
             zip    <name>-<os>-<arch>.zip, a <name>/ folder with the
                    program, the include list, and (Windows) the DLLs the
                    .exe imports, found via [link] search
             deb    <pkg>_<version>_<arch>.deb (Linux targets): the folder
                    in /opt/<name>, a /usr/bin/<name> launcher that runs it
                    from ~/.local/share/<name> (so relative saves work), a
                    menu entry, the icon. Depends come from the libraries
                    the program loads. Needs `tar`, not dpkg
    zip      true: same as build_outputs = [\"zip\"]
    include  files and folders to ship. Default: whichever of
             assets, scripts, scenes, locales exist
    icon     .png (square 256 or 512 looks best) or .svg for the menu
    title    menu name. Default: the package name, title-cased
    categories  .desktop Categories. Default Game;
    maintainer  Name <email>. Default $DEBFULLNAME/$DEBEMAIL, else git
    depends  extra .deb packages, after the ones found

[ui]
    dir      folder of .j3ui components. Default ui. All of them become
             one module, <package>.ui. `j3c dump-ui FILE` prints the J3
    theme    one global CSS file, read at compile time (scope 0)
    lint     false turns the `j3c check` CSS lint off; lint-off = [\"rule\", ...]
             skips some (`j3c doc css`, Lint)

[i18n]
    dir      folder of <locale>.txt string tables. Default locales
    default  locale shown first, and the one `j3c check` compares keys
             against. Default en
    fallback locale tried when a key is missing. Default en
             (`j3c doc i18n`; shipped in dist zips and .debs when the folder exists)

Build bookkeeping (stamps, per-module objects, the `run` dev binary)
lives in target/.j3c/. Deleting it only costs a full rebuild.

[cc]
    std      C standard. Default c11. Used when the backend is C
    opt      optimization. [emit] opt inherits this when [emit] is omitted
    fast     strip bounds-check aborts

CLI flags override the file: --emit llvm|c, --opt N, --fast, --release (opt s, no -g, stripped; `j3c size` shows what is in the binary; also writes [dist] build_outputs, `j3c doc ship`).
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
    J3_CAPTURE  out.png:N saves frame N (1-based), then quits (goldens)
    J3_FIXED_DT seconds per frame instead of measured time, so captures repeat
    SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy
                no window or sound: what `run --headless` sets. Set both on the
                same command when running a built binary. `j3c doc testing`
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
            name: "paint",
            what: "batched 2D shapes, text, clip, and transforms with j3.gfx.paint",
            text: RECIPE_PAINT,
            files: vec![("j3.toml", PAINT_TOML.to_string()), ("src/main.j3", PAINT_MAIN.to_string())],
        },
        Recipe {
            name: "display",
            what: "a Video settings page: window mode, resolution, monitor, vsync, frame cap, saved",
            text: RECIPE_DISPLAY,
            files: vec![("j3.toml", PAINT_TOML.replace("painted", "videomenu")), ("src/main.j3", DISPLAY_MAIN.to_string())],
        },
        Recipe {
            name: "components",
            what: "a `.j3ui` window: a component with a canvas and a button, styled with CSS",
            text: RECIPE_COMPONENTS,
            files: components_files("mygame"),
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
            name: "threads",
            what: "build chunks on a worker pool and accept only current results, no window",
            text: RECIPE_THREADS,
            files: vec![("j3.toml", "[package]\nname = \"chunks\"\nentry = \"src/main.j3\"\n".to_string()), ("src/main.j3", THREADS_MAIN.to_string())],
        },
        Recipe {
            name: "http",
            what: "call a web API on a worker thread: JSON body, https, reply read on the frame loop",
            text: RECIPE_HTTP,
            files: vec![("j3.toml", "[package]\nname = \"webcall\"\nentry = \"src/main.j3\"\n".to_string()), ("src/main.j3", HTTP_MAIN.to_string())],
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
`j3c new mygame --app` writes these files. Needs SDL2. `j3c run` opens a window; agents add
`--frames N --screenshot f.png --headless` (`j3c doc testing`).

- `App<World>.create(title, w, h, boot)` makes the window. `boot` runs once with the app's
  arena and returns the World; allocate from that arena, which lives until quit.
- Register systems before `run()`: `startup` once, `first`/`add`/`last` at a fixed 60 Hz,
  `always` every frame with dt, `draw` last (`j3c doc j3.app`).
- The window can resize; place things from `windowSize()` (`j3c doc j3.game windowSize`), or fix the size with `game.setVirtualSize(1920, 1080)` (letterboxed, `j3c doc j3.game setVirtualSize`).
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

/// The `components` recipe project (also what `j3c new --template ui` writes), for module `name`.
pub fn components_files(name: &str) -> Vec<(&'static str, String)> {
    let sub = |t: &str| t.replace("mygame", name);
    vec![
        ("j3.toml", sub(COMPONENTS_TOML)),
        ("ui/Screen.j3ui", sub(COMPONENTS_SCREEN)),
        ("ui/theme.css", COMPONENTS_THEME.to_string()),
        ("src/state.j3", sub(COMPONENTS_STATE)),
        ("src/main.j3", sub(COMPONENTS_MAIN)),
    ]
}

const RECIPE_COMPONENTS: &str = "\
`.j3ui` components are Svelte-like files: markup, a `<script>` in J3, and a scoped `<style>`.
`j3c` compiles every `ui/*.j3ui` to one J3 module `<package>.ui` (`j3c dump-ui` shows it).
Needs SDL2, SDL2_ttf, and libGL. `j3c new mygame --template ui` writes this project, including a
stock `assets/font.ttf` (theme.css loads it; add your own at that path when copying by hand). Full guide: `j3c doc j3ui`.

- A capitalised file is a component: `ui/Screen.j3ui` is `Screen` plus `ScreenProps`. The one nobody
  contains is the root; the host calls `root.render(windowSize(), props)` once per frame.
- `props { state: &State; }` is what the parent passes. A top-level `let` is per-instance state.
- Handlers are statements: `on:click={state.clicks += 1}`. `{expr}` in text and attributes is re-read
  every frame.
- `<canvas on:draw={...}>` draws with plain `j3.game` / `j3.gfx.paint` calls into its own box:
  `0,0` is its corner and `size` is its size. Children of a canvas are a HUD on top.
- `ui/theme.css` is global; a component's `<style>` applies to that component only. It sets the pointer
  cursor and a focus ring on buttons (`cursor`, `outline`); centre with `margin: 0 auto` or `margin: auto`
  in a flex box, and keep `aspect-ratio`, `:nth-child()`, `:not()` and `:last-child` for lists and cards.
- Dev runs (`j3c run`) reload styles when a `.j3ui` `<style>` or theme.css changes on disk. Markup,
  script and `.j3` edits need a rebuild: `j3c run --hot` does it and restarts (`j3c doc j3ui`, Dev loop).
- Check without a window: `j3c run --frames 5 --screenshot shot.png --headless`, then read the PNG.
  Goldens, `imgdiff`, and the HTML snapshot: `j3c doc testing`. CSS support: `j3c doc css`.
";

const COMPONENTS_TOML: &str = "\
[package]
name = \"mygame\"
entry = \"src/main.j3\"

[emit]
backend = \"llvm\"
opt = 0
debug = true

[link]
libs = [\"SDL2\", \"SDL2_ttf\", \"GL\"]

[ui]
dir = \"ui\"
theme = \"ui/theme.css\"
";

const COMPONENTS_SCREEN: &str = "\
<script>
    use mygame.state.State;
    use j3.game.{ drawRect, rgb, setColor };

    props { state: &State; }
</script>

<div class=\"app\">
    <canvas class=\"game\" on:draw={setColor(rgb(137, 180, 250)); drawRect(Vec2 { x: 20.0, y: 20.0 }, Vec2 { x: 20.0 + f32(state.clicks) * 10.0, y: 20.0 })}>
    </canvas>
    <button on:click={state.clicks += 1}>Clicked {state.clicks} times</button>
</div>

<style>
    .app { display: flex; flex-direction: column; gap: 12px; padding: 12px; height: 100%; }
    .game { flex: 1; background: var(--panel); border-radius: 10px; }
</style>
";

const COMPONENTS_THEME: &str = "\
@font-face {
  font-family: \"Ui\";
  src: url(\"../assets/font.ttf\");
}
* { margin: 0; padding: 0; box-sizing: border-box; }
:root {
  --bg: #11111b;
  --panel: #1e1e2e;
  --hover: #45475a;
  font-family: \"Ui\";
  font-size: 15px;
  color: #cdd6f4;
}
button { padding: 8px 14px; border: none; border-radius: 10px; background: var(--hover); color: #cdd6f4; font: inherit; cursor: pointer; }
button:hover { background: #585b70; }
button:focus-visible { outline: 2px solid #89b4fa; outline-offset: 2px; }
button:disabled { cursor: not-allowed; opacity: 0.5; }
";

const COMPONENTS_STATE: &str = "\
module mygame.state;

pub struct State {
    clicks: int = 0;
}
";

const COMPONENTS_MAIN: &str = "\
module mygame;

use mygame.state.State;
use mygame.ui.{ Screen, ScreenProps };
use j3.app.App;
use j3.game.windowSize;
use j3.memory.Arena;

struct World {
    state: State;
    root: Screen;
}

fn boot(mem: &Arena): World {
    return World { state: State {}, root: Screen.new(mem) };
}

fn draw(w: &World) {
    w.root.render(windowSize(), ScreenProps { state: &w.state });
}

fn main() {
    let app = App<World>.create(\"mygame\", 640, 360, boot);
    app.useGl();
    app.hiDpi();
    app.draw(draw);
    app.run();
}
";

const RECIPE_PAINT: &str = "\
`j3.gfx.paint` is a canvas-style 2D painter for games and HUDs: rects, rounded boxes with
borders, circles, rings, lines, text, images. Needs SDL2 and `app.useGl()` (it panics
without GL). Test with `j3c run --frames 5 --screenshot f.png --headless`, never a bare
`j3c run` (it opens a window); ship text with a .ttf in `assets/`.

- Every call queues one instance. Thousands of shapes and glyphs are one or a few draw calls;
  a texture change, a clip change, or `present` ends a batch. `j3.game.drawRect` and `drawText`
  share the batch, so mixing them keeps their order. Call `paint.flush()` only before raw GL.
- Coordinates are logical pixels from the top left, like `j3.game`. Colors are `j3.game.Color`.
- `pushClip` nests and intersects (axis-aligned). `pushTranslate`, `pushScale(origin, s)`,
  `pushAlpha`, and `pushView(pos, size)` each have a matching pop.
- Shapes are antialiased; `rect` is not, so it matches `drawRect` pixel for pixel.
- Main thread only. Next: `j3c doc j3.gfx.paint --short`.
";

const PAINT_TOML: &str = "\
[package]
name = \"painted\"
entry = \"src/main.j3\"

[link]
libs = [\"SDL2\", \"SDL2_ttf\"]
";

const PAINT_MAIN: &str = "\
module painted;

use j3.app.App;
use j3.game;
use j3.game.{ Font, Key, rgb };
use j3.gfx.paint;
use j3.gfx.paint.Corners;
use j3.math.Vec2;
use j3.memory.Arena;

struct World {
    font: Font;
    t: f32 = 0;
}

fn boot(_mem: &Arena): World {
    return World { font: Font { raw: 0 } };
}

// Put a .ttf here; an empty font (missing file) just skips the text.
const FONT_PATH: string = \"assets/font.ttf\";

fn loadFont(path: string): Font {
    return game.tryLoadFont(path, 18);
}

fn startup(w: &World) {
    w.font = loadFont(FONT_PATH);
}

fn input(w: &World, dt: f32) {
    w.t = w.t + dt;
    if game.keyDown(Key.Escape) {
        game.requestQuit();
    }
}

fn draw(w: &World) {
    paint.rect(Vec2 { x: 0, y: 0 }, Vec2 { x: 640, y: 360 }, rgb(30, 30, 46));
    let card = Corners { tl: 12, tr: 12, br: 12, bl: 12 };
    paint.roundRectEx(Vec2 { x: 20, y: 20 }, Vec2 { x: 280, y: 120 }, card, rgb(49, 50, 68), 2, rgb(137, 180, 250));
    paint.circle(Vec2 { x: 60, y: 60 }, 16, rgb(243, 139, 168));
    paint.ring(Vec2 { x: 110, y: 60 }, 16, 3, rgb(166, 227, 161));
    paint.line(Vec2 { x: 40, y: 110 }, Vec2 { x: 280, y: 90 + w.t * 10 }, 4, rgb(249, 226, 175));
    paint.pushClip(Vec2 { x: 320, y: 20 }, Vec2 { x: 120, y: 60 });
    paint.roundRect(Vec2 { x: 300, y: 10 }, Vec2 { x: 200, y: 100 }, 20, rgb(137, 180, 250));
    paint.popClip();
    if w.font.raw != 0 {
        paint.text(w.font, Vec2 { x: 150, y: 52 }, \"gold 120\", rgb(255, 255, 255));
    }
}

fn main() {
    let app = App<World>.create(\"painted\", 640, 360, boot);
    app.useGl();
    app.startup(startup);
    app.always(input);
    app.draw(draw);
    app.run();
}
";

const RECIPE_DISPLAY: &str = "\
Window settings every PC game needs. `j3.game` has the primitives (`setWindowMode`,
`setResolution`, `setVsync`, `setFpsLimit`, `windowFocused`, `displayCount`, `resolutionAt`);
`j3.game.display` is the menu layer: one `Settings` value, saved to a file.

- Bind a button or key to one call: `display.cycleMode(&v)`, `nextResolution(&v, 1)`,
  `nextDisplay`, `toggleVsync`, `nextFpsLimit`. They only edit `v`; `display.apply(&v)` makes the
  window match, `display.store(&v, path, mem)` saves. `toggleFullscreen(&v)` applies at once (F11).
- In `.j3ui`: `<button on:click={display.cycleMode(&w.video)}>{display.modeName(w.video.mode)}</button>`.
- `App.displaySettings(path)` loads and applies the file before the window opens, so a game saved
  in fullscreen never flashes a window. Missing file = the App's own size.
- Labels: `modeName`, `resolutionLabel(&v, &text)`, `fpsLabel(n, &text)`.
- Borderless is the desktop size (best alt-tab); Fullscreen switches the monitor to `v.w x v.h`.
- `game.windowFocused()` is false while another window has focus: pause or mute on it.
- Needs a real display. Under `--headless` the calls are no-ops, so test the logic with
  `stdlib/tests/display_test.j3` and the look by hand.
";

const DISPLAY_MAIN: &str = "\
module videomenu;

use j3.app.App;
use j3.game;
use j3.game.{ Font, Key, rgb };
use j3.game.display;
use j3.game.display.Settings;
use j3.gfx.paint;
use j3.math.Vec2;
use j3.memory.Arena;
use j3.text.scratchText;

const PATH: string = \"video.cfg\";
const ROWS: int = 5;

struct World {
    font: Font;
    video: Settings;
    row: int = 0;
    mem: &Arena;
}

fn boot(mem: &Arena): World {
    return World { font: Font { raw: 0 }, video: display.current(), mem: mem };
}

fn startup(w: &World) {
    w.font = game.tryLoadFont(\"assets/font.ttf\", 20);
    w.video = display.current();
}

// Up/Down pick a row, Left/Right change it, Enter saves, F11 toggles borderless fullscreen.
fn input(w: &World, _dt: f32) {
    if game.keyPressed(Key.Escape) { game.requestQuit(); }
    if game.keyPressed(Key.Down) { w.row = (w.row + 1) % ROWS; }
    if game.keyPressed(Key.Up) { w.row = (w.row + ROWS - 1) % ROWS; }
    let dir = 0;
    if game.keyPressed(Key.Right) { dir = 1; }
    if game.keyPressed(Key.Left) { dir = -1; }
    if dir != 0 {
        if w.row == 0 { display.cycleMode(&w.video); }
        if w.row == 1 { display.nextResolution(&w.video, dir); }
        if w.row == 2 { display.nextDisplay(&w.video, dir); }
        if w.row == 3 { display.toggleVsync(&w.video); }
        if w.row == 4 { display.nextFpsLimit(&w.video, dir); }
        display.apply(&w.video);
    }
    if game.keyPressed(Key.Enter) { display.store(&w.video, PATH, w.mem); }
    if game.keyPressed(Key.F11) { display.toggleFullscreen(&w.video); }
}

fn row(w: &World, i: int, name: string, value: string) {
    let y = 40.0 + f32(i) * 30.0;
    let c = if w.row == i { rgb(249, 226, 175) } else { rgb(205, 214, 244) };
    if w.font.raw != 0 {
        paint.text(w.font, Vec2 { x: 40, y: y }, name, c);
        paint.text(w.font, Vec2 { x: 240, y: y }, value, c);
    }
}

fn draw(w: &World) {
    paint.rect(Vec2 { x: 0, y: 0 }, game.windowSize(), rgb(30, 30, 46));
    let t = scratchText();
    let v = &w.video;
    row(w, 0, \"Window mode\", display.modeName(v.mode));
    row(w, 1, \"Resolution\", display.resolutionLabel(v, &t));
    row(w, 2, \"Monitor\", game.displayName(v.display));
    row(w, 3, \"VSync\", if v.vsync { \"On\" } else { \"Off\" });
    row(w, 4, \"Frame limit\", display.fpsLabel(v.fpsLimit, &t));
}

fn main() {
    let app = App<World>.create(\"videomenu\", 960, 540, boot);
    app.useGl();
    app.displaySettings(PATH);
    app.startup(startup);
    app.always(input);
    app.draw(draw);
    app.run();
}
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

const RECIPE_HTTP: &str = "\
A headless program that POSTs JSON over https from a worker thread while a loop that stands in
for the frame thread keeps running and picks the reply up with `tryPop`. Point it at a URL that
echoes JSON (`j3c run -- https://postman-echo.com/post`), or change it into your API call.
`examples/openrouter-chat` is a full one: an LLM chat request, token streaming, and curl.

The pieces:
- `j3.net.http`: `http.get(url, &arena)` / `http.post(url, body, &arena)` make a `Request`;
  `req.header(name, value)` adds headers; `http.request(&req, &arena)` returns a `Response`
  (`status`, `bodyStr()`, `header(name)`), any status counts as `Ok`. `http.open` returns a
  `Stream` instead, read with `read(buf)` or `readLine(&text)` as the body arrives (server-sent
  events, `data: {...}` lines). Errors are `HttpError`; `http.errorName(e)` names one.
- `https://` goes through `j3.net.tls` on the system OpenSSL (`libssl.so.3`, loaded on first use).
  The certificate and host name are always checked. On `HttpError.Tls`, `tls.lastError(&text)`
  says why (`certificate has expired`, `hostname mismatch`).
- `j3.data.json`: `json.parse(bytes, &arena)` gives a `&Json`; `get`, `at`, `path(\"a.0.b\")`
  never fail (a missing value is null, `asStr()` is \"\"). Build with `json.obj` / `arr` / `str` /
  `num` / `boolean` plus `set` / `push`, then `json.stringify`.
- Every call blocks (connect, TLS, the reply), up to `req.timeoutMs` per step. Never call it on the
  frame thread. Give the worker its own `Arena` and everything it needs in one struct, as in
  `j3c doc recipes threads`, and take the result back through a `Queue` with `tryPop`.
- A string a request keeps (a header value, the body) must outlive the call. A backtick string
  (`Bearer {key}`) lives in the stack frame of the function that made it, so build kept strings
  with `Text` on an arena.
- Lower level: `j3.net.tcp` (`dial`, `listen`), `j3.net.dns` (`resolve`). Another program instead
  of a library: `j3.proc.run([\"curl\", ...], stdin, &arena)` returns its exit code, stdout, and stderr.
- Linux for now; on Windows and macOS these calls compile and return errors.
";

const HTTP_MAIN: &str = "\
module webcall;

use j3.data.json;
use j3.io.printLn;
use j3.memory.Arena;
use j3.net.http;
use j3.net.http.HttpError;
use j3.net.tls;
use j3.option.Option;
use j3.result.Result;
use j3.sys;
use j3.text.Text;
use j3.thread.{ Queue, Thread, join, spawn };

/// What main hands the worker. Main does not touch it again until `done` says so.
struct Call {
    url: string;
    body: string;
    mem: &Arena;
    status: int;
    reply: string;
    done: &Queue<int>;
}

/// Runs on the worker: one blocking request, then report.
fn work(arg: ptr) {
    let c = arg as &Call;
    let req = http.post(c.url, c.body, c.mem);
    req.header(\"Content-Type\", \"application/json\");
    req.timeoutMs = 15000;
    match http.request(&req, c.mem) {
        Result.Ok(res) {
            c.status = res.status;
            c.reply = res.bodyStr();
        }
        Result.Err(e) {
            let t = Text.new(c.mem);
            t.pushStr(http.errorName(e));
            match e {
                HttpError.Tls {
                    t.pushStr(\": \");
                    tls.lastError(&t);
                }
                HttpError.BadUrl {}
                HttpError.BadRequest {}
                HttpError.Resolve {}
                HttpError.Connect {}
                HttpError.Timeout {}
                HttpError.Protocol {}
                HttpError.TooManyRedirects {}
            }
            c.status = -1;
            c.reply = t.str();
        }
    }
    c.done.push(1);
}

fn main() {
    let arena = Arena.create(1048576);
    let url = sys.arg(1);
    if url.len == 0 {
        url = \"https://postman-echo.com/post\";
    }

    let msg = json.obj(&arena);
    msg.set(\"player\", json.str(\"ada\", &arena));
    msg.set(\"score\", json.num(1250.0, &arena));

    // the worker allocates only from its own arena
    let workMem = Arena.create(1048576);
    let done = Queue<int>.create(&arena, 1);
    let call = Call { url: url, body: json.stringify(msg, &arena), mem: &workMem, status: 0, reply: \"\", done: &done };
    let th = Thread { f: work };
    spawn(&th, \"http\", work, &call as ptr);

    // the frame loop: never blocks, checks for the reply once per pass
    let frames = 0;
    let waiting = true;
    while waiting {
        frames += 1;
        match done.tryPop() {
            Option.Some(_) { waiting = false; }
            Option.None {}
        }
    }
    join(&th);

    if call.status < 0 {
        printLn(`failed: {call.reply}`);
        return;
    }
    printLn(`HTTP {call.status} after {frames} loop passes`);
    match json.parseStr(call.reply, &workMem) {
        Result.Ok(doc) {
            // postman-echo puts the parsed body under `json`
            let back = doc.get(\"json\");
            printLn(`echoed player={back.get(\"player\").asStr()} score={back.get(\"score\").asInt()}`);
        }
        Result.Err(e) { printLn(`not JSON: {e.msg}`); }
    }
}
";

const RECIPE_THREADS: &str = "\
A headless program: a `Pool` builds chunks off the main thread while a loop that stands in
for the frame thread feeds it and collects results without ever blocking. This is the shape
of world meshing, generation, decoding, and baking. `j3c doc threads` is the full guide
(rules, which modules are safe, common mistakes); `speculative/13_threads.j3` shows every
primitive.

The idea: hand work over, do not share it.
- A job is a plain `fn(ptr)` plus one `ptr` to a struct main owns. Put everything the job needs
  in that struct: its input, the buffer it alone writes, and the `&Queue` its result goes to.
  There are no closures. The struct must outlive the job (here: slots in an arena).
- A worker writes only what it was given. Main reads a result only after it comes back through
  the results `Queue` (or after `pool.wait()`). Nothing else orders threads; never spin on a
  plain flag. `@atomicAdd` / `@atomicLoad` are for counters and cancel flags.
- Never block the frame thread. `pool.trySubmit` returns false when the job queue is full:
  keep the job and try again next frame. `results.tryPop` returns a result or nothing. `submit`,
  `wait`, `push`, `pop`, and `join` can all wait; use them in loaders and tools.
- Size the results queue at least as large as the jobs in flight, or workers block in `push`
  until main drains it.
- Version stamps: each result carries the version it was built from. Main drops a result whose
  version is no longer current (here chunk 3 is edited, so its first build is stale). To cancel
  a running job, put a flag in its struct, set it with `@atomicStore`, and have the job check
  `@atomicLoad` between steps.
- GL, SDL, windows, and audio are main-thread only, and so is `j3.gfx` / `j3.game` / `j3.scene3d`
  state. Workers compute (`gfx.emitFace`, `voxel`, `noise`, `canvas`, `math`, `game.path` are
  safe); main uploads and draws.
- `memory.scratch` is per thread and a worker rewinds it after every job, so copy results into
  the struct main gave you. One `Arena` is never shared between threads.
- Results arrive in any order. If they feed saves or replays, reduce in a fixed order
  (`pool.parallelFor(n, f, ctx)` splits `0..n` for you; have `f` write `out[i]`).
- Call `pool.shutdown()` before destroying the arena the pool lives in.
";

const THREADS_MAIN: &str = "\
module chunks;

use j3.io.printLn;
use j3.memory.Arena;
use j3.option.Option;
use j3.thread.{ Pool, Queue, cores };

const CHUNKS: int = 8;
const CELLS: int = 4096;

/// What main hands a worker: a chunk, the version it is built from, its own buffer, and where
/// to send the result. It must stay alive until the job has run.
struct Work {
    chunk: int;
    version: int;
    cells: []int;
    done: &Queue<Done>;
}

/// What the worker hands back.
struct Done {
    chunk: int;
    version: int;
    sum: int;
}

/// Runs on a worker. Writes only `w.cells`, then reports.
fn build(arg: ptr) {
    let w = arg as &Work;
    let sum = 0;
    for i in 0..w.cells.len {
        w.cells[i] = (w.chunk * 31 + i * w.version) % 17;
        sum += w.cells[i];
    }
    w.done.push(Done { chunk: w.chunk, version: w.version, sum: sum });
}

fn main() {
    let arena = Arena.create(4 * 1024 * 1024);
    // A small job queue on purpose: `trySubmit` says no when it is full, and the loop below
    // just tries again on its next pass.
    let pool = Pool.create(&arena, cores(), 4);
    // At least as large as the jobs in flight, so a worker never blocks pushing its result.
    let results = Queue<Done>.create(&arena, 16);

    let jobs = CHUNKS + 1;
    let work = arena.alloc::<Work>(jobs);
    let current = arena.alloc::<int>(CHUNKS);
    for i in 0..CHUNKS {
        current[i] = 1;
        work[i] = Work { chunk: i, version: 1, cells: arena.alloc::<int>(CELLS), done: &results };
    }
    // Chunk 3 is edited right after its first build was queued: that result is now stale, and
    // a second build with its own buffer replaces it.
    current[3] = 2;
    work[CHUNKS] = Work { chunk: 3, version: 2, cells: arena.alloc::<int>(CELLS), done: &results };

    let next = 0;
    let accepted = 0;
    let stale = 0;
    let total = 0;
    // Each pass of this loop is one frame: it never waits on a worker.
    while accepted + stale < jobs {
        while next < jobs && pool.trySubmit(build, &work[next] as ptr) {
            next += 1;
        }
        while true {
            match results.tryPop() {
                Option.None {
                    break;
                }
                Option.Some(d) {
                    if d.version == current[d.chunk] {
                        accepted += 1;
                        total += d.sum;
                    } else {
                        stale += 1;
                    }
                }
            }
        }
    }
    printLn(`accepted={accepted} stale={stale} total={total}`);
    pool.shutdown();
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
                                  frame is saved and output ends with a J3STATS line.
                                  THE way to test a game: never run one without --headless
    J3_CAPTURE=out.png:30 J3_FIXED_DT=0.016 ./target/<name>   (with SDL_VIDEODRIVER=offscreen
                                  SDL_AUDIODRIVER=dummy) then `j3c -e imgdiff golden.png out.png`
                                  is a golden test: `j3c doc testing`

Paths: `--out` must be inside the project; `--screenshot` is relative to where you ran
j3c. The game's stdout and stderr come back to you. The editor's Play writes build errors
to target/j3-editor/build.log. The MCP server (`j3c -e mcp`) has the same steps as the
check, render, and play tools.
";
