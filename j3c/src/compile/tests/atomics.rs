use super::*;

const HEAD: &str = "module t;\nuse j3.io.printLn;\n";

fn run(name: &str, src: &str, want: &str) {
    for r in both_backends(name, src) {
        assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), want);
    }
}

/// Compiles `src` and expects an error mentioning `msg`.
fn rejects(src: &str, msg: &str) {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let tag = format!("atomic_err{}", N.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let e = compile_src(&tag, src).err().expect("expected an error");
    assert!(e.contains(msg), "{e}");
}

#[test]
fn every_type_every_op() {
    let src = format!(
        "{HEAD}fn main() {{
    let a: int = 5;
    printLn(`int {{@atomicAdd(&a, 3)}} {{@atomicSub(&a, 1)}} {{@atomicExchange(&a, 40)}} {{@atomicLoad(&a)}}`);
    @atomicStore(&a, 9);
    printLn(`int cas {{@atomicCas(&a, 9, 10)}} {{@atomicCas(&a, 9, 11)}} {{a}}`);
    let b: i32 = 5;
    printLn(`i32 {{@atomicAdd(&b, 3)}} {{@atomicSub(&b, 10)}} {{@atomicExchange(&b, 0 - 4)}} {{@atomicLoad(&b)}}`);
    printLn(`i32 cas {{@atomicCas(&b, 0 - 4, 6)}} {{@atomicCas(&b, 0 - 4, 7)}} {{b}}`);
    let c: u32 = 1;
    printLn(`u32 {{@atomicAdd(&c, 2)}} {{@atomicSub(&c, 4)}} {{@atomicLoad(&c)}}`);
    let d: u64 = 1;
    printLn(`u64 {{@atomicAdd(&d, 2)}} {{@atomicExchange(&d, 77)}} {{@atomicLoad(&d)}}`);
    printLn(`u64 cas {{@atomicCas(&d, 77, 78)}} {{@atomicCas(&d, 77, 79)}} {{d}}`);
    let f = false;
    printLn(`bool {{@atomicLoad(&f)}} {{@atomicExchange(&f, true)}} {{@atomicCas(&f, true, false)}} {{@atomicCas(&f, true, false)}} {{@atomicLoad(&f)}}`);
    @atomicStore(&f, true);
    printLn(`bool store {{@atomicLoad(&f)}}`);
    let x = 7;
    let y = 8;
    let q: ptr = &x as ptr;
    let same = @atomicLoad(&q) == (&x as ptr);
    let old = @atomicExchange(&q, &y as ptr);
    let swapped = @atomicCas(&q, &y as ptr, &x as ptr);
    let back = @atomicLoad(&q) == (&x as ptr);
    printLn(`ptr {{same}} {{old == (&x as ptr)}} {{swapped}} {{back}}`);
}}
"
    );
    run(
        "atomic_types",
        &src,
        "int 5 8 7 40\nint cas true false 10\ni32 5 8 -2 -4\ni32 cas true false 6\n\
         u32 1 3 4294967295\nu64 1 3 77\nu64 cas true false 78\n\
         bool false false true false false\nbool store true\nptr true true true true\n",
    );
}

/// i32/u32 are 4-byte operations: a neighbour in the same struct is untouched by wraparound.
#[test]
fn narrow_widths_leave_neighbours() {
    let src = format!(
        "{HEAD}struct Pair {{ a: u32 = 0; b: u32 = 5; }}
fn main() {{
    let p = Pair {{}};
    @atomicSub(&p.a, 1);
    printLn(`{{p.a}} {{p.b}}`);
    @atomicAdd(&p.a, 1);
    printLn(`{{p.a}} {{p.b}}`);
}}
"
    );
    run("atomic_narrow", &src, "4294967295 5\n0 5\n");
}

#[test]
fn every_valid_order_runs() {
    let mut body = String::from("    let v: int = 1;\n    let t = 0;\n");
    let all = ["relaxed", "acquire", "release", "acqrel", "seqcst"];
    for o in all {
        if o != "release" && o != "acqrel" {
            body += &format!("    t += @atomicLoad(&v, {o});\n");
        }
        if o != "acquire" && o != "acqrel" {
            body += &format!("    @atomicStore(&v, 1, {o});\n");
        }
        body += &format!("    t += @atomicAdd(&v, 0, {o});\n    t += @atomicSub(&v, 0, {o});\n");
        body += &format!("    t += @atomicExchange(&v, 1, {o});\n");
        body += &format!("    if @atomicCas(&v, 1, 1, {o}) {{ t += 1; }}\n");
    }
    // 3 loads, then add/sub/xchg each return the old 1 for 5 orders, plus 5 cas wins
    let src = format!("{HEAD}fn main() {{\n{body}    printLn(`{{t}}`);\n}}\n");
    run("atomic_orders", &src, "23\n");
}

#[test]
fn bad_orders_name_the_valid_ones() {
    let f = |call: &str| format!("{HEAD}fn main() {{\n    let v: int = 1;\n    {call};\n}}\n");
    rejects(&f("@atomicLoad(&v, release)"), "relaxed, acquire, seqcst");
    rejects(&f("@atomicLoad(&v, acqrel)"), "cannot use order `acqrel`");
    rejects(&f("@atomicStore(&v, 1, acquire)"), "relaxed, release, seqcst");
    rejects(&f("@atomicAdd(&v, 1, fast)"), "takes a memory order");
    rejects(&f("@atomicCas(&v, 1, 2, acquire, acquire)"), "optional order");
}

#[test]
fn bad_types_are_errors() {
    let f = |decl: &str, call: &str| format!("{HEAD}fn main() {{\n    {decl}\n    {call};\n}}\n");
    rejects(&f("let v = true;", "@atomicAdd(&v, true)"), "cannot work on `bool`");
    rejects(&f("let v: f64 = 1.0;", "@atomicLoad(&v)"), "cannot work on `float`");
    rejects(&f("let v: u8 = 1;", "@atomicLoad(&v)"), "cannot work on `u8`");
}

/// Release/acquire handoff between two real threads.
#[test]
fn release_acquire_handoff() {
    let src = format!(
        "{HEAD}use j3.thread.{{ Thread, spawn, join }};
struct Shared {{ data: int = 0; flag: u32 = 0; }}
fn writer(p: ptr) {{
    let s = p as &Shared;
    s.data = 4242;
    @atomicStore(&s.flag, 1, release);
}}
fn main() {{
    let s = Shared {{}};
    let t = Thread {{ f: writer }};
    spawn(&t, \"writer\", writer, &s as ptr);
    while @atomicLoad(&s.flag, acquire) == 0 {{}}
    printLn(`{{s.data}}`);
    join(&t);
}}
"
    );
    run("atomic_handoff", &src, "4242\n");
}

#[test]
fn vm_rejects_atomics() {
    let main = "module lang;\nconst S = @script(\"scripts/s.j3s\");\nfn main() {\n    let s = S {};\n    s.run();\n}\n";
    let toml = "[package]\nname = \"lang\"\nentry = \"main.j3\"\n\n[emit]\nopt = 0\n";
    let script = "pub fn run() {\n    let v: int = 1;\n    @atomicLoad(&v, acquire);\n}\n";
    let m = temp_project("atomic_script", &[("j3.toml", toml), ("main.j3", main), ("scripts/s.j3s", script)]);
    let prog = compile_manifest(&m).expect("check");
    let e = super::super::dev::bytecode(&prog).err().expect("script atomic accepted");
    assert!(e.contains("`@atomic*` isn't supported in scripts"), "{e}");
}
