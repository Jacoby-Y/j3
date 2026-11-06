//! `vec4f` / `vec4i` on LLVM and C at opt 0 and 2.

use super::*;

const HEAD: &str = "module t;\nuse j3.io.printLn;\n";

fn run_backend(name: &str, src: &str, opt: &str, backend: emit::Backend) -> emit::RunResult {
    let dir = std::env::temp_dir().join(format!("j3-simd-{}-{name}-{opt}-{backend:?}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.j3");
    std::fs::write(&file, src).unwrap();
    let prog = compile_path(&file).expect("check");
    let mut opts = llvm_opts();
    opts.backend = backend;
    opts.cc.opt = opt.to_string();
    let r = emit::run_program(&prog.cx, &prog.modules, &opts).expect("run");
    let _ = std::fs::remove_dir_all(&dir);
    r
}

/// Runs on LLVM and C at opt 0 and 2 and expects the same stdout from all four.
fn run(name: &str, body: &str, want: &str) {
    let src = format!("{HEAD}{body}");
    for opt in ["0", "2"] {
        for backend in [emit::Backend::Llvm, emit::Backend::C] {
            let r = run_backend(name, &src, opt, backend);
            assert_eq!(r.status, 0, "{backend:?} opt {opt}: {}", String::from_utf8_lossy(&r.stderr));
            assert_eq!(String::from_utf8_lossy(&r.stdout), want, "{backend:?} opt {opt}");
        }
    }
}

fn rejects(src: &str, msg: &str) {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let tag = format!("simd_err{}", N.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let e = compile_src(&tag, &format!("{HEAD}{src}")).err().expect("expected an error");
    assert!(e.contains(msg), "{e}");
}

#[test]
fn ops_lanes_and_intrinsics() {
    run(
        "ops",
        "fn show(v: vec4f) { printLn(`{v.x} {v.y} {v.z} {v.w}`); }
fn main() {
    let a = vec4f(1.0, 2.0, 3.0, 4.0);
    let b = vec4f(0.5);
    show(a + b);
    show(a * 2.0);
    show(2 * a);
    show(a / b - 1.0);
    show(-a);
    printLn(`{@vsum(a)}`);
    show(@vmin(a, vec4f(2.5)));
    show(@vmax(a, vec4f(2.5)));
    show(@vsqrt(a * a));
    show(@vabs(-a));
    show(@vshuffle(a, 3, 2, 1, 0));
    let i = vec4i(1, 2, 3, 4);
    let j = i * 3 + 1;
    printLn(`{j.x} {j.y} {j.z} {j.w} {@vsum(j)}`);
    let k = (j << 1) & 255;
    printLn(`{k.x} {k.w} {(~k).x} {(j / 2).w} {(j | 64).x} {(j ^ 1).y} {(j >> 1).w}`);
    show(@vfloat(j));
    let t = @vint(vec4f(1.9, -1.9, 0.0, 100.5));
    printLn(`{t.x} {t.y} {t.z} {t.w} {i[2]}`);
    printLn(`{@vabs(vec4i(0 - 5, 6, 0 - 7, 8)).z} {@vmin(i, vec4i(2)).w} {@vmax(i, vec4i(2)).x}`);
}
",
        "1.5 2.5 3.5 4.5\n2 4 6 8\n2 4 6 8\n1 3 5 7\n-1 -2 -3 -4\n10\n1 2 2.5 2.5\n2.5 2.5 3 4\n1 2 3 4\n1 2 3 4\n4 3 2 1\n4 7 10 13 34\n8 26 -9 6 68 6 6\n4 7 10 13\n1 -1 0 100 3\n7 2 2\n",
    );
}

#[test]
fn places_write_the_right_lane() {
    run(
        "places",
        "struct S { a: u8; v: vec4f; }
fn show(v: vec4f) { printLn(`{v.x} {v.y} {v.z} {v.w}`); }
fn main() {
    let v = vec4f(1.0, 2.0, 3.0, 4.0);
    v.x = 10.0;
    v[3] = 40.0;
    v.z += 0.5;
    show(v);
    let s = S { a: 1, v: v };
    s.v.w *= 2.0;
    s.v.y = 7.0;
    show(s.v);
    let arr: [3]vec4f = [v, v, v];
    arr[2].y += 3.0;
    arr[1] *= 2.0;
    arr[0] += vec4f(1.0);
    show(arr[0]);
    show(arr[1]);
    show(arr[2]);
    let w = vec4i(8, 8, 8, 8);
    w = w << 2;
    w.y -= 1;
    w /= 2;
    printLn(`{w.x} {w.y} {w.z} {w.w}`);
}
",
        "10 2 3.5 40\n10 7 3.5 80\n11 3 4.5 41\n20 4 7 80\n10 5 3.5 40\n16 15 16 16\n",
    );
}

#[test]
fn layout_alignment_at_opt2() {
    run(
        "layout",
        "use j3.memory.Arena;
struct S { a: u8; v: vec4f; b: u8; }
struct Holder { s: S; n: int; }
fn bump(h: &Holder) { h.s.v.y += 1.0; h.n += 1; }
fn pass(v: vec4f): vec4f { return v * 2.0; }
fn main() {
    printLn(`{@sizeof(vec4f)} {@sizeof(vec4i)} {@sizeof(S)} {@sizeof(Holder)} {@sizeof([3]vec4f)}`);
    let s = S { a: 1, v: vec4f(1.0, 2.0, 3.0, 4.0), b: 2 };
    let h = Holder { s: s, n: 0 };
    bump(&h);
    bump(&h);
    printLn(`{h.s.v.x} {h.s.v.y} {h.s.v.w} {h.s.a} {h.s.b} {h.n}`);
    let arr: [4]vec4f = [pass(s.v), pass(s.v), pass(s.v), pass(s.v)];
    arr[3].w = 0.5;
    let sum: f32 = 0.0;
    for i in 0..4 { sum += @vsum(arr[i]); }
    printLn(`{sum}`);
    let mem = Arena.create(4096);
    let heap = mem.alloc::<S>(3);
    heap[2].v = vec4f(1.0, 2.0, 3.0, 4.0);
    heap[2].v.x = 5.0;
    let addr = &heap[2].v as int;
    printLn(`{addr % 16} {heap[2].v.x + heap[2].v.w}`);
    let vs = mem.alloc::<vec4f>(5);
    vs[4] = pass(heap[2].v);
    printLn(`{(&vs[0]) as int % 16} {vs[4].x}`);
}
",
        "16 16 48 64 48\n1 4 4 1 2 2\n72.5\n0 9\n0 10\n",
    );
}

#[test]
fn sums_are_bit_identical() {
    // sequential left-to-right gives 1; the fixed tree (x + y) + (z + w) gives 0
    run(
        "sum",
        "fn main() {
    let a = vec4f(100000000.0, 1.0, -100000000.0, 1.0);
    printLn(`{@vsum(a)}`);
    let acc = vec4f(0.1);
    for i in 0..50 { acc = acc * 1.01 + vec4f(0.3, 0.2, 0.1, 0.05); }
    printLn(`{@vsum(acc)} {acc.x} {acc.w}`);
}
",
        "0\n42.5589 19.5034 3.38762\n",
    );
}

#[test]
fn load_store_and_bounds() {
    run(
        "ldst",
        "fn main() {
    let xs: [8]f32 = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
    let s = xs[0..8];
    let q = @vload(s, 3);
    printLn(`{q.x} {q.w}`);
    @vstore(s, 0, q * 2.0);
    printLn(`{xs[0]} {xs[3]} {xs[4]}`);
    let ys: [5]i32 = [1, 2, 3, 4, 5];
    let r = @vload(ys[0..5], 1);
    printLn(`{r.x} {r.w} {@vsum(r)}`);
}
",
        "4 7\n8 14 5\n2 5 14\n",
    );
    for (i, bad) in ["@vload(s, 5)", "@vload(s, 0 - 1)", "@vload(s, 8)"].iter().enumerate() {
        let src = format!("{HEAD}fn main() {{ let xs: [8]f32 = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]; let s = xs[0..8]; let q = {bad}; printLn(`{{q.x}}`); }}\n");
        for opt in ["0", "2"] {
            for backend in [emit::Backend::Llvm, emit::Backend::C] {
                let r = run_backend(&format!("oob{i}"), &src, opt, backend);
                assert_ne!(r.status, 0, "{bad} at {backend:?} opt {opt} must abort");
            }
        }
    }
}

#[test]
fn compile_errors() {
    rejects("fn main() { let v = vec4f(1.0); let i = 2; let x = v[i]; }\n", "constant 0..3");
    rejects("fn main() { let v = vec4f(1.0); let x = v[4]; }\n", "out of range");
    rejects("fn main() { let v = vec4f(1.0); let x = v.q; }\n", "no field `q`");
    rejects("fn main() { let v = vec4f(1.0); let w = vec4f(2.0); let c = v == w; }\n", "does not compare vectors");
    rejects("fn main() { let v = vec4f(1.0); let c = v < 2.0; }\n", "@vmin");
    rejects("fn main() { let v = vec4f(1.0); let i = vec4i(1); let x = v + i; }\n", "@vfloat");
    rejects("fn main() { let v = vec4f(1.0); let x = v & v; }\n", "takes integers");
    rejects("fn main() { let v = vec4f(1.0); let x = v % 2.0; }\n", "not defined on vectors");
    rejects("fn main() { let x = vec4f(1.0, 2.0); }\n", "4 lanes or 1 value");
    rejects("fn main() { let v = vec4f(1.0); let x = @vshuffle(v, 0, 1, 2, 4); }\n", "constants 0..3");
    rejects("fn main() { let v = vec4i(1); let x = @vsqrt(v); }\n", "takes a `vec4f`");
    rejects("fn main() { let v = vec4f(1.0); let x = @vint(v) + v; }\n", "needs matching operands");
    rejects("const V: vec4f = vec4f(1.0);\nfn main() { }\n", "const is not supported yet");
    rejects("#[singleton] let G: vec4f = vec4f(1.0);\nfn main() { }\n", "initializer");
}

#[test]
fn enum_payload_with_a_vector() {
    run(
        "enum",
        "enum E { A(vec4f), B(int), C, }
struct H { n: u8; e: E; m: u8; }
fn show(e: E) {
    match e {
        E.A(v) { printLn(`A {v.x} {v.w}`); }
        E.B(n) { printLn(`B {n}`); }
        E.C { printLn(`C`); }
    }
}
fn main() {
    let h = H { n: 1, e: E.A(vec4f(1.0, 2.0, 3.0, 4.0)), m: 2 };
    show(h.e);
    h.e = E.B(7);
    show(h.e);
    h.e = E.C;
    show(h.e);
    let xs: [2]E = [E.C, E.A(vec4f(5.0))];
    show(xs[1]);
    printLn(`{h.n} {h.m} {(&h.e) as int % 16}`);
}
",
        "A 1 4\nB 7\nC\nA 5 5\n1 2 0\n",
    );
}

#[test]
fn dump_ir_names_the_ops() {
    let src = format!("{HEAD}fn main() {{ let v = vec4f(1.0, 2.0, 3.0, 4.0); let s = @vshuffle(v, 3, 2, 1, 0); printLn(`{{@vsum(s) + s.x}}`); }}\n");
    let prog = compile_src("simd_dump", &src).expect("check");
    let ir = crate::ir::dump::dump_program(&prog.cx, &prog.modules);
    for want in ["vec4f", "simd.build", "simd.shuffle<3210>", "simd.sum", ".x"] {
        assert!(ir.contains(want), "missing {want} in:\n{ir}");
    }
}

/// Edge cases (NaN, -0.0, overflow, wide shifts, zero divisors): all four runs must print the same text.
fn agree(name: &str, body: &str) -> String {
    let src = format!("{HEAD}{body}");
    let mut first: Option<String> = None;
    for opt in ["0", "2"] {
        for backend in [emit::Backend::Llvm, emit::Backend::C] {
            let r = run_backend(name, &src, opt, backend);
            // the sign of a NaN made by 0/0 depends on constant folding (also true of scalars): compare as `nan`
            let out = format!("{} {}", r.status, String::from_utf8_lossy(&r.stdout).replace("-nan", "nan"));
            match &first {
                None => first = Some(out),
                Some(f) => assert_eq!(&out, f, "{backend:?} opt {opt} differs"),
            }
        }
    }
    first.unwrap()
}

#[test]
fn nan_zero_overflow_and_shifts_agree() {
    let out = agree(
        "edge",
        "fn f(v: vec4f) { printLn(`{v.x} {v.y} {v.z} {v.w}`); }
fn i(v: vec4i) { printLn(`{v.x} {v.y} {v.z} {v.w}`); }
fn main() {
    let z = vec4f(0.0);
    let nan = z / z;
    let a = vec4f(1.0, nan.x, 0.0, -0.0);
    let b = vec4f(nan.x, 2.0, -0.0, 0.0);
    f(@vmin(a, b));
    f(@vmax(a, b));
    f(@vmin(a, a));
    let big = vec4f(3000000000.0, -3000000000.0, nan.x, -2.9);
    i(@vint(big));
    i(@vint(vec4f(2147483520.0, 2147483648.0, -2147483648.0, 0.99)));
    let n = vec4i(-8, 8, 1, -1);
    i(n << vec4i(1, 31, 32, 33));
    i(n >> vec4i(1, 31, 32, 33));
    i(n << 100);
    let m = vec4i(2147483647, -2147483647 - 1, 5, 6);
    i(m + 1);
    i(m - 1);
    i(m * 2);
    i(-m);
    i(@vabs(m));
    i(@vmin(m, vec4i(0)));
    i(@vmax(m, vec4i(0)));
    i(~m);
    printLn(`{@vsum(m)} {@vsum(nan)}`);
    f(@vsqrt(vec4f(4.0, -1.0, 0.0, 2.0)));
    f(@vabs(vec4f(-0.0, -1.5, nan.x, 2.0)));
    f(-vec4f(0.0, -0.0, 1.0, 2.0));
    f(@vfloat(m));
    f(@vshuffle(a, 1, 1, 0, 3));
    let d = vec4i(7, 8, 9, 10) / vec4i(2, -1, 3, 0);
    i(d);
}
",
    );
    assert!(out.starts_with("102 nan 2 -0 0\n"), "{out}"); // vec4i / by a zero lane aborts after the rest
    let div = agree(
        "divmin",
        "fn main() { let m = vec4i(-2147483647 - 1, 1, 2, 3); let d = m / vec4i(-1, 1, 2, 3); printLn(`{d.x} {d.y} {d.w}`); }\n",
    );
    assert_eq!(div, "0 -2147483648 1 1\n");
}

#[test]
fn scripts_reject_vectors_with_a_span() {
    let main = "module lang;\nconst S = @script(\"scripts/s.j3s\");\nfn main() {\n    let s = S {};\n    s.run();\n}\n";
    let toml = "[package]\nname = \"lang\"\nentry = \"main.j3\"\n\n[emit]\nopt = 0\n";
    for script in ["pub fn run(v: vec4i) {\n}\n", "pub fn run() {\n    let v: vec4f = 1;\n}\n", "pub fn run() {\n    let v = vec4f(1.0);\n}\n", "pub fn run() {\n    let x = 1.0;\n    let s = @vsum(x);\n}\n"] {
        let m = temp_project("simd_script", &[("j3.toml", toml), ("main.j3", main), ("scripts/s.j3s", script)]);
        let e = match compile_manifest(&m) {
            Ok(_) => panic!("script vector accepted"),
            Err(e) => format!("{e:?}"),
        };
        assert!(e.contains("vec4f is not supported in scripts yet") && e.contains("s.j3s:"), "{e}");
    }
}
