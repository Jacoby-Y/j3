//! Generic bodies checked once against their bounds (`check/lower/defcheck.rs`): errors at the
//! definition without any instance, `where` on generic-struct methods, supertraits and defaults.
use super::*;

/// The rendered error of a program that uses the stdlib.
fn err(tag: &str, src: &str) -> String {
    match compile_src(tag, src) {
        Ok(_) => panic!("expected a check error"),
        Err(e) => e,
    }
}

const HEAD: &str = "module t;\nuse j3.io.printLn;\n";

#[test]
fn method_outside_bounds_errors_at_definition_without_instance() {
    let e = err(
        "bd_method",
        &format!("{HEAD}trait Weigh {{ fn weigh(this: Self): int; }}\nfn f<T: Weigh>(x: T): int {{ return x.size(); }}\nfn main() {{ }}\n"),
    );
    assert!(e.contains("type parameter `T` has no method `size`: its bounds `Weigh` do not give it"), "{e}");
    assert!(e.contains(".j3:4:"), "{e}");
    assert!(e.contains("declare a trait with `fn size(this: Self, ..)` and add it: `T: Weigh + Trait`"), "{e}");
}

#[test]
fn operator_outside_bounds_names_the_trait_to_add() {
    let e = err("bd_op", &format!("{HEAD}use j3.cmp.Hash;\nfn g<T: Hash>(a: T, b: T): bool {{ return a > b; }}\nfn main() {{ }}\n"));
    assert!(e.contains("type parameter `T` has no `>`: its bounds `Hash` do not give it"), "{e}");
    assert!(e.contains("`T: Hash + Ord`"), "{e}");
}

#[test]
fn unbounded_param_passed_to_a_bounded_generic() {
    let e = err("bd_pass", &format!("{HEAD}use j3.cmp.hashOf;\nfn h<T>(x: T): u64 {{ return hashOf(x); }}\nfn main() {{ }}\n"));
    assert!(e.contains("type parameter `T` does not satisfy `Hash` (needed by `T` of `hashOf`)"), "{e}");
    assert!(e.contains("add the bound: `T: Hash`"), "{e}");
}

#[test]
fn field_of_a_type_parameter_is_an_error() {
    let e = err("bd_field", &format!("{HEAD}fn f<T>(x: T): int {{ return x.n; }}\nfn main() {{ }}\n"));
    assert!(e.contains("type parameter `T` has no field `n`: it has no bounds"), "{e}");
}

/// The definition error is reported once, not again for each instance.
#[test]
fn instance_does_not_repeat_the_definition_error() {
    let e = err(
        "bd_once",
        &format!("{HEAD}struct P {{ x: int; }}\nfn lt<T>(a: T, b: T): bool {{ return a < b; }}\nfn main() {{ let a = lt(P {{ x: 1 }}, P {{ x: 2 }}); let b = lt(1, 2); printLn(`{{a}} {{b}}`); }}\n"),
    );
    assert_eq!(e.matches("error:").count(), 1, "{e}");
    assert!(e.contains("type parameter `T` has no `<`: it has no bounds"), "{e}");
}

/// `List.indexOf`-style: a never-called method of a generic struct is still checked.
#[test]
fn never_called_struct_method_checked_and_where_fixes_it() {
    let bag = "struct Bag<T> {\n    v: T;\n    fn has(this: &Bag<T>, x: T): bool";
    let e = err("bd_bag", &format!("{HEAD}{bag} {{ return this.v == x; }}\n}}\nfn main() {{ }}\n"));
    assert!(e.contains("type parameter `T` has no `==`: it has no bounds"), "{e}");
    assert!(e.contains("`T: Eq`"), "{e}");
    // with `where T: Eq`, a Bag of a type without `==` is fine until `has` is called
    let ok = format!(
        "{HEAD}use j3.cmp.Eq;\nstruct P {{ x: int; }}\n{bag} where T: Eq {{ return this.v == x; }}\n}}\nfn main() {{ let b = Bag<P> {{ v: P {{ x: 1 }} }}; let c = Bag<int> {{ v: 3 }}; printLn(`{{b.v.x}} {{c.has(3)}}`); }}\n"
    );
    compile_src("bd_bag_ok", &ok).expect("where-bounded method compiles");
    let call = ok.replace("{c.has(3)}", "{b.has(P { x: 1 })}");
    let e = err("bd_bag_call", &call);
    assert!(e.contains("`P` does not satisfy `Eq` (needed by `T` of `has`)"), "{e}");
}

#[test]
fn where_on_a_free_fn_is_refused() {
    let e = err("bd_where_fn", &format!("{HEAD}use j3.cmp.Eq;\nfn f<T>(x: T) where T: Eq {{ }}\nfn main() {{ }}\n"));
    assert!(e.contains("`where` is for methods of a generic struct"), "{e}");
}

/// Methods reached through a bound's supertrait and a trait's default body; `==` from `Ord: Eq`.
#[test]
fn supertraits_and_defaults_resolve_through_bounds() {
    let src = format!(
        "{HEAD}use j3.cmp.Ord;\n\
trait Named {{ fn name(this: &Self): int; fn twice(this: &Self): int {{ return this.name() * 2; }} }}\n\
trait Big: Named {{ fn big(this: &Self): int; }}\n\
struct P {{ n: int; fn name(this: &P): int {{ return this.n; }} fn big(this: &P): int {{ return 100; }} }}\n\
fn score<T: Big>(x: &T): int {{ return x.twice() + x.name() + x.big(); }}\n\
fn same<T: Ord>(a: T, b: T): bool {{ return a == b && !(a < b) && a >= b && a <= b; }}\n\
fn unused<T: Big>(x: &T): int {{ return x.twice(); }}\n\
fn main() {{ let p = P {{ n: 4 }}; printLn(`{{score(&p)}} {{same(3, 3)}} {{same(2.5, 1.0)}}`); }}\n"
    );
    for r in both_backends("bd_super", &src) {
        assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), "112 true false\n");
    }
}
