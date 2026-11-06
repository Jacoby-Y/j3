use super::*;

impl<'a> LlvmFn<'a> {
    pub(super) fn binop(
        &mut self,
        result_ty: Ty,
        lhs_ty: Ty,
        op: BinOp,
        l: &str,
        r: &str,
    ) -> Result<String, String> {
        if matches!(self.cx.ty_kind(result_ty), TyKind::Vec(_)) {
            return self.vec_binop(result_ty, op, l, r);
        }
        let ll = ll_type(self.cx, result_ty)?;
        let v = self.fresh();
        if matches!(
            op,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
        ) {
            let (ipred, fpred) = match op {
                BinOp::Eq => ("eq", "oeq"),
                BinOp::Ne => ("ne", "une"),
                BinOp::Lt => ("slt", "olt"),
                BinOp::Le => ("sle", "ole"),
                BinOp::Gt => ("sgt", "ogt"),
                BinOp::Ge => ("sge", "oge"),
                _ => unreachable!(),
            };
            if llvm_ty::is_ptr(self.cx, lhs_ty) {
                self.inst(&format!("{v} = icmp {ipred} ptr {l}, {r}"));
            } else if matches!(self.cx.ty_kind(lhs_ty), TyKind::Float(_)) {
                let ft = ll_type(self.cx, lhs_ty)?;
                self.inst(&format!("{v} = fcmp {fpred} {ft} {l}, {r}"));
            } else {
                let it = ll_type(self.cx, lhs_ty)?;
                let pred = if matches!(self.cx.ty_kind(lhs_ty), TyKind::Int(i) if !i.is_signed()) {
                    match op {
                        BinOp::Lt => "ult",
                        BinOp::Le => "ule",
                        BinOp::Gt => "ugt",
                        BinOp::Ge => "uge",
                        other => {
                            let _ = other;
                            ipred
                        }
                    }
                } else {
                    ipred
                };
                self.inst(&format!("{v} = icmp {pred} {it} {l}, {r}"));
            }
            let z = self.fresh();
            self.inst(&format!("{z} = zext i1 {v} to i8"));
            return Ok(z);
        }
        if matches!(self.cx.ty_kind(result_ty), TyKind::Float(_)) {
            let inst = match op {
                BinOp::Add => "fadd",
                BinOp::Sub => "fsub",
                BinOp::Mul => "fmul",
                BinOp::Div => "fdiv",
                BinOp::Rem => "frem",
                _ => return Err("bad float op".to_string()),
            };
            self.inst(&format!("{v} = {inst} {ll} {l}, {r}"));
            return Ok(v);
        }
        if matches!(op, BinOp::And | BinOp::Or) {
            let inst = if op == BinOp::And { "and" } else { "or" };
            self.inst(&format!("{v} = {inst} i8 {l}, {r}"));
            return Ok(v);
        }
        if wrap::wrapping_bin(op) && c_ty::is_int(self.cx, result_ty) {
            return self.wrap_int(result_ty, op, l, r);
        }
        let inst = match op {
            BinOp::Add => "add",
            BinOp::Sub => "sub",
            BinOp::Mul => "mul",
            BinOp::BitAnd => "and",
            BinOp::BitOr => "or",
            BinOp::BitXor => "xor",
            BinOp::Shl => "shl",
            BinOp::Shr => "lshr",
            _ => return Err(format!("binop {}", op.as_str())),
        };
        self.inst(&format!("{v} = {inst} {ll} {l}, {r}"));
        Ok(v)
    }

    pub(super) fn wrap_int(&mut self, ty: Ty, op: BinOp, l: &str, r: &str) -> Result<String, String> {
        let ll = ll_type(self.cx, ty)?;
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Shl | BinOp::Shr => {
                let inst = match op {
                    BinOp::Add => "add",
                    BinOp::Sub => "sub",
                    BinOp::Mul => "mul",
                    BinOp::Shl => "shl",
                    BinOp::Shr => {
                        if matches!(self.cx.ty_kind(ty), TyKind::Int(i) if i.is_signed()) {
                            "ashr"
                        } else {
                            "lshr"
                        }
                    }
                    _ => unreachable!(),
                };
                // a shift by the width or more is poison in LLVM: mask the amount, like C and Rust release
                let masked;
                let r = if matches!(op, BinOp::Shl | BinOp::Shr) {
                    let bits = match self.cx.ty_kind(ty) {
                        TyKind::Int(i) => i.bits(),
                        _ => 64,
                    };
                    masked = self.fresh();
                    self.inst(&format!("{masked} = and {ll} {r}, {}", bits - 1));
                    masked.as_str()
                } else {
                    r
                };
                let v = self.fresh();
                self.inst(&format!("{v} = {inst} {ll} {l}, {r}"));
                Ok(v)
            }
            BinOp::Div | BinOp::Rem => {
                let signed = matches!(self.cx.ty_kind(ty), TyKind::Int(i) if i.is_signed());
                if signed {
                    let bits = match self.cx.ty_kind(ty) {
                        TyKind::Int(i) => i.bits(),
                        _ => 64,
                    };
                    let min = match bits {
                        8 => "-128",
                        16 => "-32768",
                        32 => "-2147483648",
                        _ => "-9223372036854775808",
                    };
                    let ismin = self.fresh();
                    let ism1 = self.fresh();
                    let ov = self.fresh();
                    self.inst(&format!("{ismin} = icmp eq {ll} {l}, {min}"));
                    self.inst(&format!("{ism1} = icmp eq {ll} {r}, -1"));
                    self.inst(&format!("{ov} = and i1 {ismin}, {ism1}"));
                    let ov_bb = self.label();
                    let norm_bb = self.label();
                    let join = self.label();
                    self.term(&format!("br i1 {ov}, label %{ov_bb}, label %{norm_bb}"));
                    self.block(&ov_bb);
                    let wrapped = if op == BinOp::Div { min } else { "0" };
                    self.term(&format!("br label %{join}"));
                    self.block(&norm_bb);
                    let norm = self.fresh();
                    if op == BinOp::Div {
                        self.inst(&format!("{norm} = sdiv {ll} {l}, {r}"));
                    } else {
                        self.inst(&format!("{norm} = srem {ll} {l}, {r}"));
                    }
                    self.term(&format!("br label %{join}"));
                    self.block(&join);
                    let v = self.fresh();
                    self.inst(&format!(
                        "{v} = phi {ll} [ {wrapped}, %{ov_bb} ], [ {norm}, %{norm_bb} ]"
                    ));
                    Ok(v)
                } else {
                    let inst = if op == BinOp::Div { "udiv" } else { "urem" };
                    let v = self.fresh();
                    self.inst(&format!("{v} = {inst} {ll} {l}, {r}"));
                    Ok(v)
                }
            }
            _ => Err("wrap op".to_string()),
        }
    }
}
