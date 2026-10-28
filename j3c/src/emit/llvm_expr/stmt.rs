use super::*;

impl<'a> LlvmFn<'a> {
    pub(super) fn stmt(&mut self, stmt: &Stmt) -> Result<(), String> {
        if self.terminated {
            return Ok(());
        }
        self.note_span(stmt.span);
        match &stmt.kind {
            StmtKind::Assign { place, rvalue } => {
                let (ptr, _) = self.place_ptr(place)?;
                let (v, ty) = self.rvalue(rvalue)?;
                if matches!(self.cx.ty_kind(place.ty), TyKind::Array { .. }) {
                    let n = llvm_ty::size(self.cx, place.ty);
                    self.inst(&format!(
                        "call void @llvm.memcpy.p0.p0.i64(ptr {ptr}, ptr {v}, i64 {n}, i1 false)"
                    ));
                } else {
                    self.store(&ty, &v, &ptr)?;
                }
            }
            StmtKind::AssignOp { place, op, rhs } => {
                let (ptr, pty) = self.place_ptr(place)?;
                let ll = ll_type(self.cx, place.ty)?;
                let cur = self.load(&ll, &ptr);
                let (r, _) = self.operand(rhs)?;
                let out = self.binop(place.ty, place.ty, *op, &cur, &r)?;
                self.store(&ll, &out, &ptr)?;
                let _ = pty;
            }
            StmtKind::Expr(rvalue) => {
                let _ = self.rvalue(rvalue)?;
            }
            StmtKind::Return { value } => match value {
                Some(v) => {
                    let (e, ty) = self.operand(v)?;
                    let e = if matches!(self.cx.ty_kind(v.ty()), TyKind::Array { .. }) {
                        self.load(&ty, &e)
                    } else {
                        e
                    };
                    let slot = self.fresh();
                    self.inst(&format!("{slot} = alloca {ty}"));
                    self.store(&ty, &e, &slot)?;
                    self.emit_defers_through(false)?;
                    let loaded = self.load(&ty, &slot);
                    self.term(&format!("ret {ty} {loaded}"));
                }
                None => {
                    self.emit_defers_through(false)?;
                    self.term("ret void");
                }
            },
            StmtKind::Break => {
                self.emit_defers_through(true)?;
                let lab = self.loop_target(false)?;
                self.term(&format!("br label %{lab}"));
            }
            StmtKind::Continue => {
                self.emit_defers_through(true)?;
                let lab = self.loop_target(true)?;
                self.term(&format!("br label %{lab}"));
            }
            StmtKind::Abort(k) => {
                self.exit_abort(*k);
            }
            StmtKind::Div0 => {
                self.exit_abort(AbortKind::Div0);
            }
            StmtKind::Block(inner) => {
                self.emit_block(inner, false)?;
            }
            StmtKind::Defer { body } => {
                if let Some(frame) = self.frames.last_mut() {
                    frame.blocks.push(body.clone());
                }
            }
            StmtKind::If {
                cond,
                then_block,
                else_block,
            } => {
                let c = self.bool_val(cond)?;
                let t = self.label();
                let e = self.label();
                let j = self.label();
                match else_block {
                    Some(_) => self.term(&format!("br i1 {c}, label %{t}, label %{e}")),
                    None => self.term(&format!("br i1 {c}, label %{t}, label %{j}")),
                }
                self.block(&t);
                self.emit_block(then_block, false)?;
                if !self.terminated {
                    self.term(&format!("br label %{j}"));
                }
                if let Some(eb) = else_block {
                    self.block(&e);
                    self.emit_block(eb, false)?;
                    if !self.terminated {
                        self.term(&format!("br label %{j}"));
                    }
                }
                self.block(&j);
            }
            StmtKind::While { cond, body } => {
                let hdr = self.label();
                let bod = self.label();
                let end = self.label();
                self.block(&hdr);
                let c = self.bool_val(cond)?;
                self.term(&format!("br i1 {c}, label %{bod}, label %{end}"));
                self.block(&bod);
                self.emit_block_loop(body, hdr.clone(), end.clone())?;
                if !self.terminated {
                    self.term(&format!("br label %{hdr}"));
                }
                self.block(&end);
            }
            StmtKind::ForRange { index, end, body } => {
                let hdr = self.label();
                let bod = self.label();
                let inc = self.label();
                let fin = self.label();
                let (ip, _) = self.place_ptr(index)?;
                let (ev, _) = self.operand(end)?;
                self.block(&hdr);
                let iv = self.load("i64", &ip);
                let ok = self.fresh();
                self.inst(&format!("{ok} = icmp slt i64 {iv}, {ev}"));
                self.term(&format!("br i1 {ok}, label %{bod}, label %{fin}"));
                self.block(&bod);
                self.emit_block_loop(body, inc.clone(), fin.clone())?;
                if !self.terminated {
                    self.term(&format!("br label %{inc}"));
                }
                self.block(&inc);
                let cur = self.load("i64", &ip);
                let nxt = self.binop(index.ty, index.ty, BinOp::Add, &cur, "1")?;
                self.store("i64", &nxt, &ip)?;
                self.term(&format!("br label %{hdr}"));
                self.block(&fin);
            }
            StmtKind::InterpFill { buf, used, parts } => {
                self.interp_fill(buf, used, parts)?;
            }
        }
        Ok(())
    }

    pub(super) fn bool_val(&mut self, op: &Operand) -> Result<String, String> {
        let (v, ty) = self.operand(op)?;
        if ty == "i1" {
            return Ok(v);
        }
        let c = self.fresh();
        self.inst(&format!("{c} = icmp ne {ty} {v}, 0"));
        Ok(c)
    }

    pub(super) fn place_ptr(&mut self, place: &Place) -> Result<(String, Ty), String> {
        let (mut ptr, mut ty) = match place.root {
            PlaceRoot::Local(id) => {
                let ty = match self.body {
                    Some(body) => body.local(id).ty,
                    None => return Err("place needs a function body".to_string()),
                };
                (self.local_ptr(id), ty)
            }
            PlaceRoot::Global(id) => {
                let g = self.cx.global(id);
                (format!("@{}", c_expr::global_c_name(self.cx, g)), g.ty)
            }
        };
        for proj in &place.proj {
            match proj {
                Proj::Deref => {
                    ptr = self.load("ptr", &ptr);
                    ty = self
                        .cx
                        .ref_inner(ty)
                        .ok_or("deref of non-ref")?;
                }
                Proj::Field { field, .. } => {
                    let idx = field.index;
                    let sty = ll_type(self.cx, ty)?;
                    let n = self.fresh();
                    self.inst(&format!(
                        "{n} = getelementptr {sty}, ptr {ptr}, i32 0, i32 {idx}"
                    ));
                    ptr = n;
                    ty = self.cx.field_def(*field).ty;
                }
                Proj::Index { index, .. } => {
                    let (idx, _) = self.operand(index)?;
                    if llvm_ty::is_ptr(self.cx, ty) {
                        return Err("index of ptr".to_string());
                    }
                    if matches!(self.cx.ty_kind(ty), TyKind::Slice { .. } | TyKind::String) {
                        let base = self.fresh();
                        self.inst(&format!(
                            "{base} = getelementptr %j3_string, ptr {ptr}, i32 0, i32 0"
                        ));
                        let p = self.load("ptr", &base);
                        let elem = self.cx.elem_ty(ty).ok_or("index elem")?;
                        let et = ll_type(self.cx, elem)?;
                        let n = self.fresh();
                        self.inst(&format!(
                            "{n} = getelementptr {et}, ptr {p}, i64 {idx}"
                        ));
                        ptr = n;
                        ty = elem;
                    } else {
                        let elem = self.cx.elem_ty(ty).ok_or("index elem")?;
                        let at = ll_type(self.cx, ty)?;
                        let n = self.fresh();
                        self.inst(&format!(
                            "{n} = getelementptr {at}, ptr {ptr}, i64 0, i64 {idx}"
                        ));
                        ptr = n;
                        ty = elem;
                    }
                }
                Proj::Ptr => {
                    let n = self.fresh();
                    self.inst(&format!(
                        "{n} = getelementptr %j3_string, ptr {ptr}, i32 0, i32 0"
                    ));
                    ptr = n;
                    ty = self
                        .cx
                        .elem_ty(ty)
                        .and_then(|e| self.cx.lookup_ty(&TyKind::Ref(e)))
                        .ok_or("ptr of non-slice")?;
                }
                Proj::Len => {
                    let n = self.fresh();
                    self.inst(&format!(
                        "{n} = getelementptr %j3_string, ptr {ptr}, i32 0, i32 1"
                    ));
                    ptr = n;
                    ty = self.cx.common.int;
                }
                Proj::EnumTag => {
                    let n = self.fresh();
                    self.inst(&format!("{n} = getelementptr i8, ptr {ptr}, i64 0"));
                    ptr = n;
                    ty = self.cx.common.i32;
                }
                Proj::EnumPayload { ty: pty, .. } => {
                    let off = abi::field_offset(self.cx, ty, 1).unwrap_or(4);
                    let n = self.fresh();
                    self.inst(&format!("{n} = getelementptr i8, ptr {ptr}, i64 {off}"));
                    ptr = n;
                    ty = *pty;
                }
            }
        }
        Ok((ptr, ty))
    }
}
