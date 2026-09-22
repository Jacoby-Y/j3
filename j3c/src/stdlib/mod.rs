//! J3 stdlib lives in `stdlib/j3/`. This module only finds that directory.

pub fn stdlib_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("J3_STDLIB") {
        return std::path::PathBuf::from(dir);
    }
    let baked = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stdlib");
    if baked.join("j3").is_dir() {
        return baked;
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let beside = dir.join("stdlib");
            if beside.join("j3").is_dir() {
                return beside;
            }
        }
    }
    baked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdlib_j3_files_exist() {
        let dir = stdlib_dir().join("j3");
        for name in [
            "sys.j3",
            "sys_windows.j3",
            "sys_macos.j3",
            "io.j3",
            "math.j3",
            "rand.j3",
            "memory.j3",
            "option.j3",
            "result.j3",
            "text.j3",
            "game.j3",
            "game_headless.j3",
            "ecs.j3",
            "app.j3",
            "collide.j3",
            "res.j3",
            "scene.j3",
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
            let p = dir.join(name);
            assert!(p.is_file(), "missing {}", p.display());
        }
    }
}
