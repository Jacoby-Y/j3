//! `j3c new` — j3.toml, src/main.j3, AGENTS.md, and CLAUDE.md (imports AGENTS.md) for an empty directory.

use std::path::{Path, PathBuf};

use crate::fmt;

mod templates;
pub(crate) use templates::{app_main, app_toml};
use templates::*;

pub fn cmd(rest: &[String]) -> i32 {
    let mut app = false;
    let mut lib = false;
    let mut template: Option<&str> = None;
    let mut name: Option<&str> = None;
    let mut args = rest.iter();
    while let Some(a) = args.next() {
        if a == "--template" {
            match args.next().map(String::as_str) {
                Some("fps") => template = Some("fps"),
                Some("ui") => template = Some("ui"),
                other => {
                    eprintln!("j3c: unknown template `{}`", other.unwrap_or(""));
                    eprintln!("  = help: templates: fps, ui");
                    return 1;
                }
            }
        } else if a == "--app" {
            app = true;
        } else if a == "--lib" {
            lib = true;
        } else if a == "--help" || a == "-h" {
            println!("usage: j3c new [name] [--app | --lib | --template fps|ui]");
            println!("    j3c new mygame        printLn hello in ./mygame");
            println!("    j3c new mygame --app  SDL window (links SDL2)");
            println!("    j3c new mygame --template fps   first-person 3D game with a scene (SDL2 + GL)");
            println!("    j3c new mygame --template ui    .j3ui components window (SDL2 + ttf + GL; `j3c doc j3ui`)");
            println!("    j3c new               write into the current directory");
            println!("    j3c new --app         window template in the current directory");
            println!("    j3c new hello --lib   a package other projects import (`use hello;`; `j3c doc packages`)");
            println!("    j3c new --lib         make the project here a package (adds [lib]; moves nothing)");
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
    if lib {
        if app || template.is_some() {
            eprintln!("j3c: `--lib` goes alone (a package has no window or main)");
            return 1;
        }
        return match create_lib(name) {
            Ok((root, module, made)) => {
                println!("{}", root.display());
                for f in made {
                    println!("  {f}");
                }
                println!("next: j3c check; j3c run examples/demo");
                println!("use it: [dependencies] {module} = \"path/to/{module}\" (or {{ git = \"URL\", tag = \"v0.1.0\" }}), then `use {module};`");
                0
            }
            Err(e) => {
                eprintln!("j3c: {e}");
                1
            }
        };
    }
    if template.is_some() {
        if app {
            eprintln!("j3c: `--app` and `--template` don't go together");
            return 1;
        }
        let ui = template == Some("ui");
        return match if ui { create_ui(name) } else { create_fps(name) } {
            Ok(root) => {
                println!("{}", root.display());
                if ui {
                    println!("next: j3c check; j3c run --frames 30 --screenshot shot.png --headless");
                } else {
                    println!("next: j3c check; j3c editor to edit scenes/main.j3scene");
                }
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

/// The `components` recipe project in a new directory (`--template ui`), plus AGENTS.md.
pub fn create_ui(name: Option<&str>) -> Result<PathBuf, String> {
    let (root, package) = locate(name)?;
    if root.join("j3.toml").is_file() || root.join("AGENTS.md").is_file() {
        return Err(format!("j3.toml or AGENTS.md already exists in {}", root.display()));
    }
    for (path, text) in crate::helptext::components_files(&module_name(&package)) {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).map_err(|e| format!("mkdir: {e}"))?;
        std::fs::write(&file, text).map_err(|e| format!("write {path}: {e}"))?;
    }
    // The editor's own font, so text draws out of the box. Swap it for yours at the same path.
    std::fs::create_dir_all(root.join("assets")).map_err(|e| format!("mkdir: {e}"))?;
    std::fs::write(root.join("assets/font.ttf"), crate::editor::FONT).map_err(|e| format!("write assets/font.ttf: {e}"))?;
    std::fs::write(root.join("assets/LICENSE-DejaVu.txt"), include_str!("../editor/LICENSE-DejaVu.txt"))
        .map_err(|e| format!("write assets/LICENSE-DejaVu.txt: {e}"))?;
    write_agent_files(&root, false)?;
    let agents = root.join("AGENTS.md");
    let text = std::fs::read_to_string(&agents).map_err(|e| format!("read AGENTS.md: {e}"))?;
    std::fs::write(&agents, format!("{text}\n{UI_COMPONENTS}")).map_err(|e| format!("write AGENTS.md: {e}"))?;
    Ok(root)
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

/// `j3c new [name] --lib`: a package (`[lib]`, `src/<name>.j3`, `examples/demo`). In a project that
/// already has a `j3.toml`, only adds `[lib]` and the files that are missing; nothing is moved.
pub fn create_lib(name: Option<&str>) -> Result<(PathBuf, String, Vec<String>), String> {
    let (root, package) = locate(name)?;
    let toml_path = root.join("j3.toml");
    let mut made = Vec::new();
    let module = if toml_path.is_file() {
        let text = std::fs::read_to_string(&toml_path).map_err(|e| format!("read j3.toml: {e}"))?;
        let m = crate::project::toml_str(&text, "package", "name").ok_or("j3.toml has no [package] name")?;
        if m != module_name(&m) {
            return Err(format!("package name `{m}` is not a module name (letters, digits, `_`); rename it in j3.toml first"));
        }
        if !text.lines().any(|l| l.trim() == "[lib]") {
            let sep = if text.ends_with('\n') { "" } else { "\n" };
            std::fs::write(&toml_path, format!("{text}{sep}\n# A package: `use {m};` loads src/{m}.j3 (`j3c doc packages`).\n[lib]\n")).map_err(|e| format!("write j3.toml: {e}"))?;
            made.push("j3.toml (+ [lib])".to_string());
        }
        m
    } else {
        let m = module_name(&package);
        std::fs::create_dir_all(&root).map_err(|e| format!("mkdir: {e}"))?;
        std::fs::write(&toml_path, lib_toml(&m)).map_err(|e| format!("write j3.toml: {e}"))?;
        made.push("j3.toml".to_string());
        m
    };
    let files = [
        (format!("src/{module}.j3"), lib_main(&module)),
        ("examples/demo/j3.toml".to_string(), lib_demo_toml(&module)),
        ("examples/demo/src/main.j3".to_string(), lib_demo_main(&module)),
        ("README.md".to_string(), lib_readme(&module)),
    ];
    for (rel, text) in files {
        let p = root.join(&rel);
        if p.exists() {
            continue;
        }
        let text = if rel.ends_with(".j3") { fmt::format_src(&text).map_err(|e| format!("fmt template: {e}"))? } else { text };
        std::fs::create_dir_all(p.parent().unwrap_or(&root)).map_err(|e| format!("mkdir: {e}"))?;
        std::fs::write(&p, text).map_err(|e| format!("write {rel}: {e}"))?;
        made.push(rel);
    }
    if !root.join("AGENTS.md").exists() {
        write_agent_files(&root, false)?;
        made.push("AGENTS.md".to_string());
    }
    Ok((root, module, made))
}

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

#[cfg(test)]
mod tests;
