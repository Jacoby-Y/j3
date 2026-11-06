//! The `@name(..)` builtins shader code may call: one table, with the argument rule, the GLSL name and
//! the stage that has it. Everything here exists in both GLSL 3.30 and ES 3.00, so a shader that checks
//! is valid on both. To add a builtin, add a row (and its name to `INTRINSIC_GPU` in `parse/syntax.rs`).

use super::ty::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Only {
    Any,
    Vertex,
    Fragment,
}

#[derive(Clone, Copy)]
pub enum Rule {
    /// float scalar or vector -> same
    F1,
    /// float or signed int scalar or vector -> same
    N1,
    /// two floats of one type; the second may be an `f32` when the first is a vector (`scalar2`)
    F2 { scalar2: bool },
    /// float or int, min/max: second may be a scalar
    MinMax,
    Mix,
    Clamp,
    Step,
    Smooth,
    Dot,
    Length,
    Distance,
    Cross,
    Reflect,
    Refract,
    Mat1,
    Texture,
    TextureLod,
    TexelFetch,
    TextureSize,
    /// `gl_FragCoord`: a vec4
    FragCoord,
    /// `gl_VertexID`: an i32
    VertexId,
    /// `discard;`
    Discard,
}

#[derive(Clone, Copy)]
pub struct Builtin {
    pub name: &'static str,
    pub glsl: &'static str,
    pub min: usize,
    pub max: usize,
    pub rule: Rule,
    pub only: Only,
}

const fn b(name: &'static str, glsl: &'static str, min: usize, max: usize, rule: Rule, only: Only) -> Builtin {
    Builtin { name, glsl, min, max, rule, only }
}

use Only::{Any, Fragment, Vertex};
use Rule::*;

pub static TABLE: &[Builtin] = &[
    b("sin", "sin", 1, 1, F1, Any),
    b("cos", "cos", 1, 1, F1, Any),
    b("tan", "tan", 1, 1, F1, Any),
    b("exp", "exp", 1, 1, F1, Any),
    b("log", "log", 1, 1, F1, Any),
    b("sqrt", "sqrt", 1, 1, F1, Any),
    b("floor", "floor", 1, 1, F1, Any),
    b("ceil", "ceil", 1, 1, F1, Any),
    b("fract", "fract", 1, 1, F1, Any),
    b("normalize", "normalize", 1, 1, F1, Any),
    b("fwidth", "fwidth", 1, 1, F1, Fragment),
    b("dFdx", "dFdx", 1, 1, F1, Fragment),
    b("dFdy", "dFdy", 1, 1, F1, Fragment),
    b("abs", "abs", 1, 1, N1, Any),
    b("sign", "sign", 1, 1, N1, Any),
    b("pow", "pow", 2, 2, F2 { scalar2: false }, Any),
    b("atan", "atan", 1, 2, F2 { scalar2: false }, Any),
    b("mod", "mod", 2, 2, F2 { scalar2: true }, Any),
    b("min", "min", 2, 2, MinMax, Any),
    b("max", "max", 2, 2, MinMax, Any),
    b("mix", "mix", 3, 3, Mix, Any),
    b("clamp", "clamp", 3, 3, Clamp, Any),
    b("step", "step", 2, 2, Step, Any),
    b("smoothstep", "smoothstep", 3, 3, Smooth, Any),
    b("dot", "dot", 2, 2, Dot, Any),
    b("length", "length", 1, 1, Length, Any),
    b("distance", "distance", 2, 2, Distance, Any),
    b("cross", "cross", 2, 2, Cross, Any),
    b("reflect", "reflect", 2, 2, Reflect, Any),
    b("refract", "refract", 3, 3, Refract, Any),
    b("transpose", "transpose", 1, 1, Mat1, Any),
    b("inverse", "inverse", 1, 1, Mat1, Any),
    b("texture", "texture", 2, 2, Texture, Any),
    b("textureLod", "textureLod", 3, 3, TextureLod, Any),
    b("texelFetch", "texelFetch", 3, 3, TexelFetch, Any),
    b("textureSize", "textureSize", 2, 2, TextureSize, Any),
    b("discard", "discard", 0, 0, Discard, Fragment),
    b("vertexId", "gl_VertexID", 0, 0, VertexId, Vertex),
    b("fragCoord", "gl_FragCoord", 0, 0, FragCoord, Fragment),
];

pub fn lookup(name: &str) -> Option<&'static Builtin> {
    TABLE.iter().find(|x| x.name == name)
}

fn gf(t: &GTy) -> bool {
    matches!(t, GTy::Scalar(Base::F) | GTy::Vec(Base::F, _))
}
fn gn(t: &GTy, unsigned: bool) -> bool {
    gf(t) || matches!(t, GTy::Scalar(Base::I) | GTy::Vec(Base::I, _)) || (unsigned && matches!(t, GTy::Scalar(Base::U) | GTy::Vec(Base::U, _)))
}
/// `x` is `t` itself, or (for a vector `t`) a scalar of its element type.
fn same_or_scalar(t: &GTy, x: &GTy) -> bool {
    x == t || (matches!(t, GTy::Vec(..)) && t.elem().as_ref() == Some(x))
}

impl Builtin {
    /// The result type for these (already literal-resolved) argument types, or what the call wants.
    pub fn result(&self, a: &[GTy]) -> Result<GTy, String> {
        let bad = |want: &str| Err(want.to_string());
        let a0 = a.first().cloned().unwrap_or(GTy::Void);
        let a1 = a.get(1).cloned().unwrap_or(GTy::Void);
        let a2 = a.get(2).cloned().unwrap_or(GTy::Void);
        match self.rule {
            F1 if gf(&a0) => Ok(a0),
            F1 => bad("an f32 or a float vector"),
            N1 if gn(&a0, false) => Ok(a0),
            N1 => bad("an f32/i32 or a vector of them"),
            F2 { scalar2 } => {
                if !gf(&a0) {
                    return bad("float arguments of one type");
                }
                if a.len() == 1 || a1 == a0 || (scalar2 && same_or_scalar(&a0, &a1)) {
                    Ok(a0)
                } else {
                    bad(if scalar2 { "two floats of one type (the second may be an f32)" } else { "two floats of one type" })
                }
            }
            MinMax if gn(&a0, true) && same_or_scalar(&a0, &a1) => Ok(a0),
            MinMax => bad("two numbers of one type (the second may be a scalar)"),
            Mix if gf(&a0) && a1 == a0 && same_or_scalar(&a0, &a2) => Ok(a0),
            Mix => bad("(x, y, t): x and y one float type, t the same or an f32"),
            Clamp if gn(&a0, true) && same_or_scalar(&a0, &a1) && same_or_scalar(&a0, &a2) => Ok(a0),
            Clamp => bad("(x, lo, hi): lo and hi the type of x or a scalar"),
            Step if gf(&a1) && same_or_scalar(&a1, &a0) => Ok(a1),
            Step => bad("(edge, x): edge the type of x or an f32"),
            Smooth if gf(&a2) && a1 == a0 && same_or_scalar(&a2, &a0) => Ok(a2),
            Smooth => bad("(edge0, edge1, x): edges the type of x or f32"),
            Dot if gf(&a0) && a1 == a0 => Ok(F32),
            Dot => bad("two float vectors of one size"),
            Length if gf(&a0) => Ok(F32),
            Length => bad("a float vector"),
            Distance if gf(&a0) && a1 == a0 => Ok(F32),
            Distance => bad("two float vectors of one size"),
            Cross if a0 == GTy::Vec(Base::F, 3) && a1 == a0 => Ok(a0),
            Cross => bad("two vec3"),
            Reflect if gf(&a0) && a1 == a0 => Ok(a0),
            Reflect => bad("(incident, normal) of one float type"),
            Refract if gf(&a0) && a1 == a0 && a2 == F32 => Ok(a0),
            Refract => bad("(incident, normal, eta: f32)"),
            Mat1 if matches!(a0, GTy::Mat(_)) => Ok(a0),
            Mat1 => bad("a matrix"),
            Texture => match (&a0, &a1) {
                (GTy::Sampler(false), GTy::Vec(Base::F, 2)) => Ok(GTy::Vec(Base::F, 4)),
                (GTy::Sampler(true), GTy::Vec(Base::F, 3)) => Ok(F32),
                _ => bad("(sampler2D, vec2) or (sampler2DShadow, vec3)"),
            },
            TextureLod if a0 == GTy::Sampler(false) && a1 == GTy::Vec(Base::F, 2) && a2 == F32 => Ok(GTy::Vec(Base::F, 4)),
            TextureLod => bad("(sampler2D, vec2, lod: f32)"),
            TexelFetch if a0 == GTy::Sampler(false) && a1 == GTy::Vec(Base::I, 2) && a2 == I32 => Ok(GTy::Vec(Base::F, 4)),
            TexelFetch => bad("(sampler2D, ivec2, lod: i32)"),
            TextureSize if matches!(a0, GTy::Sampler(_)) && a1 == I32 => Ok(GTy::Vec(Base::I, 2)),
            TextureSize => bad("(sampler, lod: i32)"),
            FragCoord => Ok(GTy::Vec(Base::F, 4)),
            VertexId => Ok(I32),
            Discard => Ok(GTy::Void),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::syntax::INTRINSIC_GPU;

    #[test]
    fn names_match_the_parser_list() {
        for n in INTRINSIC_GPU {
            assert!(*n == "shader" || lookup(n).is_some(), "{n} is parsed but has no builtin row");
        }
        for t in TABLE {
            assert!(INTRINSIC_GPU.contains(&t.name), "{} has a row but the parser does not know it", t.name);
        }
    }
}
