//! Shader types: the GLSL scalar, vector, matrix and sampler types, structs, and fixed arrays.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Base {
    F,
    I,
    U,
    B,
}

#[derive(Clone, PartialEq, Debug)]
pub enum GTy {
    Void,
    Scalar(Base),
    Vec(Base, u8),
    /// `mat2`, `mat3`, `mat4` (square, float).
    Mat(u8),
    /// `sampler2D`; `true` is `sampler2DShadow`.
    Sampler(bool),
    /// `key` is `module::Name`, `glsl` the emitted name.
    Struct { key: String, glsl: String },
    Array(Box<GTy>, u64),
    /// An integer literal that has not met a type yet; takes the other operand's, or `i32`.
    LitInt,
    /// A float literal; `f32`.
    LitFloat,
}

pub const F32: GTy = GTy::Scalar(Base::F);
pub const I32: GTy = GTy::Scalar(Base::I);
pub const U32: GTy = GTy::Scalar(Base::U);
pub const BOOL: GTy = GTy::Scalar(Base::B);

impl Base {
    pub fn word(self) -> &'static str {
        match self {
            Base::F => "f32",
            Base::I => "i32",
            Base::U => "u32",
            Base::B => "bool",
        }
    }
    fn prefix(self) -> &'static str {
        match self {
            Base::F => "",
            Base::I => "i",
            Base::U => "u",
            Base::B => "b",
        }
    }
}

impl GTy {
    /// The GLSL spelling (`vec3`, `float[4]`, a struct's emitted name).
    pub fn glsl(&self) -> String {
        match self {
            GTy::Void => "void".into(),
            GTy::Scalar(Base::F) | GTy::LitFloat => "float".into(),
            GTy::Scalar(Base::I) | GTy::LitInt => "int".into(),
            GTy::Scalar(Base::U) => "uint".into(),
            GTy::Scalar(Base::B) => "bool".into(),
            GTy::Vec(b, n) => format!("{}vec{n}", b.prefix()),
            GTy::Mat(n) => format!("mat{n}"),
            GTy::Sampler(false) => "sampler2D".into(),
            GTy::Sampler(true) => "sampler2DShadow".into(),
            GTy::Struct { glsl, .. } => glsl.clone(),
            GTy::Array(e, n) => format!("{}[{n}]", e.glsl()),
        }
    }

    /// The name in diagnostics (`f32` for a scalar, the J3 name for a struct).
    pub fn name(&self) -> String {
        match self {
            GTy::Scalar(b) => b.word().into(),
            GTy::LitInt => "an integer literal".into(),
            GTy::LitFloat => "a float literal".into(),
            GTy::Struct { key, .. } => key.rsplit("::").next().unwrap_or(key).to_string(),
            GTy::Array(e, n) => format!("[{n}]{}", e.name()),
            t => t.glsl(),
        }
    }

    pub fn is_scalar(&self) -> bool {
        matches!(self, GTy::Scalar(_) | GTy::LitInt | GTy::LitFloat)
    }

    /// `f32`/`vec3`/`mat4`: float math.
    pub fn is_float(&self) -> bool {
        matches!(self, GTy::Scalar(Base::F) | GTy::LitFloat | GTy::Vec(Base::F, _) | GTy::Mat(_))
    }

    pub fn is_int(&self) -> bool {
        matches!(self, GTy::Scalar(Base::I | Base::U) | GTy::LitInt | GTy::Vec(Base::I | Base::U, _))
    }

    /// Scalar type of a scalar or vector (`vec3` -> `f32`).
    pub fn elem(&self) -> Option<GTy> {
        match self {
            GTy::Scalar(_) | GTy::LitInt | GTy::LitFloat => Some(self.clone()),
            GTy::Vec(b, _) => Some(GTy::Scalar(*b)),
            _ => None,
        }
    }

    /// Component count of a scalar (1) or vector.
    pub fn lanes(&self) -> Option<u8> {
        match self {
            GTy::Scalar(_) | GTy::LitInt | GTy::LitFloat => Some(1),
            GTy::Vec(_, n) => Some(*n),
            _ => None,
        }
    }

    /// The type a literal settles on when nothing else decides.
    pub fn settle(&self) -> GTy {
        match self {
            GTy::LitInt => I32,
            GTy::LitFloat => F32,
            t => t.clone(),
        }
    }
}

/// `vec3`, `ivec2`, `mat4`, `sampler2D`, ... and the four scalars; `None` for any other word.
pub fn builtin_type(name: &str) -> Option<GTy> {
    let scalar = match name {
        "f32" => Some(F32),
        "i32" => Some(I32),
        "u32" => Some(U32),
        "bool" => Some(BOOL),
        _ => None,
    };
    if scalar.is_some() {
        return scalar;
    }
    match name {
        "sampler2D" => return Some(GTy::Sampler(false)),
        "sampler2DShadow" => return Some(GTy::Sampler(true)),
        "mat2" => return Some(GTy::Mat(2)),
        "mat3" => return Some(GTy::Mat(3)),
        "mat4" => return Some(GTy::Mat(4)),
        _ => {}
    }
    let (base, rest) = if let Some(r) = name.strip_prefix("ivec") {
        (Base::I, r)
    } else if let Some(r) = name.strip_prefix("uvec") {
        (Base::U, r)
    } else if let Some(r) = name.strip_prefix("bvec") {
        (Base::B, r)
    } else if let Some(r) = name.strip_prefix("vec") {
        (Base::F, r)
    } else {
        return None;
    };
    match rest {
        "2" => Some(GTy::Vec(base, 2)),
        "3" => Some(GTy::Vec(base, 3)),
        "4" => Some(GTy::Vec(base, 4)),
        _ => None,
    }
}

/// Component count of swizzle `name` on an `n`-lane vector (`xyz`, `rg`, `stp`; one letter set), or `None`.
pub fn swizzle_len(name: &str, n: u8) -> Option<u8> {
    for set in ["xyzw", "rgba", "stpq"] {
        if !name.is_empty() && name.len() <= 4 && name.chars().all(|c| set[..n as usize].contains(c)) {
            return Some(name.len() as u8);
        }
    }
    None
}

/// Whether the swizzle repeats a component (`xx`), which cannot be assigned to.
pub fn swizzle_repeats(name: &str) -> bool {
    let b = name.as_bytes();
    (0..b.len()).any(|i| b[i + 1..].contains(&b[i]))
}
