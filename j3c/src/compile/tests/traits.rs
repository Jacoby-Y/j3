//! Traits (chunk 14 of plan 029): bounds, `#[impl]` fns, comparison rewrites.
use super::script_dev::{dev_built, TICK_TOML};
use super::*;

fn run(name: &str, src: &str, want: &str) {
    for r in both_backends(name, src) {
        assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), want);
    }
}

fn err_of(src: &str) -> check::CheckError {
    errs_of(&[("t.j3", src)])
}

fn errs_of(files: &[(&str, &str)]) -> check::CheckError {
    let parsed = files.iter().enumerate().map(|(i, (p, s))| parse_file(i as u32 + 1, p, s)).collect();
    match check::check_files(parsed) {
        Ok(_) => panic!("expected a check error"),
        Err(e) => e,
    }
}

fn full(e: &check::CheckError) -> String {
    format!("{}\n{}\n{}", e.message, e.notes.join("\n"), e.helps.join("\n"))
}

const HEAD: &str = "module t;\nuse j3.io.printLn;\n";
const BARE: &str = "module t;\ntrait Hash { fn hash(this: Self): u64; }\nstruct P { x: int; }\n";

#[test]
fn bounded_fn_struct_and_primitive_run_on_both_backends() {
    run(
        "tr_basic",
        &format!(
            "{HEAD}\
trait Weigh {{ fn weigh(this: Self): int; }}\n\
struct P {{ x: int; fn weigh(this: &P): int {{ return this.x; }} }}\n\
#[impl]\nfn weigh(v: f64): int {{ return 100; }}\n\
fn total<T: Weigh>(a: T, b: T): int {{ return a.weigh() + b.weigh(); }}\n\
struct Both<T: Weigh> {{ a: T; fn sum(this: &Both<T>): int {{ return this.a.weigh() * 2; }} }}\n\
fn main() {{\n\
    printLn(`{{total(P {{ x: 1 }}, P {{ x: 2 }})}} {{total(1.5, 2.5)}}`);\n\
    let b = Both<P> {{ a: P {{ x: 21 }} }};\n\
    printLn(`{{b.sum()}}`);\n\
}}\n"
        ),
        "3 200\n42\n",
    );
}

#[test]
fn own_method_wins_over_impl_fn() {
    run(
        "tr_own_wins",
        &format!(
            "{HEAD}\
trait Weigh {{ fn weigh(this: Self): int; }}\n\
struct P {{ x: int; fn weigh(this: &P): int {{ return 1; }} }}\n\
#[impl]\nfn weigh(v: P): int {{ return 2; }}\n\
fn go<T: Weigh>(a: T): int {{ return a.weigh(); }}\n\
fn main() {{ printLn(`{{go(P {{ x: 0 }})}}`); }}\n"
        ),
        "1\n",
    );
}

#[test]
fn missing_method_error_names_type_trait_signature_and_declaration() {
    let src = format!("{BARE}fn f<T: Hash>(x: T) {{ }}\nfn main() {{ f(P {{ x: 1 }}); }}\n");
    let e = err_of(&src);
    assert_eq!(e.message, "`P` does not satisfy `Hash` (needed by `T` of `f`)");
    let at = src.find("f(P").unwrap() as u32;
    assert_eq!(e.span.start, at, "points at the call");
    let all = full(&e);
    assert!(all.contains("bound declared at t.j3:"), "{all}");
    assert!(all.contains("missing: fn hash(this: Self): u64"), "{all}");
    assert!(all.contains("add `fn hash(this: &P): u64` to `struct P`"), "{all}");
    assert!(all.contains("#[impl] fn hash(x: P): u64"), "{all}");
}

#[test]
fn wrong_signature_prints_both() {
    let src = format!(
        "{BARE}struct Q {{ fn hash(this: &Q, seed: int): u64 {{ return 0; }} }}\nfn f<T: Hash>(x: T) {{ }}\nfn main() {{ f(Q {{}}); }}\n"
    );
    let all = full(&err_of(&src));
    assert!(all.contains("`hash` has the wrong signature"), "{all}");
    assert!(all.contains("found `fn hash(&Q, int): u64`"), "{all}");
    assert!(all.contains("wants `fn hash(this: Self): u64`"), "{all}");
}

#[test]
fn plain_fn_does_not_satisfy_and_the_error_says_so() {
    let src = format!("{BARE}fn hash(p: P): u64 {{ return 1; }}\nfn f<T: Hash>(x: T) {{ }}\nfn main() {{ f(P {{ x: 1 }}); }}\n");
    let all = full(&err_of(&src));
    assert!(all.contains("a plain fn `hash` is in scope, but fns do not satisfy bounds; mark it `#[impl]`"), "{all}");
}

#[test]
fn struct_mention_is_checked_at_the_mention() {
    let src = format!("{BARE}struct Map<K: Hash, V> {{ k: K; v: V; }}\nfn g(m: Map<P, int>) {{ }}\nfn main() {{ }}\n");
    let e = err_of(&src);
    assert_eq!(e.message, "`P` does not satisfy `Hash` (needed by `K` of `Map`)");
    assert_eq!(e.span.start, src.find("Map<P, int>").unwrap() as u32);
    // The same in a body, and for an enum.
    let src = format!("{BARE}enum Opt<T: Hash> {{ No, Yes(T), }}\nfn main() {{ let o: Opt<P> = Opt.No; }}\n");
    let e = err_of(&src);
    assert!(e.message.contains("`P` does not satisfy `Hash` (needed by `T` of `Opt`)"), "{}", e.message);
}

#[test]
fn generic_method_bounds_are_checked_on_the_call() {
    let src = format!(
        "{BARE}struct Bag {{ fn put<U: Hash>(this: &Bag, u: U) {{ }} }}\nfn main() {{ let b = Bag {{}}; b.put(P {{ x: 1 }}); }}\n"
    );
    let e = err_of(&src);
    // The method infers nothing from a call: its type argument is written.
    let _ = e;
    let src = format!(
        "{BARE}struct Bag {{ fn put<U: Hash>(this: &Bag, u: U) {{ }} }}\nfn main() {{ let b = Bag {{}}; b.put::<P>(P {{ x: 1 }}); }}\n"
    );
    let e = err_of(&src);
    assert_eq!(e.message, "`P` does not satisfy `Hash` (needed by `U` of `put`)");
}

#[test]
fn impl_fns_duplicate_and_orphan_rules() {
    let trait_mod = "module tr;\npub trait Hash { fn hash(this: Self): u64; }\n#[impl]\npub fn hash(x: int): u64 { return 1; }\n";
    let user = "module u;\nuse tr.Hash;\nstruct V { x: int; }\n#[impl]\nfn hash(v: V): u64 { return 2; }\nfn f<T: Hash>(x: T) { }\nfn main() { f(1); f(V { x: 1 }); }\n";
    check::check_files(vec![parse_file(1, "tr.j3", trait_mod), parse_file(2, "u.j3", user)]).expect("trait module and type module may impl");

    // `int` is owned by nobody: only the trait's module may implement it.
    let bad = "module u;\nuse tr.Hash;\n#[impl]\nfn hash(x: int): u64 { return 3; }\nfn main() { }\n";
    let e = errs_of(&[("tr.j3", trait_mod), ("u.j3", bad)]);
    let all = full(&e);
    assert!(e.message.starts_with("orphan rule: `#[impl] fn hash` for `int`"), "{all}");

    let orphan = "module tr;\npub trait Hash { fn hash(this: Self): u64; }\n";
    let other = "module u;\nuse tr.Hash;\n#[impl]\nfn hash(x: string): u64 { return 3; }\nfn main() { }\n";
    let e = errs_of(&[("tr.j3", orphan), ("u.j3", other)]);
    assert!(e.message.starts_with("orphan rule: `#[impl] fn hash` for `string`"), "{}", e.message);
    assert_eq!(e.path, "u.j3");

    let dup = "module u;\ntrait Hash { fn hash(this: Self): u64; }\n#[impl]\nfn hash(x: int): u64 { return 1; }\n#[impl]\nfn hash(y: int): u64 { return 2; }\nfn main() { }\n";
    let e = err_of(dup);
    assert_eq!(e.message, "`hash` is already implemented for `int`");
    assert!(full(&e).contains("first `#[impl] fn hash` is at t.j3:4"), "{}", full(&e));

    // Two types in one module do not clash.
    let two = "module u;\ntrait Hash { fn hash(this: Self): u64; }\n#[impl]\nfn hash(x: int): u64 { return 1; }\n#[impl]\nfn hash(x: string): u64 { return 2; }\nfn main() { }\n";
    check::check_files(vec![parse_file(1, "t.j3", two)]).expect("overloads by type");
}

#[test]
fn trait_and_bound_name_errors() {
    let e = err_of("module t;\nfn f<T: Nope>(x: T) { }\nfn main() { }\n");
    assert_eq!(e.message, "unknown trait `Nope`");
    assert!(full(&e).contains("use j3.cmp.Nope"), "{}", full(&e));
    let e = err_of("module t;\nstruct S { }\nfn f<T: S>(x: T) { }\nfn main() { }\n");
    assert_eq!(e.message, "`S` is not a trait");
    let e = err_of("module t;\ntrait S { fn a(this: Self): int; }\nstruct S { }\nfn main() { }\n");
    assert_eq!(e.message, "`S` is both a trait and a type");
    let e = err_of("module t;\nfn f(x: Self) { }\nfn main() { }\n");
    assert!(e.message.contains("`Self` is only valid inside a `trait` body"), "{}", e.message);
    let e = err_of("module t;\ntrait A { fn a(x: int): int; }\nfn main() { }\n");
    assert!(e.message.contains("must take `this: Self`"), "{}", e.message);
    let e = err_of("module t;\ntrait A { fn a(this: Self, f: closure(int): int): int; }\nfn main() { }\n");
    assert!(e.message.contains("closure types cannot appear in a trait method"), "{}", e.message);
    let e = err_of("module t;\ntrait A { fn a(this: Self): int; }\n#[impl]\nfn a<T>(x: T): int { return 0; }\nfn main() { }\n");
    assert!(e.message.contains("cannot be generic"), "{}", e.message);
    for (src, want) in [
        ("module t;\nfn f<T: A + A>(x: T) { }\n", "bound `A` is listed twice"),
        ("module t;\ntrait A { fn a(this: Self): int { return 1; } }\n", "traits have no default bodies"),
        ("module t;\ntrait A<T> { fn a(this: Self): int; }\n", "traits take no type parameters"),
        ("module t;\n#[impl]\nstruct S { }\n", "`#[impl]` cannot sit on `struct`"),
    ] {
        let err = parse::parse(src, FileId(1)).expect_err(src);
        assert!(err.message.contains(want), "{src}: {}", err.message);
    }
    // `trait` stays usable as a name.
    parse::parse("module t;\nfn main() { let trait = 1; let b = trait + 1; }\n", FileId(1)).expect("contextual");
}

#[test]
fn unbounded_generics_are_unchanged() {
    run(
        "tr_unbounded",
        &format!(
            "{HEAD}\
fn pick<T>(a: T, b: T): T {{ if a > b {{ return a; }} return b; }}\n\
struct Box<T> {{ v: T; }}\n\
fn main() {{ let b = Box<int> {{ v: pick(3, 9) }}; printLn(`{{b.v}}`); }}\n"
        ),
        "9\n",
    );
}

const CMP: &str = "\
struct P { x: int; }\n\
struct E { x: int; fn operator==(this: E, other: E): bool { return this.x == other.x; } }\n\
struct L {\n    x: int;\n    fn operator<(this: L, other: L): bool { return this.x < other.x; }\n}\n\
struct X {\n    x: int;\n    fn operator==(this: X, other: X): bool { return this.x == other.x; }\n    fn operator!=(this: X, other: X): bool { return true; }\n    fn operator<(this: X, other: X): bool { return this.x < other.x; }\n    fn operator>(this: X, other: X): bool { return false; }\n    fn operator<=(this: X, other: X): bool { return false; }\n    fn operator>=(this: X, other: X): bool { return false; }\n}\n";

#[test]
fn comparison_rewrites_apply_only_when_the_operator_is_missing() {
    run(
        "tr_rewrites",
        &format!(
            "{HEAD}{CMP}\
fn main() {{\n\
    let a = E {{ x: 1 }}; let b = E {{ x: 2 }};\n\
    printLn(`{{a == a}} {{a != a}} {{a != b}}`);\n\
    let c = L {{ x: 1 }}; let d = L {{ x: 2 }};\n\
    printLn(`{{c < d}} {{c > d}} {{d > c}} {{c <= d}} {{d <= c}} {{c >= d}} {{d >= c}} {{c >= c}} {{c <= c}}`);\n\
    let p = X {{ x: 1 }}; let q = X {{ x: 1 }};\n\
    printLn(`{{p == q}} {{p != q}} {{p > q}} {{p <= q}} {{p >= q}} {{p < q}}`);\n\
}}\n"
        ),
        "true false true\ntrue false true true false false true true true\ntrue true false false false false\n",
    );
}

#[test]
fn comparison_rewrites_evaluate_each_side_once_and_in_order() {
    run(
        "tr_rewrite_order",
        &format!(
            "{HEAD}\
struct L {{ x: int; fn operator<(this: L, other: L): bool {{ return this.x < other.x; }} }}\n\
fn mk(n: &int, x: int): L {{ printLn(`mk {{x}}`); *n += 1; return L {{ x: x }}; }}\n\
fn main() {{\n\
    let n = 0;\n\
    let r = mk(&n, 1) > mk(&n, 2);\n\
    let s = mk(&n, 3) <= mk(&n, 4);\n\
    printLn(`{{r}} {{s}} {{n}}`);\n\
}}\n"
        ),
        "mk 1\nmk 2\nmk 3\nmk 4\nfalse true 4\n",
    );
}

#[test]
fn no_equality_operator_is_still_an_error() {
    let defs = "struct P { x: int; }\nstruct L { x: int; fn operator<(this: L, other: L): bool { return this.x < other.x; } }\n";
    for (a, op) in [("P", "!="), ("P", "=="), ("L", "==")] {
        let src = format!("module t;\n{defs}fn main() {{\n    let a = {a} {{ x: 1 }};\n    let b = {a} {{ x: 1 }};\n    let c = a {op} b;\n}}\n");
        let e = err_of(&src);
        assert!(e.message.contains(&format!("`{op}` does not compare structs")), "{a} {op}: {}", e.message);
    }
}

#[test]
fn builtin_operators_satisfy_eq_and_ord() {
    run(
        "tr_builtin",
        &format!(
            "{HEAD}\
trait Eq {{ fn operator==(this: Self, other: Self): bool; }}\n\
trait Ord {{ fn operator<(this: Self, other: Self): bool; }}\n\
enum Color {{ Red, Green }}\n\
fn same<T: Eq>(a: T, b: T): bool {{ return a == b; }}\n\
fn lt<T: Ord>(a: T, b: T): bool {{ return a < b; }}\n\
fn main() {{\n\
    printLn(`{{same(1, 1)}} {{same(1.5, 2.5)}} {{same(true, true)}} {{same(\"a\", \"b\")}} {{same(Color.Red, Color.Red)}} {{lt(1, 2)}} {{lt(2.5, 1.5)}}`);\n\
}}\n"
        ),
        "true false true false true true false\n",
    );
    // A string has no order; a payload enum has no equality.
    let e = err_of("module t;\ntrait Ord { fn operator<(this: Self, other: Self): bool; }\nfn lt<T: Ord>(a: T, b: T): bool { return true; }\nfn main() { lt(\"a\", \"b\"); }\n");
    assert!(e.message.starts_with("`string` does not satisfy `Ord`"), "{}", e.message);
    let e = err_of("module t;\ntrait Eq { fn operator==(this: Self, other: Self): bool; }\nenum E { A(int), B }\nfn same<T: Eq>(a: T, b: T): bool { return true; }\nfn main() { same(E.B, E.B); }\n");
    assert!(e.message.starts_with("`E` does not satisfy `Eq`"), "{}", e.message);
}

#[test]
fn trait_bounded_generic_across_modules() {
    let cmp = "module cmp;\npub trait Eq { fn operator==(this: Self, other: Self): bool; }\n\
pub fn contains<T: Eq>(xs: []T, x: T): bool {\n    for i in 0..xs.len {\n        if xs[i] == x { return true; }\n    }\n    return false;\n}\n";
    let main = "module t;\nuse cmp.contains;\nstruct P { x: int; }\nfn main() {\n    let xs: [2]P = [P { x: 1 }, P { x: 2 }];\n    contains(xs, P { x: 2 });\n}\n";
    let e = errs_of(&[("cmp.j3", cmp), ("t.j3", main)]);
    assert!(e.message.starts_with("`P` does not satisfy `Eq` (needed by `T` of `contains`)"), "{}", e.message);
    assert_eq!(e.path, "t.j3");
    assert!(full(&e).contains("bound declared at cmp.j3:3"), "{}", full(&e));
    let private = "module cmp;\ntrait Eq { fn operator==(this: Self, other: Self): bool; }\n";
    let main = "module t;\nuse cmp.Eq;\nfn main() { }\n";
    let e = errs_of(&[("cmp.j3", private), ("t.j3", main)]);
    assert_eq!(e.message, "`Eq` is private in `cmp`");
}

#[test]
fn scripts_cannot_declare_traits_but_may_call_functions_that_use_them() {
    let e = parse::parse_script("trait A { fn a(this: Self): int; }\n", FileId(1), &["s".to_string()]).expect_err("trait in script");
    assert!(e.message.contains("traits live in .j3 modules"), "{}", e.message);
    let lib = "module kit;\n\
pub trait Weigh { fn weigh(this: Self): int; }\n\
#[impl]\nfn weigh(x: int): int { return x * 10; }\n\
struct P { x: int; fn weigh(this: &P): int { return this.x; } }\n\
fn heavier<T: Weigh>(a: T, b: T): int { if a.weigh() > b.weigh() { return a.weigh(); } return b.weigh(); }\n\
pub fn score(n: int): int { return heavier(n, 3) + heavier(P { x: n }, P { x: 2 }); }\n";
    three_ways_with(
        "tr_vm",
        &[("src/kit.j3", lib)],
        "use j3.io.printLn;\nuse kit.score;\npub fn run() {\n    printLn(`{score(5)}`);\n}\n",
        "55\n",
    );
}

#[test]
fn scripts_cannot_call_impl_fns_by_method_syntax() {
    let lib = "module kit;\npub trait Weigh { fn weigh(this: Self): int; }\n#[impl]\nfn weigh(x: int): int { return x * 10; }\n";
    let main = "module lang;\nconst S = @script(\"scripts/s.j3s\");\nfn main() { let s = S {}; s.run(); }\n";
    let toml = "[package]\nname = \"lang\"\nentry = \"main.j3\"\n";
    let script = "use kit.Weigh;\npub fn run() { let n = 5; let w = n.weigh(); }\n";
    let m = temp_project("tr_vm_impl", &[("j3.toml", toml), ("main.j3", main), ("scripts/s.j3s", script), ("src/kit.j3", lib)]);
    let err = compile_manifest(&m).err().expect("impl fn in a script");
    assert!(err.contains("no method `weigh`"), "{err}");
    let _ = std::fs::remove_dir_all(&m.root);
}

/// Bounded instances live in the defining module's IR, so an unchanged rebuild and a script edit
/// still skip clang.
#[test]
fn incremental_rebuild_with_traits_is_unchanged() {
    let kit = "module kit;\npub trait Weigh { fn weigh(this: Self): int; }\n#[impl]\nfn weigh(x: int): int { return x * 10; }\n\
fn twice<T: Weigh>(a: T): int { return a.weigh() * 2; }\npub fn score(n: int): int { return twice(n); }\n";
    let main = "module ticker;\nuse kit.score;\nconst Tick = @script(\"scripts/tick.j3s\");\nfn main() {\n    let tick = Tick {};\n    tick.say(score(2));\n}\n";
    let script = "use j3.io.printLn;\n\npub fn say(i: int) {\n    printLn(`v {i}`);\n}\n";
    let m = temp_project(
        "tr_incr",
        &[("j3.toml", TICK_TOML), ("main.j3", main), ("scripts/tick.j3s", script), ("src/kit.j3", kit)],
    );
    assert!(dev_built(&m).rebuilt, "first build compiles");
    assert!(!dev_built(&m).rebuilt, "nothing changed");
    let path = m.root.join("scripts/tick.j3s");
    std::fs::write(&path, script.replace("v {i}", "w {i}")).unwrap();
    assert!(!dev_built(&m).rebuilt, "script edit skips clang");
    let _ = std::fs::remove_dir_all(&m.root);
}

#[test]
fn nan_compares_the_same_on_both_backends() {
    run(
        "tr_nan",
        "module t;\nuse j3.io.printLn;\nfn main() {\n    let z = 0.0;\n    let n = z / z;\n    let one = 1.0;\n    printLn(`{n != n} {n == n} {n < one} {n <= one} {n > one} {n >= one}`);\n}\n",
        "true false false false false false\n",
    );
}
