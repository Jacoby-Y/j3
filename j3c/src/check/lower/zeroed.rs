//! `let buf: [64]u8;`: a typed `let` with no value starts as zeros. Arrays fill in a loop (the
//! backends turn it into a memset), structs field by field; numbers are 0, bools `false`,
//! pointers null, strings empty. Field defaults do not apply, as for a global without a value.
use super::*;
use crate::ir::stmt::{Stmt as IrStmt, StmtKind};

impl<'a> Lower<'a> {
    pub(super) fn zero_fill(&mut self, place: Place, span: Span) -> Result<(), CheckError> {
        let ty = place.ty;
        let scalar = match self.cx.ty_kind(ty).clone() {
            TyKind::Bool => Operand::bool_const(span, ty, false),
            TyKind::Int(_) => Operand::int(span, ty, 0),
            TyKind::Float(_) if ty == self.cx.common.f32 => Operand::f32_const(span, ty, 0.0),
            TyKind::Float(_) => Operand::f64_const(span, ty, 0.0),
            TyKind::Half => self.half_const(span, 0.0),
            TyKind::Ptr | TyKind::CStr => Operand::null(span, ty),
            TyKind::String => self.operand(&Expr { span, kind: ExprKind::String(Vec::new()) }, Some(ty))?,
            TyKind::Struct(sid) => {
                for k in 0..self.cx.struct_def(sid).fields.len() {
                    let f = self.b.field(self.cx, place.clone(), FieldId { struct_id: sid, index: k as u32 }, span);
                    self.zero_fill(f, span)?;
                }
                return Ok(());
            }
            TyKind::Array { len, .. } => {
                let int_ty = self.cx.common.int;
                let n = self.fresh("z");
                let id = self.b.tmp(n, int_ty, span);
                let i = self.b.local_place(id, span);
                self.b.assign(span, i.clone(), Rvalue::use_op(span, Operand::int(span, int_ty, 0)));
                let saved = self.b.begin_block();
                let elem = self.b.index(self.cx, place, Operand::Copy(i.clone()), span);
                self.zero_fill(elem, span)?;
                let body = self.b.end_block(saved);
                let end = Operand::int(span, int_ty, len);
                self.b.push(IrStmt { ty: self.cx.common.void, span, kind: StmtKind::ForRange { index: i, end, body } });
                return Ok(());
            }
            _ => {
                return Err(CheckError::new(self.path, span, format!("`{}` has no zero value", self.ty_str(ty)))
                    .help("`let x: T;` zero-fills numbers, bools, pointers, strings, and arrays and structs of them; give this `let` a value"))
            }
        };
        self.b.assign(span, place, Rvalue::use_op(span, scalar));
        Ok(())
    }
}
