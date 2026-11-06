//! `vec4f` / `vec4i` as C: GCC/clang vector extensions, same results as the LLVM backend bit for
//! bit (see `emit/llvm_expr/simd.rs`). Anything with a subtle meaning goes through a `j3_v4_*`
//! helper from [`PRELUDE`]; the helpers are only emitted when the program names a vector type.

use super::*;
use crate::ir::simd::SimdOp;
use crate::ir::ty::VecTy;

/// C type name of a vector. `may_alias` lets lane places (`*(float *)&v + k`) and `memcpy`-free
/// loads share storage with the vector without strict-aliasing surprises.
pub fn vec_c_name(v: VecTy) -> &'static str {
    match v {
        VecTy::F32x4 => "j3_vec4f",
        VecTy::I32x4 => "j3_vec4i",
    }
}

/// True if the generated C names a vector type.
pub fn uses_vectors(c: &str) -> bool {
    c.contains("j3_vec4")
}

/// Typedefs and helpers, put after the includes.
pub const PRELUDE: &str = r#"typedef float j3_vec4f __attribute__((vector_size(16), may_alias));
typedef int32_t j3_vec4i __attribute__((vector_size(16), may_alias));
typedef uint32_t j3_vec4u __attribute__((vector_size(16)));
/* a < b ? a : b per lane (NaN or a tie gives b), like LLVM's fcmp + select */
#define J3_V4_SEL(m, a, b, T) ((T)(((m) & (j3_vec4i)(a)) | (~(m) & (j3_vec4i)(b))))
static inline j3_vec4f j3_v4_minf(j3_vec4f a, j3_vec4f b) { j3_vec4i m = a < b; return J3_V4_SEL(m, a, b, j3_vec4f); }
static inline j3_vec4f j3_v4_maxf(j3_vec4f a, j3_vec4f b) { j3_vec4i m = a > b; return J3_V4_SEL(m, a, b, j3_vec4f); }
static inline j3_vec4i j3_v4_mini(j3_vec4i a, j3_vec4i b) { j3_vec4i m = a < b; return J3_V4_SEL(m, a, b, j3_vec4i); }
static inline j3_vec4i j3_v4_maxi(j3_vec4i a, j3_vec4i b) { j3_vec4i m = a > b; return J3_V4_SEL(m, a, b, j3_vec4i); }
static inline j3_vec4f j3_v4_sqrt(j3_vec4f a) {
#if defined(__clang__)
    return __builtin_elementwise_sqrt(a);
#else
    return (j3_vec4f){ __builtin_sqrtf(a[0]), __builtin_sqrtf(a[1]), __builtin_sqrtf(a[2]), __builtin_sqrtf(a[3]) };
#endif
}
static inline j3_vec4f j3_v4_absf(j3_vec4f a) { return (j3_vec4f)((j3_vec4i)a & 0x7fffffff); }
static inline j3_vec4i j3_v4_absi(j3_vec4i a) { j3_vec4i m = a < 0; j3_vec4i n = (j3_vec4i)(-(j3_vec4u)a); return (m & n) | (~m & a); }
static inline float j3_v4_sumf(j3_vec4f a) { return (a[0] + a[1]) + (a[2] + a[3]); }
static inline int32_t j3_v4_sumi(j3_vec4i a) { return (int32_t)(((uint32_t)a[0] + (uint32_t)a[1]) + ((uint32_t)a[2] + (uint32_t)a[3])); }
/* truncating, saturating, NaN gives 0: llvm.fptosi.sat */
static inline j3_vec4i j3_v4_toint(j3_vec4f a) {
    j3_vec4i r;
    for (int i = 0; i < 4; i++) {
        float x = a[i];
        r[i] = x != x ? 0 : x >= 2147483648.0f ? INT32_MAX : x <= -2147483648.0f ? INT32_MIN : (int32_t)x;
    }
    return r;
}
static inline j3_vec4i j3_v4_add(j3_vec4i a, j3_vec4i b) { return (j3_vec4i)((j3_vec4u)a + (j3_vec4u)b); }
static inline j3_vec4i j3_v4_sub(j3_vec4i a, j3_vec4i b) { return (j3_vec4i)((j3_vec4u)a - (j3_vec4u)b); }
static inline j3_vec4i j3_v4_mul(j3_vec4i a, j3_vec4i b) { return (j3_vec4i)((j3_vec4u)a * (j3_vec4u)b); }
/* the shift amount is masked to 0..31, like the scalar i32 shift */
static inline j3_vec4i j3_v4_shl(j3_vec4i a, j3_vec4i b) { return (j3_vec4i)((j3_vec4u)a << (j3_vec4u)(b & 31)); }
static inline j3_vec4i j3_v4_shr(j3_vec4i a, j3_vec4i b) { return a >> (b & 31); }
static inline j3_vec4f j3_v4_loadf(const void *p) { j3_vec4f r; memcpy(&r, p, 16); return r; }
static inline j3_vec4i j3_v4_loadi(const void *p) { j3_vec4i r; memcpy(&r, p, 16); return r; }
static inline void j3_v4_storef(void *p, j3_vec4f v) { memcpy(p, &v, 16); }
static inline void j3_v4_storei(void *p, j3_vec4i v) { memcpy(p, &v, 16); }

"#;

impl<'a> CWriter<'a> {
    fn vec_of(&self, ty: Ty) -> Result<VecTy, String> {
        match self.cx.ty_kind(ty) {
            TyKind::Vec(v) => Ok(*v),
            _ => Err("vector op on a non-vector".to_string()),
        }
    }

    /// Lane `k` as a place: an lvalue of the lane type inside the vector's storage.
    pub(super) fn lane_place(&self, text: &str, vec_ty: Ty, k: u8) -> Result<String, String> {
        let lane = match self.vec_of(vec_ty)? {
            VecTy::F32x4 => "float",
            VecTy::I32x4 => "int32_t",
        };
        Ok(format!("(*(({lane} *)&({text}) + {k}))"))
    }

    /// `+ - * /` and the bit ops on vectors. Signed int arithmetic wraps (done on unsigned lanes).
    pub(super) fn vec_binop(&self, ty: Ty, op: BinOp, l: &str, r: &str) -> Result<String, String> {
        let v = self.vec_of(ty)?;
        Ok(match (v, op) {
            (VecTy::F32x4, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div) => format!("({l} {} {r})", op.as_str()),
            (VecTy::I32x4, BinOp::Add) => format!("j3_v4_add({l}, {r})"),
            (VecTy::I32x4, BinOp::Sub) => format!("j3_v4_sub({l}, {r})"),
            (VecTy::I32x4, BinOp::Mul) => format!("j3_v4_mul({l}, {r})"),
            (VecTy::I32x4, BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor) => format!("({l} {} {r})", op.as_str()),
            (VecTy::I32x4, BinOp::Shl) => format!("j3_v4_shl({l}, {r})"),
            (VecTy::I32x4, BinOp::Shr) => format!("j3_v4_shr({l}, {r})"),
            _ => return Err(format!("`{}` on {}", op.as_str(), v.name())),
        })
    }

    pub(super) fn vec_unary(&self, ty: Ty, op: UnaryOp, a: &str) -> Result<String, String> {
        let v = self.vec_of(ty)?;
        Ok(match (v, op) {
            (VecTy::F32x4, UnaryOp::Neg) => format!("(-{a})"),
            (VecTy::I32x4, UnaryOp::Neg) => format!("j3_v4_sub(({}){{0, 0, 0, 0}}, {a})", vec_c_name(v)),
            (VecTy::I32x4, UnaryOp::BitNot) => format!("(~{a})"),
            _ => return Err(format!("`{}` on {}", op.as_str(), v.name())),
        })
    }

    pub(super) fn simd(&mut self, rv: &Rvalue, op: SimdOp, args: &[Operand]) -> Result<String, String> {
        let mut a = Vec::with_capacity(args.len());
        for arg in args {
            a.push(self.operand(arg)?);
        }
        let float = |ty: Ty| matches!(self.cx.ty_kind(ty), TyKind::Vec(VecTy::F32x4));
        let f = |ty: Ty, fl: &'static str, it: &'static str| if float(ty) { fl } else { it };
        Ok(match op {
            SimdOp::Splat => format!("(({}){{{x}, {x}, {x}, {x}}})", c_type(self.cx, rv.ty)?, x = a[0]),
            SimdOp::Build => format!("(({}){{{}, {}, {}, {}}})", c_type(self.cx, rv.ty)?, a[0], a[1], a[2], a[3]),
            SimdOp::Min => format!("{}({}, {})", f(rv.ty, "j3_v4_minf", "j3_v4_mini"), a[0], a[1]),
            SimdOp::Max => format!("{}({}, {})", f(rv.ty, "j3_v4_maxf", "j3_v4_maxi"), a[0], a[1]),
            SimdOp::Sqrt => format!("j3_v4_sqrt({})", a[0]),
            SimdOp::Abs => format!("{}({})", f(rv.ty, "j3_v4_absf", "j3_v4_absi"), a[0]),
            SimdOp::Sum => format!("{}({})", f(args[0].ty(), "j3_v4_sumf", "j3_v4_sumi"), a[0]),
            SimdOp::Shuffle(ix) => {
                format!("__builtin_shufflevector({0}, {0}, {1}, {2}, {3}, {4})", a[0], ix[0], ix[1], ix[2], ix[3])
            }
            SimdOp::ToFloat => format!("__builtin_convertvector({}, j3_vec4f)", a[0]),
            SimdOp::ToInt => format!("j3_v4_toint({})", a[0]),
            SimdOp::Load => format!("{}({})", f(rv.ty, "j3_v4_loadf", "j3_v4_loadi"), a[0]),
            SimdOp::Store => format!("{}({}, {})", f(args[1].ty(), "j3_v4_storef", "j3_v4_storei"), a[0], a[1]),
        })
    }
}
