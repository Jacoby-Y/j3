use super::*;

impl<'a> LlvmFn<'a> {
    pub fn operand_const(&mut self, op: &Operand) -> Result<String, String> {
        match op {
            Operand::Const(c) => self.const_bits(c.ty, &c.kind),
            Operand::Copy(_) => Err("const init needs a constant".to_string()),
        }
    }

    pub(super) fn operand(&mut self, op: &Operand) -> Result<(String, String), String> {
        match op {
            Operand::Copy(place) => {
                let (ptr, ty) = self.place_ptr(place)?;
                if matches!(self.cx.ty_kind(ty), TyKind::Array { .. }) {
                    return Ok((ptr, ll_type(self.cx, ty)?));
                }
                let ll = ll_type(self.cx, ty)?;
                Ok((self.load(&ll, &ptr), ll))
            }
            Operand::Const(c) => {
                let ll = ll_type(self.cx, c.ty)?;
                Ok((self.const_val(c.ty, &c.kind)?, ll))
            }
        }
    }

    pub(super) fn const_bits(&self, ty: Ty, kind: &ConstKind) -> Result<String, String> {
        match kind {
            ConstKind::Bool(true) => Ok("1".to_string()),
            ConstKind::Bool(false) => Ok("0".to_string()),
            ConstKind::Int(bits) => {
                if let TyKind::Int(int_ty) = self.cx.ty_kind(ty) {
                    if int_ty.is_signed() {
                        return Ok(format!("{}", *bits as i64));
                    }
                }
                Ok(format!("{bits}"))
            }
            ConstKind::F32Bits(b) => {
                // LLVM `0x` hex is IEEE *double* bits even for `float`.
                let f = f32::from_bits(*b);
                Ok(format!("0x{:016X}", (f as f64).to_bits()))
            }
            ConstKind::F64Bits(b) => Ok(format!("0x{b:016X}")),
            ConstKind::Null => Ok("null".to_string()),
            ConstKind::Void => Ok("undef".to_string()),
            ConstKind::FnAddr(_) | ConstKind::Bytes(_) => {
                Err("const init of fn/bytes not in global scalar".to_string())
            }
        }
    }

    pub(super) fn const_val(&mut self, ty: Ty, kind: &ConstKind) -> Result<String, String> {
        match kind {
            ConstKind::FnAddr(id) => {
                let func = self.cx.func(*id);
                let name = match &func.kind {
                    FuncKind::Lib { symbol, .. } => c_ty::sanitize(self.cx.name_str(*symbol)),
                    _ => c_expr::func_c_name(self.cx, func),
                };
                Ok(format!("@{name}"))
            }
            ConstKind::Bytes(bytes) => {
                let lit = self.bytes_global(bytes);
                let s = self.fresh();
                let t = self.fresh();
                self.inst(&format!(
                    "{s} = insertvalue %j3_string poison, ptr {lit}, 0"
                ));
                self.inst(&format!(
                    "{t} = insertvalue %j3_string {s}, i64 {}, 1",
                    bytes.len()
                ));
                let _ = ty;
                Ok(t)
            }
            other => self.const_bits(ty, other),
        }
    }

    pub(super) fn bytes_global(&mut self, bytes: &[u8]) -> String {
        let n = STR_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name = format!("@.b{n}");
        let lit = ll_bytes_lit(bytes);
        // Module-level constants are prepended by rewriting: emit as a commentless
        // function-local via alloca+store for tiny lits, or a private global appended
        // at the top by collecting. We inject into `text` at the start via a marker.
        self.globals.push_str(&format!(
            "{name} = private unnamed_addr constant [{} x i8] {lit}\n",
            bytes.len()
        ));
        let p = self.fresh();
        self.inst(&format!(
            "{p} = getelementptr [{} x i8], ptr {name}, i64 0, i64 0",
            bytes.len()
        ));
        p
    }

    pub(super) fn rvalue(&mut self, rv: &Rvalue) -> Result<(String, String), String> {
        let ll = ll_type(self.cx, rv.ty)?;
        match &rv.kind {
            RvalueKind::Use(op) => self.operand(op),
            RvalueKind::Ref { place } => {
                let (ptr, _) = self.place_ptr(place)?;
                Ok((ptr, "ptr".to_string()))
            }
            RvalueKind::Binary { op, lhs, rhs } => {
                let (l, _) = self.operand(lhs)?;
                let (r, _) = self.operand(rhs)?;
                let v = self.binop(rv.ty, lhs.ty(), *op, &l, &r)?;
                Ok((v, ll))
            }
            RvalueKind::Unary { op, arg } => {
                let (a, at) = self.operand(arg)?;
                let v = match op {
                    UnaryOp::Not => {
                        let c = self.fresh();
                        self.inst(&format!("{c} = icmp eq {at} {a}, 0"));
                        let z = self.fresh();
                        self.inst(&format!("{z} = zext i1 {c} to i8"));
                        z
                    }
                    UnaryOp::Neg if crate::emit::c_ty::is_int(self.cx, rv.ty) => {
                        self.binop(rv.ty, rv.ty, BinOp::Sub, "0", &a)?
                    }
                    UnaryOp::Neg => {
                        let v = self.fresh();
                        self.inst(&format!("{v} = fneg {ll} {a}"));
                        v
                    }
                    UnaryOp::BitNot => {
                        let v = self.fresh();
                        self.inst(&format!("{v} = xor {ll} {a}, -1"));
                        v
                    }
                };
                Ok((v, ll))
            }
            RvalueKind::Call { func, args } => {
                let (f, _) = self.operand(func)?;
                let callee = match func {
                    Operand::Const(c) => match &c.kind {
                        ConstKind::FnAddr(id) => crate::emit::llvm::variadic_call_ty(self.cx, self.cx.func(*id)),
                        _ => None,
                    },
                    _ => None,
                };
                let mut alist = String::new();
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        alist.push_str(", ");
                    }
                    let (v, t) = self.operand(a)?;
                    let v = if matches!(self.cx.ty_kind(a.ty()), TyKind::Array { .. }) {
                        self.load(&t, &v)
                    } else {
                        v
                    };
                    alist.push_str(&format!("{t} {v}"));
                }
                if rv.ty == self.cx.common.void {
                    self.inst(&format!("call void {f}({alist})"));
                    Ok(("undef".to_string(), "void".to_string()))
                } else {
                    let v = self.fresh();
                    let cty = callee.unwrap_or_else(|| ll.clone());
                    self.inst(&format!("{v} = call {cty} {f}({alist})"));
                    if matches!(self.cx.ty_kind(rv.ty), TyKind::Array { .. }) {
                        // Array rvalues use storage addresses inside a function.
                        let slot = self.fresh();
                        self.inst(&format!("{slot} = alloca {ll}"));
                        self.store(&ll, &v, &slot)?;
                        Ok((slot, ll))
                    } else {
                        Ok((v, ll))
                    }
                }
            }
            RvalueKind::Syscall { nr, args } => {
                let (n, _) = self.operand(nr)?;
                let mut filled = Vec::new();
                for a in args {
                    filled.push(self.syscall_word(a)?);
                }
                while filled.len() < 6 {
                    filled.push("0".to_string());
                }
                let v = self.fresh();
                self.inst(&format!(
                    "{v} = call i64 (i64, ...) @syscall(i64 {n}, i64 {}, i64 {}, i64 {}, i64 {}, i64 {}, i64 {})",
                    filled[0], filled[1], filled[2], filled[3], filled[4], filled[5]
                ));
                Ok((v, "i64".to_string()))
            }
            RvalueKind::Atomic { op, args } => {
                let (p, _) = self.operand(&args[0])?;
                let mut vals = Vec::new();
                for a in &args[1..] {
                    vals.push(self.syscall_word(a)?);
                }
                use crate::ir::op::AtomicOp;
                match op {
                    AtomicOp::Load => {
                        let v = self.fresh();
                        self.inst(&format!("{v} = load atomic i64, ptr {p} seq_cst, align 8"));
                        Ok((v, "i64".to_string()))
                    }
                    AtomicOp::Store => {
                        self.inst(&format!("store atomic i64 {}, ptr {p} seq_cst, align 8", vals[0]));
                        Ok(("0".to_string(), "i64".to_string()))
                    }
                    AtomicOp::Add => {
                        let v = self.fresh();
                        self.inst(&format!("{v} = atomicrmw add ptr {p}, i64 {} seq_cst", vals[0]));
                        Ok((v, "i64".to_string()))
                    }
                    AtomicOp::Cas => {
                        let r = self.fresh();
                        self.inst(&format!(
                            "{r} = cmpxchg ptr {p}, i64 {}, i64 {} seq_cst seq_cst",
                            vals[0], vals[1]
                        ));
                        let ok = self.fresh();
                        self.inst(&format!("{ok} = extractvalue {{ i64, i1 }} {r}, 1"));
                        let z = self.fresh();
                        self.inst(&format!("{z} = zext i1 {ok} to i8"));
                        Ok((z, "i8".to_string()))
                    }
                }
            }
            RvalueKind::Aggregate { fields } => self.aggregate(rv.ty, fields),
            RvalueKind::Enum { variant, payload } => {
                if self.cx.enum_def(variant.enum_id).is_c_like() {
                    return Ok((format!("{}", variant.index), "i32".to_string()));
                }
                let slot = self.fresh();
                self.inst(&format!("{slot} = alloca {ll}"));
                self.inst(&format!("store i32 {}, ptr {slot}", variant.index));
                if let Some(p) = payload {
                    let (v, vt) = self.operand(p)?;
                    let off = abi::field_offset(self.cx, rv.ty, 1).unwrap_or(4);
                    let pp = self.fresh();
                    self.inst(&format!("{pp} = getelementptr i8, ptr {slot}, i64 {off}"));
                    self.store(&vt, &v, &pp)?;
                }
                Ok((self.load(&ll, &slot), ll))
            }
            RvalueKind::Cast { from } => {
                let (v, ft) = self.operand(from)?;
                Ok((self.cast(from.ty(), rv.ty, &v, &ft, &ll)?, ll))
            }
            RvalueKind::Interpolate { .. } => {
                Err("interpolate must be InterpFill before LLVM emit".to_string())
            }
        }
    }

    pub(super) fn syscall_word(&mut self, op: &Operand) -> Result<String, String> {
        let (v, ty) = self.operand(op)?;
        if ty == "ptr" {
            let n = self.fresh();
            self.inst(&format!("{n} = ptrtoint ptr {v} to i64"));
            Ok(n)
        } else if ty == "i64" {
            Ok(v)
        } else {
            let n = self.fresh();
            self.inst(&format!("{n} = sext {ty} {v} to i64"));
            Ok(n)
        }
    }

    pub(super) fn cast(&mut self, from: Ty, to: Ty, v: &str, ft: &str, tt: &str) -> Result<String, String> {
        if ft == tt {
            return Ok(v.to_string());
        }
        let n = self.fresh();
        if llvm_ty::is_ptr(self.cx, from) && llvm_ty::is_ptr(self.cx, to) {
            return Ok(v.to_string());
        }
        if llvm_ty::is_ptr(self.cx, from) && tt == "i64" {
            self.inst(&format!("{n} = ptrtoint ptr {v} to i64"));
            return Ok(n);
        }
        if ft == "i64" && llvm_ty::is_ptr(self.cx, to) {
            self.inst(&format!("{n} = inttoptr i64 {v} to ptr"));
            return Ok(n);
        }
        if c_ty::is_int(self.cx, from) && c_ty::is_int(self.cx, to) {
            let fb = match self.cx.ty_kind(from) {
                TyKind::Int(i) => i.bits(),
                _ => 64,
            };
            let tb = match self.cx.ty_kind(to) {
                TyKind::Int(i) => i.bits(),
                _ => 64,
            };
            if tb > fb {
                let op = if matches!(self.cx.ty_kind(from), TyKind::Int(i) if i.is_signed()) {
                    "sext"
                } else {
                    "zext"
                };
                self.inst(&format!("{n} = {op} {ft} {v} to {tt}"));
            } else {
                self.inst(&format!("{n} = trunc {ft} {v} to {tt}"));
            }
            return Ok(n);
        }
        if c_ty::is_int(self.cx, from) && matches!(self.cx.ty_kind(to), TyKind::Float(_)) {
            let op = if matches!(self.cx.ty_kind(from), TyKind::Int(i) if i.is_signed()) {
                "sitofp"
            } else {
                "uitofp"
            };
            self.inst(&format!("{n} = {op} {ft} {v} to {tt}"));
            return Ok(n);
        }
        if matches!(self.cx.ty_kind(from), TyKind::Float(_)) && c_ty::is_int(self.cx, to) {
            let op = if matches!(self.cx.ty_kind(to), TyKind::Int(i) if i.is_signed()) {
                "fptosi"
            } else {
                "fptoui"
            };
            self.inst(&format!("{n} = {op} {ft} {v} to {tt}"));
            return Ok(n);
        }
        if matches!(self.cx.ty_kind(from), TyKind::Float(_))
            && matches!(self.cx.ty_kind(to), TyKind::Float(_))
        {
            let op = if tt == "double" { "fpext" } else { "fptrunc" };
            self.inst(&format!("{n} = {op} {ft} {v} to {tt}"));
            return Ok(n);
        }
        if from == self.cx.common.string || matches!(self.cx.ty_kind(from), TyKind::Slice { .. }) {
            if to == self.cx.common.string || matches!(self.cx.ty_kind(to), TyKind::Slice { .. }) {
                return Ok(v.to_string());
            }
        }
        Err(format!("cast {ft} -> {tt} not in LLVM emit"))
    }

    pub(super) fn aggregate(&mut self, ty: Ty, fields: &[Operand]) -> Result<(String, String), String> {
        let ll = ll_type(self.cx, ty)?;
        match self.cx.ty_kind(ty) {
            TyKind::Slice { .. } | TyKind::String => {
                let (p, _) = self.operand(&fields[0])?;
                let (n, _) = self.operand(&fields[1])?;
                let a = self.fresh();
                let b = self.fresh();
                self.inst(&format!(
                    "{a} = insertvalue %j3_string poison, ptr {p}, 0"
                ));
                self.inst(&format!("{b} = insertvalue %j3_string {a}, i64 {n}, 1"));
                Ok((b, "%j3_string".to_string()))
            }
            TyKind::Struct(_) | TyKind::Array { .. } => {
                let slot = self.fresh();
                self.inst(&format!("{slot} = alloca {ll}"));
                for (i, f) in fields.iter().enumerate() {
                    let (v, vt) = self.operand(f)?;
                    let p = self.fresh();
                    self.inst(&format!(
                        "{p} = getelementptr {ll}, ptr {slot}, i32 0, i32 {i}"
                    ));
                    if matches!(self.cx.ty_kind(f.ty()), TyKind::Array { .. }) {
                        let n = llvm_ty::size(self.cx, f.ty());
                        self.inst(&format!(
                            "call void @llvm.memcpy.p0.p0.i64(ptr {p}, ptr {v}, i64 {n}, i1 false)"
                        ));
                    } else {
                        self.store(&vt, &v, &p)?;
                    }
                }
                if matches!(self.cx.ty_kind(ty), TyKind::Array { .. }) {
                    Ok((slot, ll))
                } else {
                    Ok((self.load(&ll, &slot), ll))
                }
            }
            _ => Err("aggregate of this type".to_string()),
        }
    }
}
