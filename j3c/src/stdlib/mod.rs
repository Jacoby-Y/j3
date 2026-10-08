//! Stdlib sources. `stdlib/j3/*.j3` is the source of truth, and those files
//! are compiled into the binary. `J3_STDLIB` replaces them with a directory.

use std::path::{Path, PathBuf};

const FILES: &[(&str, &str)] = &[
    ("game/anim.j3", include_str!("../../stdlib/j3/game/anim.j3")),
    ("app.j3", include_str!("../../stdlib/j3/app.j3")),
    ("data/assets.j3", include_str!("../../stdlib/j3/data/assets.j3")),
    ("gfx/canvas.j3", include_str!("../../stdlib/j3/gfx/canvas.j3")),
    ("game/collide.j3", include_str!("../../stdlib/j3/game/collide.j3")),
    ("data/debug.j3", include_str!("../../stdlib/j3/data/debug.j3")),
    ("game/ecs.j3", include_str!("../../stdlib/j3/game/ecs.j3")),
    ("game/fx.j3", include_str!("../../stdlib/j3/game/fx.j3")),
    ("game.j3", include_str!("../../stdlib/j3/game.j3")),
    ("scene3d/fpwalk.j3", include_str!("../../stdlib/j3/scene3d/fpwalk.j3")),
    ("scene3d/fx.j3", include_str!("../../stdlib/j3/scene3d/fx.j3")),
    ("game_headless.j3", include_str!("../../stdlib/j3/game_headless.j3")),
    ("gfx.j3", include_str!("../../stdlib/j3/gfx.j3")),
    ("game/input.j3", include_str!("../../stdlib/j3/game/input.j3")),
    ("io.j3", include_str!("../../stdlib/j3/io.j3")),
    ("math.j3", include_str!("../../stdlib/j3/math.j3")),
    ("memory.j3", include_str!("../../stdlib/j3/memory.j3")),
    ("gfx/mesh.j3", include_str!("../../stdlib/j3/gfx/mesh.j3")),
    ("gfx/meshgen.j3", include_str!("../../stdlib/j3/gfx/meshgen.j3")),
    ("option.j3", include_str!("../../stdlib/j3/option.j3")),
    ("game/path.j3", include_str!("../../stdlib/j3/game/path.j3")),
    ("game/physics.j3", include_str!("../../stdlib/j3/game/physics.j3")),
    ("rand.j3", include_str!("../../stdlib/j3/rand.j3")),
    ("data/res.j3", include_str!("../../stdlib/j3/data/res.j3")),
    ("result.j3", include_str!("../../stdlib/j3/result.j3")),
    ("data/save.j3", include_str!("../../stdlib/j3/data/save.j3")),
    ("data/scene.j3", include_str!("../../stdlib/j3/data/scene.j3")),
    ("scene3d.j3", include_str!("../../stdlib/j3/scene3d.j3")),
    ("script.j3", include_str!("../../stdlib/j3/script.j3")),
    ("gfx/noise.j3", include_str!("../../stdlib/j3/gfx/noise.j3")),
    ("gfx/bvh.j3", include_str!("../../stdlib/j3/gfx/bvh.j3")),
    ("game/space.j3", include_str!("../../stdlib/j3/game/space.j3")),
    ("sys.j3", include_str!("../../stdlib/j3/sys.j3")),
    ("sys_macos.j3", include_str!("../../stdlib/j3/sys_macos.j3")),
    ("sys_windows.j3", include_str!("../../stdlib/j3/sys_windows.j3")),
    ("text.j3", include_str!("../../stdlib/j3/text.j3")),
    ("thread.j3", include_str!("../../stdlib/j3/thread.j3")),
    ("threadsys.j3", include_str!("../../stdlib/j3/threadsys.j3")),
    ("threadsys_windows.j3", include_str!("../../stdlib/j3/threadsys_windows.j3")),
    ("game/tile.j3", include_str!("../../stdlib/j3/game/tile.j3")),
    ("ui.j3", include_str!("../../stdlib/j3/ui.j3")),
    ("gfx/urp/raytrace.j3", include_str!("../../stdlib/j3/gfx/urp/raytrace.j3")),
    ("gfx/urp/forest.j3", include_str!("../../stdlib/j3/gfx/urp/forest.j3")),
    ("gfx/urp/shaders.j3", include_str!("../../stdlib/j3/gfx/urp/shaders.j3")),
    ("scene3d/editor/overrides.j3", include_str!("../../stdlib/j3/scene3d/editor/overrides.j3")),
    ("scene3d/editor/props.j3", include_str!("../../stdlib/j3/scene3d/editor/props.j3")),
    ("scene3d/editor/paste.j3", include_str!("../../stdlib/j3/scene3d/editor/paste.j3")),
    ("scene3d/editor/uses.j3", include_str!("../../stdlib/j3/scene3d/editor/uses.j3")),
    ("gfx/voxel.j3", include_str!("../../stdlib/j3/gfx/voxel.j3")),
    ("net.j3", include_str!("../../stdlib/j3/net.j3")),
    ("netsys.j3", include_str!("../../stdlib/j3/netsys.j3")),
    ("netsys_macos.j3", include_str!("../../stdlib/j3/netsys_macos.j3")),
    ("netsys_windows.j3", include_str!("../../stdlib/j3/netsys_windows.j3")),
];

/// A short hash of what `j3c doc`, `guide`, and help print in this build: the embedded stdlib
/// and the Rust files holding the other pages. Two binaries with the same id print the same docs.
pub fn docs_id() -> String {
    const PAGES: &[&str] = &[
        include_str!("../doc.rs"),
        include_str!("../helptext.rs"),
        include_str!("../cli.rs"),
        include_str!("../editor.rs"),
        include_str!("../scaffold.rs"),
        include_str!("../scene.rs"),
        include_str!("../mcp.rs"),
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
        for name in [
            "game/anim.j3",
            "gfx/canvas.j3",
            "sys.j3",
            "sys_windows.j3",
            "sys_macos.j3",
            "net.j3",
            "netsys.j3",
            "netsys_windows.j3",
            "netsys_macos.j3",
            "threadsys.j3",
            "threadsys_windows.j3",
            "game/input.j3",
            "io.j3",
            "math.j3",
            "rand.j3",
            "memory.j3",
            "option.j3",
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
        ] {
            assert!(names.contains(&name), "missing {name}");
        }
        let sys = embedded(&stdlib_dir().join("j3").join("sys.j3")).expect("sys");
        assert!(sys.contains("module j3.sys"), "{sys}");
    }
}
