use super::*;

impl<'a> CWriter<'a> {
    pub fn stmt(&mut self, stmt: &Stmt, indent: usize) -> Result<String, String> {
        let pad = "    ".repeat(indent);
        let dir = self.line_dir(stmt.span, indent);
        let body = match &stmt.kind {
            StmtKind::Assign { place, rvalue } => {
                if let RvalueKind::Aggregate { fields } = &rvalue.kind {
                    if let TyKind::Struct(id) = self.cx.ty_kind(rvalue.ty) {
                        let def = self.cx.struct_def(*id);
                        if def.fields.len() != fields.len() {
                            return Err("aggregate field count != struct".to_string());
                        }
                        if def
                            .fields
                            .iter()
                            .any(|f| c_ty::array_parts(self.cx, f.ty).is_some())
                        {
                            let p = self.place(place)?;
                            let ty_name = c_type(self.cx, rvalue.ty)?;
                            let mut parts = Vec::new();
                            for (i, field) in def.fields.iter().enumerate() {
                                if c_ty::array_parts(self.cx, field.ty).is_some() {
                                    continue;
                                }
                                let fname = c_ty::sanitize(self.cx.name_str(field.name));
                                let v = self.operand(&fields[i])?;
                                parts.push(format!(".{fname} = {v}"));
                            }
                            let mut s = String::new();
                            if parts.is_empty() {
                                s.push_str(&format!("{pad}{p} = ({ty_name}){{0}};\n"));
                            } else {
                                s.push_str(&format!(
                                    "{pad}{p} = ({ty_name}){{ {} }};\n",
                                    parts.join(", ")
                                ));
                            }
                            for (i, field) in def.fields.iter().enumerate() {
                                if c_ty::array_parts(self.cx, field.ty).is_none() {
                                    continue;
                                }
                                let fname = c_ty::sanitize(self.cx.name_str(field.name));
                                let src = self.operand(&fields[i])?;
                                let dest = format!("({p}).{fname}");
                                s.push_str(&format!(
                                    "{pad}memcpy({dest}, {src}, sizeof({dest}));\n"
                                ));
                            }
                            Ok::<String, String>(s)
                        } else {
                            let p = self.place(place)?;
                            let v = self.rvalue(rvalue)?;
                            let line = if c_ty::array_parts(self.cx, place.ty).is_some() {
                                format!("{pad}memcpy({p}, {v}, sizeof({p}));\n")
                            } else {
                                format!("{pad}{p} = {v};\n")
                            };
                            Ok(line)
                        }
                    } else {
                        let p = self.place(place)?;
                        let v = self.rvalue(rvalue)?;
                        let line = if c_ty::array_parts(self.cx, place.ty).is_some() {
                            format!("{pad}memcpy({p}, {v}, sizeof({p}));\n")
                        } else {
                            format!("{pad}{p} = {v};\n")
                        };
                        Ok(line)
                    }
                } else {
                    let p = self.place(place)?;
                    let v = self.rvalue(rvalue)?;
                    let line = if c_ty::array_parts(self.cx, place.ty).is_some() {
                        format!("{pad}memcpy({p}, {v}, sizeof({p}));\n")
                    } else {
                        format!("{pad}{p} = {v};\n")
                    };
                    Ok(line)
                }
            }
            StmtKind::AssignOp { place, op, rhs } => {
                let p = self.place(place)?;
                let r = self.operand(rhs)?;
                let op_s = match op.assign_op_str() {
                    Some(s) => s,
                    None => return Err("binary op cannot be assign-op in C".to_string()),
                };
                if c_ty::is_int(self.cx, place.ty) && crate::ir::wrap::wrapping_bin(*op) {
                    let tmp = self.fresh("j3_ap_");
                    let ty = c_type(self.cx, place.ty)?;
                    let wrap = wrap_int(self.cx, place.ty, *op, &format!("(*{tmp})"), &r)?;
                    let mut s = String::new();
                    s.push_str(&format!("{pad}{{\n"));
                    s.push_str(&format!("{pad}    {ty} *{tmp} = &({p});\n"));
                    s.push_str(&format!("{pad}    *{tmp} = {wrap};\n"));
                    s.push_str(&format!("{pad}}}\n"));
                    Ok(s)
                } else if *op == BinOp::Rem && !c_ty::is_int(self.cx, place.ty) {
                    // Float `%=` is `fmod`, like `%`.
                    let f = if self.cx.ty_kind(place.ty) == &TyKind::Float(FloatTy::F32) { "__builtin_fmodf" } else { "__builtin_fmod" };
                    let tmp = self.fresh("j3_ap_");
                    let ty = c_type(self.cx, place.ty)?;
                    Ok(format!("{pad}{{\n{pad}    {ty} *{tmp} = &({p});\n{pad}    *{tmp} = {f}(*{tmp}, {r});\n{pad}}}\n"))
                } else {
                    Ok(format!("{pad}{p} {op_s} {r};\n"))
                }
            }
            StmtKind::Expr(rvalue) => {
                let v = self.rvalue(rvalue)?;
                Ok(format!("{pad}(void)({v});\n"))
            }
            StmtKind::Return { value } => {
                match value {
                    Some(v) => {
                        let e = self.operand(v)?;
                        let ty = c_type(self.cx, v.ty())?;
                        let tmp = self.fresh("j3_ret_");
                        let mut s = format!("{pad}{ty} {tmp} = {e};\n");
                        s.push_str(&self.emit_defers_through(indent, None)?);
                        s.push_str(&format!("{pad}return {tmp};\n"));
                        Ok(s)
                    }
                    None => {
                        let mut s = self.emit_defers_through(indent, None)?;
                        s.push_str(&format!("{pad}return;\n"));
                        Ok(s)
                    }
                }
            }
            StmtKind::Break(n) | StmtKind::Continue(n) => {
                let cont = matches!(stmt.kind, StmtKind::Continue(_));
                let mut s = self.emit_defers_to_loop(indent, *n)?;
                match (self.jump_goto(*n, cont), cont) {
                    (Some(id), false) => s.push_str(&format!("{pad}goto j3_brk_{id};\n")),
                    (Some(id), true) => s.push_str(&format!("{pad}goto j3_cont_{id};\n")),
                    (None, false) => s.push_str(&format!("{pad}break;\n")),
                    (None, true) => s.push_str(&format!("{pad}continue;\n")),
                }
                Ok(s)
            }
            StmtKind::Abort(k, vals) => {
                let [da, db] = k.detail();
                if let ([a, b], false) = (vals.as_slice(), da.is_empty()) {
                    let (a, b) = (self.operand(a)?, self.operand(b)?);
                    return Ok(format!(
                        "{pad}J3_FAILV(\"{}{da}%lld{db}%lld\", (long long)({a}), (long long)({b}));\n",
                        k.message()
                    ));
                }
                Ok(format!("{pad}J3_FAIL(\"{}\");\n", k.message()))
            }
            StmtKind::Div0 => Ok(format!("{pad}J3_FAIL(\"{}\");\n", crate::ir::stmt::AbortKind::Div0.message())),
            StmtKind::Block(inner) => {
                let mut s = format!("{pad}{{\n");
                s.push_str(&self.emit_block(inner, false, indent + 1)?);
                s.push_str(&pad);
                s.push_str("}\n");
                Ok(s)
            }
            StmtKind::If {
                cond,
                then_block,
                else_block,
            } => {
                let c = self.operand(cond)?;
                let mut s = format!("{pad}if ({c}) {{\n");
                s.push_str(&self.emit_block(then_block, false, indent + 1)?);
                s.push_str(&pad);
                s.push('}');
                if let Some(else_block) = else_block {
                    s.push_str(" else {\n");
                    s.push_str(&self.emit_block(else_block, false, indent + 1)?);
                    s.push_str(&pad);
                    s.push('}');
                }
                s.push('\n');
                Ok(s)
            }
            StmtKind::While { cond, body } => {
                let c = self.operand(cond)?;
                let id = self.next_loop;
                let mut s = format!("{pad}while ({c}) {{\n");
                s.push_str(&self.emit_block(body, true, indent + 1)?);
                s.push_str(&pad);
                s.push_str("}\n");
                s.push_str(&self.brk_label(id, &pad));
                Ok(s)
            }
            StmtKind::Switch { scrut, cases, default } => {
                let v = self.operand(scrut)?;
                let mut s = format!("{pad}switch ({v}) {{\n");
                self.frames.push(DeferFrame { blocks: Vec::new(), is_loop: false, is_switch: true, loop_id: 0 });
                for (k, b) in cases {
                    let k = self.const_val(scrut.ty(), &ConstKind::Int(*k))?;
                    s.push_str(&format!("{pad}case {k}: {{\n"));
                    s.push_str(&self.emit_block(b, false, indent + 1)?);
                    s.push_str(&format!("{pad}    break;\n{pad}}}\n"));
                }
                s.push_str(&format!("{pad}default: {{\n"));
                s.push_str(&self.emit_block(default, false, indent + 1)?);
                s.push_str(&format!("{pad}    break;\n{pad}}}\n{pad}}}\n"));
                self.frames.pop();
                Ok(s)
            }
            StmtKind::Defer { body } => {
                if let Some(frame) = self.frames.last_mut() {
                    frame.blocks.push(body.clone());
                }
                Ok(String::new())
            }
            StmtKind::ForRange { index, end, body } => {
                let p = self.place(index)?;
                let e = self.operand(end)?;
                let step = wrap_int(self.cx, index.ty, BinOp::Add, &p, "1ll")?;
                let id = self.next_loop;
                let mut s = format!("{pad}for (; {p} < {e}; {p} = {step}) {{\n");
                s.push_str(&self.emit_block(body, true, indent + 1)?);
                s.push_str(&pad);
                s.push_str("}\n");
                s.push_str(&self.brk_label(id, &pad));
                Ok(s)
            }
            StmtKind::InterpFill { buf, used, parts } => {
                let buf_s = self.place(buf)?;
                let used_s = self.place(used)?;
                let cap = crate::ir::wrap::INTERP_CAP;
                let mut s = String::new();
                for part in parts {
                    match part {
                        crate::ir::rvalue::InterpPart::Lit(bytes) => {
                            let lit = c_bytes_literal(bytes);
                            s.push_str(&format!(
                                "{pad}if (({used_s}) + {len}ll > {cap}ll) {{ J3_FAIL(\"string interpolation overflowed its buffer\"); }}\n\
                                 {pad}memcpy(({buf_s}) + ({used_s}), {lit}, {len}ull);\n\
                                 {pad}{used_s} = ({used_s}) + {len}ll;\n",
                                len = bytes.len()
                            ));
                        }
                        crate::ir::rvalue::InterpPart::Value(op) => {
                            let e = self.operand(op)?;
                            s.push_str(&self.interp_append(&pad, &buf_s, &used_s, &e, op.ty())?);
                        }
                    }
                }
                Ok(s)
            }
        }?;
        Ok(format!("{dir}{body}"))
    }

    pub fn emit_block(
        &mut self,
        block: &crate::ir::body::Block,
        is_loop: bool,
        indent: usize,
    ) -> Result<String, String> {
        let loop_id = self.next_loop;
        if is_loop {
            self.next_loop += 1;
        }
        self.frames.push(DeferFrame {
            blocks: Vec::new(),
            is_loop,
            is_switch: false,
            loop_id,
        });
        let mut s = String::new();
        for inner in &block.stmts {
            s.push_str(&self.stmt(inner, indent)?);
        }
        s.push_str(&self.flush_current(indent)?);
        self.frames.pop();
        if is_loop && self.cont_used.contains(&loop_id) {
            s.push_str(&format!("{}j3_cont_{loop_id}: ;\n", "    ".repeat(indent)));
        }
        Ok(s)
    }

    /// `Some(loop id)` when C's own `break`/`continue` would miss: the target loop is `depth`
    /// levels out, or (for `break`) a `switch` lies between here and it.
    fn jump_goto(&mut self, depth: u32, cont: bool) -> Option<u32> {
        let mut crossed = false;
        let mut loops = 0;
        for frame in self.frames.iter().rev() {
            if frame.is_loop {
                if loops == depth {
                    if depth == 0 && (cont || !crossed) {
                        return None;
                    }
                    let set = if cont { &mut self.cont_used } else { &mut self.brk_used };
                    set.insert(frame.loop_id);
                    return Some(frame.loop_id);
                }
                loops += 1;
            }
            crossed |= frame.is_switch;
        }
        None
    }

    fn brk_label(&self, id: u32, pad: &str) -> String {
        if self.brk_used.contains(&id) {
            format!("{pad}j3_brk_{id}: ;\n")
        } else {
            String::new()
        }
    }

    /// Defers of every frame out to (and including) the loop `depth` levels out.
    pub(super) fn emit_defers_to_loop(&mut self, indent: usize, depth: u32) -> Result<String, String> {
        let mut blocks = Vec::new();
        let mut loops = 0;
        for frame in self.frames.iter().rev() {
            for b in frame.blocks.iter().rev() {
                blocks.push(b.clone());
            }
            if frame.is_loop {
                if loops == depth {
                    break;
                }
                loops += 1;
            }
        }
        self.write_defer_blocks(&blocks, indent)
    }

    pub(super) fn flush_current(&mut self, indent: usize) -> Result<String, String> {
        let blocks = match self.frames.last_mut() {
            Some(f) => f.blocks.drain(..).rev().collect::<Vec<_>>(),
            None => return Ok(String::new()),
        };
        self.write_defer_blocks(&blocks, indent)
    }

    /// `None` = all frames (return). `Some(true)` = through the nearest loop.
    pub(super) fn emit_defers_through(
        &mut self,
        indent: usize,
        stop_at_loop: Option<bool>,
    ) -> Result<String, String> {
        let mut blocks = Vec::new();
        for frame in self.frames.iter().rev() {
            for b in frame.blocks.iter().rev() {
                blocks.push(b.clone());
            }
            if stop_at_loop == Some(true) && frame.is_loop {
                break;
            }
        }
        self.write_defer_blocks(&blocks, indent)
    }

    pub(super) fn write_defer_blocks(
        &mut self,
        blocks: &[crate::ir::body::Block],
        indent: usize,
    ) -> Result<String, String> {
        let mut s = String::new();
        for block in blocks {
            for inner in &block.stmts {
                s.push_str(&self.stmt(inner, indent)?);
            }
        }
        Ok(s)
    }
    pub(super) fn line_dir(&self, span: crate::span::Span, indent: usize) -> String {
        if span.is_dummy() {
            return String::new();
        }
        let Some(file) = self.cx.file(span.file) else {
            return String::new();
        };
        let (path, line) = file.loc(span.start);
        let path = path.replace('\\', "\\\\").replace('"', "\\\"");
        format!("{}#line {line} \"{path}\"\n", "    ".repeat(indent))
    }
}
