//! Multi-field enum variants, nested and struct patterns, destructuring `let`, `#[Debug]` enums.

use super::*;

/// A 3-field variant built and matched; a variant nested in a variant; struct patterns
/// with and without `..`; `let` destructuring. LLVM, C and the bytecode VM print the same.
#[test]
fn multi_field_variants_three_ways() {
    three_ways(
        "variants",
        "use j3.io.printLn;\n\
enum Op { Add, Sub }\n\
struct Span { lo: int; hi: int; }\n\
enum Inner { Pair(int, int), Empty }\n\
enum E { Bin(Op, int, int), Wrap(Inner), At(Span, bool), Leaf }\n\
fn eval(e: E): int {\n\
    match e {\n\
        E.Bin(Op.Add, a, b) { return a + b; }\n\
        E.Bin(Op.Sub, a, 0) { return a * 1000; }\n\
        E.Bin(Op.Sub, a, b) { return a - b; }\n\
        E.Wrap(Inner.Pair(x, _)) { return x * 10; }\n\
        E.Wrap(Inner.Empty) { return -1; }\n\
        E.At(Span { lo, .. }, true) { return lo; }\n\
        E.At(Span { lo, hi }, false) { return hi - lo; }\n\
        E.Leaf { return 0; }\n\
    }\n\
}\n\
pub fn run() {\n\
    let a = E.Bin(Op.Add, 2, 3);\n\
    let b = E.Bin(Op.Sub, 7, 0);\n\
    let c = E.Bin(Op.Sub, 7, 2);\n\
    let d = E.Wrap(Inner.Pair(4, 9));\n\
    let s = Span { lo: 3, hi: 8 };\n\
    printLn(`{eval(a)} {eval(b)} {eval(c)} {eval(d)} {eval(E.Wrap(Inner.Empty))} {eval(E.At(s, true))} {eval(E.At(s, false))} {eval(E.Leaf)}`);\n\
    let Span { lo, hi: top } = s;\n\
    let Span { hi, .. } = s;\n\
    printLn(`{lo} {top} {hi}`);\n\
}\n",
        "5 7000 5 40 -1 3 5 0\n3 8 8\n",
    );
}

/// `#[Debug]` of 3-field and nested variants, and of a recursive type through pointers (a
/// pointer prints as its type, so printing ends).
#[test]
fn recursive_variant_prints_without_recursing() {
    let src = "module t;\nuse j3.io.printLn;\n\
#[Debug]\n\
enum Node { Bin(&Node, &Node, int), Lit(int) }\n\
#[Debug]\n\
enum Wrap { One(Node), Two(Node, bool, string) }\n\
fn sum(n: &Node): int {\n\
    match *n {\n\
        Node.Bin(l, r, k) { return sum(l) + sum(r) + k; }\n\
        Node.Lit(v) { return v; }\n\
    }\n\
}\n\
fn main() {\n\
    let a = Node.Lit(1);\n\
    let b = Node.Lit(2);\n\
    let c = Node.Bin(&a, &b, 10);\n\
    let d = Node.Bin(&c, &a, 100);\n\
    printLn(`{sum(&d)} {d}`);\n\
    printLn(`{Wrap.One(a)} {Wrap.Two(c, false, \"s\")}`);\n\
}\n";
    for r in both_backends("recursive_variant", src) {
        assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), "114 Node.Bin(&Node, &Node, 100)\nWrap.One(Node.Lit(1)) Wrap.Two(Node.Bin(&Node, &Node, 10), false, s)\n");
    }
}

/// An enum mixing `f32`, `vec4f` and pointers: the checker's layout (`ir/abi.rs`), the C struct and
/// the LLVM type agree on size, alignment and offsets, and values round-trip on both backends.
#[test]
fn mixed_variant_layout_agrees() {
    let src = "module t;\nuse j3.io.printLn;\n\
enum Mix { A(f32, vec4f, &int), B(&int, f32), C }\n\
fn get(m: Mix): f32 {\n\
    match m {\n\
        Mix.A(x, v, p) { return x + v.y + f32(*p); }\n\
        Mix.B(_, y) { return y; }\n\
        Mix.C { return -1.0; }\n\
    }\n\
}\n\
fn main() {\n\
    let n = 40;\n\
    let xs: [3]Mix = [Mix.A(1.5, vec4f(0.0, 2.0, 0.0, 0.0), &n), Mix.B(&n, 2.5), Mix.C];\n\
    printLn(`{@sizeof(Mix)} {get(xs[0])} {get(xs[1])} {get(xs[2])}`);\n\
}\n";
    let mut prog = compile_src("mixed_layout", src).expect("check");
    let eid = prog.cx.enums().iter().position(|e| prog.cx.name_str(e.name) == "Mix").expect("Mix");
    let ety = prog.cx.ty_enum(crate::ir::ids::EnumId(eid as u32));
    let cx = &prog.cx;
    let a = cx.enums()[eid].variants[0].payload.expect("A has a payload");
    assert!(cx.enums()[eid].variants[0].multi);
    let abi = crate::ir::abi::size;
    assert_eq!((abi(cx, a), crate::ir::abi::align(cx, a)), (48, 16), "A's payload: f32 @0, vec4f @16, ptr @32");
    assert_eq!(crate::ir::abi::field_offset(cx, a, 1), Some(16));
    assert_eq!(crate::ir::abi::field_offset(cx, a, 2), Some(32));
    assert_eq!((abi(cx, ety), crate::ir::abi::align(cx, ety)), (64, 16), "tag, pad to 16, 48-byte union");
    assert_eq!(crate::ir::abi::field_offset(cx, ety, 1), Some(16));

    // C: the compiler itself checks the emitted structs
    let mut c = emit::emit_program(cx, &prog.modules).expect("c");
    let mix = type_name(&c, "struct j3_t_Mix_");
    let pay = type_name(&c, "struct j3_t_Mix_A_");
    c.push_str(&format!(
        "\n_Static_assert(sizeof(struct {mix}) == 64, \"size\");\n_Static_assert(_Alignof(struct {mix}) == 16, \"align\");\n\
         _Static_assert(__builtin_offsetof(struct {mix}, u) == 16, \"union\");\n\
         _Static_assert(sizeof(struct {pay}) == 48, \"payload\");\n_Static_assert(__builtin_offsetof(struct {pay}, _1) == 16, \"vec\");\n\
         _Static_assert(__builtin_offsetof(struct {pay}, _2) == 32, \"ptr\");\n"
    ));
    let dir = std::env::temp_dir().join(format!("j3-mixlayout-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("m.c"), &c).unwrap();
    let out = std::process::Command::new("cc").args(["-fsyntax-only", "-w"]).arg(dir.join("m.c")).output().expect("cc");
    assert!(out.status.success(), "C layout: {}", String::from_utf8_lossy(&out.stderr));

    // LLVM (the enum is `{ i32, [N x i8] }`, its payload read at the `ir/abi.rs` offset): fold
    // `getelementptr` on the emitted types into globals and read them back
    let mut ll = emit::emit_llvm(cx, &prog.modules, &emit::EmitOpts::default()).expect("llvm");
    let mix = type_name(&ll, "%j3_t_Mix_");
    let pay = type_name(&ll, "%j3_t_Mix_A_");
    for (g, ty, idx) in [("size", &mix, "i32 1"), ("psize", &pay, "i32 1"), ("vec", &pay, "i32 0, i32 1"), ("ptr", &pay, "i32 0, i32 2")] {
        ll.push_str(&format!("@probe_{g} = global i64 ptrtoint (ptr getelementptr (%{ty}, ptr null, {idx}) to i64)\n"));
    }
    std::fs::write(dir.join("m.ll"), &ll).unwrap();
    let out = std::process::Command::new("clang").args(["-S", "-w", "-o", "-"]).arg(dir.join("m.ll")).output().expect("clang");
    assert!(out.status.success(), "LLVM: {}", String::from_utf8_lossy(&out.stderr));
    let asm = String::from_utf8_lossy(&out.stdout);
    for (g, want) in [("size", 64), ("psize", 48), ("vec", 16), ("ptr", 32)] {
        let at = asm.find(&format!("probe_{g}:")).unwrap_or_else(|| panic!("probe_{g} in asm"));
        let quad = asm[at..].lines().nth(1).unwrap_or("");
        let v = quad.trim().trim_start_matches(".quad").trim().trim_start_matches("0+");
        assert_eq!(v, want.to_string(), "LLVM {g}: {quad}");
    }
    let _ = std::fs::remove_dir_all(&dir);

    for r in both_backends("mixed_layout_run", src) {
        assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), "64 43.5 2.5 -1\n");
    }
}

/// `prefix` followed by the id digits, as written in emitted text (without `struct `/`%`).
fn type_name(text: &str, prefix: &str) -> String {
    let bare = prefix.trim_start_matches("struct ").trim_start_matches('%');
    for (at, _) in text.match_indices(prefix) {
        let digits: String = text[at + prefix.len()..].chars().take_while(|c| c.is_ascii_digit()).collect();
        let next = text[at + prefix.len() + digits.len()..].chars().next();
        if !digits.is_empty() && !next.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
            return format!("{bare}{digits}");
        }
    }
    panic!("{prefix}<id> in emitted text")
}

#[test]
fn variant_and_struct_pattern_errors() {
    let wrap = |body: &str| {
        format!("module t;\nenum Op {{ Add, Sub }}\nstruct S {{ a: int; b: int; }}\nenum E {{ Bin(Op, int, int), Pair(Op, Op), At(S), Leaf }}\nfn main() {{\n    let e = E.Leaf;\n    let s = S {{ a: 1, b: 2 }};\n{body}\n}}\n")
    };
    for (body, want) in [
        ("    let x = E.Bin(Op.Add, 1);", "`E.Bin` takes 3 fields, got 2"),
        ("    let x = E.Bin(Op.Add, 1, \"q\");", "field 2 of `E.Bin`: type mismatch"),
        ("    let x = E.Leaf(1);", "`E.Leaf` has no payload"),
        ("    match e { E.Bin(o, a) {} _ {} }", "`E.Bin` has 3 fields, the pattern gives 2"),
        ("    match e { E.Bin {} _ {} }", "missing the payload: `E.Bin(_, _, _)`"),
        ("    match e { E.Bin(Op.Add, _, _) {} E.Pair(_, _) {} E.At(_) {} E.Leaf {} }", "missing `E.Bin(Op.Sub, _, _)`"),
        ("    match e { E.Bin(_, 0, _) {} E.Pair(_, _) {} E.At(_) {} E.Leaf {} }", "missing `E.Bin(_, _, _)`"),
        ("    match e { E.Bin(_, _, _) {} E.Pair(Op.Add, Op.Sub) {} E.Pair(Op.Sub, _) {} E.At(_) {} E.Leaf {} }", "missing `E.Pair(Op.Add, Op.Add)`"),
        ("    match e { E.Bin(_, _, _) {} E.Pair(_, _) {} E.At(S { a: 1, .. }) {} E.Leaf {} }", "missing `E.At(_)`"),
        ("    match e { E.At(S { a }) {} _ {} }", "struct pattern is missing `b`"),
        ("    match e { E.At(S { c, .. }) {} _ {} }", "`S` has no field `c`"),
        ("    match e { E.At(S { a, a, .. }) {} _ {} }", "field `a` is listed twice"),
        ("    match e { E.At(T { a, .. }) {} _ {} }", "expected `S`, got `T`"),
        ("    let S { a, b: 3 } = s;", "this `let` pattern may not match"),
        ("    let E.At(x) = e;", "this `let` pattern may not match"),
        ("    let S { a, .. } = e;", "a struct pattern on a `E`"),
    ] {
        let err = check_err(&wrap(body));
        assert!(err.contains(want), "{body}\n  want: {want}\n  got: {err}");
    }
    // nested coverage that is complete needs no `_`
    check::check_files(vec![parse_file(
        1,
        "t.j3",
        &wrap("    match e { E.Bin(_, _, _) {} E.Pair(Op.Add, _) {} E.Pair(_, Op.Add) {} E.Pair(Op.Sub, Op.Sub) {} E.At(S { a, .. }) {} E.Leaf {} }"),
    )])
    .expect("exhaustive across two fields");
}

/// `j3c fmt` keeps multi-field variants and struct patterns parseable and stable.
#[test]
fn fmt_round_trips_variant_patterns() {
    let src = "module t;\nenum E { Bin(int, int, int), At(S) }\nfn f(e: E, s: S) {\n    let S { a, .. } = s;\n    match e {\n        E.Bin(x, _, 0) { }\n        E.At(S { a: q, b }) | E.At(S { b: q, a }) { }\n        _ { }\n    }\n}\n";
    let once = crate::fmt::format_src(src).unwrap();
    assert_eq!(crate::fmt::format_src(&once).unwrap(), once, "fmt is stable");
    for line in ["    let S { a, .. } = s;", "        E.At(S { a: q, b }) | E.At(S { b: q, a }) {"] {
        assert!(once.contains(line), "kept on one line: {line}\n{once}");
    }
    crate::parse::parse(&once, FileId(1)).expect("formatted text parses");
}

/// A `[N]T` payload (flat, nested, of structs): built and matched on all three; printed by
/// `#[Debug]` (not in scripts) on both AOT backends.
#[test]
fn array_payload_variants_three_ways() {
    let decl = "struct P { x: int; y: int; }\nenum E { A([2]int), M([2][2]int), B([2]P), C }\n";
    let body = "fn pick(e: E): int {\n\
    match e {\n\
        E.A(xs) { return xs[0] + xs[1]; }\n\
        E.M(m) { return m[1][0]; }\n\
        E.B(ps) { return ps[1].y; }\n\
        E.C { return -1; }\n\
    }\n\
}\n";
    let uses = "let m = [[1, 2], [3, 4]];\n    printLn(`{pick(E.A([3, 4]))} {pick(E.M(m))} {pick(E.B([P { x: 1, y: 2 }, P { x: 3, y: 9 }]))} {pick(E.C)}`);\n";
    three_ways("variants_arr", &format!("use j3.io.printLn;\n{decl}{body}pub fn run() {{\n    {uses}}}\n"), "7 3 9 -1\n");
    let src = format!(
        "module t;\nuse j3.io.printLn;\n{}{body}fn main() {{\n    {uses}    printLn(`{{E.A([3, 4])}} {{E.M(m)}} {{E.C}}`);\n}}\n",
        decl.replace("enum E", "#[Debug]\nenum E").replace("struct P", "#[Debug]\nstruct P")
    );
    for r in both_backends("variants_arr_dbg", &src) {
        assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), "7 3 9 -1\nE.A([3, 4]) E.M([[1, 2], [3, 4]]) E.C\n");
    }
}
