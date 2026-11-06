//! `f16` (IEEE-754 binary16) conversions done in integers, so the compiler's constants and both
//! backends agree bit for bit. Round to nearest even; overflow gives infinity; NaN stays NaN
//! (quiet, payload top bits kept, like compiler-rt's `__truncdfhf2`). The C backend carries the
//! same algorithm as text in `emit/c_expr/half.rs`; keep them in step.

/// `f64` to binary16 bits.
pub fn from_f64(d: f64) -> u16 {
    let bits = d.to_bits();
    let sign = ((bits >> 48) & 0x8000) as u16;
    let abs = bits & 0x7fff_ffff_ffff_ffff;
    if abs > 0x7ff0_0000_0000_0000 {
        return sign | 0x7c00 | 0x200 | ((abs & 0x0007_ffff_ffff_ffff) >> 42) as u16;
    }
    let e = (abs >> 52) as i32 - 1023;
    if e > 15 {
        return sign | 0x7c00;
    }
    let mant = (abs & 0x000f_ffff_ffff_ffff) | 0x0010_0000_0000_0000;
    let (mut r, shift) = if e >= -14 {
        ((((e + 15) as u64) << 10) + ((mant >> 42) - 1024), 42)
    } else {
        (0, 42 + (-14 - e) as u32)
    };
    if abs >> 52 == 0 || shift > 53 {
        return sign;
    }
    let rem = mant & ((1u64 << shift) - 1);
    let half = 1u64 << (shift - 1);
    if e < -14 {
        r = mant >> shift;
    }
    if rem > half || (rem == half && r & 1 == 1) {
        r += 1;
    }
    sign | r.min(0x7c00) as u16
}

#[cfg(test)]
mod tests {
    use super::from_f64;

    #[test]
    fn known_values() {
        assert_eq!(from_f64(0.0), 0);
        assert_eq!(from_f64(-0.0), 0x8000);
        assert_eq!(from_f64(1.0), 0x3c00);
        assert_eq!(from_f64(65504.0), 0x7bff);
        assert_eq!(from_f64(65520.0), 0x7c00);
        assert_eq!(from_f64(65519.9), 0x7bff);
        assert_eq!(from_f64(1e-8), 0);
        assert_eq!(from_f64(2f64.powi(-24)), 1);
        assert_eq!(from_f64(2f64.powi(-25)), 0);
        assert_eq!(from_f64(2f64.powi(-25) * 1.0000001), 1);
        assert_eq!(from_f64(1.0 + 2f64.powi(-11)), 0x3c00);
        assert_eq!(from_f64(1.0 + 3.0 * 2f64.powi(-11)), 0x3c02);
        assert_eq!(from_f64(2f64.powi(-14)), 0x0400);
        assert_eq!(from_f64(f64::INFINITY), 0x7c00);
        assert_eq!(from_f64(f64::NEG_INFINITY), 0xfc00);
        assert_eq!(from_f64(f64::NAN) & 0x7e00, 0x7e00);
    }
}
