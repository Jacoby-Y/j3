//! `j3c new` — j3.toml, src/main.j3, and AGENTS.md for an empty directory.

use std::path::PathBuf;

use crate::fmt;

pub fn cmd(rest: &[String]) -> i32 {
    let mut app = false;
    let mut name: Option<&str> = None;
    for a in rest {
        if a == "--app" {
            app = true;
        } else if a == "--help" || a == "-h" {
            println!("usage: j3c new [name] [--app]");
            println!("    j3c new mygame        printLn hello in ./mygame");
            println!("    j3c new mygame --app  SDL window (links SDL2)");
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
    match create(name, app) {
        Ok((root, module)) => {
            println!("{}", root.display());
            println!("  j3.toml");
            println!("  src/main.j3");
            println!("  AGENTS.md");
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
    std::fs::write(root.join("AGENTS.md"), AGENTS).map_err(|e| format!("write AGENTS.md: {e}"))?;
    Ok((root, module))
}

const AGENTS: &str = r#"# This project

Learn J3 from `j3c` only: `j3c guide`, `j3c doc`, `j3c help`. Do not search the machine, home directory, or other projects to figure out the language.

## Shape

- `src/main.j3` is the entry. `fn main` sets up the app and returns. Game code lives in other files.
- One file, one job. Split a module when it starts doing two things.
- One way to do a thing. No copied blocks, no long if/else chains. Name the case and call it.
- Build the smallest thing that works, and leave a place to add detail later. Do not invent extra systems, settings, or polish.

## Screen

The window can resize, including fullscreen. Pick one and keep it:

- Fixed: draw into a constant pixel size. `setMinSize` / `setMaxSize` can lock it (`j3c doc j3.game`).
- Ratio: that same layout, scaled to the window (`j3c doc j3.game windowSize`).
- Fluid: place things from the current size so windowed and fullscreen both read well.

Do not lock drawing to the size passed to `App.create` and ignore later resizes.

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

fn app_toml(name: &str) -> String {
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

[link]
libs = ["SDL2"]
"#
    )
}

fn app_main(name: &str) -> String {
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
        assert!(agents.contains("buttonOff"), "{agents}");
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
