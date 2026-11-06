//! `a == b` / `a != b` on `string`: length first, then bytes. Inline IR, so no stdlib module
//! has to be loaded and every backend (LLVM, C, VM) sees only ordinary loops and compares.
use super::*;
use crate::ir::stmt::{Stmt as IrStmt, StmtKind};

impl<'a> Lower<'a> {
    pub(super) fn string_eq(&mut self, span: Span, ne: bool, l: Operand, r: Operand) -> Rvalue {
        let (bool_ty, int_ty, str_ty, void) =
            (self.cx.common.bool, self.cx.common.int, self.cx.common.string, self.cx.common.void);
        // Each operand is evaluated once: copy both into temps.
        let tmp = |me: &mut Self, hint: &str, ty| {
            let n = me.fresh(hint);
            let id = me.b.tmp(n, ty, span);
            me.b.local_place(id, span)
        };
        let (lp, rp) = (tmp(self, "sl", str_ty), tmp(self, "sr", str_ty));
        self.b.assign(span, lp.clone(), Rvalue::use_op(span, l));
        self.b.assign(span, rp.clone(), Rvalue::use_op(span, r));
        let res = tmp(self, "se", bool_ty);
        let flag = |v: bool| Rvalue::use_op(span, Operand::bool_const(span, bool_ty, v));
        self.b.assign(span, res.clone(), flag(false));

        let llen = Operand::Copy(self.b.slice_len(self.cx, lp.clone(), span));
        let rlen = Operand::Copy(self.b.slice_len(self.cx, rp.clone(), span));
        let same_len = tmp(self, "sn", bool_ty);
        self.b.assign(span, same_len.clone(), Rvalue::binary(span, bool_ty, BinOp::Eq, llen.clone(), rlen));

        let saved = self.b.begin_block();
        self.b.assign(span, res.clone(), flag(true));
        let i = tmp(self, "si", int_ty);
        self.b.assign(span, i.clone(), Rvalue::use_op(span, Operand::int(span, int_ty, 0)));
        let ib = Operand::Copy(i.clone());
        let lb = Operand::Copy(self.b.index(self.cx, lp, ib.clone(), span));
        let rb = Operand::Copy(self.b.index(self.cx, rp, ib, span));
        let saved_loop = self.b.begin_block();
        let differ = tmp(self, "sd", bool_ty);
        self.b.assign(span, differ.clone(), Rvalue::binary(span, bool_ty, BinOp::Ne, lb, rb));
        let saved_if = self.b.begin_block();
        self.b.assign(span, res.clone(), flag(false));
        self.b.push(IrStmt { ty: void, span, kind: StmtKind::Break(0) });
        let then_block = self.b.end_block(saved_if);
        self.b.push(IrStmt {
            ty: void,
            span,
            kind: StmtKind::If { cond: Operand::Copy(differ), then_block, else_block: None },
        });
        let body = self.b.end_block(saved_loop);
        self.b.push(IrStmt { ty: void, span, kind: StmtKind::ForRange { index: i, end: llen, body } });
        let then_block = self.b.end_block(saved);
        self.b.push(IrStmt {
            ty: void,
            span,
            kind: StmtKind::If { cond: Operand::Copy(same_len), then_block, else_block: None },
        });
        if ne {
            return Rvalue { ty: bool_ty, span, kind: RvalueKind::Unary { op: UnaryOp::Not, arg: Operand::Copy(res) } };
        }
        Rvalue::use_op(span, Operand::Copy(res))
    }
}
