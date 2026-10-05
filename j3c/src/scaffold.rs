//! `j3c new` — j3.toml, src/main.j3, AGENTS.md, and CLAUDE.md (imports AGENTS.md) for an empty directory.

use std::path::{Path, PathBuf};

use crate::fmt;

pub fn cmd(rest: &[String]) -> i32 {
    let mut app = false;
    let mut template: Option<&str> = None;
    let mut name: Option<&str> = None;
    let mut args = rest.iter();
    while let Some(a) = args.next() {
        if a == "--template" {
            match args.next().map(String::as_str) {
                Some("fps") => template = Some("fps"),
                other => {
                    eprintln!("j3c: unknown template `{}`", other.unwrap_or(""));
                    eprintln!("  = help: templates: fps");
                    return 1;
                }
            }
        } else if a == "--app" {
            app = true;
        } else if a == "--help" || a == "-h" {
            println!("usage: j3c new [name] [--app | --template fps]");
            println!("    j3c new mygame        printLn hello in ./mygame");
            println!("    j3c new mygame --app  SDL window (links SDL2)");
            println!("    j3c new mygame --template fps   first-person 3D game with a scene (SDL2 + GL)");
            println!("    j3c new               write into the current directory");
            println!("    j3c new --app         window template in the current directory");
            return 0;
        } else if a.starts_with('-') {
            eprintln!("j3c: unknown flag `{a}`");
            eprintln!("  = help: `j3c new --help`");
            return 1;
        } else if name.is_none() {
            name = Some(a.as_str());
        } else {
            eprintln!("j3c: unexpected argument `{a}`");
            return 1;
        }
    }
    if template.is_some() {
        if app {
            eprintln!("j3c: `--app` and `--template` don't go together");
            return 1;
        }
        return match create_fps(name) {
            Ok(root) => {
                println!("{}", root.display());
                println!("next: j3c check; j3c editor to edit scenes/main.j3scene");
                0
            }
            Err(e) => {
                eprintln!("j3c: {e}");
                1
            }
        };
    }
    match create(name, app) {
        Ok((root, module)) => {
            println!("{}", root.display());
            println!("  j3.toml");
            println!("  src/main.j3");
            println!("  AGENTS.md");
            println!("  CLAUDE.md");
            let dir = root
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            if dir != module {
                println!("  module {module}");
            }
            if app {
                println!("next: j3c check   (run needs SDL2)");
            } else {
                println!("next: j3c check");
            }
            0
        }
        Err(e) => {
            eprintln!("j3c: {e}");
            1
        }
    }
}

pub fn create(name: Option<&str>, app: bool) -> Result<(PathBuf, String), String> {
    let (root, package) = locate(name)?;
    let module = module_name(&package);
    if root.join("j3.toml").is_file()
        || root.join("src/main.j3").is_file()
        || root.join("AGENTS.md").is_file()
    {
        return Err(format!(
            "j3.toml, src/main.j3, or AGENTS.md already exists in {}",
            root.display()
        ));
    }
    std::fs::create_dir_all(root.join("src")).map_err(|e| format!("mkdir: {e}"))?;
    let (toml, main) = if app {
        (app_toml(&module), app_main(&module))
    } else {
        (hello_toml(&module), hello_main(&module))
    };
    let main = fmt::format_src(&main).map_err(|e| format!("fmt template: {e}"))?;
    std::fs::write(root.join("j3.toml"), toml).map_err(|e| format!("write j3.toml: {e}"))?;
    std::fs::write(root.join("src/main.j3"), main).map_err(|e| format!("write src/main.j3: {e}"))?;
    write_agent_files(&root, false)?;
    Ok((root, module))
}

/// The editor's project files with a first-person main (`--template fps`), plus AGENTS.md.
pub fn create_fps(name: Option<&str>) -> Result<PathBuf, String> {
    let (root, _) = locate(name)?;
    let root = if root.is_absolute() {
        root
    } else {
        std::env::current_dir().map_err(|e| format!("cwd: {e}"))?.join(root)
    };
    if root.join("AGENTS.md").is_file() {
        return Err(format!("AGENTS.md already exists in {}", root.display()));
    }
    std::fs::create_dir_all(&root).map_err(|e| format!("mkdir: {e}"))?;
    // The editor project writes AGENTS.md and CLAUDE.md with it.
    crate::editor::create_fps_project(&root)?;
    Ok(root)
}

/// `AGENTS.md` (plus the 3D scene and agent-tool section for an editor project) and a
/// `CLAUDE.md` that imports it, because Claude Code reads only `CLAUDE.md`. An existing
/// `CLAUDE.md` is left alone.
pub(crate) fn write_agent_files(root: &Path, scenes: bool) -> Result<(), String> {
    let text = if scenes { format!("{AGENTS}\n{SCENES}") } else { AGENTS.to_string() };
    std::fs::write(root.join("AGENTS.md"), text).map_err(|e| format!("write AGENTS.md: {e}"))?;
    if !root.join("CLAUDE.md").exists() {
        std::fs::write(root.join("CLAUDE.md"), "@AGENTS.md\n").map_err(|e| format!("write CLAUDE.md: {e}"))?;
    }
    Ok(())
}

/// `j3c editor` on a project with an `AGENTS.md` that predates the editor: add the scene
/// section once.
pub(crate) fn add_scenes_section(root: &Path) {
    let path = root.join("AGENTS.md");
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    if text.contains(SCENES_MARKER) { return; }
    let _ = std::fs::write(&path, format!("{}\n\n{SCENES}", text.trim_end()));
}

const SCENES_MARKER: &str = "## 3D scenes and the editor";

const SCENES: &str = r#"## 3D scenes and the editor

Scenes are `scenes/*.j3scene` text files that `j3c editor` edits and the game loads with `j3.scene3d` (`j3c doc recipes scene3d`, `j3c doc editor`). Edit them through the tools below, not by hand: they keep ids, parents, and undo correct. Nothing here needs the editor window open.

MCP server (once): `claude mcp add -s user j3 -- j3c -e mcp`. Its startup instructions and tool descriptions say when to use each tool. Without MCP, the same operations are `j3c -e scene --help`, `j3c -e render --help`, `j3c -e gen`, and `j3c -e mcp --tools` (every tool's arguments next to its CLI twin).

Loop: change, look, check.

1. Learn the scene: `scene_list` (glob `filter`, `group_by=prefix`, `tree` for the scene file's own lines), `scene_get` (an object and its children as file lines).
2. Build with bulk tools, not many single calls: `place` (`ring`, `wall` with a door gap, `stairs`, `spiral`, `grid`, `scatter`, `ground`, `ontop`), `scene_add_many`, `scene_apply` (many varied objects, or a subtree you copied from `scene_get`, written as scene lines), and globs in `scene_set` / `scene_delete` (`DecoTree*`). `scene_split` / `scene_inline` move objects into their own scene file and back. Meshes built in code: `assets/gen/*.j3` (`j3c doc recipes meshgen`).
3. Look from where the player stands: `render` with `at` / `yaw` / `pitch`, `views` for several angles in one image, `hide` (`Roof*`) to see inside a building, `view=top` with `ortho` for a plan. Render after every few edits, not once at the end.
4. Verify: `check` (scene problems, this project's own rules, and code), `scene_stats` (draw cost: above 60 distinct looks is slow, so snap tints to a few values), `scene_diff` (review your own edits against git HEAD), `play` (runs the game headless and returns a frame).

Facts that cost time:

- A scene file is one line per object (`Name kind key=value #id`, indent for children, defaults left out) and rotations are degrees there and in the tools; `j3c doc editor` has the format. Edit through the tools; after a hand edit run `j3c -e scene fmt`. `check` warns on a rotation over two turns. Yaw 0 looks toward -z, 90 toward -x. A slab pointing along `(dx, dz)` needs `rot` y `= -atan2(dz, dx)` degrees.
- With `j3c editor` open, tool edits are live undo steps there and unsaved until the person saves. With it closed they write the file and `undo` restores it (20 kept). Every reply says which.
- Give objects `tags` (`scene_set tags=nocollide,launch=30`) and read them with `scene3d.hasTag` / `tagValue`, instead of matching name prefixes.
- Put this project's rules in a check script so a mistake names the object instead of showing up in play: `check = "tools/check.j3s"` under `[editor]` in `j3.editor.toml`. It runs as `j3c run tools/check.j3s <scene>` and prints `error: ...` or `warning: ...` lines (read the scene with `scene3d.read`). Project-specific notes for agents go in a file named by `notes = "AGENT_NOTES.md"` there; the MCP server adds it to its instructions.
"#;

const AGENTS: &str = r#"# This project

Learn J3 from `j3c` only: `j3c guide`, `j3c doc`, `j3c help`. Do not search the machine, home directory, or other projects to figure out the language.

- Start from the task: `j3c doc` lists recipes and modules by what you're building; `j3c doc recipes <name>` prints a whole program that checks.
- Find a name: `j3c doc search <word>`, or `j3c doc <name>`. A module on one screen: `j3c doc <module> --short`.
- Heavy work off the frame thread (meshing, generation, decoding): `j3c doc recipes threads`, then `j3c doc threads`. Window, GL, SDL, and `j3.gfx`/`j3.game`/`j3.scene3d` state are main-thread only; workers compute and hand results back.
- Check your work: `j3c check`, then `j3c run --frames 120 --screenshot shot.png --headless` and look at the picture (`j3c doc recipes check`).
- 3D scenes and the editor: `j3c doc recipes scene3d`, `j3c doc editor`. `j3c editor` on this project adds the scene-tool guide here.

## Shape

- `src/main.j3` is the entry. `fn main` sets up the app and returns. Game code lives in other files.
- One file, one job. Split a module when it starts doing two things.
- One way to do a thing. No copied blocks, no long if/else chains. Name the case and call it.
- Build the smallest thing that works, and leave a place to add detail later. Do not invent extra systems, settings, or polish.

## Scripts

Per-object logic that you edit while the game runs goes in a `.j3s` script (`j3c doc lang`, Scripts). A script is J3 with a `props` block (read-only, set by whoever makes the instance), per-instance `let` state, `shared` values, and `pub fn`s. Make instances from J3 with `const Door = @script("scripts/door.j3s");` and `Door { hinge: h }`, or attach a script to an object in a scene (`script=scripts/door.j3s` on its line, props as `param.<prop>=value` beside it) and run them with `j3.scene3d.scripts` (`start`, `update`, `scene.script::<Door>(node)`).

`j3c run` never recompiles the game for a script edit, and swaps new script code into the running game. Keep engine-level code (drawing, input plumbing, data structures) in `.j3`; keep tunable gameplay in scripts.

## Screen

The window can resize, including fullscreen. Pick one and keep it:

- Fixed: draw into a constant pixel size. `setMinSize` / `setMaxSize` can lock it (`j3c doc j3.game`).
- Ratio: that same layout, scaled to the window (`j3c doc j3.game windowSize`).
- Fluid: place things from the current size so windowed and fullscreen both read well.

Do not lock drawing to the size passed to `App.create` and ignore later resizes.

## Programmatic art

Use `j3c doc j3.gfx.canvas` for `Canvas`. Generate sprites, tiles, icons, and textures in J3 without a window. `Mode.Pixel` keeps exact pixel edges; `Mode.Smooth` antialiases circles, lines, and triangles. Build once during startup, call `game.textureFromCanvas`, draw the resulting texture each frame, and destroy it at shutdown. For changing art, call `game.updateTextureFromCanvas` with only the changed rectangle. `Canvas.savePng` writes a PNG from a headless J3 program into an existing directory.

    use j3.gfx.canvas;
    use j3.gfx.canvas.{ Canvas, Mode };
    use j3.game;
    use j3.game.Texture;
    use j3.memory.Arena;

    fn makeSprite(arena: &Arena): Texture {
        let sprite = Canvas.create(arena, 64, 64, Mode.Pixel);
        sprite.clear(canvas.rgba(0, 0, 0, 0));
        sprite.fillCircle(32, 32, 20, canvas.rgb(235, 176, 75));
        return game.textureFromCanvas(sprite);
    }

`j3.gfx.canvas` colors are for image pixels; `j3.game` colors are for screen drawing. The texture keeps its uploaded pixels after the canvas arena is released.

Make graphics readable at the size they appear in game. Start with a clear silhouette and a small color palette. Pick one light direction; add a darker side, a lit side, and a few highlights. Separate the subject from the background with value and contrast. Add noise sparingly after the main shapes read well. Use a fixed seed so regeneration is repeatable. Preview at native size and at the size used in game. Avoid rebuilding or uploading a static texture every frame.

For large generated images, build with `[emit] opt = 2` and measure the drawing work before using `--fast`. Normal builds keep pixel bounds checks.

## UI

Write each screen as its own `@j3ui` component so `draw` stays a short list of calls. Which names take a body: `j3c doc j3ui`. Each widget (`label`, `button`, `buttonOff`, `toggle`, `check`, `slider`, `range`, `stepper`, `scroll`, `card`, and the rest): `j3c doc j3.ui`.

    @j3ui title(ui: &Ui) {
        panel(280, 120) {
            label("title");
            button("play") {
                start();
            }
        }
    }

    fn draw(w: &World) {
        let ui = Ui.begin(w.font, x, y);
        ui.theme = Theme.dusk();
        title(&ui);
    }

`begin` opens a downward column (width 220, row height 28). Inside `@j3ui`, a bare widget name is a method on the first parameter. `panel`, `row`, `scroll`, and `card` close themselves. `button` runs its body on the click. `buttonOff` never clicks. Set `ui.theme` after `begin` (`Theme.dusk`, `Theme.yard`, or a few colors of your own). Place `x` and `y` from `windowSize` so a panel stays put when the window changes. Two widgets with the same caption need `pushId` / `popId`.

## Fonts

`Ui.begin` takes a font. Load a `.ttf` that lives in the project (`assets/font.ttf`).

`tryLoadFont` returns an empty font when the file is missing or SDL_ttf is not installed. Labels still draw. `loadFont` panics. The same split holds for images (`tryLoadTexture` / `loadTexture`) and sounds (`tryLoadSound` / `loadSound`).

`j3c check` does not run the game. It fails when a library or a file the program loads is missing on this machine. `j3c run` uses that check and does not launch when it fails. `j3c: program exited` is a failed run, even when a window appeared. Read stderr.
"#;

fn locate(name: Option<&str>) -> Result<(PathBuf, String), String> {
    match name {
        None | Some(".") => {
            let cwd = std::env::current_dir().map_err(|e| format!("cwd: {e}"))?;
            let package = cwd
                .file_name()
                .and_then(|s| s.to_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "current directory has no name\n  = help: `j3c new mygame`".to_string())?
                .to_string();
            Ok((cwd, package))
        }
        Some(p) => {
            let root = PathBuf::from(p);
            let package = root
                .file_name()
                .and_then(|s| s.to_str())
                .filter(|s| !s.is_empty() && *s != "." && *s != "..")
                .ok_or_else(|| format!("`{p}` has no package name"))?
                .to_string();
            Ok((root, package))
        }
    }
}

/// Directory label → module name. `my-game` and `My Game` become `my_game`.
fn module_name(raw: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    for c in raw.chars() {
        if c.is_alphanumeric() {
            if gap && !out.is_empty() {
                out.push('_');
            }
            gap = false;
            for lower in c.to_lowercase() {
                out.push(lower);
            }
        } else {
            gap = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.chars().next().is_some_and(|c| !c.is_alphabetic() && c != '_') {
        out.insert(0, '_');
    }
    if out.is_empty() || out.chars().all(|c| c == '_') {
        return "game".to_string();
    }
    if crate::parse::keyword(&out).is_some() {
        out.push('_');
    }
    out
}

fn hello_toml(name: &str) -> String {
    format!(
        r#"# Hello. From this directory: j3c check / j3c run
# A window instead: j3c new --app  (see j3c guide)

[package]
name = "{name}"
entry = "src/main.j3"

[emit]
backend = llvm
opt = 0
debug = true
# Dev loop. Ship with `j3c build --release` (opt s, no -g, stripped: about 3x smaller).
"#
    )
}

fn hello_main(name: &str) -> String {
    format!(
        r#"module {name};

use j3.io.printLn;

fn main() {{
    printLn("Hello, world!");
}}
"#
    )
}

pub(crate) fn app_toml(name: &str) -> String {
    format!(
        r#"# SDL window. Needs SDL2.
# Image, mixer, and font libraries load the first time you use them.
# A missing library leaves the handle empty and does not close the window.
# From this directory: j3c check / j3c run
# Left and Right move the bar. Escape quits.

[package]
name = "{name}"
entry = "src/main.j3"

[emit]
backend = llvm
opt = 0
debug = true
# Dev loop. Ship with `j3c build --release` (opt s, no -g, stripped: about 3x smaller).

[link]
libs = ["SDL2"]
"#
    )
}

pub(crate) fn app_main(name: &str) -> String {
    format!(
        r#"module {name};

use j3.app.App;
use j3.game.{{ Key, clear, drawRect, keyDown, requestQuit, rgb, setColor }};
use j3.math.Vec2;
use j3.memory.Arena;

struct World {{
    x: f32;
}}

fn boot(_mem: &Arena): World {{
    return World {{ x: 280 }};
}}

fn input(w: &World, dt: f32) {{
    if keyDown(Key.Escape) {{
        requestQuit();
    }}
    if keyDown(Key.Left) {{
        w.x = w.x - f32(200) * dt;
    }}
    if keyDown(Key.Right) {{
        w.x = w.x + f32(200) * dt;
    }}
}}

fn draw(w: &World) {{
    setColor(rgb(32, 48, 72));
    clear();
    setColor(rgb(240, 200, 64));
    drawRect(Vec2 {{ x: w.x, y: f32(160) }}, Vec2 {{ x: f32(80), y: f32(24) }});
}}

fn main() {{
    let app = App<World>.create("{name}", 640, 360, boot);
    app.always(input);
    app.draw(draw);
    app.run();
}}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> PathBuf {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("j3-new-{n}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn hello_project_typechecks() {
        let dir = scratch();
        let root = dir.join("hello");
        let (err, module) = create(Some(root.to_str().unwrap()), false).expect("create");
        assert_eq!(module, "hello");
        assert!(err.join("j3.toml").is_file());
        let src = std::fs::read_to_string(err.join("src/main.j3")).unwrap();
        assert!(src.contains("module hello;"), "{src}");
        assert!(src.contains("printLn"), "{src}");
        let agents = std::fs::read_to_string(err.join("AGENTS.md")).unwrap();
        assert!(agents.contains("Learn J3 from `j3c` only"), "{agents}");
        assert!(agents.contains("@j3ui"), "{agents}");
        assert!(agents.contains("j3c doc j3ui"), "{agents}");
        assert!(agents.contains("j3c doc j3.gfx.canvas"), "{agents}");
        assert!(agents.contains("textureFromCanvas"), "{agents}");
        assert!(agents.contains("buttonOff"), "{agents}");
        assert!(!agents.contains(SCENES_MARKER), "plain projects skip the scene section");
        assert_eq!(std::fs::read_to_string(err.join("CLAUDE.md")).unwrap(), "@AGENTS.md\n");
        assert!(agents.contains("tryLoadFont"), "{agents}");
        assert!(agents.contains("program exited"), "{agents}");
        let code = crate::cli::run(["check".into(), err.display().to_string()]);
        assert_eq!(code, 0);
        let again = create(Some(root.to_str().unwrap()), false).unwrap_err();
        assert!(again.contains("already exists"), "{again}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn app_project_typechecks() {
        let dir = scratch();
        let root = dir.join("window");
        let (made, _) = create(Some(root.to_str().unwrap()), true).expect("create");
        let toml = std::fs::read_to_string(made.join("j3.toml")).unwrap();
        assert!(toml.contains("libs = [\"SDL2\"]"), "{toml}");
        assert!(toml.contains("does not close the window"), "{toml}");
        let code = crate::cli::run(["check".into(), made.display().to_string()]);
        assert_eq!(code, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fpwalk_template_typechecks() {
        let dir = scratch();
        let made = create_fps(Some(dir.join("walk").to_str().unwrap())).expect("create");
        for file in ["scripts/spin.j3s", "scenes/main.j3scene", "j3.editor.toml", "AGENTS.md", "CLAUDE.md"] {
            assert!(made.join(file).is_file(), "{file}");
        }
        let agents = std::fs::read_to_string(made.join("AGENTS.md")).unwrap();
        for want in [SCENES_MARKER, "scene_stats", "place", "degrees", "claude mcp add", "hasTag", "check = "] {
            assert!(agents.contains(want), "{want}");
        }
        // An older AGENTS.md gets the section once.
        std::fs::write(made.join("AGENTS.md"), "# old\n").unwrap();
        add_scenes_section(&made);
        add_scenes_section(&made);
        let agents = std::fs::read_to_string(made.join("AGENTS.md")).unwrap();
        assert_eq!(agents.matches(SCENES_MARKER).count(), 1, "{agents}");
        assert!(agents.starts_with("# old"), "{agents}");
        let main = std::fs::read_to_string(made.join("src/main.j3")).unwrap();
        assert!(main.contains("drawFrame"), "{main}");
        let code = crate::cli::run(["check".into(), made.display().to_string()]);
        assert_eq!(code, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn directory_name_becomes_a_module() {
        assert_eq!(module_name("my-game"), "my_game");
        assert_eq!(module_name("My Game"), "my_game");
        assert_eq!(module_name("bounce"), "bounce");
        assert_eq!(module_name("123go"), "_123go");
        assert_eq!(module_name("fn"), "fn_");
        assert_eq!(module_name("!!!"), "game");
        let dir = scratch();
        let root = dir.join("my-game");
        let (made, module) = create(Some(root.to_str().unwrap()), false).expect("create");
        assert_eq!(module, "my_game");
        let src = std::fs::read_to_string(made.join("src/main.j3")).unwrap();
        assert!(src.contains("module my_game;"), "{src}");
        let toml = std::fs::read_to_string(made.join("j3.toml")).unwrap();
        assert!(toml.contains("name = \"my_game\""), "{toml}");
        let code = crate::cli::run(["check".into(), made.display().to_string()]);
        assert_eq!(code, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
