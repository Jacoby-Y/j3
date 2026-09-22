//! Stdlib sources. `stdlib/j3/*.j3` is the source of truth, and those files
//! are compiled into the binary. `J3_STDLIB` replaces them with a directory.

use std::path::{Path, PathBuf};

const FILES: &[(&str, &str)] = &[
    ("anim.j3", include_str!("../../stdlib/j3/anim.j3")),
    ("app.j3", include_str!("../../stdlib/j3/app.j3")),
    ("assets.j3", include_str!("../../stdlib/j3/assets.j3")),
    ("collide.j3", include_str!("../../stdlib/j3/collide.j3")),
    ("debug.j3", include_str!("../../stdlib/j3/debug.j3")),
    ("ecs.j3", include_str!("../../stdlib/j3/ecs.j3")),
    ("fx.j3", include_str!("../../stdlib/j3/fx.j3")),
    ("game.j3", include_str!("../../stdlib/j3/game.j3")),
    ("game_headless.j3", include_str!("../../stdlib/j3/game_headless.j3")),
    ("gfx.j3", include_str!("../../stdlib/j3/gfx.j3")),
    ("input.j3", include_str!("../../stdlib/j3/input.j3")),
    ("io.j3", include_str!("../../stdlib/j3/io.j3")),
    ("math.j3", include_str!("../../stdlib/j3/math.j3")),
    ("memory.j3", include_str!("../../stdlib/j3/memory.j3")),
    ("mesh.j3", include_str!("../../stdlib/j3/mesh.j3")),
    ("option.j3", include_str!("../../stdlib/j3/option.j3")),
    ("path.j3", include_str!("../../stdlib/j3/path.j3")),
    ("physics.j3", include_str!("../../stdlib/j3/physics.j3")),
    ("rand.j3", include_str!("../../stdlib/j3/rand.j3")),
    ("res.j3", include_str!("../../stdlib/j3/res.j3")),
    ("result.j3", include_str!("../../stdlib/j3/result.j3")),
    ("save.j3", include_str!("../../stdlib/j3/save.j3")),
    ("scene.j3", include_str!("../../stdlib/j3/scene.j3")),
    ("space.j3", include_str!("../../stdlib/j3/space.j3")),
    ("sys.j3", include_str!("../../stdlib/j3/sys.j3")),
    ("sys_macos.j3", include_str!("../../stdlib/j3/sys_macos.j3")),
    ("sys_windows.j3", include_str!("../../stdlib/j3/sys_windows.j3")),
    ("text.j3", include_str!("../../stdlib/j3/text.j3")),
    ("tile.j3", include_str!("../../stdlib/j3/tile.j3")),
    ("ui.j3", include_str!("../../stdlib/j3/ui.j3")),
    ("voxel.j3", include_str!("../../stdlib/j3/voxel.j3")),
];

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
    let name = path.file_name()?.to_str()?;
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
            "anim.j3",
            "sys.j3",
            "sys_windows.j3",
            "sys_macos.j3",
            "input.j3",
            "io.j3",
            "math.j3",
            "rand.j3",
            "memory.j3",
            "option.j3",
            "path.j3",
            "physics.j3",
            "result.j3",
            "text.j3",
            "game.j3",
            "game_headless.j3",
            "ecs.j3",
            "app.j3",
            "collide.j3",
            "res.j3",
            "scene.j3",
            "space.j3",
            "assets.j3",
            "tile.j3",
            "ui.j3",
            "save.j3",
            "fx.j3",
            "debug.j3",
            "gfx.j3",
            "mesh.j3",
            "voxel.j3",
        ] {
            assert!(names.contains(&name), "missing {name}");
        }
        let sys = embedded(&stdlib_dir().join("j3").join("sys.j3")).expect("sys");
        assert!(sys.contains("module j3.sys"), "{sys}");
    }
}
