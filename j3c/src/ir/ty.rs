//! Primitive widths and interned types.
//!
//! `int` is `i64`. `float` is `f64`. The IR stores the canonical primitive;
//! dumps print the alias names (`int`, `float`) for those two.

use crate::ir::ids::{EnumId, StructId};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Ty(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum IntTy {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
}

impl IntTy {
    pub fn width_bytes(self) -> u64 {
        (self.bits() / 8) as u64
    }

    pub fn bits(self) -> u32 {
        match self {
            IntTy::I8 | IntTy::U8 => 8,
            IntTy::I16 | IntTy::U16 => 16,
            IntTy::I32 | IntTy::U32 => 32,
            IntTy::I64 | IntTy::U64 => 64,
        }
    }

    pub fn is_signed(self) -> bool {
        match self {
            IntTy::I8 | IntTy::I16 | IntTy::I32 | IntTy::I64 => true,
            IntTy::U8 | IntTy::U16 | IntTy::U32 | IntTy::U64 => false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FloatTy {
    F32,
    F64,
}

/// SIMD vector types: 4 lanes, 16 bytes, align 16. `vec4f` is 4 x f32, `vec4i` is 4 x i32.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum VecTy {
    F32x4,
    I32x4,
}

impl VecTy {
    pub const LANES: u64 = 4;

    /// Source spelling.
    pub fn name(self) -> &'static str {
        match self {
            VecTy::F32x4 => "vec4f",
            VecTy::I32x4 => "vec4i",
        }
    }

    /// LLVM type.
    pub fn llvm(self) -> &'static str {
        match self {
            VecTy::F32x4 => "<4 x float>",
            VecTy::I32x4 => "<4 x i32>",
        }
    }

    /// LLVM type of one lane.
    pub fn lane_llvm(self) -> &'static str {
        match self {
            VecTy::F32x4 => "float",
            VecTy::I32x4 => "i32",
        }
    }
}

/// Canonical type. Interned in [`crate::ir::cx::IrCx`].
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum TyKind {
    Void,
    Bool,
    Int(IntTy),
    Float(FloatTy),
    /// Untyped address. FFI / stdlib only.
    Ptr,
    /// `char*`. Stdlib / `use lib` only.
    CStr,
    /// Lang item. Read-only `[]u8` view. Not writable.
    String,
    Ref(Ty),
    Array {
        elem: Ty,
        len: u64,
    },
    Slice {
        elem: Ty,
    },
    Fn {
        params: Vec<Ty>,
        ret: Ty,
    },
    Struct(StructId),
    Enum(EnumId),
    /// `f16`: storage only (2 bytes, align 2). Held as its IEEE-754 binary16 bits.
    Half,
    /// `vec4f` / `vec4i`. Lanes are `.x .y .z .w`.
    Vec(VecTy),
}

impl TyKind {
    pub fn is_int_alias(&self) -> bool {
        matches!(self, TyKind::Int(IntTy::I64))
    }

    pub fn is_float_alias(&self) -> bool {
        matches!(self, TyKind::Float(FloatTy::F64))
    }
}
