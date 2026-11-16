//! `==`, `!=` and `hash()` on tuples: element by element, each through the ordinary operator or
//! method lookup (so a struct element uses its `operator==`, a string compares bytes, a nested
//! tuple recurses). Lowered as a small expression over temporaries, so nothing here knows types.

use super::*;

/// Multiplier between element hashes (FNV-1a's 64-bit prime); element hashes are already mixed.
const HASH_MUL: i64 = 1099511628211;

fn at(span: Span, kind: ExprKind) -> Expr {
    Expr { span, kind }
}

fn field(span: Span, base: Expr, name: String) -> Expr {
    at(span, ExprKind::Field { base: Box::new(base), field: Ident { name, span } })
}

fn bin(span: Span, op: parse::BinOp, lhs: Expr, rhs: Expr) -> Expr {
    at(span, ExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) })
}

impl<'a> Lower<'a> {
    /// Binds `op` to a fresh temporary reachable by the returned name expression (`$t3`, which
    /// no source name can shadow). `unbind` removes the names again.
    fn bind_named(&mut self, span: Span, op: Operand) -> (String, Expr) {
        let n = self.fresh("tup");
        let id = self.b.tmp(n, op.ty(), span);
        let place = self.b.local_place(id, span);
        self.b.assign(span, place, Rvalue::use_op(span, op));
        let key = format!("${}", self.cx.name_str(n));
        self.locals.insert(key.clone(), id);
        let name = at(span, ExprKind::Name(Ident { name: key.clone(), span }));
        (key, name)
    }

    /// `l == r` (or `!=`) for two values of one tuple type.
    pub(super) fn tuple_eq(&mut self, span: Span, ne: bool, l: Operand, r: Operand) -> Result<Rvalue, CheckError> {
        let n = self.cx.tuple_elems(l.ty()).map_or(0, |e| e.len());
        let (ka, a) = self.bind_named(span, l);
        let (kb, b) = self.bind_named(span, r);
        let mut all: Option<Expr> = None;
        for i in 0..n {
            let eq = bin(span, parse::BinOp::Eq, field(span, a.clone(), i.to_string()), field(span, b.clone(), i.to_string()));
            all = Some(match all {
                None => eq,
                Some(prev) => bin(span, parse::BinOp::And, prev, eq),
            });
        }
        let mut e = all.expect("a tuple has two or more elements");
        if ne {
            e = at(span, ExprKind::Unary { op: parse::UnaryOp::Not, expr: Box::new(e) });
        }
        let bool_ty = self.cx.common.bool;
        let rv = self.rvalue(&e, Some(bool_ty));
        self.locals.remove(&ka);
        self.locals.remove(&kb);
        rv
    }

    /// `t.hash()`: `h = hash(t.0); h = h * P ^ hash(t.k) ..; h ^ (h >> 29)`.
    pub(super) fn tuple_hash(&mut self, span: Span, recv: Place) -> Result<Rvalue, CheckError> {
        let mut place = recv;
        while self.cx.ref_inner(place.ty).is_some() {
            place = self.b.deref(self.cx, place, span);
        }
        let n = self.cx.tuple_elems(place.ty).map_or(0, |e| e.len());
        let (kt, t) = self.bind_named(span, Operand::Copy(place));
        let hash_of = |i: usize| {
            let f = field(span, field(span, t.clone(), i.to_string()), "hash".to_string());
            at(span, ExprKind::Call { func: Box::new(f), type_args: Vec::new(), args: Vec::new() })
        };
        let mut h = hash_of(0);
        for i in 1..n {
            h = bin(span, parse::BinOp::BitXor, bin(span, parse::BinOp::Mul, h, at(span, ExprKind::Int(HASH_MUL))), hash_of(i));
        }
        let u64_ty = self.cx.common.u64;
        let folded = self.operand(&h, Some(u64_ty));
        self.locals.remove(&kt);
        let folded = folded?;
        if folded.ty() != u64_ty {
            return Err(CheckError::new(self.path, span, format!("`hash` of a tuple element returns `{}`, not `u64`", self.ty_str(folded.ty()))));
        }
        let (kh, h) = self.bind_named(span, folded);
        let e = bin(span, parse::BinOp::BitXor, h.clone(), bin(span, parse::BinOp::Shr, h, at(span, ExprKind::Int(29))));
        let rv = self.rvalue(&e, Some(u64_ty));
        self.locals.remove(&kh);
        rv
    }
}
