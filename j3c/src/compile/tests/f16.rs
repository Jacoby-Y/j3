//! `f16`: storage only, bit-exact conversions on LLVM and C at opt 0 and 2.

use super::*;
use crate::ir::half::from_f64;

const HEAD: &str = "module t;\nuse j3.io.printLn;\nuse j3.memory.Arena;\nuse j3.memory.List;\n";

/// A half's value in units of 2^-24 (exact), -1 NaN, -77 for -0, +-999999999999999 for infinity.
const UNITS: &str = "fn units(h: f16): int {
    let x = f32(h);
    if x != x { return -1; }
    if x > 1000000000.0 { return 999999999999999; }
    if x < -1000000000.0 { return -999999999999999; }
    if x == 0.0 { if 1.0 / x < 0.0 { return -77; } return 0; }
    return int(x * 16777216.0);
}
";

fn units(bits: u16) -> i64 {
    let (neg, e, m) = (bits & 0x8000 != 0, ((bits >> 10) & 31) as i64, (bits & 0x3ff) as i64);
    let mag = match e {
        31 if m != 0 => return -1,
        31 => 999999999999999,
        0 => m,
        _ => (m | 1024) << (e - 1),
    };
    match (neg, mag) {
        (true, 0) => -77,
        (true, _) => -mag,
        _ => mag,
    }
}

/// Runs on both backends at opt 0 and 2; every run must print `want`.
fn run(name: &str, src: &str, want: &str) {
    for opt in ["0", "2"] {
        for (i, r) in both_backends_opt(&format!("f16_{name}_{opt}"), src, opt).iter().enumerate() {
            assert_eq!(r.status, 0, "backend {i} opt {opt}: {}", String::from_utf8_lossy(&r.stderr));
            assert_eq!(String::from_utf8_lossy(&r.stdout), want, "backend {i} opt {opt}");
        }
    }
}

fn rejects(src: &str, msg: &str) {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let tag = format!("f16_err{}", N.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    let e = compile_src(&tag, &format!("{HEAD}{src}")).err().expect("expected an error");
    assert!(e.contains(msg) && e.contains("convert with `f32(x)`"), "{e}");
}

#[test]
fn conversions_are_bit_exact() {
    let ulp = 2f64.powi(-11);
    // f32 inputs (exact in f32)
    let f32s: Vec<f64> = vec![
        0.0, 1.0, 65504.0, 65520.0, 65536.0, 1.0e10, 6.103515625e-5, 5.9604645e-8, 2.9802322e-8,
        4.4703484e-8, 1.0 + ulp, 1.0 + 3.0 * ulp, 2048.0 + 1.0, 0.5, -2.5, 3.140625, -65504.0, -1.0e10,
    ];
    let mut body = format!("{HEAD}{UNITS}fn t32(x: f32) {{ printLn(`{{units(f16(x))}}`); }}\nfn t64(x: float) {{ printLn(`{{units(f16(x))}}`); }}\nfn main() {{\n");
    let mut want = String::new();
    for v in &f32s {
        body += &format!("    t32({v:.30});\n");
        want += &format!("{}\n", units(from_f64(*v as f32 as f64)));
    }
    // f64 inputs: 1e-8 and 0.1 are not exact in f32; the last one is above the tie in f64 only
    let f64s = [1.0e-8, 0.1, 65519.999999, 1.0 + ulp + 2f64.powi(-40), 2f64.powi(-25) * 1.0000001];
    for v in &f64s {
        body += &format!("    t64({v:.30});\n");
        want += &format!("{}\n", units(from_f64(*v)));
    }
    // -0, infinities, NaN built at run time; ints
    body += "    let z = 0.0;\n    let one: f32 = 1.0;\n    let zero: f32 = 0.0;\n    t32(-zero);\n    t32(one / zero);\n    t32(-one / zero);\n    t32(zero / zero);\n    t64(-z);\n";
    want += "-77\n999999999999999\n-999999999999999\n-1\n-77\n";
    for n in [0i64, 7, 2049, 2051, 65519, 65520, 70000, -5, -2049] {
        body += &format!("    printLn(`{{units(f16({n}))}}`);\n");
        want += &format!("{}\n", units(from_f64(n as f64)));
    }
    body += "}\n";
    run("conv", &body, &want);
}

#[test]
fn storage_layout_and_round_trips() {
    run(
        "storage",
        &format!(
            "{HEAD}struct V {{ a: f16; b: f32; }}
struct P {{ a: f16; b: f16; c: u8; }}
fn pick(x: f16, y: f16, first: bool): f16 {{ if first {{ return x; }} return y; }}
fn main() {{
    let v = V {{ a: 1.5, b: 2.0 }};
    let w = v;
    let xs: [4]f16 = [1.0, 2.5, -0.5, f16(100.0)];
    xs[3] = xs[1];
    let mem = Arena.create(4096);
    defer mem.destroy();
    let l = List<f16>.new(&mem);
    l.push(f16(0.1));
    l.push(pick(xs[0], xs[2], false));
    printLn(`{{@sizeof(f16)}} {{@sizeof(V)}} {{@sizeof(P)}} {{@sizeof([4]f16)}}`);
    printLn(`{{f32(w.a)}} {{w.b}} {{f32(xs[3])}} {{f32(l.items[1])}} {{f64(l.items[0])}} {{l.len}}`);
}}
"
        ),
        "2 8 6 8\n1.5 2 2.5 -0.5 0.0999756 2\n",
    );
}

#[test]
fn arithmetic_compare_and_interpolation_are_errors() {
    let pre = "fn main() {\n    let a: f16 = 1.0;\n    let b: f16 = 2.0;\n";
    for (stmt, msg) in [
        ("let c = a + b;", "`+` is not defined on `f16`"),
        ("let c = a * 2.0;", "`*` is not defined on `f16`"),
        ("let c = 1.0 - a;", "`-` is not defined on `f16`"),
        ("let c = a < b;", "`<` is not defined on `f16`"),
        ("let c = a == b;", "`==` is not defined on `f16`"),
        ("a += b;", "`+=` is not defined on `f16`"),
        ("a *= 2.0;", "`*=` is not defined on `f16`"),
        ("let c = -a;", "unary `-` is not defined on `f16`"),
    ] {
        rejects(&format!("{pre}    {stmt}\n}}\n"), msg);
    }
    let e = compile_src("f16_interp", &format!("{HEAD}fn main() {{ let a: f16 = 1.0; printLn(`{{a}}`); }}\n")).err().unwrap();
    assert!(e.contains("cannot appear in interpolate") && e.contains("f32(x)"), "{e}");
    let e = compile_src("f16_int", &format!("{HEAD}fn main() {{ let a: f16 = 1.0; let n = int(a); }}\n")).err().unwrap();
    assert!(e.contains("not defined for `f16`"), "{e}");
}

#[test]
fn scripts_reject_f16_with_a_span() {
    let main = "module lang;\nconst S = @script(\"scripts/s.j3s\");\nfn main() {\n    let s = S {};\n    s.run();\n}\n";
    let toml = "[package]\nname = \"lang\"\nentry = \"main.j3\"\n\n[emit]\nopt = 0\n";
    for script in ["pub fn run(v: f16) {\n}\n", "pub fn run() {\n    let v = f16(1.0);\n}\n"] {
        let m = temp_project("f16_script", &[("j3.toml", toml), ("main.j3", main), ("scripts/s.j3s", script)]);
        let e = match compile_manifest(&m) {
            Ok(_) => panic!("script f16 accepted"),
            Err(e) => format!("{e:?}"),
        };
        assert!(e.contains("f16 is not supported in scripts yet") && e.contains("s.j3s:"), "{e}");
    }
}

#[test]
fn dump_ir_names_the_type() {
    let p = compile_src("f16_dump", &format!("{HEAD}fn main() {{ let a: f16 = 1.0; printLn(`{{f32(a)}}`); }}\n")).unwrap();
    assert!(crate::ir::dump::dump_program(&p.cx, &p.modules).contains("f16"));
}
