use super::*;

/// `j3c dump-glsl [path] [--es]`: every `@shader(vs, fs)` program as the GLSL the compiler emitted.
pub(super) fn dump_glsl(rest: &[String]) -> i32 {
    const USAGE: &str = "usage: j3c dump-glsl [path] [--es]";
    if rest.iter().any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        println!("  prints each `const X = @shader(vs, fs);` program: vertex, fragment, uniforms, attributes");
        println!("  --es   the text an OpenGL ES 3.0 driver gets (`#version 300 es`, precision lines), as `glsl.prep` makes it");
        return 0;
    }
    if let Some(bad) = rest.iter().find(|a| a.starts_with('-') && a.as_str() != "--es") {
        eprintln!("j3c dump-glsl: unknown flag `{bad}`; {USAGE}");
        return 1;
    }
    let es = rest.iter().any(|a| a == "--es");
    let path = rest.iter().find(|a| !a.starts_with('-')).map(|s| s.as_str());
    let prog = match load_input(path).and_then(|i| compile_input(&i)) {
        Ok((p, _)) => p,
        Err(e) => {
            eprintln!("j3c: {e}");
            return 1;
        }
    };
    if prog.shaders.is_empty() {
        eprintln!("j3c dump-glsl: no `@shader(vs, fs)` in the program");
        return 1;
    }
    for s in &prog.shaders {
        println!("// ===== {}.{} ({}) =====", s.module, s.name, s.at);
        for (label, text) in [("vertex", &s.vert), ("fragment", &s.frag)] {
            println!("// ----- {label} -----");
            let t = if es { crate::emit::glsl::es::prep_es(text) } else { text.clone() };
            print!("{t}");
        }
        println!("// ----- uniforms: {}", s.uniforms.join(" "));
        for a in &s.attribs {
            println!("// ----- attrib loc={} comps={} gltype=0x{:x} offset={} stride={} divisor={} int={}", a[0], a[1], a[2], a[3], a[4], a[5], a[6]);
        }
    }
    0
}
