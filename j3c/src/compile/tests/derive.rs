//! Plan 08 B4: `#[derive(..)]`, default trait methods, supertraits.
use super::*;

fn run_both(name: &str, src: &str, want: &str) {
    for r in both_backends(name, src) {
        assert_eq!(r.status, 0, "{}{}", String::from_utf8_lossy(&r.stdout), String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), want);
    }
}

fn err(tag: &str, src: &str) -> String {
    match compile_src(tag, src) {
        Ok(_) => panic!("expected a check error: {src}"),
        Err(e) => e,
    }
}

/// A derived struct and its hand-written twin (`==` field by field, `hash` by the documented fold)
/// agree on `==` and `hash` for every pair, and `Map<D, int>` / `Map<H, int>` end the same after
/// the same random puts and removes.
#[test]
fn derived_eq_hash_agree_with_hand_written() {
    let src = "module t;\nuse j3.io.printLn;\nuse j3.data.map.Map;\nuse j3.memory.Arena;\n\
#[derive(Eq, Hash)]\n\
struct D { a: int; s: string; on: bool; }\n\
struct H {\n\
    a: int; s: string; on: bool;\n\
    fn operator==(this: H, o: H): bool { return this.a == o.a && this.s == o.s && this.on == o.on; }\n\
    fn hash(this: &H): u64 {\n\
        let h: u64 = this.a.hash();\n\
        h = (h * 1099511628211) ^ this.s.hash();\n\
        h = (h * 1099511628211) ^ this.on.hash();\n\
        return h ^ (h >> 29);\n\
    }\n\
}\n\
fn main() {\n\
    let names = [\"\", \"a\", \"bb\"];\n\
    let bad = 0;\n\
    for i in 0..12 { for j in 0..12 {\n\
        let d1 = D { a: i % 2, s: names[i % 3], on: i % 4 < 2 };\n\
        let d2 = D { a: j % 2, s: names[j % 3], on: j % 4 < 2 };\n\
        let h1 = H { a: i % 2, s: names[i % 3], on: i % 4 < 2 };\n\
        let h2 = H { a: j % 2, s: names[j % 3], on: j % 4 < 2 };\n\
        if (d1 == d2) != (h1 == h2) || (d1 != d2) != (h1 != h2) || d1.hash() != h1.hash() { bad += 1; }\n\
    } }\n\
    let arena = Arena.create(1 << 22);\n\
    let md = Map<D, int>.new(&arena, 8);\n\
    let mh = Map<H, int>.new(&arena, 8);\n\
    let seed: u64 = 12345;\n\
    for i in 0..200000 {\n\
        seed = seed * 6364136223846793005 + 1442695040888963407;\n\
        let r = int(seed >> 33);\n\
        let k = r % 500;\n\
        let d = D { a: k / 3, s: names[k % 3], on: k % 2 == 0 };\n\
        let h = H { a: k / 3, s: names[k % 3], on: k % 2 == 0 };\n\
        if r % 4 == 0 { if md.remove(d) != mh.remove(h) { bad += 1; } }\n\
        else { md.put(d, i); mh.put(h, i); }\n\
        if md.getOr(d, -1) != mh.getOr(h, -1) { bad += 1; }\n\
    }\n\
    printLn(`bad={bad} same-len={md.len == mh.len}`);\n\
}\n";
    run_both("derive_agree", src, "bad=0 same-len=true\n");
}

/// Derived `<` on structs and enums (variant order, then fields), `clone`, `#[derive(Debug)]`, and
/// a generic struct's derived methods (checked per instance).
#[test]
fn derived_ord_clone_debug() {
    let src = "module t;\nuse j3.io.printLn;\nuse j3.cmp.sort;\n\
#[derive(Eq, Ord, Debug, Clone)]\n\
struct V { major: int; minor: int; }\n\
#[derive(Eq, Ord, Hash, Debug, Clone)]\n\
enum T { Eof, Num(int), Pair(int, string) }\n\
#[derive(Eq, Ord)]\n\
enum C { Red, Green }\n\
#[derive(Eq, Hash, Debug)]\n\
struct Box<A> { v: A; }\n\
fn main() {\n\
    let vs = [V { major: 1, minor: 5 }, V { major: 0, minor: 9 }, V { major: 1, minor: 2 }];\n\
    sort(vs);\n\
    printLn(`{vs[0]} {vs[1]} {vs[2]} {vs[0].clone() == vs[0]}`);\n\
    let ts = [T.Pair(1, \"b\"), T.Num(3), T.Eof, T.Pair(1, \"a\"), T.Num(-2)];\n\
    sort(ts);\n\
    printLn(`{ts[0]} {ts[1]} {ts[2]} {ts[3]} {ts[4]}`);\n\
    printLn(`{T.Num(1) == T.Num(1)} {T.Num(1) == T.Eof} {T.Num(1).hash() == T.Num(1).hash()} {T.Eof.clone() == T.Eof} {C.Red < C.Green} {C.Green < C.Red}`);\n\
    printLn(`{Box<int> { v: 3 } == Box<int> { v: 3 }} {Box<string> { v: \"s\" }}`);\n\
}\n";
    run_both(
        "derive_ord",
        src,
        "V { major: 0, minor: 9 } V { major: 1, minor: 2 } V { major: 1, minor: 5 } true\n\
T.Eof T.Num(-2) T.Num(3) T.Pair(1, a) T.Pair(1, b)\n\
true false true true true false\ntrue Box { v: s }\n",
    );
}

#[test]
fn derive_errors() {
    for (src, want) in [
        ("module t;\nstruct Inner { x: int; }\n#[derive(Hash)]\nstruct O { i: Inner; }\nfn main() { }\n", "no method `hash` on `Inner`"),
        ("module t;\n#[derive(Eq)]\nstruct S { x: int; fn operator==(this: S, o: S): bool { return true; } }\nfn main() { }\n", "derives `Eq` and also defines `operator==`"),
        ("module t;\n#[derive(Eq)]\nenum E<T> { A(T) }\nstruct S { x: int; }\nfn main() { let a: E<S> = E.A(S { x: 1 }); let b = a == a; }\n", "`S` does not satisfy `Eq` (needed by `T` of the derived `operator==`"),
        ("module t;\nuse j3.cmp.Hash;\n#[derive(Hash)]\nenum E<T> { A(T) }\nstruct S { x: int; }\nfn h<T: Hash>(v: T): u64 { return v.hash(); }\nfn main() { let a: E<S> = E.A(S { x: 1 }); h(a); }\n", "a derived `hash` needs it on every type argument; `S` does not have it"),
    ] {
        let e = err("derive_err", src);
        assert!(e.contains(want), "{src}: {e}");
    }
    for (src, want) in [
        ("module t;\n#[derive(Show)]\nstruct S { }\n", "cannot derive `Show`"),
        ("module t;\n#[derive(Eq, Eq)]\nstruct S { }\n", "`Eq` is derived twice"),
        ("module t;\n#[derive(Eq)]\nfn f() { }\n", "`#[derive]` cannot sit on `fn`"),
    ] {
        let e = parse::parse(src, FileId(1)).expect_err(src);
        assert!(e.message.contains(want), "{src}: {}", e.message);
    }
}

/// A default body serves types without their own method (called directly and through a bound);
/// an own method wins; a supertrait's methods are required and usable under the subtrait's bound.
#[test]
fn default_methods_and_supertraits() {
    let src = "module t;\nuse j3.io.printLn;\n\
pub trait Area { fn area(this: Self): int; fn twice(this: Self): int { return this.area() * 2; } }\n\
pub trait Named { fn name(this: Self): string; }\n\
pub trait Shown: Named + Area { fn score(this: Self): int { return this.name().len * 100 + this.twice(); } }\n\
struct Sq { s: int; fn area(this: Sq): int { return this.s * this.s; } fn name(this: &Sq): string { return \"sq\"; } }\n\
struct Odd { fn area(this: Odd): int { return 1; } fn twice(this: Odd): int { return 7; } }\n\
#[impl]\nfn area(x: int): int { return x; }\n\
fn viaBound<T: Area>(v: T): int { return v.twice(); }\n\
fn viaSub<T: Shown>(v: T): int { return v.score() + v.area(); }\n\
fn main() {\n\
    let q = Sq { s: 3 };\n\
    printLn(`{q.twice()} {viaBound(q)} {Odd {}.twice()} {viaBound(Odd {})} {5.twice()} {viaSub(q)}`);\n\
}\n";
    run_both("defaults", src, "18 18 7 7 10 227\n");
}

#[test]
fn default_and_supertrait_errors() {
    let missing = "module t;\npub trait Named { fn name(this: Self): string; }\n\
pub trait Shown: Named { fn show(this: Self): int { return this.name().len; } }\n\
struct S { x: int; }\nfn f<T: Shown>(v: T): int { return v.show(); }\n";
    let e = err("super_missing", &format!("{missing}fn main() {{ f(S {{ x: 1 }}); }}\n"));
    assert!(e.contains("does not satisfy `Shown`") && e.contains("requires `Named`") && e.contains("missing: fn name"), "{e}");
    let e = err("default_missing", &format!("{missing}fn main() {{ let n = S {{ x: 1 }}.show(); }}\n"));
    assert!(e.contains("the default `show`") && e.contains("missing: fn name"), "{e}");
    let e = err(
        "default_ambiguous",
        "module t;\npub trait A { fn a(this: Self): int; fn go(this: Self): int { return 1; } }\n\
pub trait B { fn b(this: Self): int; fn go(this: Self): int { return 2; } }\n\
struct S { fn a(this: S): int { return 0; } fn b(this: S): int { return 0; } }\nfn main() { let x = S {}.go(); }\n",
    );
    assert!(e.contains("`go` is ambiguous"), "{e}");
    let e = err("super_cycle", "module t;\ntrait A: B { fn a(this: Self): int; }\ntrait B: A { fn b(this: Self): int; }\nfn main() { }\n");
    assert!(e.contains("is its own supertrait"), "{e}");
}
