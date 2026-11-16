//! Literal and loop gaps: string `match`, zero-filled `let`, `u64` literals above i64::MAX,
//! float exponents, `<<=`/`>>=`, `for x in` over an iterator. The language sample is
//! `speculative/lang/literals.j3` (`samples.rs`); here the VM and the errors.

use super::*;
use crate::parse::TokenKind;

/// The six features in a `.j3s`: the same output on LLVM, C and the VM (the VM has no shifts).
#[test]
fn literal_gaps_three_ways() {
    three_ways(
        "literals",
        "use j3.io.printLn;\n\
use j3.option.Option;\n\
struct Down {\n\
    n: int;\n\
    fn next(this: &Down): Option<int> {\n\
        if this.n == 0 { return Option.None; }\n\
        this.n -= 1;\n\
        return Option.Some(this.n + 1);\n\
    }\n\
}\n\
fn kw(w: string): int {\n\
    match w { \"let\" { return 1; } \"fn\" | \"func\" { return 2; } \"if\" if w.len > 9 { return 9; } _ { return 0; } }\n\
}\n\
pub fn run() {\n\
    let buf: [16]u8;\n\
    let fs: [2]f32;\n\
    let s = 0;\n\
    for b in buf { s += int(b); }\n\
    let m: u64 = 0xFFFFFFFFFFFFFFFF;\n\
    let f = 2.5e1 + 1E+1;\n\
    let t = 0;\n\
    for v in Down { n: 4 } { if v == 3 { continue; } t += v; }\n\
    let k = kw(\"let\") * 100 + kw(\"func\") * 10 + kw(\"if\");\n\
    printLn(`k={k} s={s} fs={int(fs[1])} max={m == 18446744073709551615} f={int(f)} t={t}`);\n\
}\n",
        "k=120 s=0 fs=0 max=true f=35 t=7\n",
    );
}

#[test]
fn shift_assign_runs_on_both_backends() {
    let src = "module t;\nuse j3.io.printLn;\nfn main() {\n    let a = 3;\n    a <<= 4;\n    let b: u64 = 0x8000000000000000;\n    b >>= 60;\n    let c: i8 = -64;\n    c >>= 2;\n    printLn(`{a} {b} {c}`);\n}\n";
    for r in both_backends("shiftassign", src) {
        assert_eq!(String::from_utf8_lossy(&r.stdout), "48 8 -16\n");
    }
}

#[test]
fn literal_gap_errors() {
    let wrap = |body: &str| {
        format!("module t;\nenum Option<T> {{ None, Some(T) }}\nstruct P {{ r: &int; }}\nstruct NoNext {{ n: int; }}\nstruct BadNext {{ n: int; fn next(this: &BadNext): int {{ return 0; }} }}\nfn main() {{\n    let w = \"a\";\n{body}\n}}\n")
    };
    for (body, want) in [
        ("    match w { \"a\" {} \"b\" {} }", "a `match` on a `string` needs a `_` arm"),
        ("    match w { \"a\" {} \"a\" {} _ {} }", "duplicate match arm `\"a\"`"),
        ("    match w { 3 {} _ {} }", "a `string` match arm is a string literal or `_`"),
        ("    let x = 0xFFFFFFFFFFFFFFFF;", "integer literal `18446744073709551615` does not fit `int`"),
        ("    let x: int = 9223372036854775808;", "does not fit `int`"),
        ("    let x: i64 = 0xFFFFFFFFFFFFFFFF;", "does not fit `int`"),
        ("    let p: P;", "`&int` has no zero value"),
        ("    let o: Option<int>;", "has no zero value"),
        ("    for x in NoNext { n: 1 } {}", "which has no `next` method"),
        ("    for x in BadNext { n: 1 } {}", "is not `fn next(this: &BadNext): Option<T>`"),
        ("    for x in 5 {}", "`for` over a `int`"),
    ] {
        let err = check_err(&wrap(body));
        assert!(err.contains(want), "{body}\n  want: {want}\n  got: {err}");
    }
    // a guarded arm may repeat a literal: the guard decides
    check::check_files(vec![parse_file(1, "t.j3", &wrap("    match w { \"a\" if w.len > 2 {} \"a\" {} _ {} }"))])
        .expect("a guarded string arm may repeat");
    let err = check_err("module t;\nconst A = 0xFFFFFFFFFFFFFFFF;\nfn main() {}\n");
    assert!(err.contains("does not fit `int`"), "{err}");
    check::check_files(vec![parse_file(1, "t.j3", "module t;\nconst A: u64 = 0xFFFFFFFFFFFFFFFF;\n#[singleton]\nlet G: u64 = 18446744073709551615;\nfn main() { let a: u64 = A; }\n")])
        .expect("u64 const and global above i64::MAX");
    // a u64 const folds on 64 bits; another int const may not hold a literal above i64::MAX
    let err = check_err("module t;\nconst A: int = 0xFFFFFFFFFFFFFFFF & 3;\nfn main() {}\n");
    assert!(err.contains("does not fit `int`"), "{err}");
    let err = check_err("module t;\nconst A: u64 = 0xFFFFFFFFFFFFFFFF;\nfn f(x: u32): int { match x { A {} _ {} } return 0; }\nfn main() {}\n");
    assert!(err.contains("match arm `18446744073709551615` does not fit `u32`"), "{err}");
}

/// `1e9` is a float; `1.e`, `1..e`, `x.0.e`, `0x1e5` keep their meaning; a sign with no digits is an error.
#[test]
fn float_exponent_lexing() {
    let kinds = |src: &str| -> Vec<(TokenKind, String)> {
        crate::parse::lex::lex(src, FileId(1)).unwrap().into_iter().map(|t| (t.kind, t.text)).filter(|t| t.0 != TokenKind::Eof).collect()
    };
    let f = |s: &str| (TokenKind::Float, s.to_string());
    let i = |s: &str| (TokenKind::Int, s.to_string());
    let n = |s: &str| (TokenKind::Ident, s.to_string());
    let p = |k| (k, String::new());
    assert_eq!(kinds("1e9 2.5e-3 1E+10"), vec![f("1e9"), f("2.5e-3"), f("1E+10")]);
    assert_eq!(kinds("1.e"), vec![i("1"), p(TokenKind::Dot), n("e")]);
    assert_eq!(kinds("1..e"), vec![i("1"), p(TokenKind::DotDot), n("e")]);
    assert_eq!(kinds("x.0.e"), vec![n("x"), p(TokenKind::Dot), i("0"), p(TokenKind::Dot), n("e")]);
    assert_eq!(kinds("0x1e5"), vec![i("0x1e5")]);
    assert_eq!(kinds("a <<= 2 >>= b << c"), vec![n("a"), p(TokenKind::ShlEq), i("2"), p(TokenKind::ShrEq), n("b"), p(TokenKind::Lt), p(TokenKind::Lt), n("c")]);
    let err = crate::parse::lex::lex("let x = 1e+;", FileId(1)).unwrap_err();
    assert!(err.message.contains("malformed float exponent"), "{}", err.message);
    assert_eq!((err.span.start, err.span.end), (8, 11));
}
