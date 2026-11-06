//! Generic methods across modules, untyped operands, compound assignment, match, labels, block-scoped let.

use super::*;

/// A generic method of a plain struct resolves its signature in its own module, so a caller
/// reaching the struct through another module's field needs no `use` of it; an error inside
/// the method names the method's file.
#[test]
fn generic_method_resolves_in_defining_module() {
    let a = "\
module a;
pub struct Box { n: int; }
pub struct Pool {
    fn take<T>(this: &Pool, b: &Box): int { return b.n; }
}
pub struct Holder { pool: &Pool; box: &Box; }
";
    let b = "\
module b;
use a.Holder;
fn f(h: &Holder): int { return h.pool.take::<int>(h.box); }
fn main() {}
";
    check::check_files(vec![parse_file(1, "a.j3", a), parse_file(2, "b.j3", b)])
        .expect("generic method from another module");
    let bad = a.replace("b: &Box", "b: &Nope");
    let err = check::check_files(vec![parse_file(1, "a.j3", &bad), parse_file(2, "b.j3", b)])
        .err()
        .expect("unknown type");
    assert_eq!(err.path, "a.j3", "{}", err.message);
}

#[test]
fn untyped_left_operand_takes_the_other_type() {
    three_ways(
        "left_lit",
        "use j3.io.printLn;\n\
fn twice(far: f32) { return 2 * far; }\n\
pub fn run() {\n\
    let far: f32 = 3.0;\n\
    let a = 2 * far;\n\
    let b = (2 + 1) * far;\n\
    let c: f32 = 7.0 / 2;\n\
    let d = 1 - far;\n\
    let e = 0 < far;\n\
    let n: i32 = 5;\n\
    let k = 2 * n;\n\
    let f: i32 = k;\n\
    let x = 7 / 2;\n\
    printLn(`{a} {b} {c} {d} {e} {f} {twice(1.5)} {x}`);\n\
}\n",
        "6 9 3.5 -2 true 10 3 3\n",
    );
    let err = check_err("module t;\nfn main() {\n    let n: int = 3;\n    let x = 2.5 * n;\n}\n");
    assert!(err.contains("type mismatch"), "{err}");
    let err = check_err("module t;\nfn main() {\n    let x = 2.0;\n    let y: f32 = x;\n}\n");
    assert!(err.contains("does not match"), "{err}");
}

#[test]
fn div_and_rem_assign() {
    three_ways(
        "divrem_assign",
        "use j3.io.printLn;\n\
pub fn run() {\n\
    let a = 17;\n\
    a /= 5;\n\
    let b = -7;\n\
    b %= 3;\n\
    let xs: [2]i32 = [9, 100];\n\
    xs[1] /= 7;\n\
    xs[0] %= 4;\n\
    let f: f32 = 7.0;\n\
    f /= 2;\n\
    let m = -9223372036854775807 - 1;\n\
    let n = -1;\n\
    m /= n;\n\
    printLn(`{a} {b} {xs[0]} {xs[1]} {f} {m == -9223372036854775807 - 1}`);\n\
}\n",
        "3 -1 1 14 3.5 true\n",
    );
    for r in both_backends(
        "frem_assign",
        "module a;\nuse j3.io.printLn;\nfn main() {\n    let f: f32 = 7.5;\n    f %= 2;\n    let d = -7.5;\n    d %= 2.0;\n    printLn(`{f} {d}`);\n}\n",
    ) {
        assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), "1.5 -1.5\n");
    }
    // Divide by zero fails exactly like `/` and `%` do.
    let div0 = |op: &str| {
        both_backends(
            &format!("div0_{}", op.len()),
            &format!("module a;\nuse j3.sys.argCount;\nfn main() {{\n    let z = argCount() - 1;\n    let a = 5;\n    {op};\n}}\n"),
        )
    };
    for (assign, plain) in div0("a %= z").iter().zip(div0("a = a % z").iter()) {
        assert_eq!(assign.status, 102, "{}", String::from_utf8_lossy(&assign.stderr));
        assert_eq!(assign.status, plain.status);
        for r in [assign, plain] {
            let err = String::from_utf8_lossy(&r.stderr);
            assert!(err.contains("integer divide by zero"), "{err}");
        }
    }
    let err = check_err("module t;\nfn main() {\n    let a = 3;\n    a /= 2.5;\n}\n");
    assert!(err.contains("mismatch"), "{err}");
}

#[test]
fn match_on_integers() {
    three_ways(
        "int_match",
        "use j3.io.printLn;\n\
const BIG = 1000;\n\
fn name(n: int): string {\n\
    match n {\n\
        0 { return \"zero\"; }\n\
        -1 { return \"minus\"; }\n\
        BIG { return \"big\"; }\n\
        _ { return \"other\"; }\n\
    }\n\
    return \"unreachable\";\n\
}\n\
pub fn run() {\n\
    let b: u8 = 255;\n\
    match b {\n\
        255 { printLn(\"max\"); }\n\
        _ {}\n\
    }\n\
    for i in 0..10 {\n\
        defer { printLn(\".\"); }\n\
        match i {\n\
            1 { continue; }\n\
            3 { let k = i * 2; printLn(`k{k}`); }\n\
            5 { break; }\n\
            _ { printLn(`{i}`); }\n\
        }\n\
    }\n\
    printLn(`{name(0)} {name(-1)} {name(1000)} {name(7)}`);\n\
}\n",
        "max\n0\n.\n.\n2\n.\nk6\n.\n4\n.\n.\nzero minus big other\n",
    );
    let src = "module t;\nfn main() {\n    let n = 2;\n    match n {\n        1 { }\n        2 { }\n        _ { }\n    }\n}\n";
    let path = std::env::temp_dir().join(format!("j3c_int_match_{}.j3", std::process::id()));
    std::fs::write(&path, src).unwrap();
    let prog = compile_path(&path).expect("compile");
    let _ = std::fs::remove_file(&path);
    let ll = emit::emit_llvm(&prog.cx, &prog.modules, &emit::EmitOpts::default()).expect("llvm");
    assert!(ll.contains("switch i64"), "{ll}");
    let c = emit::emit_program(&prog.cx, &prog.modules).expect("c");
    assert!(c.contains("switch ("), "{c}");
    let wrap = |arms: &str| format!("module t;\nconst X = 1;\nfn main() {{\n    let n: i8 = 2;\n    let m = 0;\n    match n {{\n{arms}\n    }}\n}}\n");
    for (arms, want) in [
        ("        1 { }", "needs a `_` arm"),
        ("        1 { }\n        X { }\n        _ { }", "duplicate match arm `1`"),
        ("        m { }\n        _ { }", "int literal or an int `const`"),
        ("        200 { }\n        _ { }", "does not fit `i8`"),
        ("        1 { }\n        _ { }\n        _ { }", "duplicate `_` arm"),
    ] {
        let err = check_err(&wrap(arms));
        assert!(err.contains(want), "{arms}: {err}");
    }
}

#[test]
fn labeled_break_and_continue() {
    three_ways(
        "labels",
        "use j3.io.printLn;\n\
pub fn run() {\n\
    let xs: [3]int = [1, 2, 3];\n\
    'rows: for r in 0..4 {\n\
        defer { printLn(`end row {r}`); }\n\
        'cols: for x in xs {\n\
            defer { printLn(`  end col {x}`); }\n\
            let n = 0;\n\
            while n < 5 {\n\
                defer { printLn(\"    end while\"); }\n\
                n += 1;\n\
                if r == 0 && x == 2 { continue 'cols; }\n\
                if r == 1 && x == 1 { continue 'rows; }\n\
                match r {\n\
                    2 { break 'rows; }\n\
                    _ { break; }\n\
                }\n\
            }\n\
            printLn(`  r{r} x{x}`);\n\
        }\n\
    }\n\
    printLn(\"done\");\n\
}\n",
        "    end while\n  r0 x1\n  end col 1\n    end while\n  end col 2\n    end while\n  r0 x3\n  end col 3\nend row 0\n    end while\n  end col 1\nend row 1\n    end while\n  end col 1\nend row 2\ndone\n",
    );
    let wrap = |body: &str| format!("module t;\nfn main() {{\n{body}\n}}\n");
    for (body, want) in [
        ("    'a: while true {\n        break 'b;\n    }", "no enclosing loop is labeled `'b`"),
        ("    'a: for i in 0..2 {\n    }\n    for j in 0..2 {\n        continue 'a;\n    }", "no enclosing loop is labeled `'a`"),
        ("    'a: for i in 0..2 {\n        'a: for j in 0..2 {\n        }\n    }", "already used by an enclosing loop"),
        ("    break;", "`break` outside a loop"),
    ] {
        let err = check_err(&wrap(body));
        assert!(err.contains(want), "{body}: {err}");
    }
    let err = parse::parse(&wrap("    'a: if true {\n    }"), FileId(1)).err().expect("parse error");
    assert!(err.message.contains("a label goes on a `while` or `for` loop"), "{}", err.message);
}

#[test]
fn let_is_scoped_to_its_block() {
    let wrap = |body: &str| format!("module t;\nfn main() {{\n    let xs: [1]int = [1];\n{body}\n    let b = a;\n}}\n");
    for body in [
        "    if true { let a = 1; }",
        "    if false { } else { let a = 1; }",
        "    while false { let a = 1; }",
        "    for i in 0..1 { let a = 1; }",
        "    for x in xs { let a = 1; }",
        "    { let a = 1; }",
        "    defer { let a = 1; }",
        "    match 1 { 1 { let a = 1; } _ { } }",
    ] {
        let err = check_err(&wrap(body));
        assert!(err.contains("unknown name `a`"), "{body}: {err}");
    }
    // A name ends with its block, so siblings and later code can reuse it.
    three_ways(
        "let_scope",
        "use j3.io.printLn;\n\
pub fn run() {\n\
    let n = 0;\n\
    if n == 0 { let a = 1; n += a; } else { let a = 2; n += a; }\n\
    for i in 0..2 { let a = i * 10; n += a; }\n\
    { let a = 100; n += a; }\n\
    let a = 1000;\n\
    printLn(`{n + a}`);\n\
}\n",
        "1111\n",
    );
    // Shadowing a name that is still in scope stays an error.
    let err = check_err("module t;\nfn main() {\n    let a = 1;\n    if true { let a = 2; }\n}\n");
    assert!(err.contains("shadowing"), "{err}");
}
