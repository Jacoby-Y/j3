//! Entry fns: a vertex stage, a fragment stage, and the pair that `@shader(vs, fs)` makes. The entry's
//! params and return become `in`/`out` declarations and a `main` that calls it.

use crate::emit::glsl::names::mangle;
use crate::emit::glsl::program::StageText;
use crate::parse::{FieldItem, FnItem, GpuStage, GpuStruct, StructItem};

use super::builtins::Only;
use super::gen::{Field, Gen};
use super::index::Index;
use super::layout::{self, VAttr};
use super::ty::*;
use super::R;

/// A varying carries numbers: scalar or vector of f32/i32/u32.
pub fn check_varying_field(g: &Gen, f: &FieldItem, ty: &GTy) -> R<()> {
    match ty {
        GTy::Scalar(Base::F | Base::I | Base::U) | GTy::Vec(Base::F | Base::I | Base::U, _) => Ok(()),
        t => g.err_help(f.ty.span, format!("a `#[varying]` field cannot be {}", t.name()), "use f32, i32, u32 or a vector of them"),
    }
}

/// The first varying field is `position: vec4`, the clip-space position.
pub fn check_varying_head(g: &Gen, s: &StructItem, fields: &[Field]) -> R<()> {
    match fields.first() {
        Some(f) if f.name == "position" && f.ty == GTy::Vec(Base::F, 4) => Ok(()),
        _ => g.err_help(s.name.span, format!("`{}` must start with `position: vec4`", s.name.name), "the first varying field is the clip-space position (`gl_Position`); the fragment shader sees `gl_FragCoord` there"),
    }
}

pub struct Stage {
    pub text: String,
    pub uniforms: Vec<String>,
    /// Vertex: the varying struct returned. Fragment: the one taken, if any.
    pub varying: Option<String>,
    pub attribs: Vec<VAttr>,
}

fn struct_of<'a>(g: &Gen<'a>, key: &str) -> &'a StructItem {
    let (m, n) = key.split_once("::").unwrap_or(("", key));
    g.ix.module(m).and_then(|m| m.structs.get(n).copied()).expect("struct was resolved")
}

fn finish(g: Gen, title: String, io: Vec<String>, main: Vec<String>) -> (String, Vec<String>) {
    let uniforms = g.uniforms_out.iter().map(|(n, _)| n.clone()).collect();
    let t = StageText {
        title,
        structs: g.structs_out,
        io,
        uniforms: g.uniforms_out.into_iter().map(|(_, d)| d).collect(),
        consts: g.consts_out,
        fns: g.fns_out,
        main: format!("void main() {{\n{}}}\n", main.iter().map(|l| format!("    {l}\n")).collect::<String>()),
    };
    (t.render(), uniforms)
}

pub fn vertex_stage<'a>(ix: &'a Index<'a>, module: &str, f: &'a FnItem, title: &str) -> R<Stage> {
    let mut g = Gen::new(ix, Only::Vertex);
    let sig = g.emit_fn(module, f, f.name.span)?;
    let (mut io, mut main, mut attribs) = (Vec::new(), Vec::new(), Vec::new());
    let mut args = Vec::new();
    let mut loc = 0;
    for (pi, (p, (ty, _))) in f.params.iter().zip(sig.params.iter()).enumerate() {
        let GTy::Struct { key, glsl } = ty else {
            return g.err_help(p.ty.span, "a vertex fn takes `#[vertex]` structs", "one struct per vertex buffer; `#[vertex(instance)]` for per-instance data");
        };
        let info = g.sinfo[key].clone();
        if !matches!(info.kind, GpuStruct::Vertex | GpuStruct::VertexInstance) {
            return g.err_help(p.ty.span, format!("`{}` is not a vertex struct", ty.name()), "mark it `#[vertex]` (or `#[vertex(instance)]`)");
        }
        let s = struct_of(&g, key);
        layout::check_layout(&g, s)?;
        let div = u32::from(info.kind == GpuStruct::VertexInstance);
        layout::check_limit(&g, s, loc)?;
        let mine = layout::attribs(&g, s, loc, div)?;
        let mut vars = Vec::new();
        let mut at = mine.iter();
        for fl in &info.fields {
            let var = format!("a_{}_{}", mangle(&p.name.name), mangle(&fl.name));
            let first = at.next().expect("one attribute per field");
            for _ in 1..(if fl.ty == GTy::Mat(4) { 4 } else { 1 }) {
                at.next();
            }
            io.push(format!("layout(location={}) in {} {var};", first.loc, fl.ty.glsl()));
            vars.push(var);
        }
        loc += mine.len() as u32;
        attribs.extend(mine);
        // main's locals never use user names (`input`, `r`, a fn called like a param)
        main.push(format!("{glsl} p_{pi} = {glsl}({});", vars.join(", ")));
        args.push(format!("p_{pi}"));
    }
    let GTy::Struct { key: rkey, glsl: rglsl } = &sig.ret else {
        return g.err_help(f.name.span, "a vertex fn returns a `#[varying]` struct", "its first field is `position: vec4`");
    };
    let rinfo = g.sinfo[rkey].clone();
    if rinfo.kind != GpuStruct::Varying {
        return g.err_help(f.name.span, format!("`{}` is not a `#[varying]` struct", sig.ret.name()), "mark it `#[varying]`");
    }
    main.push(format!("{rglsl} p_ret = {}({});", sig.glsl, args.join(", ")));
    for (i, fl) in rinfo.fields.iter().enumerate() {
        if i == 0 {
            main.push(format!("gl_Position = p_ret.{};", fl.glsl));
            continue;
        }
        let v = mangle(&fl.name);
        io.push(format!("{}out {} v_{v};", flat_word(fl, &fl.ty), fl.ty.glsl()));
        main.push(format!("v_{v} = p_ret.{};", fl.glsl));
    }
    let varying = Some(rkey.clone());
    let (text, uniforms) = finish(g, format!("{title} (vertex)"), io, main);
    Ok(Stage { text, uniforms, varying, attribs })
}

fn flat_word(f: &Field, ty: &GTy) -> &'static str {
    if f.flat || ty.is_int() { "flat " } else { "" }
}

pub fn fragment_stage<'a>(ix: &'a Index<'a>, module: &str, f: &'a FnItem, title: &str) -> R<Stage> {
    let mut g = Gen::new(ix, Only::Fragment);
    let sig = g.emit_fn(module, f, f.name.span)?;
    if sig.ret != GTy::Vec(Base::F, 4) {
        return g.err_help(f.name.span, format!("a fragment fn returns the color as vec4, not {}", sig.ret.name()), "write `: vec4`");
    }
    if sig.params.len() > 1 {
        return g.err_help(f.params[1].name.span, "a fragment fn takes the `#[varying]` struct and nothing else", "read other values through `#[uniform]`s");
    }
    let (mut io, mut main) = (Vec::new(), Vec::new());
    let mut varying = None;
    let mut call_args = String::new();
    if let Some((ty, _)) = sig.params.first() {
        let GTy::Struct { key, glsl } = ty else {
            return g.err_help(f.params[0].ty.span, "a fragment fn takes the `#[varying]` struct the vertex fn returns", "or no parameters");
        };
        let info = g.sinfo[key].clone();
        if info.kind != GpuStruct::Varying {
            return g.err(f.params[0].ty.span, format!("`{}` is not a `#[varying]` struct", ty.name()));
        }
        let mut vals = Vec::new();
        for (i, fl) in info.fields.iter().enumerate() {
            if i == 0 {
                vals.push("gl_FragCoord".to_string());
                continue;
            }
            let v = mangle(&fl.name);
            io.push(format!("{}in {} v_{v};", flat_word(fl, &fl.ty), fl.ty.glsl()));
            vals.push(format!("v_{v}"));
        }
        main.push(format!("{glsl} p_0 = {glsl}({});", vals.join(", ")));
        call_args = "p_0".to_string();
        varying = Some(key.clone());
    }
    io.push("layout(location=0) out vec4 oColor;".to_string());
    main.push(format!("oColor = {}({call_args});", sig.glsl));
    let (text, uniforms) = finish(g, format!("{title} (fragment)"), io, main);
    Ok(Stage { text, uniforms, varying, attribs: Vec::new() })
}

pub struct Built {
    pub vert: String,
    pub frag: String,
    pub uniforms: Vec<String>,
    pub attribs: Vec<VAttr>,
}

/// The program for `@shader(vs, fs)`: both stages, and the check that the varying structs agree.
pub fn pair<'a>(ix: &'a Index<'a>, vm: &str, vf: &'a FnItem, fm: &str, ff: &'a FnItem, title: &str, at: crate::span::Span) -> R<Built> {
    let v = vertex_stage(ix, vm, vf, title)?;
    let f = fragment_stage(ix, fm, ff, title)?;
    if let Some(fk) = &f.varying {
        if v.varying.as_ref() != Some(fk) {
            let short = |k: &str| k.rsplit("::").next().unwrap_or(k).to_string();
            return Err(super::err_at(ix, at, format!("`{}` takes `{}` but `{}` returns `{}`", ff.name.name, short(fk), vf.name.name, v.varying.as_deref().map(short).unwrap_or_default()))
                .help("a vertex and a fragment fn are paired by the same `#[varying]` struct"));
        }
    }
    let mut uniforms = v.uniforms;
    for u in f.uniforms {
        if !uniforms.contains(&u) {
            uniforms.push(u);
        }
    }
    Ok(Built { vert: v.text, frag: f.text, uniforms, attribs: v.attribs })
}

/// Check one shader fn that no `@shader` reaches (so nothing is left unchecked).
pub fn check_alone<'a>(ix: &'a Index<'a>, module: &str, f: &'a FnItem) -> R<()> {
    let title = format!("{module}.{}", f.name.name);
    match f.shader {
        Some(GpuStage::Vertex) => vertex_stage(ix, module, f, &title).map(drop),
        Some(GpuStage::Fragment) => fragment_stage(ix, module, f, &title).map(drop),
        _ => Gen::new(ix, Only::Any).emit_fn(module, f, f.name.span).map(drop),
    }
}
