//! `f16` as C: a `uint16_t` holding the binary16 bits, converted by plain integer code so the
//! results match the LLVM backend (`fptrunc`/`fpext` through `half`) and `ir/half.rs` bit for bit
//! on every C compiler, with or without `_Float16`. Emitted only when the program names `j3_f16_`.

use super::*;

/// True if the generated C calls an `f16` helper.
pub fn uses_half(c: &str) -> bool {
    c.contains("j3_f16_")
}

/// Cast text for a cast where `from` or `to` is `f16`; `None` when neither is.
pub fn cast(cx: &IrCx, from: Ty, to: Ty, v: &str) -> Option<String> {
    let h = cx.common.f16;
    if to == h && from != h {
        return Some(format!("j3_f16_from_f64((double)({v}))"));
    }
    if from == h && to != h {
        return Some(format!("({})j3_f16_to_f32({v})", c_type(cx, to).ok()?));
    }
    None
}

/// Helpers, put after the includes.
pub const PRELUDE: &str = r#"static inline uint16_t j3_f16_from_f64(double d) {
    uint64_t b; memcpy(&b, &d, 8);
    uint16_t s = (uint16_t)((b >> 48) & 0x8000u);
    uint64_t a = b & 0x7fffffffffffffffull;
    if (a > 0x7ff0000000000000ull) return (uint16_t)(s | 0x7e00u | ((a & 0x0007ffffffffffffull) >> 42));
    int e = (int)(a >> 52) - 1023;
    if (e > 15) return (uint16_t)(s | 0x7c00u);
    uint64_t m = (a & 0x000fffffffffffffull) | 0x0010000000000000ull, r;
    int sh = 42;
    if (e >= -14) { r = ((uint64_t)(e + 15) << 10) + ((m >> 42) - 1024); }
    else { sh = 42 + (-14 - e); r = 0; }
    if ((a >> 52) == 0 || sh > 53) return s;
    uint64_t rem = m & ((1ull << sh) - 1), half = 1ull << (sh - 1);
    if (e < -14) r = m >> sh;
    if (rem > half || (rem == half && (r & 1))) r++;
    if (r > 0x7c00u) r = 0x7c00u;
    return (uint16_t)(s | r);
}
static inline float j3_f16_to_f32(uint16_t h) {
    uint32_t s = (uint32_t)(h & 0x8000u) << 16, e = (h >> 10) & 0x1fu, m = h & 0x3ffu, b;
    if (e == 0) {
        if (m == 0) b = s;
        else { int k = 0; while (!(m & 0x400u)) { m <<= 1; k++; } b = s | ((uint32_t)(113 - k) << 23) | ((m & 0x3ffu) << 13); }
    } else if (e == 31) b = s | 0x7f800000u | (m << 13) | (m ? 0x400000u : 0u);
    else b = s | ((e + 112) << 23) | (m << 13);
    float f; memcpy(&f, &b, 4); return f;
}
"#;
