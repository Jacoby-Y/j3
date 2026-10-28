use super::*;

impl<'a> LlvmFn<'a> {
    pub(super) fn interp_fill(
        &mut self,
        buf: &Place,
        used: &Place,
        parts: &[InterpPart],
    ) -> Result<(), String> {
        let (bp, _) = self.place_ptr(buf)?;
        let (up, _) = self.place_ptr(used)?;
        let cap = wrap::INTERP_CAP;
        for part in parts {
            match part {
                InterpPart::Lit(bytes) => {
                    self.interp_memcpy(&bp, &up, bytes, cap)?;
                }
                InterpPart::Value(op) => {
                    let (e, et) = self.operand(op)?;
                    self.interp_value(&bp, &up, &e, op.ty(), &et, cap)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn interp_memcpy(
        &mut self,
        buf: &str,
        used: &str,
        bytes: &[u8],
        cap: u64,
    ) -> Result<(), String> {
        let n = bytes.len() as u64;
        let u = self.load("i64", used);
        let sum = self.fresh();
        self.inst(&format!("{sum} = add i64 {u}, {n}"));
        let ok = self.fresh();
        self.inst(&format!("{ok} = icmp sle i64 {sum}, {cap}"));
        let t = self.label();
        let e = self.label();
        self.term(&format!("br i1 {ok}, label %{t}, label %{e}"));
        self.block(&e);
        self.exit_abort(AbortKind::Fmt);
        self.block(&t);
        let dst = self.fresh();
        self.inst(&format!("{dst} = getelementptr i8, ptr {buf}, i64 {u}"));
        if !bytes.is_empty() {
            let src = self.bytes_global(bytes);
            self.inst(&format!(
                "call void @llvm.memcpy.p0.p0.i64(ptr {dst}, ptr {src}, i64 {n}, i1 false)"
            ));
        }
        self.store("i64", &sum, used)?;
        Ok(())
    }

    pub(super) fn interp_value(
        &mut self,
        buf: &str,
        used: &str,
        e: &str,
        ty: Ty,
        et: &str,
        cap: u64,
    ) -> Result<(), String> {
        if ty == self.cx.common.string {
            let p = self.fresh();
            let n = self.fresh();
            self.inst(&format!("{p} = extractvalue %j3_string {e}, 0"));
            self.inst(&format!("{n} = extractvalue %j3_string {e}, 1"));
            let u = self.load("i64", used);
            let sum = self.fresh();
            self.inst(&format!("{sum} = add i64 {u}, {n}"));
            let ok = self.fresh();
            self.inst(&format!("{ok} = icmp sle i64 {sum}, {cap}"));
            let t = self.label();
            let bad = self.label();
            self.term(&format!("br i1 {ok}, label %{t}, label %{bad}"));
            self.block(&bad);
            self.exit_abort(AbortKind::Fmt);
            self.block(&t);
            let dst = self.fresh();
            self.inst(&format!("{dst} = getelementptr i8, ptr {buf}, i64 {u}"));
            self.inst(&format!(
                "call void @llvm.memcpy.p0.p0.i64(ptr {dst}, ptr {p}, i64 {n}, i1 false)"
            ));
            self.store("i64", &sum, used)?;
            return Ok(());
        }
        if ty == self.cx.common.bool {
            let c = self.fresh();
            self.inst(&format!("{c} = icmp ne i8 {e}, 0"));
            let p = self.fresh();
            self.inst(&format!(
                "{p} = select i1 {c}, ptr @.j3_true, ptr @.j3_false"
            ));
            return self.interp_snprintf(buf, used, "@.j3_fmt_s", "ptr", &p, cap);
        }
        if c_ty::is_int(self.cx, ty) {
            let mut val = e.to_string();
            if et != "i64" {
                let n = self.fresh();
                let signed = matches!(self.cx.ty_kind(ty), TyKind::Int(IntTy::I8 | IntTy::I16 | IntTy::I32 | IntTy::I64));
                let op = if signed { "sext" } else { "zext" };
                if et == "i64" {
                    val = e.to_string();
                } else {
                    self.inst(&format!("{n} = {op} {et} {e} to i64"));
                    val = n;
                }
            }
            return self.interp_snprintf(buf, used, "@.j3_fmt_i64", "i64", &val, cap);
        }
        if matches!(self.cx.ty_kind(ty), TyKind::Float(_)) {
            let mut val = e.to_string();
            if et != "double" {
                let n = self.fresh();
                self.inst(&format!("{n} = fpext {et} {e} to double"));
                val = n;
            }
            return self.interp_snprintf(buf, used, "@.j3_fmt_f", "double", &val, cap);
        }
        if let TyKind::Struct(id) = self.cx.ty_kind(ty) {
            let def = self.cx.struct_def(*id);
            if !def.debug {
                return Err("interpolate of non-#[Debug] struct".to_string());
            }
            let name = self.cx.name_str(def.name).to_string();
            self.interp_memcpy(buf, used, format!("{name} {{ ").as_bytes(), cap)?;
            let fields = def.fields.clone();
            for (i, field) in fields.iter().enumerate() {
                if i > 0 {
                    self.interp_memcpy(buf, used, b", ", cap)?;
                }
                let fname = self.cx.name_str(field.name).to_string();
                self.interp_memcpy(buf, used, format!("{fname}: ").as_bytes(), cap)?;
                let slot = self.fresh();
                let ll = ll_type(self.cx, ty)?;
                self.inst(&format!("{slot} = alloca {ll}"));
                self.store(&ll, e, &slot)?;
                let fp = self.fresh();
                self.inst(&format!(
                    "{fp} = getelementptr {ll}, ptr {slot}, i32 0, i32 {i}"
                ));
                let ft = ll_type(self.cx, field.ty)?;
                let fv = self.load(&ft, &fp);
                self.interp_value(buf, used, &fv, field.ty, &ft, cap)?;
            }
            self.interp_memcpy(buf, used, b" }", cap)?;
            return Ok(());
        }
        Err("interpolate of this type is not implemented".to_string())
    }

    pub(super) fn interp_snprintf(
        &mut self,
        buf: &str,
        used: &str,
        fmt: &str,
        vty: &str,
        val: &str,
        cap: u64,
    ) -> Result<(), String> {
        let u = self.load("i64", used);
        let room = self.fresh();
        self.inst(&format!("{room} = sub i64 {cap}, {u}"));
        let ok = self.fresh();
        self.inst(&format!("{ok} = icmp sgt i64 {room}, 0"));
        let t = self.label();
        let bad = self.label();
        self.term(&format!("br i1 {ok}, label %{t}, label %{bad}"));
        self.block(&bad);
        self.exit_abort(AbortKind::Fmt);
        self.block(&t);
        let dst = self.fresh();
        self.inst(&format!("{dst} = getelementptr i8, ptr {buf}, i64 {u}"));
        let w = self.fresh();
        self.inst(&format!(
            "{w} = call i32 (ptr, i64, ptr, ...) @snprintf(ptr {dst}, i64 {room}, ptr {fmt}, {vty} {val})"
        ));
        let wi = self.fresh();
        self.inst(&format!("{wi} = sext i32 {w} to i64"));
        let badw = self.fresh();
        self.inst(&format!("{badw} = icmp slt i64 {wi}, 0"));
        let ge = self.fresh();
        self.inst(&format!("{ge} = icmp sge i64 {wi}, {room}"));
        let fail = self.fresh();
        self.inst(&format!("{fail} = or i1 {badw}, {ge}"));
        let ok2 = self.label();
        let bad2 = self.label();
        self.term(&format!("br i1 {fail}, label %{bad2}, label %{ok2}"));
        self.block(&bad2);
        self.exit_abort(AbortKind::Fmt);
        self.block(&ok2);
        let sum = self.fresh();
        self.inst(&format!("{sum} = add i64 {u}, {wi}"));
        self.store("i64", &sum, used)?;
        Ok(())
    }
}
