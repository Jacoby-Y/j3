//! Integer wrap and `/0` — one rule for every printer.
//!
//! Overflow wraps (two's complement). Division or remainder by zero is
//! `Abort`, not a host panic and not C UB. `INT_MIN / -1` wraps to `MIN`.

use crate::ir::op::{BinOp, UnaryOp};

/// Debug interpolate cap. Overflow of this buffer is `Abort`.
pub const INTERP_CAP: u64 = 512;

pub fn wrapping_bin(op: BinOp) -> bool {
    matches!(
        op,
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem | BinOp::Shl | BinOp::Shr
    )
}

pub fn wrapping_unary(op: UnaryOp) -> bool {
    matches!(op, UnaryOp::Neg)
}

/// Fold a primitive int binop. `None` means do not fold (`/0`, non-int op).
pub fn fold_bin(op: BinOp, a: i64, b: i64) -> Option<i64> {
    match op {
        BinOp::Add => Some(a.wrapping_add(b)),
        BinOp::Sub => Some(a.wrapping_sub(b)),
        BinOp::Mul => Some(a.wrapping_mul(b)),
        BinOp::Div => {
            if b == 0 {
                None
            } else {
                Some(a.wrapping_div(b))
            }
        }
        BinOp::Rem => {
            if b == 0 {
                None
            } else {
                Some(a.wrapping_rem(b))
            }
        }
        _ => None,
    }
}

pub fn fold_neg(a: i64) -> i64 {
    a.wrapping_neg()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn div_zero_does_not_panic_host() {
        assert_eq!(fold_bin(BinOp::Div, 1, 0), None);
        assert_eq!(fold_bin(BinOp::Rem, 1, 0), None);
        assert_eq!(fold_bin(BinOp::Div, i64::MIN, -1), Some(i64::MIN));
        assert_eq!(fold_bin(BinOp::Rem, i64::MIN, -1), Some(0));
        assert_eq!(fold_neg(i64::MIN), i64::MIN);
    }
}
