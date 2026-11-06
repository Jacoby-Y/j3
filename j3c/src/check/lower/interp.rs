//! String interpolation lowering: `"{x}"` parts, constant folding, text-to-string.

use super::*;

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
                    let op = self.operand(e, None)?;
                    if self.is_half(op.ty()) {
                        return Err(CheckError::new(self.path, e.span, "an `f16` cannot appear in interpolate")
                            .help("write `{f32(x)}`"));
                    }
                    if !self.interp_ok(op.ty()) {
                        let e = CheckError::new(
                            self.path,
                            e.span,
                            "this value cannot appear in interpolate (need primitive, string, or #[Debug])",
                        );
                        let mut t = op.ty();
                        while let Some(inner) = self.cx.ref_inner(t) {
                            t = inner;
                        }
                        return Err(match self.cx.ty_kind(t) {
                            TyKind::Struct(sid) => e.help(format!("put `#[Debug]` on `struct {}`", self.cx.name_str(self.cx.struct_def(*sid).name))),
                            TyKind::Enum(eid) => e.help(format!("put `#[Debug]` on `enum {}`", self.cx.name_str(self.cx.enum_def(*eid).name))),
                            _ => e,
                        });
                    }
                    // `{s}` alone with `s: string` is `s`: no copy, no buffer limit
                    if parts.len() == 1 && op.ty() == self.cx.common.string {
                        return Ok(Rvalue::use_op(span, op));
                    }
                    ir_parts.push(crate::ir::rvalue::InterpPart::Value(op));
                }
            }
        }
        let cap = crate::ir::wrap::INTERP_CAP;
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
        self.b.push(crate::ir::stmt::Stmt {
            ty: self.cx.common.void,
            span,
            kind: crate::ir::stmt::StmtKind::InterpFill {
                buf: buf.clone(),
                used: used.clone(),
                parts: ir_parts,
            },
        });
        let start = self.b.index(
            self.cx,
            buf,
            Operand::int(span, self.cx.common.int, 0),
            span,
        );
        let ptr_ty = self.cx.ty_ref(self.cx.common.u8);
        let ptr_n = self.fresh("iptr");
        let ptr_id = self.b.tmp(ptr_n, ptr_ty, span);
        let ptr_place = self.b.local_place(ptr_id, span);
        self.b.assign(span, ptr_place.clone(), Rvalue::ref_place(span, ptr_ty, start));
        Ok(Rvalue::aggregate(
            span,
            self.cx.common.string,
            vec![Operand::Copy(ptr_place), Operand::Copy(used)],
        ))
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
            TyKind::Struct(id) => self.cx.struct_def(*id).debug,
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
