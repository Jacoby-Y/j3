//! `f16` in LLVM IR: held as `i16` bits everywhere; `half` appears only inside a conversion
//! (`fptrunc` / `fpext`, round to nearest even). Ints and `f64` go through `double` so there is a
//! single rounding, the same as the C backend's `j3_f16_from_f64` (`emit/c_expr/half.rs`).

use super::*;

impl<'a> LlvmFn<'a> {
    /// Cast text when `from` or `to` is `f16`; `None` when neither is.
    pub(super) fn half_cast(&mut self, from: Ty, to: Ty, v: &str, ft: &str) -> Result<Option<String>, String> {
        let h = self.cx.common.f16;
        if to == h && from != h {
            let mut src = v.to_string();
            let mut st = ft.to_string();
            if c_ty::is_int(self.cx, from) {
                let op = if matches!(self.cx.ty_kind(from), TyKind::Int(i) if i.is_signed()) { "sitofp" } else { "uitofp" };
                src = self.fresh();
                self.inst(&format!("{src} = {op} {ft} {v} to double"));
                st = "double".to_string();
            }
            let half = self.fresh();
            self.inst(&format!("{half} = fptrunc {st} {src} to half"));
            let bits = self.fresh();
            self.inst(&format!("{bits} = bitcast half {half} to i16"));
            return Ok(Some(bits));
        }
        if from == h && to != h {
            let half = self.fresh();
            self.inst(&format!("{half} = bitcast i16 {v} to half"));
            let tt = llvm_ty::ll_type(self.cx, to)?;
            let out = self.fresh();
            self.inst(&format!("{out} = fpext half {half} to {tt}"));
            return Ok(Some(out));
        }
        Ok(None)
    }
}
