//! Stdlib sources. `stdlib/j3/*.j3` is the source of truth, and those files
//! are compiled into the binary. `J3_STDLIB` replaces them with a directory.

use std::path::{Path, PathBuf};

mod files;
use files::FILES;

/// A short hash of what `j3c doc`, `guide`, and help print in this build: the embedded stdlib
/// and the Rust files holding the other pages. Two binaries with the same id print the same docs.
pub fn docs_id() -> String {
    const PAGES: &[&str] = &[
        include_str!("../doc/mod.rs"),
        include_str!("../doc/pages.rs"),
        include_str!("../doc/pages/j3ui.md"),
        include_str!("../doc/pages/css.md"),
        include_str!("../doc/pages/testing.md"),
        include_str!("../doc/pages/threads.md"),
        include_str!("../doc/pages/i18n.md"),
        include_str!("../doc/pages/ship.md"),
        include_str!("../doc/pages/packages.md"),
        include_str!("../doc/pages/android.md"),
        include_str!("../doc/pages/tasks.md"),
        include_str!("../doc/pages/j2.md"),
        include_str!("../helptext/mod.rs"),
        include_str!("../helptext/guide.md"),
        include_str!("../helptext/manifest.md"),
        include_str!("../helptext/recipes/app2d.md"),
        include_str!("../helptext/recipes/audio_main.j3"),
        include_str!("../helptext/recipes/audio.md"),
        include_str!("../helptext/recipes/canvas_main.j3"),
        include_str!("../helptext/recipes/canvas.md"),
        include_str!("../helptext/recipes/check.md"),
        include_str!("../helptext/recipes/components_main.j3"),
        include_str!("../helptext/recipes/components.md"),
        include_str!("../helptext/recipes/components_Screen.j3ui"),
        include_str!("../helptext/recipes/components_state.j3"),
        include_str!("../helptext/recipes/components_theme.css"),
        include_str!("../helptext/recipes/components.toml"),
        include_str!("../helptext/recipes/display_main.j3"),
        include_str!("../helptext/recipes/display.md"),
        include_str!("../helptext/recipes/http_main.j3"),
        include_str!("../helptext/recipes/http.md"),
        include_str!("../helptext/recipes/j2.md"),
        include_str!("../helptext/recipes/j2_hello.j2"),
        include_str!("../helptext/recipes/j2_main.j3"),
        include_str!("../helptext/recipes/material.md"),
        include_str!("../helptext/recipes/meshgen.md"),
        include_str!("../helptext/recipes/paint_main.j3"),
        include_str!("../helptext/recipes/paint.md"),
        include_str!("../helptext/recipes/paint.toml"),
        include_str!("../helptext/recipes/scene3d.md"),
        include_str!("../helptext/recipes/script_counter.j3s"),
        include_str!("../helptext/recipes/script_main.j3"),
        include_str!("../helptext/recipes/script.md"),
        include_str!("../helptext/recipes/threads_main.j3"),
        include_str!("../helptext/recipes/threads.md"),
        include_str!("../helptext/recipes/urp.md"),
        include_str!("../cli/mod.rs"),
        include_str!("../editor/mod.rs"),
        include_str!("../editor/templates.rs"),
        include_str!("../scaffold.rs"),
        include_str!("../scene/mod.rs"),
        include_str!("../mcp/mod.rs"),
        include_str!("../mcp/schema.rs"),
        include_str!("../gen.rs"),
        include_str!("../../editor/README.md"),
        include_str!("../../stdlib/j3/gfx/urp/README.md"),
    ];
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for text in FILES.iter().flat_map(|(name, src)| [*name, *src]).chain(PAGES.iter().copied()) {
        for b in text.bytes() {
            h = (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
        }
    }
    format!("{:08x}", h >> 32)
}

/// Where this build reads the stdlib from, and its docs id.
pub fn provenance() -> String {
    if using_override() {
        format!("stdlib from J3_STDLIB={}, docs {}", stdlib_dir().display(), docs_id())
    } else {
        format!("stdlib embedded, docs {}", docs_id())
    }
}

/// True when `J3_STDLIB` names a directory that replaces the embedded sources.
pub fn using_override() -> bool {
    match std::env::var("J3_STDLIB") {
        Ok(dir) if !dir.is_empty() => true,
        _ => false,
    }
}

/// Parent of `j3/`. With no override this is only the path key for the
/// embedded files. The directory does not have to exist.
pub fn stdlib_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("J3_STDLIB") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stdlib")
}

pub fn files() -> impl Iterator<Item = (&'static str, &'static str)> {
    FILES.iter().copied()
}

/// Embedded source when `path` is `stdlib_dir()/j3/<file>` and `J3_STDLIB` is unset.
pub fn embedded(path: &Path) -> Option<&'static str> {
    if using_override() {
        return None;
    }
    if !path.starts_with(stdlib_dir()) {
        return None;
    }
    let name = path.strip_prefix(stdlib_dir().join("j3")).ok()?.to_str()?;
    FILES.iter().find(|(n, _)| *n == name).map(|(_, src)| *src)
}

/// The extra files of a module that is spread over several: `dir` is the module path without
/// `.j3` (`.../ui/dom` for `ui/dom.j3`). Embedded when it is under the stdlib and `J3_STDLIB` is
/// unset, else `*.j3` on disk. Sorted, so the merge order is stable. The caller keeps the ones
/// whose `module` line names the main module.
pub fn parts(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !using_override() {
        if let Ok(rel) = dir.strip_prefix(stdlib_dir().join("j3")) {
            let Some(rel) = rel.to_str() else { return out };
            let prefix = format!("{rel}/");
            for (name, _) in FILES {
                if name.strip_prefix(&prefix).is_some_and(|rest| !rest.contains('/')) {
                    out.push(stdlib_dir().join("j3").join(name));
                }
            }
            out.sort();
            return out;
        }
    }
    if let Ok(ents) = std::fs::read_dir(dir) {
        for ent in ents.flatten() {
            let p = ent.path();
            if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("j3") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Embedded source, or the file on disk.
pub fn read(path: &Path) -> Option<String> {
    if let Some(src) = embedded(path) {
        return Some(src.to_string());
    }
    std::fs::read_to_string(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_files_match_the_tree() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stdlib/j3");
        let mut names = Vec::new();
        for (name, src) in files() {
            names.push(name);
            let disk = std::fs::read_to_string(dir.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(disk, src, "{name}");
            assert!(src.contains("module j3."), "{name}");
        }
        // Every stdlib file must be embedded: a missing one still loads from the source tree
        // (`stdlib_dir` falls back to CARGO_MANIFEST_DIR) but breaks a copied binary.
        let mut stack = vec![dir.clone()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "j3") {
                    let rel = p.strip_prefix(&dir).unwrap().to_string_lossy().replace('\\', "/");
                    assert!(names.contains(&rel.as_str()), "{rel} is not in FILES (src/stdlib/files.rs)");
                }
            }
        }
        for name in [
            "game/anim.j3",
            "gfx/canvas.j3",
            "sys.j3",
            "sys_windows.j3",
            "sys_macos.j3",
            "sys_android.j3",
            "net.j3",
            "netsys.j3",
            "netsys_windows.j3",
            "netsys_macos.j3",
            "netsys_android.j3",
            "instance.j3",
            "instance_other.j3",
            "net/dns.j3",
            "net/tcp.j3",
            "net/tls.j3",
            "net/http.j3",
            "proc.j3",
            "term.j3",
            "term/ansi.j3",
            "term/width.j3",
            "term/grid.j3",
            "term/pty.j3",
            "term/input.j3",
            "term/tty.j3",
            "term/screen.j3",
            "term/view.j3",
            "data/json.j3",
            "threadsys.j3",
            "threadsys_windows.j3",
            "threadsys_android.j3",
            "game/input.j3",
            "io.j3",
            "math.j3",
            "rand.j3",
            "memory.j3",
            "option.j3",
            "cmp.j3",
            "j2.j3",
            "j2/lex.j3",
            "j2/parse.j3",
            "data/map.j3",
            "game/path.j3",
            "game/physics.j3",
            "result.j3",
            "text.j3",
            "game.j3",
            "game_headless.j3",
            "game/ecs.j3",
            "app.j3",
            "game/collide.j3",
            "data/res.j3",
            "data/scene.j3",
            "game/space.j3",
            "game/letterbox.j3",
            "game/display.j3",
            "game/hotkey.j3",
            "data/assets.j3",
            "game/tile.j3",
            "ui.j3",
            "data/save.j3",
            "game/fx.j3",
            "data/debug.j3",
            "gfx.j3",
            "gfx/urp/raytrace.j3",
            "gfx/urp/forest.j3",
            "gfx/urp/shaders.j3",
            "gfx/mesh.j3",
            "gfx/voxel.j3",
            "gfx/paint.j3",
            "gfx/post.j3",
            "gfx/path.j3",
            "gfx/clip.j3",
            "gfx/nineslice.j3",
            "ui/ansi.j3",
            "ui/cssimage.j3",
            "ui/imgfit.j3",
            "ui/media.j3",
            "ui/css.j3",
            "ui/csslex.j3",
            "ui/cssvalue.j3",
            "ui/csscalc.j3",
            "ui/cssmotion.j3",
            "ui/cssprop.j3",
            "ui/cssparse.j3",
            "ui/cssmatch.j3",
            "ui/keyed.j3",
            "ui/dom.j3",
            "ui/domhot.j3",
            "ui/domhtml.j3",
            "ui/panelmath.j3",
            "ui/panels.j3",
            "ui/sticky.j3",
            "ui/textfit.j3",
            "ui/textanim.j3",
            "gfx/textfx.j3",
        ] {
            assert!(names.contains(&name), "missing {name}");
        }
        let sys = embedded(&stdlib_dir().join("j3").join("sys.j3")).expect("sys");
        assert!(sys.contains("module j3.sys"), "{sys}");
    }
}
