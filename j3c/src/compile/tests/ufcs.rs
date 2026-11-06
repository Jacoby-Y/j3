use super::*;

const HEAD: &str = "module t;\nuse j3.io.printLn;\nstruct P {\n    x: int;\n    y: int;\n    fn sum(this: P): int { return 100; }\n}\nfn sum(p: P): int { return p.x + p.y; }\nfn bump(p: &P, by: int) { p.x += by; }\nfn dbl(n: int): int { return n * 2; }\nfn tick(n: int): int { printLn(`tick{n}`); return n; }\nfn mk(): P { printLn(\"mk\"); return P { x: 1, y: 2 }; }\nfn peek(p: &P): int { return p.x; }\nfn add(a: int, b: int): int { return a + b; }\n";

fn run(name: &str, body: &str, want: &str) {
    let src = format!("{HEAD}fn main() {{\n{body}\n}}\n");
    for r in both_backends(name, &src) {
        assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), want);
    }
}

#[test]
fn ufcs_both_backends() {
    run(
        "ufcs_a",
        "let p = P { x: 1, y: 2 };\n\
         p.bump(10);\n\
         let n = 4;\n\
         printLn(`{p.x} {n.dbl()} {n.dbl().dbl()} {p.sum()} {n.add(3)}`);\n\
         printLn(`{mk().peek()}`);\n\
         printLn(`{tick(1).add(tick(2))}`);",
        "11 8 16 100 7\nmk\n1\ntick1\ntick2\n3\n",
    );
}

#[test]
fn ufcs_three_ways() {
    three_ways(
        "ufcs",
        "use j3.io.printLn;\nuse j3.math.{ abs };\n\
pub fn run() {\n\
    let far: f32 = -2.5;\n\
    printLn(`{far.abs()}`);\n\
}\n",
        "2.5\n",
    );
}

#[test]
fn ufcs_errors() {
    let head = "module t;\nstruct P {\n    x: int;\n}\nfn dbl(n: int): int { return n * 2; }\nfn peek(p: &P): int { return p.x; }\n";
    let e = check_err(&format!("{head}fn main() {{\n    let n = 1;\n    n.nothing();\n}}\n"));
    assert!(e.contains("no method `nothing`"), "{e}");
    let e = check_err(&format!("{head}fn main() {{\n    let p = P {{ x: 1 }};\n    p.dbl();\n}}\n"));
    assert!(e.contains("no method `dbl`"), "{e}");
    let e = check_err(&format!("{head}fn main() {{\n    let p = 1.5;\n    p.peek();\n}}\n"));
    assert!(e.contains("no method `peek`"), "{e}");
}
