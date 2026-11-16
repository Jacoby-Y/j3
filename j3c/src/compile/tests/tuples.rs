//! Plan 08 B3: tuple types, values, `.0`, destructuring, tuple patterns, `==`/`hash`, `Map` keys.
use super::*;

fn run_both(name: &str, src: &str, want: &str) {
    for r in both_backends(name, src) {
        assert_eq!(r.status, 0, "{}{}", String::from_utf8_lossy(&r.stdout), String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), want);
    }
}

/// Return, destructure, nest, match, `==` and printing: LLVM, C and the VM agree. (`hash()` of a
/// primitive element is an `#[impl]` fn, which scripts cannot call.)
#[test]
fn tuples_three_ways() {
    three_ways(
        "tuples",
        "use j3.io.printLn;\n\
enum Op { Add, Mul }\n\
fn divmod(a: int, b: int): (int, int) { return (a / b, a % b); }\n\
fn pick(p: (Op, int), x: int): int {\n\
    match p {\n\
        (Op.Add, n) { return x + n; }\n\
        (Op.Mul, 0) { return -1; }\n\
        (Op.Mul, n) { return x * n; }\n\
    }\n\
}\n\
pub fn run() {\n\
    let (q, r) = divmod(17, 5);\n\
    let t = divmod(9, 4);\n\
    let n: ((int, bool), int) = ((1, true), 7);\n\
    let ((a, flag), b) = n;\n\
    printLn(`{q} {r} {t.0} {t.1} {n.0.0} {n.0.1} {a} {flag} {b}`);\n\
    printLn(`{pick((Op.Add, 2), 3)} {pick((Op.Mul, 0), 3)} {pick((Op.Mul, 4), 3)}`);\n\
    printLn(`{n} {n == ((1, true), 7)} {n != ((1, false), 7)}`);\n\
}\n",
        "3 2 2 1 1 true 1 true 7\n5 -1 12\n((1, true), 7) true true\n",
    );
}

/// `==`, `!=`, `hash()`, a tuple as a `Map` key (through `Map`'s `Hash + Eq` bound), `for (k, v)`,
/// tuples inside `Option`, printing nested tuples, and one tuple type shared by two modules.
#[test]
fn tuple_equality_hash_map_key_and_print() {
    let src = "module t;\nuse j3.io.printLn;\nuse j3.data.map.Map;\nuse j3.memory.Arena;\nuse j3.option.Option;\n\
struct P { x: int; fn operator==(this: P, o: P): bool { return this.x == o.x; } }\n\
fn find(xs: []int, x: int): Option<(int, string)> {\n\
    for i in 0..xs.len { if xs[i] == x { return Option.Some((i, \"hit\")); } }\n\
    return Option.None;\n\
}\n\
fn main() {\n\
    let a = (1, \"x\");\n\
    printLn(`{a == (1, \"x\")} {a == (2, \"x\")} {a != (1, \"y\")} {(P { x: 1 }, 2.5) == (P { x: 1 }, 2.5)} {a.hash() == (1, \"x\").hash()} {a.hash() == (1, \"y\").hash()}`);\n\
    let arena = Arena.create(1 << 20);\n\
    let m = Map<(int, string), int>.new(&arena, 4);\n\
    let names = [\"a\", \"b\", \"c\"];\n\
    for i in 0..3000 { m.put((i % 100, names[i % 3]), i); }\n\
    let sum = 0;\n\
    for (k, v) in m.entries(&arena) { sum += k.0 + v; }\n\
    printLn(`{m.len} {m.getOr((7, \"b\"), -1)} {m.has((7, \"a\"))} {sum}`);\n\
    let xs = [4, 5];\n\
    match find(xs, 5) {\n\
        Option.Some((i, s)) { printLn(`{i} {s}`); }\n\
        Option.None { printLn(\"none\"); }\n\
    }\n\
    printLn(`{((1, 2.5), (true, \"s\"))}`);\n\
}\n";
    // keys (i % 100, names[i % 3]) take 300 distinct values; the last put of each wins
    let mut last = std::collections::HashMap::new();
    for i in 0..3000i64 {
        last.insert((i % 100, i % 3), i);
    }
    let sum: i64 = last.iter().map(|((k, _), v)| k + v).sum();
    let (at, has) = (last[&(7, 1)], last.contains_key(&(7, 0)));
    let want = format!("true false true true true false\n300 {at} {has} {sum}\n1 hit\n((1, 2.5), (true, s))\n");
    run_both("tuple_map", src, &want);
}

/// `(int, int)` written in two modules is one type: values pass between them.
#[test]
fn tuple_type_is_shared_across_modules() {
    let lib = "module geom;\npub fn swap(p: (int, int)): (int, int) { return (p.1, p.0); }\n";
    let main = "module main;\nuse j3.io.printLn;\nuse geom.swap;\nfn main() {\n    let p: (int, int) = (1, 2);\n    let q = swap(p);\n    printLn(`{q.0}{q.1} {q == (2, 1)}`);\n}\n";
    let toml = "[package]\nname = \"tup\"\nentry = \"src/main.j3\"\n";
    let m = temp_project("tuple_modules", &[("j3.toml", toml), ("src/main.j3", main), ("src/geom.j3", lib)]);
    let prog = compile_manifest(&m).expect("check");
    for backend in [emit::Backend::Llvm, emit::Backend::C] {
        let mut opts = llvm_opts();
        opts.backend = backend;
        let r = emit::run_program(&prog.cx, &prog.modules, &opts).expect("run");
        assert_eq!(String::from_utf8_lossy(&r.stdout), "21 true\n", "{backend:?}: {}", String::from_utf8_lossy(&r.stderr));
    }
    let _ = std::fs::remove_dir_all(&m.root);
}

#[test]
fn tuple_errors() {
    for (src, want) in [
        ("module t;\nfn main() { let (a, b, c) = (1, 2); }\n", "has 2 elements, the pattern gives 3"),
        ("module t;\nstruct Q { x: int; }\nfn main() { let a = (Q { x: 1 }, 2); let b = a == a; }\n", "does not compare structs"),
        ("module t;\nfn main() { let t = (1, 2); let x = t.2; }\n", "no field `2`"),
        ("module t;\nenum E { A, B }\nfn main() { match (E.A, true) { (E.A, true) {} (E.B, _) {} } }\n", "missing `(E.A, false)`"),
        ("module t;\nfn main() { let t = (1, 2); match t { 3 {} _ {} } }\n", "a tuple match arm"),
    ] {
        let e = check_err(src);
        assert!(e.contains(want), "{src}: {e}");
    }
    let e = match compile_src("tuple_bound", "module t;\nuse j3.data.map.Map;\nuse j3.memory.Arena;\nstruct Q { x: int; }\nfn main() { let a = Arena.create(4096); let m = Map<(Q, int), int>.new(&a, 4); }\n") {
        Ok(_) => panic!("expected a bound error"),
        Err(e) => e,
    };
    assert!(e.contains("does not satisfy `Hash`") && e.contains("`Q` does not"), "{e}");
    for (src, want) in [
        ("module t;\nfn main() { let x = (1,); }\n", "two or more elements"),
        ("module t;\nfn f(x: (int)) { }\n", "two or more elements"),
    ] {
        let err = parse::parse(src, FileId(1)).expect_err(src);
        assert!(err.message.contains(want), "{src}: {}", err.message);
    }
    // `t.0.1` is two field accesses, `(x)` a parenthesized expression
    parse::parse("module t;\nfn main() { let a = t.0.1; let b = (x); let c = 1.5; }\n", FileId(1)).expect("tuple fields");
}

/// `*m.getPtr(k) += 1` (a deref of a call is a place) and `for x in [..]` (a literal as the range).
#[test]
fn deref_of_call_is_a_place() {
    let src = "module t;\nuse j3.io.printLn;\nuse j3.data.map.Map;\nuse j3.memory.Arena;\n\
fn main() {\n\
    let arena = Arena.create(4096);\n\
    let m = Map<string, int>.new(&arena, 4);\n\
    m.put(\"z\", 1);\n\
    *m.getPtr(\"z\") += 5;\n\
    let s = 0;\n\
    for x in [4, 5] { s += x; }\n\
    printLn(`{m.getOr(\"z\", 0)} {s}`);\n\
}\n";
    run_both("deref_call", src, "6 9\n");
}

/// `j3c fmt` spaces tuple types, values and patterns, and keeps `#[derive(..)]` together; a second run changes nothing.
#[test]
fn fmt_round_trips_tuples_and_derives() {
    let src = "module t;\n#[derive(Eq, Hash)]\nstruct K { a: int; }\nfn f(p: (int, int)): (int, int) { let (a, b) = p; return (b, a); }\n";
    let once = crate::fmt::format_src(src).expect("fmt");
    for want in ["#[derive(Eq, Hash)]", "fn f(p: (int, int)): (int, int) {", "let (a, b) = p;", "return (b, a);"] {
        assert!(once.contains(want), "{want}: {once}");
    }
    assert_eq!(crate::fmt::format_src(&once).expect("fmt"), once);
}
