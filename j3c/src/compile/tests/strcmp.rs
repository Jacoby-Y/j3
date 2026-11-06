use super::*;

#[test]
fn string_ordering_is_a_check_error() {
    for op in ["<", "<=", ">", ">="] {
        let src = format!("module t;\nfn main() {{\n    let a = \"x\";\n    let b = \"y\";\n    let c = a {op} b;\n}}\n");
        let e = check_err(&src);
        assert!(e.contains(&format!("`{op}` does not order strings")), "{op}: {e}");
    }
}

#[test]
fn string_equality_still_checks() {
    for op in ["==", "!="] {
        let src = format!("module t;\nfn main() {{\n    let a = \"x\";\n    let b = \"y\";\n    let c = a {op} b;\n}}\n");
        check::check_files(vec![parse_file(1, "t.j3", &src)])
            .unwrap_or_else(|e| panic!("{op}: {}", e.message));
    }
}

#[test]
fn string_equality_three_ways() {
    three_ways(
        "streq",
        "use j3.io.printLn;\n\
fn pick(n: &int, s: string): string {\n\
    *n += 1;\n\
    return s;\n\
}\n\
pub fn run() {\n\
    let built = `a{1}c`;\n\
    let lit = \"a1c\";\n\
    let calls = 0;\n\
    let e = \"\";\n\
    printLn(`{built == lit} {built != lit} {lit == \"a1d\"} {lit != \"a1\"} {e == \"\"} {lit == e} {\"quit\" == \"quit\"}`);\n\
    let same = pick(&calls, lit) == pick(&calls, built);\n\
    printLn(`{same} {calls}`);\n\
    let n = 0;\n\
    while built == lit && n < 3 {\n\
        n += 1;\n\
    }\n\
    if built != \"zzz\" && lit == built {\n\
        printLn(`{n} chain`);\n\
    }\n\
    let tail = lit[1..3];\n\
    printLn(`{tail == \"1c\"} {tail != \"1c\"}`);\n\
}\n",
        "true false false true true false true\ntrue 2\n3 chain\ntrue false\n",
    );
}

#[test]
fn struct_and_payload_enum_equality_is_a_check_error() {
    let defs = "struct P { x: int; }\nenum E { A(int), B }\n";
    for (decl, ty) in [("let a = P { x: 1 };\n    let b = P { x: 1 };", "P"), ("let a = E.B;\n    let b = E.B;", "E")] {
        for op in ["==", "!="] {
            let src = format!("module t;\n{defs}fn main() {{\n    {decl}\n    let c = a {op} b;\n}}\n");
            let e = check_err(&src);
            assert!(e.contains(&format!("`{op}` does not compare structs")), "{ty} {op}: {e}");
        }
    }
}

#[test]
fn operator_eq_and_plain_enum_equality_still_check() {
    let src = "module t;\nstruct P { x: int; fn operator==(this: P, o: P): bool { return this.x == o.x; } }\nenum C { R, G }\nfn main() {\n    let a = P { x: 1 };\n    let b = P { x: 1 };\n    let c = a == b;\n    let d = C.R == C.G;\n}\n";
    check::check_files(vec![parse_file(1, "t.j3", src)]).unwrap_or_else(|e| panic!("{}", e.message));
}
