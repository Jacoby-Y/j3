//! Left literals, for loops and struct field defaults.

use super::*;

#[test]
fn left_literal_takes_right_numeric_type() {
    let src = "\
module t;
fn scale(far: f32): f32 {
    return 2 * far;
}
fn below(far: f32): bool {
    return 0 < far;
}
fn keep(): float {
    return 2.0 + 2;
}
fn main() {}
";
    check::check_files(vec![parse_file(1, "t.j3", src)]).expect("left literal");
}

#[test]
fn inclusive_for_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/programs/inclusive_for.j3");
    let prog = compile_path(&path).expect("compile inclusive_for");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "0123\n");
}

#[test]
fn for_index_ends_with_the_loop() {
    let src = "\
module t;
fn main() {
    let n = 0;
    for i in 0..2 {
        n = n + i;
    }
    for i in 0..2 {
        n = n + i;
    }
}
";
    check::check_files(vec![parse_file(1, "t.j3", src)]).expect("two fors");
}

#[test]
fn let_then_for_still_shadows() {
    let src = "\
module t;
fn main() {
    let i = 0;
    for i in 0..2 {
    }
}
";
    let err = match check::check_files(vec![parse_file(1, "t.j3", src)]) {
        Ok(_) => panic!("expected shadowing"),
        Err(e) => e,
    };
    assert!(err.message.contains("shadowing"), "{}", err.message);
}

#[test]
fn default_fields_fill_omitted() {
    let src = "\
module t;
struct Hit {
    ok: bool = false;
    x: int = 0;
}
fn main() {
    let a = Hit {};
    let b = Hit { ok: true };
    if a.ok || a.x != 0 || !b.ok || b.x != 0 {
        return;
    }
}
";
    check::check_files(vec![parse_file(1, "t.j3", src)]).expect("defaults");
}

#[test]
fn omitted_hit_texture_mesh_use_zeros() {
    let path = std::env::temp_dir().join(format!(
        "j3c_omitted_hit_texture_mesh_{}.j3",
        std::process::id()
    ));
    let src = "\
module t;
use j3.gfx.voxel.Hit;
use j3.game.Texture;
use j3.gfx.Mesh;
use j3.io.printLn;
fn main() {
    let h = Hit {};
    let tex = Texture {};
    let mesh = Mesh {};
    if h.ok || h.x != 0 || h.face != 0 {
        printLn(\"hit\");
    }
    if tex.w != 0 || tex.h != 0 {
        printLn(\"tex\");
    }
    if mesh.count != 0 || mesh.vao != 0 {
        printLn(\"mesh\");
    }
}
";
    std::fs::write(&path, src).expect("write temp");
    let result = compile_path(&path);
    let _ = std::fs::remove_file(&path);
    let prog = result.expect("compile");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "");
}

#[test]
fn missing_field_without_default_is_error() {
    let src = "\
module t;
struct Pair {
    a: int = 0;
    b: int;
}
fn main() {
    let p = Pair {};
}
";
    let err = match check::check_files(vec![parse_file(1, "t.j3", src)]) {
        Ok(_) => panic!("expected missing field"),
        Err(e) => e,
    };
    assert!(err.message.contains("missing field `b`"), "{}", err.message);
}

#[test]
fn generic_field_default_is_error() {
    let src = "\
module t;
struct Box<T> {
    v: T = 0;
}
fn main() {}
";
    let err = match check::check_files(vec![parse_file(1, "t.j3", src)]) {
        Ok(_) => panic!("expected generic default error"),
        Err(e) => e,
    };
    assert!(
        err.message.contains("default on a generic field"),
        "{}",
        err.message
    );
}

#[test]
fn non_literal_field_default_is_error() {
    let src = "\
module t;
struct Hit {
    x: int = 1 + 2;
}
fn main() {}
";
    let err = match check::check_files(vec![parse_file(1, "t.j3", src)]) {
        Ok(_) => panic!("expected literal default error"),
        Err(e) => e,
    };
    assert!(
        err.message.contains("field default must be a literal"),
        "{}",
        err.message
    );
}

#[test]
fn two_literals_still_default_from_the_left() {
    let src = "\
module t;
fn main() {
    let x = 2 + 2.0;
}
";
    let err = match check::check_files(vec![parse_file(1, "t.j3", src)]) {
        Ok(_) => panic!("expected arithmetic mismatch"),
        Err(e) => e,
    };
    assert!(
        err.message.contains("type mismatch"),
        "{}",
        err.message
    );
}
