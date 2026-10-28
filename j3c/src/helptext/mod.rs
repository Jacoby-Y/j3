//! Text for `j3c guide` and `j3c help manifest`. The editor schema in
//! `editors/vscode/toml.js` matches these keys.

pub const GUIDE: &str = include_str!("guide.md");

pub const MANIFEST: &str = include_str!("manifest.md");

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

const RECIPE_APP2D: &str = include_str!("recipes/app2d.md");

const RECIPE_SCENE3D: &str = include_str!("recipes/scene3d.md");

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

const RECIPE_COMPONENTS: &str = include_str!("recipes/components.md");

const COMPONENTS_TOML: &str = include_str!("recipes/components.toml");

const COMPONENTS_SCREEN: &str = include_str!("recipes/components_Screen.j3ui");

const COMPONENTS_THEME: &str = include_str!("recipes/components_theme.css");

const COMPONENTS_STATE: &str = include_str!("recipes/components_state.j3");

const COMPONENTS_MAIN: &str = include_str!("recipes/components_main.j3");

const RECIPE_PAINT: &str = include_str!("recipes/paint.md");

const PAINT_TOML: &str = include_str!("recipes/paint.toml");

const PAINT_MAIN: &str = include_str!("recipes/paint_main.j3");

const RECIPE_DISPLAY: &str = include_str!("recipes/display.md");

const DISPLAY_MAIN: &str = include_str!("recipes/display_main.j3");

const RECIPE_CANVAS: &str = include_str!("recipes/canvas.md");

const CANVAS_MAIN: &str = include_str!("recipes/canvas_main.j3");

const RECIPE_HTTP: &str = include_str!("recipes/http.md");

const HTTP_MAIN: &str = include_str!("recipes/http_main.j3");

const RECIPE_THREADS: &str = include_str!("recipes/threads.md");

const THREADS_MAIN: &str = include_str!("recipes/threads_main.j3");

const RECIPE_SCRIPT: &str = include_str!("recipes/script.md");

const SCRIPT_COUNTER: &str = include_str!("recipes/script_counter.j3s");

const SCRIPT_MAIN: &str = include_str!("recipes/script_main.j3");

const RECIPE_MESHGEN: &str = include_str!("recipes/meshgen.md");

const RECIPE_MATERIAL: &str = include_str!("recipes/material.md");

const RECIPE_URP: &str = include_str!("recipes/urp.md");

const RECIPE_CHECK: &str = include_str!("recipes/check.md");

