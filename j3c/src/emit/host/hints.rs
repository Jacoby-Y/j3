//! Failure hints: which package to install when a build or link error names a missing library.

use std::path::Path;

use super::*;

/// Extra line for a failed `run`/`build` that is not a J3 diagnostic.
/// Names a package when the log mentions a known library or a missing compiler.
pub fn failure_hint(text: &str) -> Option<String> {
    if text.contains("-->") {
        return None;
    }
    let mut pkgs: Vec<&str> = Vec::new();
    let mut push = |pkg: &'static str| {
        if !pkgs.contains(&pkg) {
            pkgs.push(pkg);
        }
    };
    if text.contains("SDL2_image") || text.contains("libSDL2_image") || text.contains("IMG_") {
        push("libsdl2-image-2.0-0");
    }
    if text.contains("SDL2_mixer") || text.contains("libSDL2_mixer") || text.contains("Mix_") {
        push("libsdl2-mixer-2.0-0");
    }
    if text.contains("SDL2_ttf") || text.contains("libSDL2_ttf") || text.contains("TTF_") {
        push("libsdl2-ttf-2.0-0");
    }
    if sdl2_core(text) {
        push("libsdl2-2.0-0");
    }
    if text.contains("library `GL`") || text.contains("-lGL") || text.contains("libGL") {
        push("libgl1");
    }
    let mut lines: Vec<String> = Vec::new();
    if !pkgs.is_empty() {
        let list = pkgs.join(" ");
        lines.push(format!("install {list}\n         `sudo apt install {list}`"));
    }
    if text.contains("spawn clang")
        || text.contains("spawn cc")
        || text.contains("clang: not found")
        || text.contains("cc: not found")
    {
        lines.push("install clang\n         `sudo apt install clang`".to_string());
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n  = help: "))
    }
}

/// `SDL2` itself, not `SDL2_image` / `SDL2_mixer` / `SDL2_ttf`.
fn sdl2_core(text: &str) -> bool {
    let mut rest = text;
    while let Some(i) = rest.find("SDL2") {
        let after = rest[i + 4..].chars().next();
        if after != Some('_') {
            return true;
        }
        rest = &rest[i + 4..];
    }
    false
}

pub(super) fn find_soname(dir: &Path, name: &str) -> Option<String> {
    let prefix = format!("lib{name}");
    let entries = fs::read_dir(dir).ok()?;
    let mut best: Option<PathBuf> = None;
    for ent in entries.flatten() {
        let p = ent.path();
        let fname = match p.file_name().and_then(|n| n.to_str()) {
            Some(s) => s,
            None => continue,
        };
        let dashed = format!("{prefix}-");
        let dotted = format!("{prefix}.so");
        if (fname.starts_with(&dashed) || fname.starts_with(&dotted)) && fname.contains(".so") {
            best = Some(p);
            break;
        }
    }
    best.map(|p| p.display().to_string())
}

/// The error for a compiler or linker that did not start. A missing one is a one-line fix.
/// `cc` is the C-backend fallback, only tried once clang is already known to be missing.
pub(crate) fn spawn_err(tool: &str, e: std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::NotFound {
        let tool = if tool == "cc" { "clang (or cc)" } else { tool };
        format!("error: {tool} not found on PATH; install clang (Debian/Ubuntu: sudo apt install clang)")
    } else {
        format!("spawn {tool}: {e}")
    }
}
