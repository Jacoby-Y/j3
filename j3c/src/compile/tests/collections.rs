//! Plan 08 A8 + A5: compiler-workload collections, lazy generic-struct methods, arena trees, typed handles.
use super::*;

fn run_opt(name: &str, src: &str, opt: &str, want: &str) {
    for r in both_backends_opt(name, src, opt) {
        assert_eq!(r.status, 0, "{}{}", String::from_utf8_lossy(&r.stdout), String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), want);
    }
}

/// List/Map/Set/sortedKeys/Interner each against a naive reference for ~1M random ops, plus sort stability.
#[test]
fn collections_test_runs_on_both_backends() {
    run_opt("coll8", include_str!("../../../stdlib/tests/collections_test.j3"), "2", "collections_test ok\n");
}

/// `Arena.new`, a 10k-node tree, `#[Debug]` on a recursive enum, `Pool`/`Id`, `Id` as a `Map` key.
#[test]
fn handles_test_runs_on_both_backends() {
    run_opt("handles8", include_str!("../../../stdlib/tests/handles_test.j3"), "0", "handles_test ok\n");
}

/// The rendered check error of a program that uses the stdlib.
fn stdlib_err(tag: &str, src: &str) -> String {
    match compile_src(tag, src) {
        Ok(_) => panic!("expected a check error"),
        Err(e) => e,
    }
}

const NO_EQ: &str = "module t;\nuse j3.memory.{ Arena, List };\nstruct P { a: int; }\nfn main() {\n    let mem = Arena.create(4096);\n    let xs = List<P>.new(&mem);\n    xs.push(P { a: 1 });\n";

/// A method body is lowered only for instances that call it: `indexOf` needs `==`.
#[test]
fn unused_generic_method_body_is_not_checked() {
    compile_src("lazy_ok", &format!("{NO_EQ}}}\n")).expect("List of a type without == compiles");
}

/// `indexOf` says `where T: Eq`: the call is checked against it, before any body is lowered.
#[test]
fn called_generic_method_reports_at_the_call_site() {
    let err = stdlib_err("lazy_err", &format!("{NO_EQ}    let _ = xs.indexOf(P {{ a: 1 }});\n}}\n"));
    assert!(err.contains("`P` does not satisfy `Eq` (needed by `T` of `indexOf`)"), "{err}");
    assert!(err.contains(".j3:8:"), "{err}");
    assert!(err.contains("memory.j3"), "{err}");
}

#[test]
fn handles_of_different_types_do_not_mix() {
    let err = stdlib_err(
        "idmix",
        "module t;\nuse j3.data.pool.{ Id, Pool };\nuse j3.memory.Arena;\nstruct A { x: int; }\nstruct B { y: int; }\n\
         fn takeB(id: Id<B>) {}\nfn main() {\n    let mem = Arena.create(4096);\n    let pa = Pool<A>.new(&mem);\n    \
         let a = pa.add(A { x: 1 });\n    takeB(a);\n}\n",
    );
    assert!(err.contains("expected `Id<B>`, found `Id<A>`"), "{err}");
}

#[test]
fn sorting_needs_ord_at_the_call() {
    let err = stdlib_err("sortord", "module t;\nuse j3.cmp.sort;\nuse j3.memory.Arena;\nstruct P { a: int; }\nfn main() {\n    let mem = Arena.create(64);\n    sort(mem.alloc::<P>(2));\n}\n");
    assert!(err.contains("does not satisfy `Ord`"), "{err}");
}

/// Laziness is for generic struct instances only: a plain struct's uncalled method is still checked.
#[test]
fn plain_struct_methods_are_still_checked() {
    let err = stdlib_err("eager", "module t;\nstruct P {\n    a: int;\n    fn bad(this: &P): int { return this.nope; }\n}\nfn main() {}\n");
    assert!(err.contains("nope"), "{err}");
}
