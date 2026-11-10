//! Stdlib modules typecheck and their check programs run.

use super::*;

#[test]
fn stdlib_game_j3_typechecks() {
    let game = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stdlib/j3/game.j3");
    let prog = check_path_lib(&game).expect("check shipped j3.game");
    assert!(
        prog.modules.iter().any(|m| {
            m.name.len() == 2
                && prog.cx.name_str(m.name[0]) == "j3"
                && prog.cx.name_str(m.name[1]) == "game"
        }),
        "expected j3.game module"
    );
    let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
    assert!(c.contains("use lib \"SDL2\""), "{c}");

    check_path_lib(&speculative("09_stdlib_game.j3")).expect("check 09");
}

#[test]
fn stdlib_option_result_typecheck() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stdlib/j3");
    check_path_lib(&dir.join("option.j3")).expect("check j3.option");
    check_path_lib(&dir.join("result.j3")).expect("check j3.result");
    check_path_lib(&dir.join("rand.j3")).expect("check j3.rand");
    check_path_lib(&dir.join("math.j3")).expect("check j3.math");
    check_path_lib(&dir.join("game/ecs.j3")).expect("check j3.game.ecs");
    check_path_lib(&dir.join("app.j3")).expect("check j3.app");
    check_path_lib(&dir.join("game/collide.j3")).expect("check j3.game.collide");
    check_path_lib(&dir.join("data/res.j3")).expect("check j3.data.res");
    check_path_lib(&dir.join("data/scene.j3")).expect("check j3.data.scene");
    check_path_lib(&dir.join("data/save.j3")).expect("check j3.data.save");
    check_path_lib(&dir.join("game/tile.j3")).expect("check j3.game.tile");
    check_path_lib(&dir.join("game/fx.j3")).expect("check j3.game.fx");
    check_path_lib(&dir.join("data/debug.j3")).expect("check j3.data.debug");
    check_path_lib(&dir.join("ui.j3")).expect("check j3.ui");
    check_path_lib(&speculative("10_j3ui.j3")).expect("check 10 j3ui");
    check_path_lib(&dir.join("data/assets.j3")).expect("check j3.data.assets");
    check_path_lib(&dir.join("gfx.j3")).expect("check j3.gfx");
    check_path_lib(&dir.join("gfx/mesh.j3")).expect("check j3.gfx.mesh");
    check_path_lib(&dir.join("gfx/voxel.j3")).expect("check j3.gfx.voxel");
}

#[test]
fn math_rand_j3_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/programs/math_rand.j3");
    let prog = compile_path(&path).expect("compile math_rand");
    let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}\nc:\n{c}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "n=12\n");
}

#[test]
fn mesh_fmt_j3_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/programs/mesh.j3");
    let prog = compile_path(&path).expect("compile mesh");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
}

#[test]
fn scene3d_check_j3_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/programs/scene3d_check.j3");
    let prog = compile_path(&path).expect("compile scene3d_check");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
}

#[test]
fn fpwalk_check_j3_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/programs/fpwalk_check.j3");
    let prog = compile_path(&path).expect("compile fpwalk_check");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
}

#[test]
fn cull_check_j3_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/programs/cull_check.j3");
    let prog = compile_path(&path).expect("compile cull_check");
    let run = exec(&prog);
    assert_eq!(run.status, 0, "stderr {}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
}

#[test]
fn bvh_check_j3_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/programs/bvh_check.j3");
    let prog = compile_path(&path).expect("compile bvh_check");
    let run = exec(&prog);
    assert_eq!(run.status, 0, "stderr {}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
}

#[test]
fn png_check_j3_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/programs/png_check.j3");
    let prog = compile_path(&path).expect("compile png_check");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
}

#[test]
fn ray_aabb_j3_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/programs/ray_aabb.j3");
    let prog = compile_path(&path).expect("compile ray_aabb");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
}

#[test]
fn voxel_grid_j3_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/programs/voxel_grid.j3");
    let prog = compile_path(&path).expect("compile voxel_grid");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
}

#[test]
fn voxel_ao_j3_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/programs/voxel_ao.j3");
    let prog = compile_path(&path).expect("compile voxel_ao");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
}

#[test]
fn ecs_basic_j3_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/programs/ecs_basic.j3");
    let prog = compile_path(&path).expect("compile ecs_basic");
    let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}\nc:\n{c}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
}
