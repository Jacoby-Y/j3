//! `dyn Trait` (plan B1): fat pointer + one method table per (type, trait).
use super::*;

fn run(name: &str, src: &str, want: &str) {
    for r in both_backends(name, src) {
        assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), want);
    }
}

fn err_full(src: &str) -> String {
    static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    match compile_src(&format!("dyn_err{n}"), src) {
        Ok(_) => panic!("expected a check error"),
        Err(e) => e,
    }
}

const BACKEND: &str = "module t;\nuse j3.io.printLn;\nuse j3.memory.{ Arena, List };\n\
pub trait Named { fn name(this: &Self): string; }\n\
pub trait Backend: Named {\n    fn emit(this: &Self, n: int): int;\n    fn lines(this: &Self): int { return this.emit(1) + 100; }\n}\n\
struct Llvm { k: int; fn name(this: &Llvm): string { return \"llvm\"; } fn emit(this: &Llvm, n: int): int { this.k += n; return n * 2; } }\n\
struct C { base: int; fn emit(this: C, n: int): int { return this.base + n; } fn lines(this: &C): int { return 7; } }\n\
#[impl]\nfn name(x: &C): string { return \"c\"; }\n";

#[test]
fn two_impls_list_defaults_supertraits_run_on_both_backends() {
    let src = format!(
        "{BACKEND}struct Pass {{ out: dyn Backend; }}\n\
fn pick(a: dyn Backend, b: dyn Backend, first: bool): dyn Backend {{ if first {{ return a; }} return b; }}\n\
fn main() {{\n    let l = Llvm {{ k: 0 }};\n    let c = C {{ base: 5 }};\n    let mem = Arena.create(4096);\n\
    let all = List<dyn Backend>.new(&mem);\n    all.push(&l);\n    all.push(&c);\n    all.push(&l);\n\
    let total = 0;\n    for i in 0..all.len {{ let d = all.items[i]; total += d.emit(i) + d.lines(); printLn(d.name()); }}\n\
    let p = Pass {{ out: &c }};\n    let r = &p.out;\n    let n: dyn Named = &c;\n\
    printLn(`total={{total}} k={{l.k}} pass={{r.emit(1)}} pick={{pick(&l, &c, true).name()}} named={{n.name()}}`);\n}}\n"
    );
    // llvm: emit(0)=0 + lines (emit(1)=2, +100) ; c: 6 + 7 ; llvm: emit(2)=4 + 102
    run("dyn_basic", &src, "llvm\nc\nllvm\ntotal=221 k=4 pass=6 pick=llvm named=c\n");
}

#[test]
fn generic_struct_instance_lazy_method_reached_only_through_a_table() {
    run(
        "dyn_lazy",
        "module t;\nuse j3.io.printLn;\ntrait Size { fn size(this: &Self): int; }\n\
struct Box<T> { v: T; n: int; fn size(this: &Box<T>): int { return this.n * 3; } }\n\
fn main() {\n    let b = Box<string> { v: \"x\", n: 4 };\n    let s: dyn Size = &b;\n    printLn(`{s.size()}`);\n}\n",
        "12\n",
    );
}

#[test]
fn missing_method_names_method_and_trait() {
    let e = err_full(&format!("{BACKEND}struct Bad {{ fn emit(this: &Bad, n: int): int {{ return n; }} }}\nfn main() {{ let b = Bad {{}}; let d: dyn Backend = &b; }}\n"));
    assert!(e.contains("`Bad` does not satisfy `Backend` (needed by `dyn Backend`)"), "{e}");
    assert!(e.contains("requires `Named`"), "{e}");
    assert!(e.contains("missing: fn name(this: &Self): string"), "{e}");
}

#[test]
fn dyn_rejections_are_clear() {
    let head = "module t;\ntrait V { fn take(this: Self): int; }\ntrait S { fn same(this: &Self, o: Self): bool; }\ntrait G { fn go(this: &Self): int; }\n\
struct X { fn take(this: X): int { return 1; } fn same(this: &X, o: X): bool { return true; } fn go<T>(this: &X): int { return 0; } }\n";
    let cases = [
        ("let x = X {}; let v: dyn V = &x;", "`dyn V` cannot call `take`: it takes `this: Self` by value"),
        ("let x = X {}; let s: dyn S = &x;", "`dyn S` cannot call `same`: it uses `Self` outside `this`"),
        ("let x = X {}; let g: dyn G = &x;", "`go` is a generic method"),
        ("let x = X {}; let g: dyn G = x;", "a `dyn G` is made from a reference"),
        ("let a: dyn Nope = 0;", "unknown trait `Nope`"),
        ("let a: dyn X = 0;", "`X` is a type, not a trait"),
    ];
    for (body, want) in cases {
        let e = err_full(&format!("{head}fn main() {{ {body} }}\n"));
        assert!(e.contains(want), "{body}: {e}");
    }
    let e = err_full(&format!("{BACKEND}fn main() {{ let l = Llvm {{ k: 0 }}; let a: dyn Backend = &l; let b: dyn Backend = &l; let e = a == b; }}\n"));
    assert!(e.contains("`==` does not compare `dyn Backend` values"), "{e}");
    let e = err_full(&format!("{BACKEND}fn main() {{ let l = Llvm {{ k: 0 }}; let a: dyn Backend = &l; a.nope(); }}\n"));
    assert!(e.contains("no method `nope` on `dyn Backend`") && e.contains("`emit`, `lines`, `name`"), "{e}");
}

#[test]
fn dyn_in_scripts_is_rejected() {
    let main = "module lang;\nconst S = @script(\"scripts/s.j3s\");\nfn main() {\n    let s = S {};\n    s.run();\n}\n";
    let toml = "[package]\nname = \"lang\"\nentry = \"main.j3\"\n\n[emit]\nopt = 0\n";
    let lib = "module shapes;\npub trait Area { fn area(this: &Self): int; }\npub struct Sq { s: int; fn area(this: &Sq): int { return this.s; } }\n";
    let script = "use shapes.{ Area, Sq };\npub fn run() {\n    let q = Sq { s: 2 };\n    let a: dyn Area = &q;\n}\n";
    let m = temp_project("dyn_script", &[("j3.toml", toml), ("main.j3", main), ("scripts/s.j3s", script), ("src/shapes.j3", lib)]);
    let e = match compile_manifest(&m) {
        Ok(_) => panic!("script dyn accepted"),
        Err(e) => format!("{e:?}"),
    };
    assert!(e.contains("`dyn` values are not supported in scripts"), "{e}");
    let _ = std::fs::remove_dir_all(&m.root);
}

/// Three modules convert the same (type, trait): one table global, defined by one object, in a
/// per-module (incremental) build, a rebuild after the owner changes, and one C file.
#[test]
fn one_table_per_type_and_trait_across_modules() {
    let toml = "[package]\nname = \"dynmm\"\nentry = \"src/main.j3\"\n\n[emit]\nbackend = llvm\nopt = 0\n";
    let shapes = "module shapes;\npub trait Shape {\n    fn area(this: &Self): int;\n    fn label(this: &Self): string { return \"shape\"; }\n}\n\
pub struct Sq { s: int; fn area(this: &Sq): int { return this.s * this.s; } }\npub fn asShape(q: &Sq): dyn Shape { return q; }\n";
    let other = "module other;\nuse shapes.{ Shape, Sq };\npub fn twice(q: &Sq): int { let d: dyn Shape = q; return d.area() * 2; }\n";
    let main = "module dynmm;\nuse j3.io.printLn;\nuse shapes.{ Shape, Sq, asShape };\nuse other.twice;\n\
fn main() {\n    let q = Sq { s: 3 };\n    let a: dyn Shape = &q;\n    printLn(`{a.area()} {a.label()} {twice(&q)} {asShape(&q).area()}`);\n}\n";
    let m = temp_project("dyn_mm", &[("j3.toml", toml), ("src/main.j3", main), ("src/shapes.j3", shapes), ("src/other.j3", other)]);
    let build = |m: &Manifest, backend: emit::Backend| {
        let prog = compile_manifest(m).expect("check");
        let tables = prog.modules.iter().flat_map(|md| md.globals.iter()).filter(|g| prog.cx.name_str(prog.cx.global(**g).name).starts_with("$vt.")).count();
        assert_eq!(tables, 1, "one table for (Sq, Shape)");
        let mut opts = llvm_opts();
        opts.backend = backend;
        opts.cc.out = Some(m.root.join("target/dynmm"));
        let r = emit::run_program(&prog.cx, &prog.modules, &opts).expect("run");
        assert_eq!(r.status, 0, "{backend:?}: {}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), "9 shape 18 9\n", "{backend:?}");
    };
    build(&m, emit::Backend::Llvm);
    // the first converter (`shapes`) stops converting: another module now owns the table
    std::fs::write(m.root.join("src/shapes.j3"), shapes.replace("pub fn asShape(q: &Sq): dyn Shape { return q; }", "pub fn asShape(q: &Sq): &Sq { return q; }")).unwrap();
    build(&m, emit::Backend::Llvm);
    build(&m, emit::Backend::C);
    let _ = std::fs::remove_dir_all(&m.root);
}

/// `dyn Sub` to `dyn Super`: the sub table's `$up` entry gives the same type's super table, for
/// every type behind the `dyn`; a trait that is not a supertrait is an error.
#[test]
fn upcast_to_a_supertrait_on_both_backends() {
    let src = format!("{BACKEND}pub trait Fancy: Backend {{ fn fancy(this: &Self): int; }}\n\
#[impl]\nfn fancy(x: &C): int {{ return 3; }}\n\
fn named(n: dyn Named): string {{ return n.name(); }}\n\
fn main() {{\n    let c = C {{ base: 5 }};\n    let f: dyn Fancy = &c;\n    let b: dyn Backend = f;\n    let n: dyn Named = b;\n\
    printLn(`{{named(f)}} {{n.name()}} {{b.emit(1)}} {{b.lines()}} {{f.fancy()}}`);\n}}\n");
    run("dyn_up", &src, "c c 6 7 3\n");
    let e = err_full(&format!("{BACKEND}fn main() {{\n    let c = C {{ base: 5 }};\n    let n: dyn Named = &c;\n    let b: dyn Backend = n;\n}}\n"));
    assert!(e.contains("a `dyn Named` is not a `dyn Backend`"), "{e}");
}
