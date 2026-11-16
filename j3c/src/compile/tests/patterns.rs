//! Char literals and match patterns: `_` on enums, `|`, guards, int and char ranges.

use super::*;
use crate::parse::TokenKind;

/// Chars, ranges, or-patterns, guards and enum `_` in a `.j3s`: the same output on LLVM, C and the VM.
#[test]
fn chars_and_int_patterns_three_ways() {
    three_ways(
        "patterns",
        "use j3.io.printLn;\n\
fn kind(c: u8): int {\n\
    match c {\n\
        'a'..='z' | 'A'..='Z' | '_' { return 1; }\n\
        '0'..='9' { return 2; }\n\
        '\\n' | '\\t' | ' ' { return 3; }\n\
        '\\'' | '\"' | '\\\\' { return 4; }\n\
        _ { return 0; }\n\
    }\n\
}\n\
enum Op { Add, Sub, Lit(int) }\n\
fn eval(o: Op): int {\n\
    match o {\n\
        Op.Add | Op.Sub { return 1; }\n\
        Op.Lit(v) if v < 0 { return 0; }\n\
        _ { return 9; }\n\
    }\n\
}\n\
fn grade(n: int, strict: bool): int {\n\
    match n {\n\
        0 { return 100; }\n\
        1..=3 | 7 if strict { return 10; }\n\
        1..=3 { return 11; }\n\
        -5..=-1 { return -1; }\n\
        _ if n > 50 { return 50; }\n\
        _ { return 0; }\n\
    }\n\
}\n\
pub fn run() {\n\
    let s = \"aZ_9 \\n'\\\"\\\\+\";\n\
    let sum = 0;\n\
    for i in 0..s.len { sum = sum * 10 + kind(s[i]); }\n\
    let wide: int = 'A';\n\
    let x = '\\x7f';\n\
    let u = '\\u{1F600}';\n\
    let total = 0;\n\
    'rows: for r in 0..3 {\n\
        if r == 1 { continue 'rows; }\n\
        total += r;\n\
    }\n\
    printLn(`{eval(Op.Sub)} {eval(Op.Lit(-1))} {eval(Op.Lit(1))}`);\n\
    printLn(`{sum} {wide} {x} {u} {'\\0'} {'\\r'} {total}`);\n\
    printLn(`{grade(0, true)} {grade(2, true)} {grade(2, false)} {grade(7, true)} {grade(7, false)} {grade(-3, false)} {grade(60, false)} {grade(9, true)}`);\n\
}\n",
        "1 0 9\n1112334440 65 127 128512 0 13 2\n100 10 11 10 0 -1 50 0\n",
    );
}

/// Enum `_`, or-patterns with bindings, guards with fallthrough and nested payloads, on LLVM and C.
#[test]
fn enum_patterns_run_on_both_backends() {
    let src = "module t;\nuse j3.io.printLn;\n\
enum Tok { Plus, Minus, Star, Num(int), Name(string), Eof }\n\
enum Shape { Dot(int), Pair(Tok) }\n\
fn one(t: Tok): int {\n\
    match t {\n\
        Tok.Plus { return 1; }\n\
        _ { return 0; }\n\
    }\n\
}\n\
fn many(t: Tok): int {\n\
    match t {\n\
        Tok.Plus | Tok.Minus { return 1; }\n\
        Tok.Num(n) if n == 0 { return 100; }\n\
        Tok.Num(n) if n < 0 { return -n; }\n\
        Tok.Num(n) { return n * 2; }\n\
        _ { return 7; }\n\
    }\n\
}\n\
fn size(s: Shape): int {\n\
    match s {\n\
        Shape.Dot(n) | Shape.Pair(Tok.Num(n)) { return n; }\n\
        Shape.Pair(Tok.Name(_)) { return 50; }\n\
        _ { return -1; }\n\
    }\n\
}\n\
fn main() {\n\
    printLn(`{one(Tok.Plus)} {one(Tok.Eof)} {one(Tok.Num(3))}`);\n\
    printLn(`{many(Tok.Minus)} {many(Tok.Num(0))} {many(Tok.Num(-4))} {many(Tok.Num(5))} {many(Tok.Star)} {many(Tok.Name(\"x\"))}`);\n\
    printLn(`{size(Shape.Dot(4))} {size(Shape.Pair(Tok.Num(9)))} {size(Shape.Pair(Tok.Name(\"q\")))} {size(Shape.Pair(Tok.Eof))}`);\n\
}\n";
    for r in both_backends("enum_patterns", src) {
        assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), "1 0 0\n1 100 4 10 7 7\n4 9 50 -1\n");
    }
}

#[test]
fn pattern_errors() {
    let wrap = |body: &str| {
        format!("module t;\nenum E {{ A, B(int), C(string), D(int) }}\nconst LIM = 4;\nfn main() {{\n    let e = E.A;\n    let n = 3;\n    let b: u8 = 1;\n{body}\n}}\n")
    };
    for (body, want) in [
        ("    match e { E.A {} E.B(x) | E.C(x) {} E.D(y) {} }", "`x` is `string` here but `int`"),
        ("    match e { E.A {} E.B(x) | E.D(y) {} _ {} }", "this alternative binds `y`, the first binds `x`"),
        ("    match e { E.A {} E.B(x) | E.D(_) {} _ {} }", "this alternative binds no names"),
        ("    match e { E.A {} E.B(x) if x > 1 {} E.C(s) {} E.D(_) {} }", "missing `E.B(_)`"),
        ("    match e { E.A {} E.C(_) {} }", "missing `E.B(_)`, `E.D(_)`"),
        ("    match e { E.A {} _ {} E.B(x) {} }", "this arm never runs"),
        ("    match e { _ {} _ {} }", "duplicate `_` arm"),
        ("    match e { E.A {} E.B(x) if x { n += 1; } _ { } }", "a match guard is a `bool`"),
        ("    match e { 3 {} _ {} }", "an enum match arm is `Enum.Variant`"),
        ("    match n { 1..=5 {} 4 {} _ {} }", "match arm `4` overlaps `1..=5`"),
        ("    match n { 1..=5 {} 5..=9 {} _ {} }", "match arm `5..=9` overlaps `1..=5`"),
        ("    match n { 0..=LIM {} 4 {} _ {} }", "match arm `4` overlaps `0..=4`"),
        ("    match n { 5..=1 {} _ {} }", "empty range `5..=1`"),
        ("    match n { 1..=3 if n > 1 {} 1..=3 {} }", "needs a `_` arm"),
        ("    match b { 'a'..='z' {} 'm' {} _ {} }", "match arm `109` overlaps `97..=122`"),
        ("    match b { 0..=300 {} _ {} }", "does not fit `u8`"),
        ("    let c: u8 = 'é';", "is not ASCII, so it does not fit `u8`"),
        ("    let c: i8 = '\\u{1F600}';", "does not fit `i8`"),
    ] {
        let err = check_err(&wrap(body));
        assert!(err.contains(want), "{body}\n  want: {want}\n  got: {err}");
    }
    // guarded arms may overlap: the guard decides
    check::check_files(vec![parse_file(1, "t.j3", &wrap("    match n { 1..=5 if n > 2 {} 3 {} _ {} }"))])
        .expect("a guarded range may overlap a later arm");
}

#[test]
fn char_literal_lex_errors() {
    for (src, want, at) in [
        ("let c = 'ab';", "holds one character", "'ab'"),
        ("let c = '12';", "holds one character", "'12'"),
        ("let c = '';", "empty char literal", "''"),
        ("let c = '\\q';", "unknown char escape \\q", "'\\q"),
        ("let c = '\\x80';", "`\\x` is ASCII", "'\\x"),
        ("let c = '\\u{110000}';", "not a Unicode scalar value", "'\\u"),
        ("let c = '\\nx';", "holds one character", "'\\nx'"),
    ] {
        let full = format!("module t;\nfn main() {{\n    {src}\n}}\n");
        let err = crate::parse::parse(&full, FileId(1)).err().unwrap_or_else(|| panic!("{src}: expected an error"));
        assert!(err.message.contains(want), "{src}: {}", err.message);
        let span = &full[err.span.start as usize..err.span.end as usize];
        assert!(span.starts_with(at), "{src}: span {span:?}");
    }
    // `'rows` is a label, `'r'` a char, side by side
    let toks = crate::parse::lex::lex("'rows: for c in s { if c == 'r' { break 'rows; } }", FileId(1)).unwrap();
    let kinds: Vec<_> = toks.iter().filter(|t| matches!(t.kind, TokenKind::Label | TokenKind::Char)).map(|t| (t.kind, t.text.clone())).collect();
    assert_eq!(
        kinds,
        vec![(TokenKind::Label, "'rows".to_string()), (TokenKind::Char, "114".to_string()), (TokenKind::Label, "'rows".to_string())]
    );
}

/// No ranges, no guards, single values: still one `switch`. A range makes it an if-chain.
#[test]
fn int_match_keeps_switch_without_ranges() {
    let emit_both = |body: &str| {
        let src = format!("module t;\nuse j3.io.printLn;\nfn main() {{\n    let n = 2;\n    match n {{\n{body}\n    }}\n}}\n");
        let prog = compile_src("switch_kept", &src).expect("compile");
        let ll = emit::emit_llvm(&prog.cx, &prog.modules, &emit::EmitOpts::default()).expect("llvm");
        let c = emit::emit_program(&prog.cx, &prog.modules).expect("c");
        (ll.contains("switch i64"), c.contains("switch ("))
    };
    assert_eq!(emit_both("        1 { printLn(\"a\"); }\n        'b' { printLn(\"b\"); }\n        _ { }"), (true, true));
    assert_eq!(emit_both("        1..=3 { printLn(\"a\"); }\n        _ { }"), (false, false));
}

/// A `_` that covers no variant is a warning, not an error.
#[test]
fn wild_covering_nothing_warns() {
    let src = "module t;\nenum E { A, B(int) }\nfn main() {\n    let e = E.A;\n    match e {\n        E.A { }\n        E.B(_) { }\n        _ { }\n    }\n}\n";
    let prog = check::check_files(vec![parse_file(1, "t.j3", src)]).expect("a warning, not an error");
    assert!(prog.warnings.iter().any(|w| w.message.contains("covers no variant")), "{:?}", prog.warnings.iter().map(|w| &w.message).collect::<Vec<_>>());
    let src = src.replace("        E.B(_) { }\n", "");
    let prog = check::check_files(vec![parse_file(1, "t.j3", &src)]).expect("check");
    assert!(!prog.warnings.iter().any(|w| w.message.contains("covers no variant")));
}

/// `j3c fmt` keeps chars, labels, `|`, `..=` and guards as written.
#[test]
fn fmt_keeps_pattern_syntax() {
    let src = "fn f(c: u8, n: int) {\n    'rows: for i in 0..n {\n        match c {\n            'a'..='z' | '_' {\n                break 'rows;\n            }\n            '\\'' | '\\u{1F600}' if n > 0 {\n            }\n            _ {\n            }\n        }\n    }\n}\n";
    assert_eq!(crate::fmt::format_src(src).unwrap(), src);
}
