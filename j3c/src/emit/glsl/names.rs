//! Name mangling: J3 names that GLSL reserves are renamed; uniform names are never renamed (the CPU binds by name).

/// GLSL 3.30 core and ES 3.00 keywords, reserved words and built-in function names.
const RESERVED: &[&str] = &[
    "attribute", "const", "uniform", "varying", "buffer", "shared", "coherent", "volatile", "restrict", "readonly",
    "writeonly", "atomic_uint", "layout", "centroid", "flat", "smooth", "noperspective", "patch", "sample", "break",
    "continue", "do", "for", "while", "switch", "case", "default", "if", "else", "subroutine", "in", "out", "inout",
    "float", "double", "int", "void", "bool", "true", "false", "invariant", "precise", "discard", "return", "mat2",
    "mat3", "mat4", "mat2x2", "mat2x3", "mat2x4", "mat3x2", "mat3x3", "mat3x4", "mat4x2", "mat4x3", "mat4x4", "dmat2",
    "dmat3", "dmat4", "vec2", "vec3", "vec4", "ivec2", "ivec3", "ivec4", "bvec2", "bvec3", "bvec4", "uvec2", "uvec3",
    "uvec4", "dvec2", "dvec3", "dvec4", "uint", "lowp", "mediump", "highp", "precision", "sampler1D", "sampler2D",
    "sampler3D", "samplerCube", "sampler2DShadow", "sampler2DArray", "isampler2D", "usampler2D", "struct", "common",
    "partition", "active", "asm", "class", "union", "enum", "typedef", "template", "this", "resource", "goto", "inline",
    "noinline", "public", "static", "extern", "external", "interface", "long", "short", "half", "fixed", "unsigned",
    "superp", "input", "output", "filter", "sizeof", "cast", "namespace", "using", "main",
    "radians", "degrees", "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "asinh", "acosh",
    "atanh", "pow", "exp", "log", "exp2", "log2", "sqrt", "inversesqrt", "abs", "sign", "floor", "trunc", "round",
    "roundEven", "ceil", "fract", "mod", "modf", "min", "max", "clamp", "mix", "step", "smoothstep", "isnan", "isinf",
    "floatBitsToInt", "floatBitsToUint", "intBitsToFloat", "uintBitsToFloat", "length", "distance", "dot", "cross",
    "normalize", "faceforward", "reflect", "refract", "matrixCompMult", "outerProduct", "transpose", "determinant",
    "inverse", "lessThan", "lessThanEqual", "greaterThan", "greaterThanEqual", "equal", "notEqual", "any", "all", "not",
    "texture", "textureSize", "textureLod", "textureProj", "texelFetch", "textureGrad", "textureOffset", "dFdx",
    "dFdy", "fwidth", "oColor",
];

/// Whether `name` cannot be used as is in emitted GLSL.
pub fn is_reserved(name: &str) -> bool {
    RESERVED.contains(&name) || name.starts_with("gl_") || name.contains("__") || name.starts_with("j3_") || is_io_name(name)
}

/// Names the program writer makes for vertex inputs and varyings.
fn is_io_name(name: &str) -> bool {
    name.starts_with("v_") || name.starts_with("a_")
}

/// The emitted spelling of a local, parameter, field, fn, struct or const name.
pub fn mangle(name: &str) -> String {
    if is_reserved(name) {
        format!("j3_{name}")
    } else {
        name.to_string()
    }
}
