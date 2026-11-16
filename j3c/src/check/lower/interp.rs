//! String interpolation lowering: `"{x}"` parts, constant folding, text-to-string.
//! Interpolation appends each part to a destination ([`Dest`]): the 512-byte stack buffer for
//! a string value, or a `Text` for `t.fmt(`..`)`, which has no limit.

use super::*;

/// A buffer an `InterpFill` writes into, and its length so far.
struct Fill {
    buf: Place,
    used: Place,
}

/// Where interpolated parts go.
enum Dest {
    /// `"{x}"` as a value: one `INTERP_CAP` buffer (aborts past it).
    Buf(Fill),
    /// `t.fmt(`..`)`: literals and strings are pushed whole with `Text.pushBytes`; each number or
    /// bool is printed into `scratch` first ([`TEXT_SCRATCH`] bytes), then pushed.
    Text { this: Operand, push: FuncId, scratch: Fill },
}

/// A `Text` destination prints only `bool`, ints and floats into its scratch: at most 20 bytes
/// (`-9223372036854775808`; `%g` floats stay under 16). The backends check against `INTERP_CAP`,
/// so nothing longer may become a `Text` leaf (see `leaf`).
const TEXT_SCRATCH: u64 = 64;

impl<'a> Lower<'a> {
    pub(super) fn interpolate(&mut self, span: Span, parts: &[InterpPart]) -> Result<Rvalue, CheckError> {
        if let Some(bytes) = self.fold_interp(parts) {
            return Ok(Rvalue::use_op(
                span,
                Operand::bytes(span, self.cx.common.string, &bytes),
            ));
        }
        let mut ir_parts = Vec::new();
        for part in parts {
            match part {
                InterpPart::Lit(b) => {
                    ir_parts.push(crate::ir::rvalue::InterpPart::Lit(b.clone()));
                }
                InterpPart::Expr(e) => {
                    let op = self.interp_operand(e)?;
                    // `{s}` alone with `s: string` is `s`: no copy, no buffer limit
                    if parts.len() == 1 && op.ty() == self.cx.common.string {
                        return Ok(Rvalue::use_op(span, op));
                    }
                    ir_parts.push(crate::ir::rvalue::InterpPart::Value(op));
                }
            }
        }
        let fill = self.new_fill(span, crate::ir::wrap::INTERP_CAP);
        self.cx.frame_interps.insert(span);
        let dest = Dest::Buf(fill);
        self.append_parts(span, &dest, ir_parts)?;
        let Dest::Buf(fill) = dest else { unreachable!() };
        let s = self.cx.common.string;
        Ok(self.fill_value(span, &fill, s))
    }

    /// `t.fmt(`..`)` on a `Text` (struct `sid`): every part appended to `t`, no length limit.
    /// The value is `t` as `&Text`, like `add`.
    pub(super) fn text_fmt(&mut self, span: Span, recv: Place, sid: StructId, parts: &[InterpPart]) -> Result<Rvalue, CheckError> {
        let Some(push) = self.methods.get(&(sid.0, "pushBytes".to_string())).copied() else {
            return Err(CheckError::new(self.path, span, "`fmt` needs `Text.pushBytes`"));
        };
        let want = self.cx.fn_sig(self.cx.func(push).ty).map(|s| s.0[0]).expect("pushBytes(this, b)");
        let this = self.auto_this(span, recv, want)?;
        let this = Operand::Copy(self.materialize(this, span));
        let mut ir_parts = Vec::new();
        for part in parts {
            ir_parts.push(match part {
                InterpPart::Lit(b) => crate::ir::rvalue::InterpPart::Lit(b.clone()),
                InterpPart::Expr(e) => crate::ir::rvalue::InterpPart::Value(self.interp_operand(e)?),
            });
        }
        let scratch = self.new_fill(span, TEXT_SCRATCH);
        let dest = Dest::Text { this: this.clone(), push, scratch };
        self.append_parts(span, &dest, ir_parts)?;
        Ok(Rvalue::use_op(span, this))
    }

    /// `j3.text.Text`.
    pub(super) fn is_text(&self, sid: StructId) -> bool {
        let def = self.cx.struct_def(sid);
        self.cx.name_str(def.name) == "Text"
            && def.module.len() == 2
            && self.cx.name_str(def.module[0]) == "j3"
            && self.cx.name_str(def.module[1]) == "text"
    }

    /// `e` as an interpolation part, or why it cannot be one.
    fn interp_operand(&mut self, e: &Expr) -> Result<Operand, CheckError> {
        let op = self.operand(e, None)?;
        if self.is_half(op.ty()) {
            return Err(CheckError::new(self.path, e.span, "an `f16` cannot appear in interpolate")
                .help("write `{f32(x)}`"));
        }
        if !self.interp_ok(op.ty()) {
            let err = CheckError::new(
                self.path,
                e.span,
                "this value cannot appear in interpolate (need primitive, string, or #[Debug])",
            );
            let mut t = op.ty();
            while let Some(inner) = self.cx.ref_inner(t) {
                t = inner;
            }
            return Err(match self.cx.ty_kind(t) {
                TyKind::Struct(sid) => err.help(format!("put `#[Debug]` on `struct {}`", self.cx.name_str(self.cx.struct_def(*sid).name))),
                TyKind::Enum(eid) => err.help(format!("put `#[Debug]` on `enum {}`", self.cx.name_str(self.cx.enum_def(*eid).name))),
                _ => err,
            });
        }
        Ok(op)
    }

    /// A fresh `[cap]u8` buffer and its length, set to 0.
    fn new_fill(&mut self, span: Span, cap: u64) -> Fill {
        let buf_ty = self.cx.ty_array(self.cx.common.u8, cap);
        let buf_n = self.fresh("ibuf");
        let buf_id = self.b.tmp(buf_n, buf_ty, span);
        let buf = self.b.local_place(buf_id, span);
        let used_n = self.fresh("in");
        let used_id = self.b.tmp(used_n, self.cx.common.int, span);
        let used = self.b.local_place(used_id, span);
        self.b.assign(
            span,
            used.clone(),
            Rvalue::use_op(span, Operand::int(span, self.cx.common.int, 0)),
        );
        Fill { buf, used }
    }

    /// The filled bytes of `fill` as a `ty` (`string` or `[]u8`) `{ ptr, len }`.
    fn fill_value(&mut self, span: Span, fill: &Fill, ty: Ty) -> Rvalue {
        let start = self.b.index(
            self.cx,
            fill.buf.clone(),
            Operand::int(span, self.cx.common.int, 0),
            span,
        );
        let ptr_ty = self.cx.ty_ref(self.cx.common.u8);
        let ptr_n = self.fresh("iptr");
        let ptr_id = self.b.tmp(ptr_n, ptr_ty, span);
        let ptr_place = self.b.local_place(ptr_id, span);
        self.b.assign(span, ptr_place.clone(), Rvalue::ref_place(span, ptr_ty, start));
        Rvalue::aggregate(span, ty, vec![Operand::Copy(ptr_place), Operand::Copy(fill.used.clone())])
    }

    /// Appends `parts` to `dest`: leaves are batched into `pending`, anything else expanded.
    fn append_parts(&mut self, span: Span, dest: &Dest, parts: Vec<crate::ir::rvalue::InterpPart>) -> Result<(), CheckError> {
        let mut pending = Vec::new();
        for part in parts {
            match part {
                crate::ir::rvalue::InterpPart::Value(op) if !self.leaf(dest, op.ty()) => {
                    let place = self.materialize(op, span);
                    self.interp_expand(span, dest, place, &mut pending)?;
                }
                part => pending.push(part),
            }
        }
        self.flush_fill(span, dest, &mut pending);
        Ok(())
    }

    /// A value `dest` takes whole: what the backends print (`emit_prints`) for the buffer; for a
    /// `Text`, only strings, bools and numbers (a `#[Debug]` struct is expanded field by field,
    /// so a long string inside it is pushed whole).
    fn leaf(&self, dest: &Dest, ty: Ty) -> bool {
        match dest {
            Dest::Buf(_) => self.emit_prints(ty),
            Dest::Text { .. } => {
                ty == self.cx.common.string || ty == self.cx.common.bool || is_int(self.cx, ty) || is_float(self.cx, ty)
            }
        }
    }

    /// Values the backends print themselves inside one `InterpFill`: primitives, strings, and
    /// `#[Debug]` structs of those. Anything else is expanded by `interp_expand`.
    fn emit_prints(&self, ty: Ty) -> bool {
        if ty == self.cx.common.string || ty == self.cx.common.bool || is_int(self.cx, ty) || is_float(self.cx, ty) {
            return true;
        }
        match self.cx.ty_kind(ty) {
            TyKind::Struct(id) => {
                let def = self.cx.struct_def(*id);
                def.debug && def.fields.iter().all(|f| self.emit_prints(f.ty))
            }
            _ => false,
        }
    }

    /// Appends the text of the value at `place` to `pending`: fields one by one, and for a
    /// `#[Debug]` enum an `If` chain on its tag (`E.V(a, b)`), each branch with its own fill.
    /// A pointer prints as its type (`&Expr`), so recursive types end.
    fn interp_expand(
        &mut self,
        span: Span,
        dest: &Dest,
        place: Place,
        pending: &mut Vec<crate::ir::rvalue::InterpPart>,
    ) -> Result<(), CheckError> {
        use crate::ir::rvalue::InterpPart as P;
        let ty = place.ty;
        if self.leaf(dest, ty) {
            pending.push(P::Value(Operand::Copy(place)));
            return Ok(());
        }
        match *self.cx.ty_kind(ty) {
            TyKind::Ref(_) | TyKind::Ptr => pending.push(P::Lit(self.ty_str(ty).into_bytes())),
            TyKind::Struct(sid) if self.cx.tuple_elems(ty).is_some() => {
                let n = self.cx.struct_def(sid).fields.len();
                for k in 0..n {
                    pending.push(P::Lit(if k == 0 { b"(".to_vec() } else { b", ".to_vec() }));
                    let fp = self.b.field(self.cx, place.clone(), FieldId { struct_id: sid, index: k as u32 }, span);
                    self.interp_expand(span, dest, fp, pending)?;
                }
                pending.push(P::Lit(b")".to_vec()));
            }
            TyKind::Struct(sid) if self.cx.struct_def(sid).debug => {
                let def = self.cx.struct_def(sid);
                let names: Vec<String> = def.fields.iter().map(|f| self.cx.name_str(f.name).to_string()).collect();
                pending.push(P::Lit(format!("{} {{ ", self.cx.name_str(def.name)).into_bytes()));
                for (k, fname) in names.iter().enumerate() {
                    let sep = if k > 0 { ", " } else { "" };
                    pending.push(P::Lit(format!("{sep}{fname}: ").into_bytes()));
                    let fp = self.b.field(self.cx, place.clone(), FieldId { struct_id: sid, index: k as u32 }, span);
                    self.interp_expand(span, dest, fp, pending)?;
                }
                pending.push(P::Lit(b" }".to_vec()));
            }
            // ponytail: unrolled per element, a loop in the IR when big `[N]T` prints matter
            TyKind::Array { len, .. } if len <= 64 => {
                for k in 0..len {
                    pending.push(P::Lit(if k == 0 { b"[".to_vec() } else { b", ".to_vec() }));
                    let i = Operand::int(span, self.cx.common.int, k);
                    let ep = self.b.index(self.cx, place.clone(), i, span);
                    self.interp_expand(span, dest, ep, pending)?;
                }
                pending.push(P::Lit(if len == 0 { b"[]".to_vec() } else { b"]".to_vec() }));
            }
            TyKind::Enum(eid) if self.cx.enum_def(eid).debug => {
                self.flush_fill(span, dest, pending);
                let def = self.cx.enum_def(eid);
                let ename = self.cx.name_str(def.name).to_string();
                let n = def.variants.len();
                let mut else_block: Option<crate::ir::body::Block> = None;
                for k in (0..n).rev() {
                    let vid = VariantId { enum_id: eid, index: k as u32 };
                    let vname = self.cx.name_str(self.cx.enum_def(eid).variants[k].name).to_string();
                    let saved = self.b.begin_block();
                    let mut text = vec![P::Lit(format!("{ename}.{vname}").into_bytes())];
                    let nf = self.variant_field_tys(vid).len();
                    for f in 0..nf {
                        text.push(P::Lit(if f == 0 { b"(".to_vec() } else { b", ".to_vec() }));
                        let fp = self.variant_field_place(&place, vid, f, span);
                        self.interp_expand(span, dest, fp, &mut text)?;
                    }
                    if nf > 0 {
                        text.push(P::Lit(b")".to_vec()));
                    }
                    self.flush_fill(span, dest, &mut text);
                    let body = self.b.end_block(saved);
                    // the last variant needs no test: the tag is one of them
                    let chained = match else_block {
                        Some(else_b) if k + 1 < n => {
                            let saved = self.b.begin_block();
                            let cond = self.is_variant(&place, vid, span);
                            self.b.push(crate::ir::stmt::Stmt {
                                ty: self.cx.common.void,
                                span,
                                kind: crate::ir::stmt::StmtKind::If { cond, then_block: body, else_block: Some(else_b) },
                            });
                            self.b.end_block(saved)
                        }
                        _ => body,
                    };
                    else_block = Some(chained);
                }
                for stmt in else_block.map(|b| b.stmts).unwrap_or_default() {
                    self.b.push(stmt);
                }
            }
            _ => {
                return Err(CheckError::new(self.path, span, format!("a `{}` inside this value cannot be printed", self.ty_str(ty)))
                    .help("interpolate prints primitives, strings, pointers (as their type), tuples, arrays up to 64, and `#[Debug]` structs and enums"));
            }
        }
        Ok(())
    }

    fn flush_fill(&mut self, span: Span, dest: &Dest, pending: &mut Vec<crate::ir::rvalue::InterpPart>) {
        if pending.is_empty() {
            return;
        }
        let (this, push, scratch) = match dest {
            Dest::Buf(fill) => {
                self.b.push(crate::ir::stmt::Stmt {
                    ty: self.cx.common.void,
                    span,
                    kind: crate::ir::stmt::StmtKind::InterpFill { buf: fill.buf.clone(), used: fill.used.clone(), parts: std::mem::take(pending) },
                });
                return;
            }
            Dest::Text { this, push, scratch } => (this, *push, scratch),
        };
        use crate::ir::rvalue::InterpPart as P;
        let bytes_ty = self.cx.ty_slice(self.cx.common.u8);
        let mut lit = Vec::new();
        for part in std::mem::take(pending) {
            let op = match part {
                P::Lit(b) => {
                    lit.extend(b);
                    continue;
                }
                P::Value(op) if op.ty() == self.cx.common.string => Operand::Copy(self.materialize_rv(Rvalue::cast(span, bytes_ty, op), span)),
                P::Value(op) => {
                    self.b.assign(span, scratch.used.clone(), Rvalue::use_op(span, Operand::int(span, self.cx.common.int, 0)));
                    self.b.push(crate::ir::stmt::Stmt {
                        ty: self.cx.common.void,
                        span,
                        kind: crate::ir::stmt::StmtKind::InterpFill { buf: scratch.buf.clone(), used: scratch.used.clone(), parts: vec![P::Value(op)] },
                    });
                    let rv = self.fill_value(span, scratch, bytes_ty);
                    Operand::Copy(self.materialize_rv(rv, span))
                }
            };
            if !lit.is_empty() {
                let b = Operand::bytes(span, bytes_ty, &std::mem::take(&mut lit));
                self.push_text(span, this, push, b);
            }
            self.push_text(span, this, push, op);
        }
        if !lit.is_empty() {
            let b = Operand::bytes(span, bytes_ty, &lit);
            self.push_text(span, this, push, b);
        }
    }

    fn push_text(&mut self, span: Span, this: &Operand, push: FuncId, bytes: Operand) {
        let fty = self.cx.func(push).ty;
        let void = self.cx.common.void;
        self.b.expr(span, Rvalue::call(span, void, Operand::fn_addr(span, fty, push), vec![this.clone(), bytes]));
    }

    pub(super) fn fold_interp(&self, parts: &[InterpPart]) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        for part in parts {
            match part {
                InterpPart::Lit(b) => out.extend(b),
                InterpPart::Expr(e) => match self.fold_interp_expr(e) {
                    Some(b) => out.extend(b),
                    None => return None,
                },
            }
        }
        Some(out)
    }

    pub(super) fn fold_interp_expr(&self, expr: &Expr) -> Option<Vec<u8>> {
        if let Some(v) = self.fold_int(expr) {
            return Some(v.to_string().into_bytes());
        }
        match &expr.kind {
            ExprKind::String(b) => Some(b.clone()),
            ExprKind::Bool(v) => Some(if *v {
                b"true".to_vec()
            } else {
                b"false".to_vec()
            }),
            ExprKind::Name(n) => match self.consts.get(&n.name) {
                Some(ConstVal {
                    kind: ConstPayload::String(b),
                    ..
                }) => Some(b.clone()),
                Some(ConstVal {
                    kind: ConstPayload::Int(v),
                    ..
                }) => Some(v.to_string().into_bytes()),
                Some(ConstVal {
                    kind: ConstPayload::Float(v),
                    ..
                }) => Some(v.to_string().into_bytes()),
                Some(ConstVal {
                    kind: ConstPayload::Bool(v),
                    ..
                }) => Some(if *v { b"true".to_vec() } else { b"false".to_vec() }),
                None => None,
            },
            _ => None,
        }
    }

    pub(super) fn interp_ok(&self, ty: Ty) -> bool {
        if ty == self.cx.common.string || ty == self.cx.common.bool {
            return true;
        }
        if is_int(self.cx, ty) || is_float(self.cx, ty) {
            return true;
        }
        match self.cx.ty_kind(ty) {
            TyKind::Struct(id) => self.cx.struct_def(*id).debug || self.cx.tuple_elems(ty).is_some(),
            TyKind::Enum(id) => self.cx.enum_def(*id).debug,
            _ => false,
        }
    }

    pub(super) fn text_to_string(
        &mut self,
        span: Span,
        place: &Place,
        want: Ty,
    ) -> Result<Option<Operand>, CheckError> {
        if want != self.cx.common.string {
            return Ok(None);
        }
        let sid = match self.cx.ty_kind(place.ty) {
            TyKind::Struct(id) => *id,
            _ => return Ok(None),
        };
        if self.cx.name_str(self.cx.struct_def(sid).name) != "Text" {
            return Ok(None);
        }
        let path = match field_path(self.cx, sid, "bytes") {
            Some(p) => p,
            None => return Ok(None),
        };
        let mut p = place.clone();
        for fid in path {
            p = self.b.field(self.cx, p, fid, span);
        }
        let rv = Rvalue::cast(span, want, Operand::Copy(p));
        let n = self.fresh("txt");
        let id = self.b.tmp(n, want, span);
        let out = self.b.local_place(id, span);
        self.b.assign(span, out.clone(), rv);
        Ok(Some(Operand::Copy(out)))
    }
}
