//! Gap fixes: rem, consts, array consts, short-circuit and rvalue bases on both backends.

use super::*;

#[test]
fn gaps_rem_runs_on_both_backends() {
    run_both_ok(&honesty("rem.j3"), "ok\n");
}

#[test]
fn gaps_rem_by_zero_aborts() {
    let prog = compile_src(
        "rem0",
        "module t;\nfn main() {\n    let a = 1;\n    let b = 0;\n    let _ = a % b;\n}\n",
    )
    .expect("compile");
    let run = exec(&prog);
    assert_ne!(run.status, 0, "% 0 should abort");
}

#[test]
fn const_literal_types_run() {
    let prog = compile_src(
        "constlit",
        "module t;\nuse j3.io.printLn;\nconst FLAG: bool = true;\nconst OFF: bool = !FLAG;\nconst NUDGE: f32 = -0.5;\nfn main() {\n    if FLAG && !OFF && NUDGE < 0 {\n        printLn(`ok`);\n    }\n}\n",
    )
    .expect("compile");
    let run = exec(&prog);
    assert_eq!(run.status, 0, "stderr {}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
}

#[test]
fn const_bool_rejects_an_int() {
    let err = match compile_src("constbad", "module t;\nconst FLAG: bool = 1;\nfn main() {}\n") {
        Ok(_) => panic!("expected a type error"),
        Err(e) => e,
    };
    assert!(err.contains("const type does not match"), "{err}");
}

#[test]
fn gaps_const_rem_by_zero_is_error() {
    let files = vec![parse_file(1, "t.j3", "module t;\nconst X = 1 % 0;\nfn main() {}\n")];
    let err = match check::check_files(files) {
        Ok(_) => panic!("expected %0 error"),
        Err(e) => e,
    };
    assert!(err.message.contains("division by zero"), "{}", err.message);
}

#[test]
fn gaps_text_find_runs() {
    run_both_ok(&honesty("text_find.j3"), "ok\n");
}

#[test]
fn interp_lone_string_and_text_add_have_no_limit() {
    run_both_ok(&honesty("interp_long.j3"), "ok\n");
}

#[test]
fn gaps_module_const_runs() {
    run_both_ok(&honesty("module_const.j3"), "ok\n");
}

#[test]
fn array_const_runs_on_both_backends() {
    run_both_ok(&honesty("array_const.j3"), "ok\n");
}

#[test]
fn array_const_errors_have_spans() {
    let head = "module t;\nstruct P {\n    x: int;\n}\nconst XS: [3]int = [1, 2, 3];\nconst PS: [1]P = [P { x: 1 }];\n";
    let bad = |tag: &str, body: &str| match compile_src(tag, &format!("{head}fn main() {{\n{body}\n}}\n")) {
        Ok(_) => panic!("expected error for {body}"),
        Err(e) => e,
    };
    let e = compile_src("acl", "module t;\nconst Z: [2]int = [1, 2, 3];\nfn main() {}\n").err().expect("length mismatch");
    assert!(e.contains("array length mismatch") && e.contains(":2:"), "{e}");
    for (tag, body) in [("aca", "    XS[0] = 5;"), ("acb", "    XS[0] += 5;"), ("acc", "    PS[0].x = 2;")] {
        let e = bad(tag, body);
        assert!(e.contains("cannot assign into const array"), "{body}: {e}");
    }
    let e = bad("acd", "    let r = &XS[0];");
    assert!(e.contains("cannot take `&`"), "{e}");
}

#[test]
fn array_const_bytes_and_negative_floats_match_on_both_backends() {
    run_both_ok(&honesty("array_const_bytes.j3"), "ok\n");
}

/// Same exit code (102) and message on both backends.
#[test]
fn array_const_index_out_of_bounds_aborts() {
    let src = "module t;\nuse j3.sys.argCount;\nconst XS: [2]int = [1, 2];\nfn main() {\n    let i = 5 + argCount();\n    let v = XS[i];\n    if v == 3 {\n        return;\n    }\n}\n";
    let prog = compile_src("acoob", src).expect("check");
    for backend in [emit::Backend::Llvm, emit::Backend::C] {
        let mut o = emit::EmitOpts::default();
        o.backend = backend;
        if backend == emit::Backend::Llvm && !emit::clang_available(&o.clang) {
            continue;
        }
        let run = emit::run_program(&prog.cx, &prog.modules, &o).expect("run");
        let err = String::from_utf8_lossy(&run.stderr);
        assert_eq!(run.status, 102, "{backend:?} {err}");
        assert!(err.contains("index out of bounds"), "{backend:?} {err}");
    }
}

/// A callee that writes through a `[]T` made from a const array writes ordinary module
/// data (the array is not in read-only memory): no crash, on both backends.
#[test]
fn array_const_slice_write_does_not_crash() {
    let src = "module t;\nuse j3.io.printLn;\nconst XS: [3]int = [1, 2, 3];\nfn bump(a: []int) {\n    a[0] = 9;\n}\nfn main() {\n    bump(XS);\n    printLn(`{XS[0]}`);\n}\n";
    let path = std::env::temp_dir().join(format!("j3c_acsw_{}.j3", std::process::id()));
    std::fs::write(&path, src).unwrap();
    run_both_ok(&path, "9\n");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn gaps_module_const_missing_is_named() {
    let err = match compile_src("mcnope", "module t;\nuse j3.gfx.mesh;\nfn main() {\n    let k = mesh.NOPE;\n}\n") {
        Ok(_) => panic!("expected missing const error"),
        Err(e) => e,
    };
    assert!(err.contains("has no pub const"), "{err}");
}

#[test]
fn short_circuit_skips_oob() {
    run_ok(&honesty("short_circuit.j3"), "ok\n");
}

#[test]
fn if_expr_skips_untaken_arm() {
    run_ok(&honesty("if_expr_skip.j3"), "ok\n");
}

#[test]
fn else_if_expr_and_field_of_rvalue_on_both_backends() {
    run_both_ok(&honesty("else_if_expr.j3"), "ok\nok2\n");
}

#[test]
fn else_if_expr_skips_untaken_arms_and_needs_final_else() {
    let src = "module t;\nuse j3.io.printLn;\nfn main() {\n    let xs: [1]int = [7];\n    let i = 3;\n    let x = if i < 1 { xs[i] } else if i < 2 { xs[i + 1] } else if i < 4 { 5 } else { xs[i + 2] };\n    if x == 5 {\n        printLn(\"ok\");\n    }\n}\n";
    let path = std::env::temp_dir().join(format!("j3c_eie_{}.j3", std::process::id()));
    std::fs::write(&path, src).unwrap();
    run_both_ok(&path, "ok\n");
    let _ = std::fs::remove_file(&path);
    let err = match compile_src("eie2", "module t;\nfn main() {\n    let i = 1;\n    let x = if i < 1 { 1 } else if i < 2 { 2 };\n}\n") {
        Ok(_) => panic!("expected error"),
        Err(e) => e,
    };
    assert!(err.contains("needs `else`"), "{err}");
}

#[test]
fn rvalue_base_evaluates_once_and_slice_results_stay_writable() {
    run_both_ok(&honesty("rvalue_eval_once.j3"), "once\norder\nslice-store\n");
}

#[test]
fn assigning_into_a_temporary_is_an_error() {
    for body in ["pt().x = 1;", "(Vec2 { x: 1.0, y: 1.0 } + Vec2 { x: 1.0, y: 1.0 }).x = 1.0;", "arr()[0] = 1;", "pt().x += 1;", "arr()[sl(arr())[0]] = 1;", "id(sl(arr())[1]).x = 3;"] {
        let src = format!("module t;\nuse j3.math.Vec2;\nstruct P {{\n    x: int;\n}}\nfn pt(): P {{\n    return P {{ x: 1 }};\n}}\nfn arr(): [3]int {{\n    return [1, 2, 3];\n}}\nfn sl(a: [3]int): []int {{\n    return a;\n}}\nfn id(i: int): P {{\n    return P {{ x: i }};\n}}\nfn main() {{\n    {body}\n}}\n");
        let err = match compile_src("tmpassign", &src) {
            Ok(_) => panic!("expected error for {body}"),
            Err(e) => e,
        };
        assert!(err.contains("temporary"), "{body}: {err}");
        // span: the temporary at the root, line 13 (body line), col 5 for `pt()` / `arr()`, col 6 for the parenthesized sum
        let want = if body.starts_with('(') { ":19:6" } else { ":19:5" };
        assert!(err.contains(want), "{body}: wrong span {err}");
    }
}

#[test]
fn defer_does_not_clobber_return() {
    run_ok(&honesty("defer_return.j3"), "ok\n");
}

#[test]
fn vec2_assign_op_calls_operator() {
    run_ok(&honesty("vec2_add_eq.j3"), "ok\n");
}

#[test]
fn div_by_zero_aborts() {
    let prog = compile_path(&honesty("div0.j3")).expect("compile");
    let run = exec(&prog);
    assert_ne!(run.status, 0, "div0 should abort");
}

#[test]
fn const_div_by_zero_is_error() {
    let files = vec![parse_file(
        1,
        "t.j3",
        "module t;\nconst X = 1 / 0;\nfn main() {}\n",
    )];
    let err = match check::check_files(files) {
        Ok(_) => panic!("expected /0 error"),
        Err(e) => e,
    };
    assert!(err.message.contains("division by zero"), "{}", err.message);
}

#[test]
fn string_index_is_not_an_lvalue() {
    let src = "module t;\nfn main() {\n    let s = \"hi\";\n    let p = &s[0];\n}\n";
    let files = vec![parse_file(1, "t.j3", src)];
    let err = match check::check_files(files) {
        Ok(_) => panic!("expected &s[0] error"),
        Err(e) => e,
    };
    assert!(
        err.message.contains("cannot take `&`") || err.message.contains("not a place"),
        "{}",
        err.message
    );
}

#[test]
fn string_slice_store_is_error() {
    let src = "module t;\nfn main() {\n    let s = \"hi\";\n    let mid = s[0..1];\n    mid[0] = 65;\n}\n";
    let files = vec![parse_file(1, "t.j3", src)];
    let err = match check::check_files(files) {
        Ok(_) => panic!("expected string slice store error"),
        Err(e) => e,
    };
    assert!(
        err.message.contains("cannot assign") || err.message.contains("not a place"),
        "{}",
        err.message
    );
}

#[test]
fn arena_field_is_error() {
    let err = match compile_path(&honesty("arena_field.j3")) {
        Ok(_) => panic!("expected Arena field error"),
        Err(e) => e,
    };
    assert!(err.contains("Arena by value") || err.contains("&Arena"), "{err}");
}
