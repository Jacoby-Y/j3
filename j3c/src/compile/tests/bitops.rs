use super::*;

#[test]
fn bit_operators_three_ways() {
    three_ways(
        "bitops",
        "use j3.io.printLn;\n\
pub fn run() {\n\
    let a: i32 = -6;\n\
    let b: i32 = 12;\n\
    let u: u8 = 0;\n\
    let w: u8 = 0xf0;\n\
    let n = -1;\n\
    let m = 0b1010;\n\
    printLn(`{a & b} {a | b} {a ^ b} {~a} {~b}`);\n\
    printLn(`{u8(0xff) & w} {w ^ 0xff} {0x0f | w} {~u} {~w} {~u == 255}`);\n\
    printLn(`{n & 0xff} {0xff & n} {n ^ 5} {~n} {m | 4} {1 | 2 ^ 3 & 4}`);\n\
    printLn(`{w & 0x80 == 0x80} {a & b == 8}`);\n\
    let p = &n;\n\
    let q = n & *p;\n\
    let x = n;\n\
    x &= 0xf0;\n\
    x |= 3;\n\
    x ^= 0x11;\n\
    let y: u8 = 0x0f;\n\
    y ^= 0xff;\n\
    printLn(`{q} {x} {y}`);\n\
}\n",
        "8 -2 -10 5 -13\n\
240 15 255 255 15 true\n\
255 255 -6 0 14 3\n\
true true\n\
-1 226 240\n",
    );
}

#[test]
fn bit_operators_fold_in_const() {
    for r in both_backends(
        "bit_const",
        "module a;\nuse j3.io.printLn;\nconst A = 0xf0 & 0x3c | 1;\nconst B = ~0 ^ 0xff;\nconst C = 1 << 4 | 0b11;\nfn main() {\n    printLn(`{A} {B} {C}`);\n}\n",
    ) {
        assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), "49 -256 19\n");
    }
}

#[test]
fn bit_operator_type_errors() {
    let e = check_err("module t;\nfn main() {\n    let a: f32 = 1.5;\n    let b = a & a;\n}\n");
    assert!(e.contains("`&` takes integers"), "{e}");
    let e = check_err("module t;\nfn main() {\n    let a = true;\n    let b = a | false;\n}\n");
    assert!(e.contains("use `&&` / `||`"), "{e}");
    let e = check_err("module t;\nfn main() {\n    let a = true;\n    let b = ~a;\n}\n");
    assert!(e.contains("`~` takes an integer"), "{e}");
    let e = parse::parse("module t;\nfn main() {\n    let a = 1;\n    let b = a & ;\n}\n", FileId(1)).unwrap_err();
    assert!(e.message.contains("expected expression"), "{}", e.message);
}
