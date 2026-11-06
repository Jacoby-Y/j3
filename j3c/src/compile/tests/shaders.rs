//! Shader language: every subset rule is an error with a span and a help line, the emitted GLSL matches text
//! goldens (`J3_BLESS=1` rewrites them), and the program is stable run to run.

use super::*;

/// The error `src` gives (plus `fn main() {}`); `src` is a whole module body.
fn gpu_err(body: &str) -> check::CheckError {
    let src = format!("module t;\n{body}\nfn main() {{}}\n");
    let ast = parse::parse(&src, FileId(1)).unwrap_or_else(|e| panic!("does not parse ({}): {src}", e.message));
    match check::check_files(vec![ParsedFile { path: "t.j3".into(), src: src.clone(), ast, file_id: FileId(1) }]) {
        Ok(_) => panic!("expected a check error for:\n{src}"),
        Err(e) => e,
    }
}

/// `(what, module body, text the message must contain)`; every one needs a span and a help line.
const REJECTED: &[(&str, &str, &str)] = &[
    ("int is 64-bit", "#[shader] fn f(x: int): f32 { return 1.0; }", "64-bit"),
    ("i64", "#[shader] fn f(x: i64): f32 { return 1.0; }", "64-bit"),
    ("f64", "#[shader] fn f(x: f64): f32 { return 1.0; }", "64-bit"),
    ("u8", "#[shader] fn f(x: u8): f32 { return 1.0; }", "8-bit"),
    ("pointer local", "#[shader] fn f(x: f32): f32 { let p = &x; return 1.0; }", "pointers"),
    ("pointer type", "#[shader] fn f(): f32 { let p: &f32 = 0; return 1.0; }", "pointers"),
    ("string value", "#[shader] fn f(): f32 { let s = \"hi\"; return 1.0; }", "strings"),
    ("string type", "#[shader] fn f(s: string): f32 { return 1.0; }", "strings"),
    ("slice", "#[shader] fn f(s: []f32): f32 { return 1.0; }", "slices"),
    ("heap", "#[shader] fn f(): f32 { let a = Arena.new(8); return 1.0; }", "method call"),
    ("cpu fn", "fn cpu(x: f32): f32 { return x; }\n#[shader] fn f(): f32 { return cpu(1.0); }", "CPU fn"),
    ("direct recursion", "#[shader] fn f(x: f32): f32 { return f(x); }", "recursion: f -> f"),
    ("mutual recursion", "#[shader] fn a(x: f32): f32 { return b(x); }\n#[shader] fn b(x: f32): f32 { return a(x); }", "recursion: a -> b -> a"),
    ("closure", "#[shader] fn f(): f32 { let g = fn(x: f32): f32 { return x; }; return 1.0; }", "closures"),
    ("generic fn", "#[shader] fn f<T>(x: T): f32 { return 1.0; }", "generic"),
    ("generic type", "#[shader] fn f(x: List<f32>): f32 { return 1.0; }", "generic"),
    ("cpu vector type", "#[shader] fn f(v: Vec2): f32 { return 1.0; }", "CPU type"),
    ("simd type", "#[shader] fn f(v: vec4f): f32 { return 1.0; }", "SIMD"),
    ("non-bool condition", "#[shader] fn f(x: f32): f32 { if x { return 1.0; } return 2.0; }", "must be bool"),
    ("array index out of range", "#[shader] fn f(): f32 { let a: [3]f32 = [1.0, 2.0, 3.0]; return a[3]; }", "out of range"),
    ("keyword uniform", "#[uniform] let texture: sampler2D;", "reserves"),
    ("mix uniform", "#[uniform] let mix: f32;", "reserves"),
    ("fragCoord in a vertex", "#[varying] struct V { position: vec4; }\n#[shader(vertex)] fn vs(): V { return V { position: @fragCoord() }; }", "fragment shaders"),
    ("vertexId in a fragment", "#[varying] struct V { position: vec4; }\n#[shader(fragment)] fn fs(v: V): vec4 { let i = @vertexId(); return v.position; }", "vertex shaders"),
    ("missing return", "#[shader] fn f(x: f32): f32 { if x < 1.0 { return 1.0; } }", "without returning"),
    ("defer", "#[shader] fn f(): f32 { defer { } return 1.0; }", "defer"),
    ("label", "#[shader] fn f(): f32 { 'a: while true { break 'a; } return 1.0; }", "labels"),
    ("for each", "#[shader] fn get(): [3]f32 { return [1.0, 2.0, 3.0]; }\n#[shader] fn f(): f32 { for x in get() { } return 1.0; }", "no slices"),
    ("break in match", "#[shader] fn f(n: i32): f32 { while true { match n { 1 { break; } _ { } } } return 1.0; }", "`break` inside a `match`"),
    ("enum match", "#[shader] fn f(n: i32): f32 { match n { A.B { } _ { } } return 1.0; }", "enum patterns"),
    ("match without default", "#[shader] fn f(n: i32): f32 { match n { 1 { } } return 1.0; }", "`_` arm"),
    ("method call", "#[shader] fn f(v: vec3): f32 { return v.len(); }", "method call"),
    ("mixed scalars", "#[shader] fn f(a: f32, b: i32): f32 { return a + b; }", "cannot apply `+`"),
    ("float modulo", "#[shader] fn f(a: f32): f32 { return a % 2.0; }", "integers"),
    ("global let", "#[singleton] let g: f32;\n#[shader] fn f(): f32 { return g; }", "CPU global"),
    ("assign to uniform", "#[uniform] let u: f32;\n#[shader] fn f(): f32 { u = 1.0; return u; }", "not a local"),
    ("repeated swizzle assign", "#[shader] fn f(): f32 { let v = vec3(1.0); v.xx = vec2(1.0); return 1.0; }", "repeats"),
    ("wrong builtin args", "#[shader] fn f(): f32 { return @dot(vec2(1.0), vec3(1.0)); }", "does not take"),
    ("struct with cpu field", "#[shader] struct S { a: string; }\n#[shader] fn f(s: S): f32 { return 1.0; }", "strings"),
    ("cpu struct", "struct C { a: f32; }\n#[shader] fn f(s: C): f32 { return 1.0; }", "CPU struct"),
    ("vertex layout", "#[vertex] struct B { a: Vec2; b: vec4f; }", "packed"),
    ("vertex stride", "#[vertex] struct B { a: vec4f; b: f32; }", "stride"),
    ("vertex field type", "#[vertex] struct B { a: int; }", "`#[vertex]` field"),
    ("varying head", "#[varying] struct V { uv: vec2; }\n#[shader(vertex)] fn vs(): V { return V { uv: vec2(0.0) }; }", "position: vec4"),
    ("varying bool", "#[varying] struct V { position: vec4; b: bool; }", "varying"),
    ("too many attributes", "#[vertex] struct B { a: Mat4; b: Mat4; c: Mat4; d: Mat4; e: f32; }\n#[varying] struct V { position: vec4; }\n#[shader(vertex)] fn vs(b: B): V { return V { position: vec4(0.0) }; }", "past the limit"),
    ("small int field", "#[vertex] struct B { a: u8; }", "#[vertex]"),
    ("fragment return", "#[varying] struct V { position: vec4; }\n#[shader(fragment)] fn fs(v: V): f32 { return 1.0; }", "vec4"),
];

#[test]
fn every_subset_rule_is_an_error_with_a_span_and_a_help() {
    let mut bad = Vec::new();
    for (what, body, needle) in REJECTED {
        let e = gpu_err(body);
        let ok = e.message.contains(needle) && !e.span.is_dummy() && !e.helps.is_empty();
        if !ok {
            bad.push(format!("{what}: message {:?}, helps {:?}, dummy span {}", e.message, e.helps, e.span.is_dummy()));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn shader_fns_are_not_cpu_fns_and_plain_code_is_untouched() {
    // a module with shader items still builds for the CPU, and the shader items are gone from it
    let src = "module t;\n#[uniform] let u: f32;\n#[shader] fn k(x: f32): f32 { return x * u; }\nfn main() {}\n";
    check::check_files(vec![parse_file(1, "t.j3", src)]).expect("checks");
}

fn blessed(name: &str, got: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/compile/tests/glsl").join(name);
    if std::env::var("J3_BLESS").is_ok() {
        std::fs::write(&path, got).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing golden {name}; run with J3_BLESS=1"));
    assert_eq!(got, want, "golden {name} drifted (J3_BLESS=1 to accept)");
}

/// The toy programs; the text above the MAIN marker of the driver test (which owns the GL `main`).
const TOY: &str = include_str!("../../../stdlib/tests/gpu_glsl_test.j3");

/// The toy program built as a project (so the `// toy.j3:LINE` comments do not depend on a temp name).
fn toy_prog(main: &str) -> Program {
    let toml = "[package]\nname = \"toy\"\nentry = \"gpu_glsl_test.j3\"\n\n[emit]\nopt = 0\n";
    let src = format!("{}{main}", TOY.split("\n//MAIN\n").next().unwrap());
    let m = temp_project("gpu_toy", &[("j3.toml", toml), ("gpu_glsl_test.j3", &src)]);
    let mut prog = compile_manifest(&m).expect("compile");
    // `use j3.game` brings in the stdlib's own programs; these tests are about the toy module's
    prog.shaders.retain(|s| s.module == "gpu_glsl_test");
    prog
}

#[test]
fn toy_program_matches_the_goldens() {
    let prog = toy_prog("fn main() {}");
    assert_eq!(prog.shaders.len(), 4);
    for p in &prog.shaders {
        blessed(&format!("{}.vert.glsl", p.name), &p.vert);
        blessed(&format!("{}.frag.glsl", p.name), &p.frag);
        blessed(&format!("{}.es.frag.glsl", p.name), &crate::emit::glsl::es::prep_es(&p.frag));
    }
}

#[test]
fn emitted_glsl_is_deterministic() {
    let a = toy_prog("fn main() {}");
    let b = toy_prog("fn main() {}");
    for (x, y) in a.shaders.iter().zip(b.shaders.iter()) {
        assert_eq!((&x.vert, &x.frag, &x.uniforms), (&y.vert, &y.frag, &y.uniforms));
    }
}

#[test]
fn glsl_reserved_names_are_renamed_but_uniforms_are_not() {
    let prog = toy_prog("fn main() {}");
    let p = &prog.shaders[0];
    assert!(p.frag.contains("j3_input") && p.frag.contains("j3_sample") && p.frag.contains("j3_mix"), "{}", p.frag);
    assert!(p.frag.contains("uniform vec4 uTint;") && p.uniforms.contains(&"uTint".to_string()), "{}", p.frag);
    assert!(!p.frag.contains(" input ") && !p.frag.contains(" mix ="));
}

#[test]
fn only_reachable_helpers_are_emitted() {
    let prog = toy_prog("fn main() {}");
    let p = &prog.shaders[0];
    assert!(!p.vert.contains("unusedHelper") && !p.frag.contains("unusedHelper"));
    assert!(p.frag.contains("shade(") && !p.vert.contains("shade("));
}

#[test]
fn a_helper_edit_changes_the_using_module() {
    let other = |k: &str| format!("module a;\n#[shader] pub fn k(x: f32): f32 {{ return x * {k}; }}\n");
    let user = "module t;\nuse a.k;\nuse j3.gpu.{ Source };\n#[varying] struct V { position: vec4; }\n#[shader(vertex)] fn vs(): V { return V { position: vec4(k(1.0)) }; }\n#[shader(fragment)] fn fs(v: V): vec4 { return v.position; }\nconst P: Source = @shader(vs, fs);\nfn main() {}\n";
    let toml = "[package]\nname = \"t\"\nentry = \"src/t.j3\"\n\n[emit]\nopt = 0\n";
    let mut texts = Vec::new();
    for k in ["2.0", "3.0"] {
        let m = temp_project("gpu_incr", &[("j3.toml", toml), ("src/t.j3", user), ("src/a.j3", &other(k))]);
        texts.push(compile_manifest(&m).unwrap_or_else(|e| panic!("{e}")).shaders[0].vert.clone());
    }
    assert_ne!(texts[0], texts[1], "the helper's text must reach the using module's constant");
}

#[test]
fn shader_source_is_usable_on_both_backends() {
    for r in both_backends(
        "gpu_source",
        &(TOY.split("\n//MAIN\n").next().unwrap().replace("use j3.game;\n", "").replace("use j3.sys.panic;\n", "").replace("use j3.text.cstr;\n", "") + "fn main() { printLn(`{PROG.vert.len > 100} {PROG.count} {PROG.attrib(1).divisor} {PROG.attrib(1).offset}`); }"),
    ) {
        assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), "true 3 1 0\n");
    }
}

/// The toy programs through a real GL driver, as 3.3 core and as ES 3.0. Without a GL context (or a built `j3c`
/// beside the test binary) it prints `skipped: ...` and passes; a driver that refuses the text fails.
#[test]
fn shaders_compile_on_a_gl_driver() {
    let exe = std::env::current_exe().unwrap();
    let j3c = exe.parent().and_then(|d| d.parent()).map(|d| d.join("j3c")).filter(|p| p.is_file());
    let Some(j3c) = j3c else {
        eprintln!("skipped: no j3c binary next to the test binary");
        return;
    };
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stdlib/tests/gpu_glsl_test.j3");
    for es in [false, true] {
        let mut cmd = std::process::Command::new(&j3c);
        cmd.args(["run", file.to_str().unwrap(), "--headless"])
            .env("SDL_VIDEODRIVER", "offscreen")
            .env("SDL_AUDIODRIVER", "dummy")
            .env("__GL_SHADER_DISK_CACHE", "0");
        if es {
            cmd.env("J3_GLES", "1");
        }
        cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
        let mut child = cmd.spawn().expect("spawn j3c");
        let start = std::time::Instant::now();
        let status = loop {
            if let Some(s) = child.try_wait().unwrap() {
                break Some(s);
            }
            if start.elapsed().as_secs() > 120 {
                let _ = child.kill();
                break None;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        };
        let out = child.wait_with_output().unwrap();
        let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        let ctx = if es { "ES 3.0" } else { "3.3 core" };
        if text.contains(&format!("gpu_glsl_test ok es={es}")) {
            eprintln!("gpu driver test: {ctx} ok");
        } else if ["SDL_Init", "SDL_CreateWindow", "SDL_GL_CreateContext", "no OpenGL ES 3.0 context", "not found"].iter().any(|m| text.contains(m)) {
            eprintln!("skipped: no GL context for {ctx}: {}", text.lines().last().unwrap_or(""));
        } else {
            panic!("{ctx}: driver rejected the emitted GLSL (status {status:?}):\n{text}");
        }
    }
}

#[test]
fn cpu_code_naming_a_shader_item_is_told_what_it_is() {
    for (body, name) in [("#[shader] fn h(x: f32): f32 { return x; }\nfn use1() { let a = h(1.0); }", "h"), ("#[uniform] let u: f32;\nfn use1() { let a = u; }", "u")] {
        let e = gpu_err(body);
        assert!(e.message.contains(&format!("`{name}`")), "{}", e.message);
        assert!(e.helps.iter().any(|h| h.contains("shader item")), "{:?}", e.helps);
    }
}

#[test]
fn small_int_vertex_field_suggests_pad_fields() {
    let e = gpu_err("#[vertex] struct B { a: u8; }");
    assert!(e.helps.iter().any(|h| h.contains("_pad")), "{:?}", e.helps);
}

#[test]
fn entry_main_does_not_use_user_names() {
    let prog = toy_prog("fn main() {}");
    let q = prog.shaders.iter().find(|p| p.name == "QUIRKS").unwrap();
    assert!(q.vert.contains("p_0") && q.vert.contains("p_1") && q.vert.contains("p_ret"), "{}", q.vert);
    assert!(!q.vert.contains("A input") && !q.vert.contains("V r "), "{}", q.vert);
}
