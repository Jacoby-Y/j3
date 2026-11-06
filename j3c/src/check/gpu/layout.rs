//! `#[vertex]` structs: the CPU field types the shader sees as GPU types, and the check that the struct's real
//! (C) layout is the tightly packed one a vertex buffer attribute list describes.

use crate::parse::{FieldItem, StructItem, TypeKind};

use super::gen::Gen;
use super::ty::*;
use super::R;

/// What a CPU field type is on the GPU.
pub struct CpuTy {
    pub gpu: GTy,
    pub size: u32,
    pub align: u32,
    /// Components per attribute location (a `mat4` is four locations of 4).
    pub comps: u32,
    pub locs: u32,
    pub gl_type: u32,
    pub is_int: bool,
    /// Fixed-point bytes read as 0..1 floats (`Rgba8`).
    pub normalized: bool,
}

const GL_INT: u32 = 0x1404;
const GL_UNSIGNED_INT: u32 = 0x1405;
const GL_UNSIGNED_BYTE: u32 = 0x1401;
const GL_FLOAT: u32 = 0x1406;

pub fn cpu_ty(name: &str) -> Option<CpuTy> {
    let f = |gpu, size, align, comps, locs, gl_type, is_int| CpuTy { gpu, size, align, comps, locs, gl_type, is_int, normalized: false };
    Some(match name {
        "f32" => f(F32, 4, 4, 1, 1, GL_FLOAT, false),
        "i32" => f(I32, 4, 4, 1, 1, GL_INT, true),
        "u32" => f(U32, 4, 4, 1, 1, GL_UNSIGNED_INT, true),
        "Vec2" => f(GTy::Vec(Base::F, 2), 8, 4, 2, 1, GL_FLOAT, false),
        "Vec3" => f(GTy::Vec(Base::F, 3), 12, 4, 3, 1, GL_FLOAT, false),
        "vec4f" => f(GTy::Vec(Base::F, 4), 16, 16, 4, 1, GL_FLOAT, false),
        "Mat4" => f(GTy::Mat(4), 64, 4, 4, 4, GL_FLOAT, false),
        // four bytes, the shader sees a vec4 of 0..1
        "Rgba8" => CpuTy { normalized: true, ..f(GTy::Vec(Base::F, 4), 4, 1, 4, 1, GL_UNSIGNED_BYTE, false) },
        _ => return None,
    })
}

fn field_ty(g: &Gen, f: &FieldItem) -> R<CpuTy> {
    if let TypeKind::Named(n) = &f.ty.kind {
        if let Some(c) = cpu_ty(n) {
            return Ok(c);
        }
    }
    let mut help = "those have a GPU twin (float, int, uint, vec2, vec3, vec4, mat4; `Rgba8` is four bytes read as a 0..1 vec4)".to_string();
    if matches!(&f.ty.kind, TypeKind::Named(n) if matches!(n.as_str(), "u8" | "i8" | "u16" | "i16")) {
        help.push_str("; for padding use an f32 or Vec2 `_pad` field, not a small integer");
    }
    g.err_help(f.ty.span, "a `#[vertex]` field is f32, i32, u32, Vec2, Vec3, vec4f, Mat4 or Rgba8", help)
}

/// The GPU type of a `#[vertex]` struct field.
pub fn cpu_to_gpu(g: &mut Gen, f: &FieldItem) -> R<GTy> {
    Ok(field_ty(g, f)?.gpu)
}

fn align_up(v: u32, a: u32) -> u32 {
    v.div_ceil(a) * a
}

/// The bytes of padding a `_pad` field would need, as the type to write.
fn pad_word(bytes: u32) -> String {
    match bytes {
        4 => "`_pad: f32`".into(),
        8 => "`_pad: Vec2`".into(),
        12 => "`_pad: Vec3`".into(),
        n => format!("`_pad` fields totalling {n} bytes"),
    }
}

/// CPU offsets must equal the packed offsets, field by field and in total.
pub fn check_layout(g: &Gen, s: &StructItem) -> R<()> {
    let (mut cpu, mut packed, mut max_align) = (0u32, 0u32, 1u32);
    for f in &s.fields {
        let c = field_ty(g, f)?;
        let at = align_up(cpu, c.align);
        if at != packed {
            return g.err_help(
                f.name.span,
                format!("`{}.{}` is at byte {at} in the CPU struct but vertex data is packed, so the shader reads it at byte {packed}", s.name.name, f.name.name),
                format!("insert {} before it, or put the `vec4f` fields first", pad_word(at - packed)),
            );
        }
        cpu = at + c.size;
        packed += c.size;
        max_align = max_align.max(c.align);
    }
    let size = align_up(cpu, max_align);
    if size != packed {
        return g.err_help(
            s.name.span,
            format!("`{}` is {size} bytes on the CPU (aligned to {max_align}) but its vertex stride is {packed}", s.name.name),
            format!("add {} at the end", pad_word(size - packed)),
        );
    }
    Ok(())
}

/// Vertex attribute slots a program has (the GL minimum); `Source.data` holds this many.
pub const MAX_ATTRIBS: u32 = 16;

/// The field of `s` whose attribute location (counting from `first`) passes the limit is the error.
pub fn check_limit(g: &Gen, s: &StructItem, first: u32) -> R<()> {
    let mut loc = first;
    for f in &s.fields {
        let c = field_ty(g, f)?;
        if f.name.name.starts_with('_') {
            continue;
        }
        loc += c.locs;
        if loc > MAX_ATTRIBS {
            return g.err_help(f.name.span, format!("`{}.{}` needs attribute location {}, past the limit of {MAX_ATTRIBS}", s.name.name, f.name.name, loc - 1), format!("a program has at most {MAX_ATTRIBS} vertex attribute locations (a Mat4 takes 4); drop or pack fields"));
        }
    }
    Ok(())
}

pub struct VAttr {
    pub loc: u32,
    pub comps: u32,
    pub gl_type: u32,
    pub offset: u32,
    pub stride: u32,
    pub divisor: u32,
    pub is_int: bool,
    pub normalized: bool,
}

/// One attribute per location, from `first`; `_`-named pad fields take bytes but no location.
pub fn attribs(g: &Gen, s: &StructItem, first: u32, divisor: u32) -> R<Vec<VAttr>> {
    let mut out = Vec::new();
    let (mut off, mut loc) = (0u32, first);
    let mut all = Vec::new();
    for f in &s.fields {
        all.push((f, field_ty(g, f)?));
    }
    let stride: u32 = all.iter().map(|(_, c)| c.size).sum();
    for (f, c) in all {
        if !f.name.name.starts_with('_') {
            for i in 0..c.locs {
                out.push(VAttr { loc, comps: c.comps, gl_type: c.gl_type, offset: off + i * 16, stride, divisor, is_int: c.is_int, normalized: c.normalized });
                loc += 1;
            }
        }
        off += c.size;
    }
    Ok(out)
}
