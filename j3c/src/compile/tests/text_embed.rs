//! `t.fmt(`..`)` into a `Text` (no 512-byte cap) and `@embed*` files.

use super::*;

/// Every primitive, a string, a `#[Debug]` struct and enum: `t.fmt(`..`)` gives the same bytes as
/// the capped `"{..}"` path, on LLVM, C and the VM; past 512 bytes only `Text` works.
#[test]
fn text_fmt_matches_capped_interpolation_three_ways() {
    three_ways_with(
        "text_fmt",
        &[("src/dbg.j3", "module dbg;\n#[Debug]\npub struct P { x: int; name: string; }\n#[Debug]\npub struct Box2 { p: P; on: bool; }\n#[Debug]\npub enum E { A, B(int, string), C(P) }\n")],
        "use j3.io.printLn;\n\
use j3.memory.Arena;\n\
use j3.text.Text;\n\
use dbg.{ P, Box2, E };\n\
pub fn run() {\n\
    let arena = Arena.create(65536);\n\
    let t = Text.new(&arena);\n\
    let a: i8 = -8; let b: i16 = -300; let c: i32 = 70000; let d = -9000000000;\n\
    let e: u8 = 250; let f: u16 = 65000; let g: u32 = 4000000000; let h: u64 = 12345;\n\
    let x: f32 = 0.25; let y = 1.0 / 3.0; let s = \"str\";\n\
    let p = Box2 { p: P { x: 3, name: \"n\" }, on: false };\n\
    let ev = E.B(7, \"w\");\n\
    let capped = `{a} {b} {c} {d} {e} {f} {g} {h} {x} {y} {true} {s} {p} {ev} {E.A} {E.C(P { x: 1, name: \"q\" })}`;\n\
    t.fmt(`{a} {b} {c} {d} {e} {f} {g} {h} {x} {y} {true} {s} {p} {ev} {E.A} {E.C(P { x: 1, name: \"q\" })}`);\n\
    printLn(t.str());\n\
    printLn(if t.str() == capped { \"same\" } else { capped });\n\
    t.clear();\n\
    for i in 0..300 { t.fmt(`{i};`); }\n\
    let long = t.str();\n\
    t.fmt(`[{long}]`).fmt(`!`);\n\
    printLn(`len={t.len()}`);\n\
    let big = Text.new(&arena);\n\
    big.fmt(`{p}`);\n\
    for i in 0..40 { big.pushStr(\"0123456789\"); }\n\
    let q = P { x: 9, name: big.str() };\n\
    t.clear();\n\
    t.fmt(`{q}`);\n\
    printLn(`debug len={t.len()}`);\n\
}\n",
        "-8 -300 70000 -9000000000 250 65000 4000000000 12345 0.25 0.333333 true str Box2 { p: P { x: 3, name: n }, on: false } E.B(7, w) E.A E.C(P { x: 1, name: q })\nsame\nlen=2183\ndebug len=460\n",
    );
}

/// `pushU64`, `pushHex`, `pushF64` shortest round-trip, `pushChar`, `indent`, `pushLine`, `flushTo`.
#[test]
fn text_push_helpers() {
    let src = "module t;\n\
use j3.io.printLn;\n\
use j3.memory.Arena;\n\
use j3.text.Text;\n\
fn main() {\n\
    let arena = Arena.create(65536);\n\
    let t = Text.new(&arena);\n\
    t.pushU64(~u64(0)); t.push(' '); t.pushU64(0); t.push(' ');\n\
    t.pushHex(0xdeadbeef); t.push(' '); t.pushHex(0); t.push(' ');\n\
    t.pushF64(0.1); t.push(' '); t.pushF64(1.0 / 3.0); t.push(' '); t.pushF64(-2.5); t.push(' ');\n\
    t.pushChar('A'); t.pushChar(0xE9); t.pushChar(0x20AC); t.pushChar(0x1F600); t.pushChar(0xD800);\n\
    t.indent(3); t.pushLine(\"|\");\n\
    let ok = t.flushTo(1);\n\
    printLn(`ok={ok} left={t.len()}`);\n\
}\n";
    let want = "18446744073709551615 0 deadbeef 0 0.1 0.3333333333333333 -2.5 A\u{e9}\u{20ac}\u{1F600}\u{FFFD}   |\nok=true left=0\n";
    for r in both_backends("text_push", src) {
        assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), want);
    }
}

/// 10 MB written through one `Text` with `flushTo`: the arena never grows past its first
/// 1 MiB chunk (the program checks `arena.cap` and `arena.used`), so memory stays bounded
/// however big the output gets.
#[test]
fn text_10mb_output_stays_bounded() {
    let src = "module t;\n\
use j3.memory.Arena;\n\
use j3.sys.exit;\n\
use j3.text.Text;\n\
fn main() {\n\
    let arena = Arena.create(1 << 20);\n\
    let t = Text.new(&arena);\n\
    for i in 0..400000 {\n\
        t.fmt(`define i64 @f{i}(i64 %x) ; {i % 7}`).push('\\n');\n\
        if t.len() > 65536 { if !t.flushTo(1) { exit(3); } }\n\
    }\n\
    t.flushTo(1);\n\
    if arena.cap != 1 << 20 || arena.used > 262144 { exit(2); }\n\
}\n";
    let want: usize = (0..400000u64).map(|i| format!("define i64 @f{i}(i64 %x) ; {}\n", i % 7).len()).sum();
    assert!(want > 10_000_000);
    for r in both_backends("text_10mb", src) {
        assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(r.stdout.len(), want);
        assert!(r.stdout.starts_with(b"define i64 @f0(i64 %x) ; 0\n"));
    }
}

/// `@embed` text, `@embedBytes` with NUL and 0xFF bytes (exact round trip), `@embedDir` sorted
/// with subfolders; a path beside the file or at the package root; LLVM, C and the VM.
#[test]
fn embed_files_three_ways() {
    three_ways_with(
        "embed",
        &[
            ("scripts/near.txt", "beside the script\n"),
            ("rootonly.bin", "A\0B\u{7f}\0\n\0"),
            ("assets/b.txt", "bee"),
            ("assets/sub/a.txt", "ay"),
            ("assets/skip.bin", "no"),
        ],
        "use j3.io.printLn;\n\
const NEAR = @embed(\"near.txt\");\n\
const BIN = @embedBytes(\"rootonly.bin\");\n\
pub fn run() {\n\
    printLn(`near={NEAR.len} {NEAR}`);\n\
    let s = \"\";\n\
    for i in 0..BIN.len { s = `{s}{BIN[i]},`; }\n\
    printLn(`bin={BIN.len} {s}`);\n\
    let inline = @embed(\"assets/b.txt\");\n\
    printLn(inline);\n\
}\n",
        "near=18 beside the script\n\nbin=7 65,0,66,127,0,10,0,\nbee\n",
    );
}

/// `@embedDir` (searched in subfolders, sorted by path, glob on the file name; the folder found
/// at the package root) on LLVM and C.
#[test]
fn embed_dir_table() {
    let m = temp_project(
        "embed_dir",
        &[
            ("j3.toml", "[package]\nname = \"ed\"\nentry = \"src/main.j3\"\n"),
            ("assets/b.txt", "bee"),
            ("assets/sub/a.txt", "ay"),
            ("assets/skip.bin", "no"),
            ("assets/.hidden.txt", "no"),
            (
                "src/main.j3",
                "module ed;\nuse j3.io.printLn;\nconst ALL = @embedDir(\"assets\", \"*.txt\");\nfn main() {\n    for i in 0..ALL.len { printLn(`{ALL[i].path}={ALL[i].data}`); }\n}\n",
            ),
        ],
    );
    let prog = compile_manifest(&m).expect("check");
    for backend in [emit::Backend::Llvm, emit::Backend::C] {
        let mut opts = llvm_opts();
        opts.backend = backend;
        let r = emit::run_program(&prog.cx, &prog.modules, &opts).expect("run");
        assert_eq!(r.status, 0, "{backend:?}: {}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), "b.txt=bee\nsub/a.txt=ay\n", "{backend:?}");
    }
    let _ = std::fs::remove_dir_all(&m.root);
}

/// A large binary embed (every byte value, 1 MiB) round-trips on both backends.
#[test]
fn embed_large_binary_round_trips() {
    let dir = std::env::temp_dir().join(format!("j3-embed-big-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let data: Vec<u8> = (0..1u32 << 20).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect();
    std::fs::write(dir.join("big.bin"), &data).unwrap();
    let file = dir.join("main.j3");
    std::fs::write(
        &file,
        "module t;\nuse j3.io.printLn;\nconst BIG = @embedBytes(\"big.bin\");\nfn main() {\n    let h: u64 = 1469598103934665603;\n    for i in 0..BIG.len { h = (h ^ u64(BIG[i])) * 1099511628211; }\n    printLn(`{BIG.len} {h}`);\n}\n",
    )
    .unwrap();
    let prog = compile_path(&file).expect("check");
    let mut h: u64 = 1469598103934665603;
    for b in &data {
        h = (h ^ *b as u64).wrapping_mul(1099511628211);
    }
    let want = format!("{} {}\n", data.len(), h as i64);
    for backend in [emit::Backend::Llvm, emit::Backend::C] {
        let mut opts = llvm_opts();
        opts.backend = backend;
        let r = emit::run_program(&prog.cx, &prog.modules, &opts).expect("run");
        assert_eq!(r.status, 0, "{backend:?}: {}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), want, "{backend:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A `.j3s` script cannot hold an `@embedDir` table (the VM global image has one constant pointer):
/// the bytecode build says so instead of zeroing it.
#[test]
fn embed_dir_in_a_script_is_a_clear_vm_error() {
    let main = "module lang;\nconst S = @script(\"scripts/s.j3s\");\nfn main() {\n    let s = S {};\n    s.run();\n}\n";
    let toml = "[package]\nname = \"lang\"\nentry = \"main.j3\"\n";
    let script = "use j3.io.printLn;\nconst ALL = @embedDir(\"assets\", \"*.txt\");\npub fn run() {\n    printLn(ALL[0].path);\n}\n";
    let m = temp_project("embed_vm", &[("j3.toml", toml), ("main.j3", main), ("scripts/s.j3s", script), ("assets/a.txt", "a")]);
    let prog = compile_manifest_dev(&m).expect("dev check");
    let e = dev::bytecode(&prog).err().expect("a VM error");
    assert!(e.contains("`ALL`") && e.contains("cannot hold"), "{e}");
    let _ = std::fs::remove_dir_all(&m.root);
}

#[test]
fn embed_errors_name_the_path() {
    let e = compile_src("embed_missing", "module t;\nconst X = @embed(\"no/such/file.txt\");\nfn main() {}\n").err().expect("an error");
    assert!(e.contains("no/such/file.txt") && e.contains("@embed"), "{e}");
    let e = compile_src("embed_nodir", "module t;\nconst X = @embedDir(\"nodir\", \"*.j3\");\nfn main() {}\n").err().expect("an error");
    assert!(e.contains("nodir") && e.contains("folder"), "{e}");
    let dir = std::env::temp_dir().join(format!("j3-embed-let-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("d")).unwrap();
    std::fs::write(dir.join("d/a.txt"), "a").unwrap();
    std::fs::write(dir.join("main.j3"), "module t;\nfn main() {\n    let x = @embedDir(\"d\", \"*.txt\");\n}\n").unwrap();
    let e = compile_path(&dir.join("main.j3")).err().expect("an error");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(e.contains("only works as a const"), "{e}");
    // the source file embeds itself (`compile_src` writes it as `j3c_<tag>_<pid>.j3`)
    let me = format!("j3c_embed_ty_{}.j3", std::process::id());
    let e = compile_src("embed_ty", &format!("module t;\nconst X: string = @embedBytes(\"{me}\");\nfn main() {{}}\n")).err().expect("an error");
    assert!(e.contains("gives a `[]u8`"), "{e}");
    let e = compile_src("embed_arg", "module t;\nconst P = \"x\";\nconst X = @embed(P);\nfn main() {}\n").err().expect("an error");
    assert!(e.contains("string literal"), "{e}");
}

#[test]
fn frame_interpolation_cannot_escape() {
    let ret = check_err("module t;\nfn f(n: int): string { return `n={n}`; }\nfn main() {}\n");
    assert!(ret.contains("cannot be returned"), "{ret}");
    let lit = check_err("module t;\nstruct P { s: string; }\nfn f(n: int): P { return P { s: `n={n}` }; }\nfn main() {}\n");
    assert!(lit.contains("cannot be returned"), "{lit}");
    let global = check_err("module t;\n#[singleton]\nlet G: string = \"\";\nfn set(n: int) { G = `n={n}`; }\nfn main() { set(2); }\n");
    assert!(global.contains("outlives the function"), "{global}");
    // Static text, a lone `{s}`, a local use, and a local reassigned to a literal stay legal.
    let src = "module t;\nuse j3.io.printLn;\n\
fn lit(): string { return `fixed`; }\n\
fn same(s: string): string { return `{s}`; }\n\
fn reset(n: int): string { let s = `n={n}`; printLn(s); s = \"done\"; return s; }\n\
struct P { s: string; }\n\
fn main() { let n = 7; let p = P { s: `local {n}` }; printLn(p.s); printLn(lit()); printLn(same(\"x\")); printLn(reset(n)); }\n";
    for r in both_backends("interp_frame", src) {
        assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), "local 7\nfixed\nx\nn=7\ndone\n");
    }
}

#[test]
fn frame_interpolation_closures_have_own_frame() {
    // a closure body's locals are its own: the creator's frame strings neither leak in nor out
    let src = "module t;\nuse j3.io.printLn;\n\
fn f(a: string, b: string, c: string, d: string): string {\n\
    let g = fn(n: int) { let s = `{n}x`; let s2 = `{n}y`; printLn(s); printLn(s2); };\n\
    g(1);\n    return d;\n}\n\
fn main() {\n    let n = 3;\n    let a = `x{n}`;\n    let k = fn(p1: string, p2: string, p3: string, p4: string): string { return p4; };\n\
    printLn(a);\n    printLn(k(\"q\", \"r\", \"s\", \"t\"));\n    printLn(f(\"a\", \"b\", \"c\", \"d\"));\n}\n";
    for r in both_backends("interp_frame_cl", src) {
        assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), "x3\nt\n1x\n1y\nd\n");
    }
    // only the entry `main` itself may set a global to one, not a closure inside it
    let e = check_err("module t;\n#[singleton]\nlet G: string = \"\";\nfn main() { let set = fn(n: int) { G = `n={n}`; }; set(2); }\n");
    assert!(e.contains("outlives the function"), "{e}");
}

/// `src` (with the stdlib) must fail to check; the rendered errors.
fn frame_errs(name: &str, src: &str) -> String {
    let dir = std::env::temp_dir().join(format!("j3-frame-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.j3");
    std::fs::write(&file, src).unwrap();
    match compile_path(&file) {
        Ok(_) => panic!("{name}: expected a frame-string error"),
        Err(e) => e,
    }
}

const KEEPERS: &str = "module t;\nuse j3.io.printLn;\nuse j3.memory.{Arena, List};\nuse j3.text.Text;\n\
#[singleton]\nlet G: string = \"\";\n\
fn keep(s: string) { G = s; }\n\
fn look(s: string): int { return s.len; }\n\
fn pass(s: string) { keep(s); }\n\
fn rec(s: string, n: int) { if n == 0 { keep(s); } else { rec(s, n - 1); } }\n\
fn ping(s: string, n: int) { if n > 0 { pong(s, n - 1); } }\n\
fn pong(s: string, n: int) { if n == 0 { keep(s); } else { ping(s, n); } }\n";

/// A frame string passed to a fn that keeps it (directly, two calls down, through recursion and
/// mutual recursion, into a `List`) is an error at the backtick, with a note at the keeping store.
#[test]
fn frame_string_passed_to_a_keeping_fn() {
    let cases = [
        ("direct", "fn f(n: int) { keep(`e{n}`); }", "passed to `keep`, which keeps it"),
        ("transitive", "fn f(n: int) { pass(`e{n}`); }", "passed to `pass`, which keeps it"),
        ("recursive", "fn f(n: int) { rec(`e{n}`, 3); }", "passed to `rec`, which keeps it"),
        ("mutual", "fn f(n: int) { ping(`e{n}`, 3); }", "passed to `ping`, which keeps it"),
        ("list", "fn f(a: &Arena, n: int): int { let l = List<string>.new(a); l.push(`e{n}`); return l.len; }", "passed to `List.push`, which keeps it"),
        ("via_local", "fn f(n: int) { let s = `e{n}`; let u = s; pass(u); }", "passed to `pass`, which keeps it"),
    ];
    for (name, f, want) in cases {
        let e = frame_errs(name, &format!("{KEEPERS}{f}\nfn main() {{ }}\n"));
        assert!(e.contains(want), "{name}: {e}");
        // a container keeps it at the call, which is the error's own line: no note then
        assert!(name == "list" || e.contains("main.j3:7"), "{name}: note names the keeping store: {e}");
    }
}

/// (b) a local struct whose field was set to one, returned whole; (c) a local set in one branch;
/// (d) a `match` value; a stack-local `List` returned after a push.
#[test]
fn frame_string_returned_through_fields_branches_and_matches() {
    let cases = [
        ("field", "struct P { s: string; n: int; }\nfn f(n: int): P { let p = P { s: \"\", n: 1 }; p.s = `b{n}`; return p; }"),
        ("branch", "fn f(n: int, c: bool): string { let s = \"a\"; if c { s = `c{n}`; } return s; }"),
        ("match", "fn f(n: int): string { let s = match n { 0 { \"z\" } _ { `d{n}` } }; return s; }"),
        ("loop", "fn f(n: int): string { let s = \"\"; for i in 0..n { s = `l{i}`; } return s; }"),
    ];
    for (name, f) in cases {
        let e = frame_errs(name, &format!("module t;\n{f}\nfn main() {{ }}\n"));
        assert!(e.contains("cannot be returned"), "{name}: {e}");
    }
}

/// Uses that do not outlive the frame stay legal and run: a fn that only reads the string, `Text`
/// copies (`t.fmt`, `pushStr`), a local reassigned before the return, a local struct used in place,
/// and `main` keeping one (its frame lasts the run).
#[test]
fn frame_string_uses_that_stay_in_the_frame() {
    let src = format!(
        "{KEEPERS}\
fn read(n: int): int {{ return look(`r{{n}}`); }}\n\
fn into(t: &Text, n: int): string {{ t.fmt(`t={{n}}`); return t.str(); }}\n\
fn copy(t: &Text, n: int): string {{ t.pushStr(`c{{n}}`); return t.str(); }}\n\
fn reset(n: int): string {{ let s = `n={{n}}`; printLn(s); s = \"done\"; return s; }}\n\
struct P {{ s: string; n: int; }}\n\
fn local(n: int): int {{ let p = P {{ s: `p{{n}}`, n: n }}; printLn(p.s); return p.n; }}\n\
fn main() {{\n\
    let mem = Arena.create(4096);\n\
    let t = Text.new(&mem);\n\
    let u = Text.new(&mem);\n\
    keep(`main {{read(12)}}`);\n\
    printLn(G);\n\
    printLn(into(&t, 5));\n\
    printLn(copy(&u, 6));\n\
    printLn(reset(7));\n\
    printLn(`{{local(8)}}`);\n\
}}\n"
    );
    for r in both_backends("frame_ok", &src) {
        assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), "main 3\nt=5\nc6\nn=7\ndone\np8\n8\n");
    }
}
