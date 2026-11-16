//! `match` as a value, `if let`, `let .. else`.

use super::*;

/// Value matches on an enum, an int and a payload, a leaving arm, `if let` with and without
/// `else`, `let .. else`: LLVM, C and the bytecode VM print the same.
#[test]
fn match_values_three_ways() {
    three_ways(
        "match_expr",
        "use j3.io.printLn;\n\
enum Op { Add, Sub, Mul }\n\
enum Tok { Num(int), Pair(int, int), End }\n\
fn cost(op: Op): int {\n\
    return match op { Op.Add { 1 } Op.Sub { 2 } Op.Mul { let k = 3; k * 3 } };\n\
}\n\
fn small(n: int): int {\n\
    let v = match n { 0 { 10 } 1..=3 { 20 } _ { 30 } };\n\
    return v;\n\
}\n\
fn val(t: Tok): int {\n\
    let v = match t { Tok.Num(n) { n } Tok.Pair(a, b) { a + b } Tok.End { return -1; } };\n\
    return v * 2;\n\
}\n\
fn first(t: Tok): int {\n\
    let Tok.Pair(a, _) = t else { return 0; };\n\
    return a;\n\
}\n\
pub fn run() {\n\
    printLn(`{cost(Op.Add)} {cost(Op.Sub)} {cost(Op.Mul)} {small(0)} {small(2)} {small(9)}`);\n\
    printLn(`{val(Tok.Num(4))} {val(Tok.Pair(1, 2))} {val(Tok.End)} {first(Tok.Pair(7, 1))} {first(Tok.End)}`);\n\
    let t = Tok.Num(5);\n\
    if let Tok.Num(n) = t { printLn(`num {n}`); }\n\
    if let Tok.End = t { printLn(\"end\"); } else { printLn(\"not end\"); }\n\
    let w = if let Tok.Num(n) = t { n + 1 } else if let Tok.Pair(a, b) = t { a * b } else { 0 };\n\
    printLn(`w={w}`);\n\
}\n",
        "1 2 9 10 20 30\n8 6 -1 7 0\nnum 5\nnot end\nw=6\n",
    );
}

#[test]
fn match_value_errors() {
    let wrap = |body: &str| {
        format!("module t;\nenum E {{ A, B(int) }}\nfn main() {{\n    let e = E.A;\n{body}\n}}\n")
    };
    for (body, want) in [
        ("    let x = match e { E.A { 1 } E.B(n) { \"s\" } };", "match arms have different types: this arm is `string`, the arm at t.j3:5 is `int`"),
        ("    let Some(n) = e else { let z = 1; };", "the `else` of `let .. else` must leave"),
        ("    let E.B(n) = e else { };", "the `else` of `let .. else` must leave"),
        ("    let x = 1;\n    let _ = x;\n    let E.B(n) = e else { return; };\n    let y = n;\n    let q = match e { E.B(m) { m } };", "match is not exhaustive: missing `E.A`"),
        ("    let x = match e { E.A { 1 } E.B(n) { let q = n; } };", "this arm gives no value"),
        ("    let x = match e { E.A { return; } E.B(n) { return; } };", "every arm of this `match` leaves"),
        ("    let E.B(n) = e else { return; };\n    let y = n;\n    let E.B(k) = e else { let w = n; return; };\n    let z = k + y;", ""),
        ("    if let E.B(n) = e { let a = n; } else { let y = n; }", "unknown name `n`"),
        ("    let E.B(n) = e else { let y = n; return; };", "unknown name `n`"),
    ] {
        let src = wrap(body);
        if want.is_empty() {
            check::check_files(vec![parse_file(1, "t.j3", &src)]).unwrap_or_else(|e| panic!("{body}: {}", e.message));
            continue;
        }
        let err = check_err(&src);
        assert!(err.contains(want), "{body}\n  want: {want}\n  got: {err}");
    }
    let err = parse::parse(&wrap("    let x = if let E.B(n) = e { n };"), FileId(1)).unwrap_err();
    assert!(err.message.contains("if-expression needs `else`"), "{}", err.message);
}

/// A `match` at the start of a statement needs no `;`; arm values, `if let` chains and
/// `let .. else` survive `j3c fmt` and format stably.
#[test]
fn fmt_round_trips_match_values() {
    let src = "module t;\nfn f(e: E): int {\n    match e { E.A { g(); } _ { } }\n    let Some(x) = e else { return 0; };\n    let n = match e { E.A { 1 } E.B(k) { k } };\n    return if let E.B(k) = e { k } else if let E.A = e { 2 } else { n };\n}\n";
    let once = crate::fmt::format_src(src).unwrap();
    assert_eq!(crate::fmt::format_src(&once).unwrap(), once, "fmt is stable");
    crate::parse::parse(&once, crate::span::FileId(1)).expect("formatted text parses");
}
