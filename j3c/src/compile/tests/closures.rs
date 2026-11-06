use super::*;

fn run(name: &str, src: &str, want: &str) {
    for r in both_backends(name, src) {
        assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), want);
    }
}

/// The error for `src`, with the 1-based line its span points at.
fn err_line(src: &str) -> (check::CheckError, usize) {
    match check::check_files(vec![parse_file(1, "t.j3", src)]) {
        Ok(_) => panic!("expected a check error"),
        Err(e) => {
            let line = parse::line_of(src, e.span.start);
            (e, line)
        }
    }
}

/// Asserts the error mentions `msg`, has a fix hint, and points at line `line`.
fn rejects(src: &str, msg: &str, line: usize) {
    let (e, at) = err_line(src);
    assert!(e.message.contains(msg), "{}", e.message);
    assert!(!e.helps.is_empty(), "no hint: {}", e.message);
    assert_eq!(at, line, "{}", e.message);
}

/// Like `rejects` for errors that already existed without a hint (shadowing, loops).
fn rejects_plain(src: &str, msg: &str, line: usize) {
    let (e, at) = err_line(src);
    assert!(e.message.contains(msg), "{}", e.message);
    assert_eq!(at, line, "{}", e.message);
}

const HEAD: &str = "module t;\nuse j3.io.printLn;\n";

#[test]
fn captures_by_address_both_backends() {
    run(
        "cl_addr",
        &format!("{HEAD}fn main() {{
    let total = 0;
    let c = fn(x: int): int {{ total += x; return total; }};
    c(2);
    c(3);
    printLn(`total={{total}}`);
    let hold = 0;
    for i in 0..3 {{
        let each = fn(): int {{ return i; }};
        hold += each() * 10 + 1;
    }}
    printLn(`hold={{hold}}`);
}}\n"),
        "total=5\nhold=33\n",
    );
}

#[test]
fn nested_this_and_defer_both_backends() {
    run(
        "cl_nest",
        &format!("{HEAD}struct C {{
    n: int;
    fn go(this: &C, by: int): int {{
        let add = fn(k: int) {{ this.n += k * by; }};
        add(1);
        add(2);
        return this.n;
    }}
}}
fn main() {{
    let t = 0;
    let outer = fn(a: int): int {{
        let inner = fn(b: int): int {{ t += 1; return a + b; }};
        return inner(a) + inner(1);
    }};
    printLn(`{{outer(2)}} t={{t}}`);
    let c = C {{ n: 0 }};
    printLn(`{{c.go(5)}}`);
    let d = 1;
    let early = fn(): int {{
        defer {{ d += 100; }}
        if d == 1 {{ return 7; }}
        return 8;
    }};
    printLn(`{{early()}} d={{d}}`);
}}\n"),
        "7 t=2\n15\n7 d=101\n",
    );
}

#[test]
fn generic_instances_get_distinct_closures() {
    run(
        "cl_gen",
        &format!("{HEAD}struct Box<T> {{
    v: T;
    fn get(this: &Box<T>): T {{
        let g = fn(): T {{ return this.v; }};
        return g();
    }}
}}
fn main() {{
    let a = Box<int> {{ v: 4 }};
    let b = Box<float> {{ v: 2.5 }};
    printLn(`{{a.get()}} {{b.get()}}`);
}}\n"),
        "4 2.5\n",
    );
}

#[test]
fn fn_converts_to_closure_and_back_only_when_plain() {
    run(
        "cl_fn",
        &format!("{HEAD}fn triple(x: int): int {{ return x * 3; }}
fn apply(f: closure(int): int, x: int): int {{ return f(x); }}
fn plain(f: fn(int): int, x: int): int {{ return f(x); }}
fn main() {{
    let k = 10;
    printLn(`{{apply(triple, 2)}} {{apply(fn(x) {{ return x + k; }}, 1)}}`);
    let f: fn(int): int = fn(x: int): int {{ return x + 1; }};
    printLn(`{{plain(f, 1)}} {{plain(fn(x: int): int {{ return x - 1; }}, 1)}}`);
}}\n"),
        "6 11\n2 0\n",
    );
}

#[test]
fn closure_errors_have_spans_and_hints() {
    let pre = "module t;\nfn apply(f: closure(int): int): int { return f(1); }\n";
    // plain fn param, thread-style
    rejects(
        &format!("{pre}fn plain(f: fn(int): int) {{}}\nfn main() {{\n    let k = 1;\n    plain(fn(x: int): int {{ return x + k; }});\n}}\n"),
        "captures `k`",
        6,
    );
    rejects(
        &format!("{pre}fn plain(f: fn(int): int) {{}}\nfn main() {{\n    let c = fn(x: int): int {{ return x; }};\n    plain(c);\n}}\n"),
        "cannot become a plain fn or ptr",
        6,
    );
    rejects(
        &format!("{pre}fn main() {{\n    let c = fn(x: int): int {{ return x; }};\n    let p = c as ptr;\n}}\n"),
        "cannot be cast",
        5,
    );
    rejects(
        &format!("{pre}fn main() {{\n    let c = fn(x: int): int {{ return x; }};\n    let p: ptr = c;\n}}\n"),
        "cannot become a plain fn or ptr",
        5,
    );
    rejects(
        &format!("{pre}fn main() {{\n    let c = fn(x: int): int {{ return x; }};\n    let r = &c;\n}}\n"),
        "borrowed with `&`",
        5,
    );
    rejects(
        &format!("{pre}fn main() {{\n    let c = fn(x: int): int {{ return x; }};\n    let a = [c, c];\n}}\n"),
        "an array element",
        5,
    );
    rejects(&format!("{pre}struct S {{\n    f: closure(int): int;\n}}\nfn main() {{}}\n"), "a struct field", 4);
    rejects(&format!("{pre}#[singleton]\nlet g: closure(int): int;\nfn main() {{}}\n"), "not allowed here", 4);
    rejects(&format!("{pre}fn mk(): closure(int): int {{ return apply; }}\nfn main() {{}}\n"), "returned", 3);
    rejects(&format!("{pre}fn f(a: [2]closure(int): int) {{}}\nfn main() {{}}\n"), "an array element", 3);
    rejects(&format!("{pre}fn f(a: []closure(int): int) {{}}\nfn main() {{}}\n"), "a slice element", 3);
    rejects(&format!("{pre}fn f(a: &closure(int): int) {{}}\nfn main() {{}}\n"), "behind `&`", 3);
    rejects(
        &format!("{pre}struct L<T> {{\n    v: T;\n}}\nfn f(a: L<closure(int): int>) {{}}\nfn main() {{}}\n"),
        "a generic type argument",
        6,
    );
    rejects(
        &format!("{pre}enum E {{\n    A(closure(int): int)\n}}\nfn main() {{}}\n"),
        "not allowed here",
        4,
    );
    rejects(
        &format!("{pre}fn f(g: fn(closure(int): int)) {{}}\nfn main() {{}}\n"),
        "plain `fn(..)`",
        3,
    );
    rejects(
        &format!("{pre}fn main() {{\n    let c = fn(): closure(int): int {{ return apply; }};\n}}\n"),
        "returned",
        4,
    );
}

#[test]
fn closure_scope_errors() {
    let pre = "module t;\n";
    rejects_plain(
        &format!("{pre}fn main() {{\n    let k = 1;\n    let c = fn(x: int) {{\n        let k = 2;\n    }};\n}}\n"),
        "shadowing `k`",
        5,
    );
    rejects_plain(
        &format!("{pre}fn main() {{\n    let k = 1;\n    let c = fn(k: int) {{ }};\n}}\n"),
        "shadowing `k`",
        4,
    );
    rejects(
        &format!("{pre}fn main() {{\n    while true {{\n        let c = fn() {{ break; }};\n    }}\n}}\n"),
        "`break` outside a loop",
        4,
    );
    rejects(
        &format!("{pre}fn main() {{\n    'o: while true {{\n        let c = fn() {{ continue 'o; }};\n    }}\n}}\n"),
        "no enclosing loop is labeled `'o`",
        4,
    );
    rejects_plain(
        &format!("{pre}fn main() {{\n    let c = fn(x) {{ }};\n}}\n"),
        "needs a type here",
        3,
    );
    rejects_plain(
        &format!("{pre}fn ap(f: closure(int, int)) {{}}\nfn main() {{\n    ap(fn(x) {{ }});\n}}\n"),
        "has 1 parameter(s) but 2",
        4,
    );
}

#[test]
fn closure_return_returns_from_the_closure() {
    run(
        "cl_ret",
        &format!("{HEAD}fn call(f: closure(int): int): int {{ return f(3) + 1; }}
fn main() {{
    let r = call(fn(x) {{
        if x > 2 {{ return 10; }}
        return 0;
    }});
    printLn(`{{r}}`);
}}\n"),
        "11\n",
    );
}

#[test]
fn closures_in_scripts_are_rejected() {
    let main = "module lang;\nconst S = @script(\"scripts/s.j3s\");\nfn main() {\n    let s = S {};\n    s.run();\n}\n";
    let toml = "[package]\nname = \"lang\"\nentry = \"main.j3\"\n\n[emit]\nopt = 0\n";
    for script in [
        "pub fn run() {\n    let f = fn(x: int): int { return x; };\n}\n",
        "pub fn run(f: closure(int): int) {\n}\n",
    ] {
        let m = temp_project("cl_script", &[("j3.toml", toml), ("main.j3", main), ("scripts/s.j3s", script)]);
        let e = match compile_manifest(&m) {
            Ok(_) => panic!("script closure accepted"),
            Err(e) => format!("{e:?}"),
        };
        assert!(e.contains("closures are not supported in .j3s scripts"), "{e}");
    }
}

#[test]
fn closure_cannot_reach_thread_spawn() {
    let src = format!(
        "{HEAD}use j3.thread.{{ Thread, spawn }};\nfn start(t: &Thread, k: int) {{\n    spawn(t, \"w\", fn(p: ptr) {{ printLn(`{{k}}`); }}, 0);\n}}\nfn main() {{}}\n"
    );
    let e = compile_src("cl_spawn", &src).err().expect("spawn must reject a capturing closure");
    assert!(e.contains("captures `k`"), "{e}");
}

#[test]
fn no_capture_literal_is_a_plain_fn() {
    run(
        "cl_static",
        &format!("{HEAD}fn run(f: fn(int): int): int {{ return f(4); }}
fn main() {{
    printLn(`{{run(fn(x: int): int {{ return x * x; }})}}`);
}}\n"),
        "16\n",
    );
}

#[test]
fn closure_intrinsics_are_stdlib_only_and_round_trip() {
    let user_j3 = "module j3;\nfn f(c: closure(int): int): ptr { return @closureFn(c); }\nfn main() {}\n";
    let (e, _) = err_line(user_j3);
    assert!(e.message.contains("standard library only"), "{}", e.message);
    let sneaky = crate::stdlib::stdlib_dir().join("../escape/gate.j3");
    let (e, _) = match check::check_files(vec![parse_file(1, sneaky.to_str().unwrap(), user_j3)]) {
        Ok(_) => panic!("`..` path passed the stdlib gate"),
        Err(e) => (e, 0),
    };
    assert!(e.message.contains("standard library only"), "{}", e.message);
    let user = "module t;\nfn f(c: closure(int): int): ptr { return @closureFn(c); }\nfn main() {}\n";
    let (e, line) = err_line(user);
    assert!(e.message.contains("standard library only") && !e.helps.is_empty(), "{}", e.message);
    assert_eq!(line, 2);

    let lib = "module j3.cltest;\npub fn through(c: closure(int): int, x: int): int {\n    let f = @closureFn(c);\n    let k = @closureCtx(c);\n    let again: closure(int): int = @closureFrom(f, k);\n    return again(x);\n}\n";
    let app = "module app;\nuse j3.cltest.{ through };\nfn main() {\n    let base = 40;\n    @syscall(60, through(fn(x) { return x + base; }, 2));\n}\n";
    let prog = check::check_files(vec![parse_file(1, crate::stdlib::stdlib_dir().join("j3/cltest.j3").to_str().unwrap(), lib), parse_file(2, "app.j3", app)]).expect("check");
    for backend in [emit::Backend::Llvm, emit::Backend::C] {
        let mut opts = llvm_opts();
        opts.backend = backend;
        let r = emit::run_program(&prog.cx, &prog.modules, &opts).expect("run");
        // no stdlib in `check_files`: the answer is the exit status (`exit` is syscall 60 on x86-64 Linux)
        assert_eq!(r.status, 42, "{backend:?}: {}", String::from_utf8_lossy(&r.stderr));
    }
}

/// Optimized builds must match opt 0: captures by address, a loop variable, and `defer`.
#[test]
fn closures_at_opt2_and_s_match_both_backends() {
    let src = format!("{HEAD}fn apply(f: closure(int): int, x: int): int {{ return f(x); }}
fn main() {{
    let total = 0;
    let base = 10;
    let c = fn(x: int): int {{ total += x; return base + x; }};
    printLn(`{{c(1)}} {{c(2)}} total={{total}}`);
    printLn(`apply={{apply(fn(x) {{ return x * base; }}, 4)}}`);
    let hold = 0;
    for i in 0..3 {{
        let each = fn(): int {{ return i; }};
        hold += each() * 10 + 1;
    }}
    printLn(`hold={{hold}}`);
    let d = 1;
    let early = fn(): int {{
        defer {{ d += 100; }}
        if d == 1 {{ return 7; }}
        return 8;
    }};
    printLn(`early={{early()}} d={{d}}`);
}}\n");
    let want = "11 12 total=3\napply=40\nhold=33\nearly=7 d=101\n";
    for opt in ["2", "s"] {
        for r in both_backends_opt("cl_opt", &src, opt) {
            assert_eq!(r.status, 0, "opt {opt}: {}", String::from_utf8_lossy(&r.stderr));
            assert_eq!(String::from_utf8_lossy(&r.stdout), want, "opt {opt}");
        }
    }
}

/// `Pool.parallelForEach`: a closure over worker ranges, writing `out[i]`, on both backends.
#[test]
fn pool_parallel_for_each_both_backends() {
    let src = "module t;
use j3.io.printLn;
use j3.memory.{ Arena };
use j3.thread.{ Pool };
fn main() {
    let arena = Arena.create(1 << 20);
    let pool = Pool.create(&arena, 3, 16);
    let n = 1000;
    let out = arena.alloc::<int>(n);
    let scale = 2;
    pool.parallelForEach(n, fn(lo, hi) {
        for i in lo..hi { out[i] = i * scale; }
    });
    let total = 0;
    for i in 0..n { total += out[i]; }
    printLn(`total={total}`);
    pool.shutdown();
}
";
    run("cl_pool_each", src, "total=999000\n");
}
