//! `for x in it` over an iterator: a struct with `fn next(this: &Self): Option<T>`. The
//! iterator expression is evaluated once into a place (a call result lands in a temp; a local
//! or `&r` is advanced in place, as `while r.next()` would), then
//! `while true { o = it.next(); if o is not Some { break } x = payload; body }`.
//! `Option` is matched by shape (variants `Some(T)` and `None`), as the compiler does not
//! special-case it elsewhere. No allocation: `o` and `x` are function locals.
use super::*;
use crate::ir::stmt::{Stmt as IrStmt, StmtKind};

impl<'a> Lower<'a> {
    pub(super) fn for_iter(&mut self, s: &parse::ForEachStmt, iter: Place) -> Result<(), CheckError> {
        let span = s.iter.span;
        let tname = self.ty_str(iter.ty);
        let protocol = format!("`for` takes an array, a slice, a range, or an iterator: a struct with `fn next(this: &{tname}): Option<T>`");
        let sid = match self.cx.ty_kind(iter.ty) {
            TyKind::Struct(sid) => *sid,
            _ => return Err(CheckError::new(self.path, span, format!("`for` over a `{tname}`")).help(protocol)),
        };
        let Some(fid) = self.methods.get(&(sid.0, "next".to_string())).copied() else {
            return Err(CheckError::new(self.path, span, format!("`for` over `{tname}`, which has no `next` method")).help(protocol));
        };
        let (some, elem) = match self.option_shape(fid) {
            Some(v) => v,
            None => return Err(CheckError::new(self.path, span, format!("`{tname}.next` is not `fn next(this: &{tname}): Option<T>`")).help(protocol)),
        };
        if let Some(i) = &s.index {
            return Err(CheckError::new(self.path, i.span, "an iterator `for` has no index").help("count in a `let` before the loop"));
        }
        // the iterator, under a name the source cannot spell
        let alias = format!("$it{}", self.tmp);
        self.tmp += 1;
        self.aliases.insert(alias.clone(), iter);
        let base = Expr { span, kind: ExprKind::Name(Ident { name: alias.clone(), span }) };
        let call = Expr {
            span,
            kind: ExprKind::Call {
                func: Box::new(Expr { span, kind: ExprKind::Field { base: Box::new(base), field: Ident { name: "next".into(), span } } }),
                type_args: Vec::new(),
                args: Vec::new(),
            },
        };
        let nn = self.cx.intern_name(&s.name.name);
        let nid = self.b.let_local(nn, elem, s.name.span);
        let saved = self.b.begin_block();
        let r = self.for_iter_body(s, &call, some, nid);
        let body = self.b.end_block(saved);
        self.aliases.remove(&alias);
        self.locals.remove(&s.name.name);
        r?;
        let cond = Operand::bool_const(s.span, self.cx.common.bool, true);
        self.b.push(IrStmt { ty: self.cx.common.void, span: s.span, kind: StmtKind::While { cond, body } });
        Ok(())
    }

    fn for_iter_body(&mut self, s: &parse::ForEachStmt, call: &Expr, some: VariantId, nid: LocalId) -> Result<(), CheckError> {
        let span = s.iter.span;
        let rv = self.rvalue(call, None)?;
        let opt = self.materialize_rv(rv, span);
        let is_some = self.is_variant(&opt, some, span);
        let done = self.eq_ops(span, is_some, Operand::bool_const(span, self.cx.common.bool, false));
        let br = self.b.begin_block();
        self.b.push(IrStmt { ty: self.cx.common.void, span, kind: StmtKind::Break(0) });
        let then_block = self.b.end_block(br);
        self.b.push(IrStmt { ty: self.cx.common.void, span, kind: StmtKind::If { cond: done, then_block, else_block: None } });
        let payload = self.variant_field_place(&opt, some, 0, span);
        let nplace = self.b.local_place(nid, s.name.span);
        self.b.assign(span, nplace, Rvalue::use_op(span, Operand::Copy(payload)));
        self.locals.insert(s.name.name.clone(), nid);
        self.scoped_stmts(&s.body.stmts)
    }

    /// `next`'s `Some` variant and its payload type, when it returns an `Option`-shaped enum.
    fn option_shape(&self, fid: FuncId) -> Option<(VariantId, Ty)> {
        let (params, ret) = self.cx.fn_sig(self.cx.func(fid).ty)?;
        if params.len() != 1 || self.cx.ref_inner(params[0]).is_none() {
            return None;
        }
        let TyKind::Enum(eid) = *self.cx.ty_kind(ret) else { return None };
        let def = self.cx.enum_def(eid);
        if def.variants.len() != 2 {
            return None;
        }
        let k = def.variants.iter().position(|v| self.cx.name_str(v.name) == "Some")?;
        let (sv, nv) = (&def.variants[k], &def.variants[1 - k]);
        if self.cx.name_str(nv.name) != "None" || nv.payload.is_some() || sv.multi {
            return None;
        }
        Some((VariantId { enum_id: eid, index: k as u32 }, sv.payload?))
    }
}
