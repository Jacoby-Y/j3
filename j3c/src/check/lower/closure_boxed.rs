//! Boxed closures: `fn[mem](..) { .. }` copies its captures into arena `mem` (`closure.rs` builds the
//! fn and ctx struct; this file allocates and fills the ctx), and a `boxed closure(..)` standing in
//! for a stack `closure(..)` of the same signature (`speculative/lang/boxed_closures.j3`).

use super::*;

impl<'a> Lower<'a> {
    /// The `mem` of `fn[mem]`: a `&Arena` operand and `Arena.allocAligned`. A value `Arena` is
    /// borrowed in place, so the allocation bumps it and not a copy.
    pub(super) fn arena_arg(&mut self, e: &Expr) -> Result<(Operand, FuncId), CheckError> {
        let op = match self.try_place(e) {
            Ok(place) => Operand::Copy(place),
            Err(_) => self.operand(e, None)?,
        };
        let found = op.ty();
        let arena = self.cx.ref_inner(found).unwrap_or(found);
        let alloc = match self.cx.ty_kind(arena) {
            TyKind::Struct(sid) if self.cx.name_str(self.cx.struct_def(*sid).name) == "Arena" => {
                self.methods.get(&(sid.0, "allocAligned".to_string())).copied()
            }
            _ => None,
        };
        let op = match op {
            Operand::Copy(place) if alloc.is_some() && arena == found && place.access.take_ref => {
                let rty = self.cx.ty_ref(arena);
                self.tmp_op(e.span, Rvalue::ref_place(e.span, rty, place))
            }
            op => op,
        };
        if alloc.is_some() && self.cx.ref_inner(op.ty()).is_none() {
            return Err(CheckError::new(self.path, e.span, "`fn[..]` needs an arena it can allocate from; this one cannot be borrowed")
                .help("pass a `let` arena or a `&Arena`"));
        }
        match alloc {
            Some(f) => Ok((op, f)),
            None => Err(CheckError::new(
                self.path,
                e.span,
                format!("`fn[..]` takes the arena its captures are copied into: an `Arena` or `&Arena`, found `{}`", self.ty_str(found)),
            )
            .help("`use j3.memory.{ Arena };` and write `fn[mem](..) { .. }` with `mem: &Arena`")),
        }
    }

    /// Allocates the ctx struct `sid` from the arena and copies each captured value in; the `ptr` to it.
    pub(super) fn boxed_ctx(&mut self, span: Span, sid: StructId, mem: Operand, alloc: FuncId, caps: &[(String, Place)]) -> Operand {
        let (int, ptr) = (self.cx.common.int, self.cx.common.ptr);
        let ctx_ty = self.cx.ty_struct(sid);
        let size = crate::ir::abi::size(self.cx, ctx_ty);
        let align = crate::ir::abi::align(self.cx, ctx_ty).max(8);
        let fty = self.cx.func(alloc).ty;
        let call = Rvalue::call(
            span,
            ptr,
            Operand::fn_addr(span, fty, alloc),
            vec![mem, Operand::int(span, int, size), Operand::int(span, int, align)],
        );
        let raw = self.materialize_rv(call, span);
        let ref_ty = self.cx.ty_ref(ctx_ty);
        let typed = self.materialize_rv(Rvalue::cast(span, ref_ty, Operand::Copy(raw.clone())), span);
        for (i, (_, pl)) in caps.iter().enumerate() {
            let base = self.b.deref(self.cx, typed.clone(), span);
            let field = self.b.field(self.cx, base, FieldId { struct_id: sid, index: i as u32 }, span);
            self.b.assign(span, field, Rvalue::use_op(span, Operand::Copy(pl.clone())));
        }
        Operand::Copy(raw)
    }

    /// A `boxed closure(..)` at `place` where a stack `closure(..)` of the same signature is wanted:
    /// the same two words under the stack type. Never the other way round.
    pub(super) fn boxed_as_stack(&mut self, span: Span, place: &Place, want: Ty) -> Option<Rvalue> {
        if !self.cx.is_boxed_closure(place.ty) || self.cx.stack_closure_sig(want)? != self.cx.closure_sig(place.ty)? {
            return None;
        }
        let TyKind::Struct(sid) = *self.cx.ty_kind(place.ty) else { unreachable!("closure types are structs") };
        let f = self.b.field(self.cx, place.clone(), FieldId { struct_id: sid, index: 0 }, span);
        let c = self.b.field(self.cx, place.clone(), FieldId { struct_id: sid, index: 1 }, span);
        Some(Rvalue::aggregate(span, want, vec![Operand::Copy(f), Operand::Copy(c)]))
    }
}
