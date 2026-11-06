//! `f16` is storage only: it can be stored, passed, compared as a whole value by `=` assignment,
//! and converted with `f16(x)` / `f32(h)` / `f64(h)`. Arithmetic, comparisons, `-h`, and
//! interpolation are errors that say to convert first. Backends: `emit/llvm_expr/half.rs`,
//! `emit/c_expr/half.rs`; constants: `ir/half.rs`.

use super::*;

pub(super) const CONVERT: &str = "convert with `f32(x)` first, then convert back with `f16(y)`";

impl<'a> Lower<'a> {
    pub(super) fn is_half(&self, ty: Ty) -> bool {
        ty == self.cx.common.f16
    }

    pub(super) fn half_error(&self, span: Span, what: &str) -> CheckError {
        CheckError::new(self.path, span, format!("{what} is not defined on `f16` (storage only)")).help(CONVERT)
    }

    /// Reject a binary or unary op on `f16` operands; pass every other rvalue through.
    pub(super) fn half_guard(&self, span: Span, rv: Rvalue) -> Result<Rvalue, CheckError> {
        let bad = match &rv.kind {
            RvalueKind::Binary { op, lhs, rhs } if self.is_half(lhs.ty()) || self.is_half(rhs.ty()) => {
                Some(format!("`{}`", op.as_str()))
            }
            RvalueKind::Unary { arg, .. } if self.is_half(arg.ty()) => Some("unary `-`".to_string()),
            _ => None,
        };
        match bad {
            Some(what) => Err(self.half_error(span, &what)),
            None => Ok(rv),
        }
    }

    /// `f16(x)`, `f32(h)`, `f64(h)`, and `as`: only float and int sources convert to `f16`; an
    /// `f16` converts to `f32` / `f64` only.
    pub(super) fn half_cast(&self, span: Span, to: Ty, from: &Operand) -> Result<(), CheckError> {
        let ft = from.ty();
        if !self.is_half(to) && !self.is_half(ft) {
            return Ok(());
        }
        if self.script_sid.is_some() || self.path.ends_with(".j3s") {
            return Err(CheckError::new(self.path, span, "f16 is not supported in scripts yet")
                .help("scripts run on the VM; use `f32`, or move this code into a `.j3` module"));
        }
        let ok = if self.is_half(to) {
            ft == to || is_numeric(self.cx, ft)
        } else {
            is_float(self.cx, to)
        };
        if ok {
            return Ok(());
        }
        Err(CheckError::new(self.path, span, "this conversion is not defined for `f16`")
            .help("`f16(x)` takes a float or int; an `f16` converts only to `f32` or `f64`, so go through `f32(h)`"))
    }

    /// An untyped numeric literal stored into an `f16`, rounded at compile time.
    pub(super) fn half_const(&self, span: Span, v: f64) -> Operand {
        Operand::int(span, self.cx.common.f16, crate::ir::half::from_f64(v) as u64)
    }
}
