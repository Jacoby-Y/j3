//! `j3c dump-glsl --es`: the text `j3.gfx.glsl.prep(src, true)` hands an ES 3.0 driver. The runtime does the
//! conversion in J3 (`stdlib/j3/gfx/glsl.j3`); this is the same rule in Rust, and `gpu_glsl_test.j3` checks
//! the converted programs compile in an ES context.

const SAMPLERS: [&str; 9] = [
    "sampler2D", "samplerCube", "sampler3D", "sampler2DShadow", "sampler2DArray", "isampler2D", "usampler2D",
    "isampler3D", "usampler3D",
];

pub fn prep_es(src: &str) -> String {
    let lines: Vec<&str> = src.split_inclusive('\n').collect();
    let mut end = 0;
    let mut has_version = false;
    while end < lines.len() && lead_line(lines[end]) {
        has_version |= lines[end].trim_start().starts_with("#version");
        end += 1;
    }
    let mut out = String::new();
    if !has_version {
        out.push_str("#version 300 es\n");
    }
    for l in &lines[..end] {
        if l.trim_start().starts_with("#version") {
            out.push_str("#version 300 es\n");
        } else {
            out.push_str(l);
        }
    }
    out.push_str("precision highp float;precision highp int;");
    let rest: String = lines[end..].concat();
    for s in SAMPLERS {
        if rest.contains(s) {
            out.push_str(&format!("precision highp {s};"));
        }
    }
    out.push_str(&rest);
    out
}

fn lead_line(line: &str) -> bool {
    let t = line.trim_matches(|c| c == ' ' || c == '\t' || c == '\r' || c == '\n');
    if t.is_empty() || t.starts_with("//") {
        return true;
    }
    if let Some(body) = t.strip_prefix("/*") {
        return body.find("*/").is_some_and(|c| body[c + 2..].trim().is_empty());
    }
    let Some(d) = t.strip_prefix('#') else { return false };
    let d = d.trim_start();
    !(d.starts_with("if") || d.starts_with("else") || d.starts_with("elif") || d.starts_with("endif"))
}
