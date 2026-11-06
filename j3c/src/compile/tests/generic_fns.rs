use super::*;

fn run(name: &str, src: &str, want: &str) {
    for r in both_backends(name, src) {
        assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), want);
    }
}

fn err_of(src: &str) -> check::CheckError {
    match check::check_files(vec![parse_file(1, "t.j3", src)]) {
        Ok(_) => panic!("expected a check error"),
        Err(e) => e,
    }
}

const HEAD: &str = "module t;\nuse j3.io.printLn;\nstruct P { x: int; }\n";
/// For `check_files` alone, which has no stdlib.
const BARE: &str = "module t;\nstruct P { x: int; }\n";

#[test]
fn generic_fns_basic_and_two_params() {
    run(
        "gf_basic",
        &format!(
            "{HEAD}\
fn first<T>(xs: []T): T {{ return xs[0]; }}\n\
fn swap<T>(a: &T, b: &T) {{ let t = *a; *a = *b; *b = t; }}\n\
fn pick<A, B>(a: A, b: B): B {{ return b; }}\n\
fn maxOf<T>(a: T, b: T): T {{ if a > b {{ return a; }} return b; }}\n\
fn main() {{\n\
    let xs: [3]int = [4, 5, 6];\n\
    let fs: [2]f32 = [1.5, 2.5];\n\
    let ps: [1]P = [P {{ x: 9 }}];\n\
    printLn(`{{first(xs)}} {{first(fs)}} {{first(ps).x}}`);\n\
    let a = 1; let b = 2;\n\
    swap(&a, &b);\n\
    let p = P {{ x: 7 }}; let q = P {{ x: 8 }};\n\
    swap(&p, &q);\n\
    printLn(`{{a}} {{b}} {{p.x}} {{q.x}}`);\n\
    printLn(`{{pick(1, \"s\")}} {{pick(\"s\", 2.5)}} {{pick::<f32, int>(1.5, 3)}}`);\n\
    let f: f32 = 3.0;\n\
    printLn(`{{maxOf(1, 2)}} {{maxOf(f, 2)}} {{maxOf(2, f)}} {{maxOf(1.5, 0.5)}}`);\n\
}}\n"
        ),
        "4 1.5 9\n2 1 8 7\ns 2.5 3\n2 3 3 1.5\n",
    );
}

#[test]
fn generic_fns_ufcs_and_closure_param() {
    run(
        "gf_ufcs_closure",
        &format!(
            "{HEAD}\
fn first<T>(xs: []T): T {{ return xs[0]; }}\n\
fn bump<T>(x: &T, by: T) {{ *x = *x + by; }}\n\
fn apply<T>(x: T, f: closure(T): T): T {{ return f(f(x)); }}\n\
fn mapTo<T, U>(x: T, f: closure(T): U): U {{ return f(x); }}\n\
fn go(sl: []int) {{ printLn(`{{sl.first()}}`); }}\n\
fn main() {{\n\
    let xs: [2]int = [8, 9];\n\
    go(xs);\n\
    let n = 5;\n\
    n.bump(10);\n\
    printLn(`{{n}}`);\n\
    let k = 3;\n\
    printLn(`{{apply(4, fn(v: int): int {{ return v * k; }})}}`);\n\
    printLn(`{{mapTo(2, fn(v: int): f32 {{ return f32(v) * 1.5; }})}}`);\n\
}}\n"
        ),
        "8\n15\n36\n3\n",
    );
}

#[test]
fn generic_fns_recursion() {
    run(
        "gf_rec",
        &format!(
            "{HEAD}\
fn count<T>(x: T, n: int): int {{ if n == 0 {{ return 0; }} return 1 + count(x, n - 1); }}\n\
fn main() {{\n\
    printLn(`{{count(1, 4)}} {{count(P {{ x: 1 }}, 2)}}`);\n\
}}\n"
        ),
        "4 2\n",
    );
    let e = err_of(&format!(
        "{BARE}fn boom<T>(x: T) {{ let a: [1]T = [x]; boom(a); }}\nfn main() {{ boom(1); }}\n"
    ));
    assert!(e.message.contains("ever-deeper"), "{}", e.message);
    assert!(!e.helps.is_empty());
}

#[test]
fn generic_fns_shared_across_modules() {
    let toml = "[package]\nname = \"gf\"\nentry = \"src/main.j3\"\n\n[emit]\nopt = 0\n";
    let util = "module util;\npub fn first<T>(xs: []T): T { return xs[0]; }\npub struct Item { v: int; }\n\
pub fn twice<T>(xs: []T): T { return first(xs); }\n";
    let a = "module a;\nuse util.first;\npub fn ra(): int { let xs: [2]int = [3, 4]; return first(xs); }\n";
    let b = "module b;\nuse util;\nuse util.first;\nuse j3.io.printLn;\n\
pub fn rb() {\n    let xs: [2]int = [5, 6];\n    let fs: [1]f32 = [2.5];\n    printLn(`{first(xs)} {first(fs)} {util.twice(xs)}`);\n}\n";
    let main = "module main;\nuse a.ra;\nuse b.rb;\nuse j3.io.printLn;\nfn main() {\n    printLn(`{ra()}`);\n    rb();\n}\n";
    let m = temp_project("gf_mods", &[("j3.toml", toml), ("src/main.j3", main), ("src/util.j3", util), ("src/a.j3", a), ("src/b.j3", b)]);
    let prog = compile_manifest(&m).expect("check");
    let names: Vec<String> = prog.cx.funcs().iter().map(|f| prog.cx.name_str(f.name).to_string()).collect();
    assert_eq!(names.iter().filter(|n| n.starts_with("first<")).count(), 2, "{names:?}");
    assert_eq!(names.iter().filter(|n| n.starts_with("twice<")).count(), 1, "{names:?}");
    for backend in [emit::Backend::Llvm, emit::Backend::C] {
        let mut opts = llvm_opts();
        opts.backend = backend;
        let r = emit::run_program(&prog.cx, &prog.modules, &opts).expect("run");
        assert_eq!(r.status, 0, "{backend:?}: {}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), "3\n5 2.5 5\n", "{backend:?}");
    }
    let _ = std::fs::remove_dir_all(&m.root);
}

#[test]
fn generic_fns_errors() {
    let e = err_of(&format!("{BARE}fn zero<T>(): T {{ return 0; }}\nfn main() {{ let x = zero(); }}\n"));
    assert!(e.message.contains("cannot infer type parameter `T`"), "{}", e.message);
    assert!(e.helps.iter().any(|h| h.contains("zero::<int>")), "{:?}", e.helps);
    assert!(!e.span.is_dummy());
    let e = err_of(&format!("{BARE}fn two<T>(a: []T, b: []T) {{}}\nfn main() {{ let n: [1]int = [1]; let s: [1]string = [\"s\"]; two(n, s); }}\n"));
    assert!(e.message.contains("is both"), "{}", e.message);
    let e = err_of(&format!("{BARE}fn two<T>(a: T, b: T) {{}}\nfn main() {{ two(1, \"s\"); }}\n"));
    assert!(e.message.contains("type mismatch"), "{}", e.message);
    let e = err_of(&format!("{BARE}fn one<T>(a: T) {{}}\nfn main() {{ one(1, 2); }}\n"));
    assert!(e.message.contains("takes 1 args"), "{}", e.message);
    let e = err_of(&format!("{BARE}fn one<T>(a: T) {{}}\nfn main() {{ one::<int, int>(1); }}\n"));
    assert!(e.message.contains("type args"), "{}", e.message);
    // The body is checked per instance; the error names the line of the call.
    let e = err_of(&format!("{BARE}fn neg<T>(a: T): T {{ return -a; }}\nfn main() {{\n    neg(\"s\");\n}}\n"));
    assert!(e.notes.iter().any(|n| n.contains("while instantiating `neg<string>`") && n.contains("t.j3:5")), "{:?}", e.notes);
    // A repeated type parameter and a clash with a plain fn.
    let e = err_of(&format!("{BARE}fn dup<T, T>(a: T) {{}}\nfn main() {{}}\n"));
    assert!(e.message.contains("duplicate type parameter"), "{}", e.message);
    let e = err_of(&format!("{BARE}fn f<T>(a: T) {{}}\nfn f(a: int) {{}}\nfn main() {{}}\n"));
    assert!(e.message.contains("duplicate fn"), "{}", e.message);
}

#[test]
fn generic_fns_private_import_and_scripts() {
    let util = "module util;\nfn hidden<T>(a: T) {}\n";
    let main = "module main;\nuse util.hidden;\nfn main() {}\n";
    let err = check::check_files(vec![parse_file(1, "util.j3", util), parse_file(2, "main.j3", main)])
        .err()
        .expect("private generic");
    assert!(err.message.contains("private"), "{}", err.message);
    let toml = "[package]\nname = \"lang\"\nentry = \"src/main.j3\"\n\n[emit]\nopt = 0\n";
    let main = "module lang;\nconst S = @script(\"scripts/s.j3s\");\nfn main() {\n    let s = S {};\n    s.run();\n}\n";
    let util = "module util;\npub fn id<T>(a: T): T { return a; }\n";
    let script = "use util.id;\npub fn run() {\n    let a = id(1);\n}\n";
    let m = temp_project(
        "gf_script",
        &[("j3.toml", toml), ("src/main.j3", main), ("src/util.j3", util), ("scripts/s.j3s", script)],
    );
    let e = compile_manifest(&m).err().expect("generic fn from a script");
    assert!(e.contains("cannot be called from a .j3s script"), "{e}");
    let _ = std::fs::remove_dir_all(&m.root);
}

#[test]
fn generic_fns_expected_types_for_plain_params() {
    run(
        "gf_expected",
        "module t;\nuse j3.io.printLn;\nuse j3.option.Option;\n\
fn k<T>(x: T, o: Option<int>): T {\n    match o { Option.Some(v) { printLn(`some {v}`); } Option.None { printLn(\"none\"); } }\n    return x;\n}\n\
fn main() {\n    let a = k(1, Option.None);\n    let b = k(a, Option.Some(5));\n    printLn(`{a} {b}`);\n}\n",
        "none\nsome 5\n1 1\n",
    );
}
