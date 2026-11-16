//! `dyn Sub` to `dyn Super` (`trait Sub: Super`). The concrete type is gone behind a `dyn`, so the
//! `Sub` table carries, for each supertrait, an entry `$up.Super: fn(): ptr` that returns the same
//! type's `Super` table (made with the `Sub` table, `dyn_call.rs`). An upcast keeps `$data` and
//! swaps `$vt` for what that entry returns: one indirect call, no allocation, right for every type.

use super::*;
use crate::check::trait_short;
use crate::ir::entity::{Func, Vis};
use crate::ir::place::Access;

impl<'a> Lower<'a> {
    /// `op` (a `dyn Sub`, or a `&` of one) where a `dyn` of trait `want` is expected: the upcast,
    /// or `None` when `op` is not a `dyn`. Error when `want` is not a supertrait of `Sub`.
    pub(super) fn dyn_upcast(&mut self, span: Span, op: &Operand, want: &str) -> Result<Option<Rvalue>, CheckError> {
        let mut t = op.ty();
        while let Some(inner) = self.cx.ref_inner(t) {
            t = inner;
        }
        let Some(sub) = self.cx.dyn_trait(t).map(str::to_string) else { return Ok(None) };
        let mut place = self.tmp_place(span, Rvalue::use_op(span, op.clone()));
        while self.cx.ref_inner(place.ty).is_some() {
            place = self.b.deref(self.cx, place, span);
        }
        let table = self.dyn_table(span, &sub)?;
        let Some(j) = table.ups.iter().position(|u| u == want) else {
            return Err(CheckError::new(self.path, span, format!("a `dyn {}` is not a `dyn {}`", trait_short(&sub), trait_short(want)))
                .note(format!("a `dyn` converts only to a supertrait of its trait, and `{}` is not one of `{}`'s", trait_short(want), trait_short(&sub))));
        };
        let (TyKind::Struct(fat), TyKind::Struct(tsid)) = (self.cx.ty_kind(place.ty).clone(), self.cx.ty_kind(table.ty).clone()) else {
            return Err(CheckError::new(self.path, span, "internal: a `dyn` type is not a struct"));
        };
        let data = self.b.field(self.cx, place.clone(), FieldId { struct_id: fat, index: 0 }, span);
        let vt = self.b.field(self.cx, place, FieldId { struct_id: fat, index: 1 }, span);
        let tref = self.cx.ty_ref(table.ty);
        let tp = self.tmp_place(span, Rvalue::cast(span, tref, Operand::Copy(vt)));
        let tp = self.b.deref(self.cx, tp, span);
        let index = (table.slots.len() + j) as u32;
        let entry = self.b.field(self.cx, tp, FieldId { struct_id: tsid, index }, span);
        let ptr = self.cx.common.ptr;
        let up = self.tmp_op(span, Rvalue::call(span, ptr, Operand::Copy(entry), Vec::new()));
        let want_ty = self.cx.dyn_ty(want);
        Ok(Some(Rvalue::aggregate(span, want_ty, vec![Operand::Copy(data), up])))
    }

    /// `$dyn.<stem>.$up.<key>`: returns `ty`'s table for supertrait `up`.
    pub(super) fn dyn_up_thunk(&mut self, span: Span, ty: Ty, stem: &str, up: &str) -> Result<FuncId, CheckError> {
        let g = self.vtable(span, ty, up)?;
        let ptr = self.cx.common.ptr;
        let gty = self.cx.global(g).ty;
        let gref = self.cx.ty_ref(gty);
        let mut b = BodyBuilder::new(self.cx.common.void);
        let r = b.tmp(self.cx.intern_name("$r"), gref, span);
        let rp = b.local_place(r, span);
        b.assign(span, rp.clone(), Rvalue::ref_place(span, gref, Place::global(g, gty, Access::let_binding(), span)));
        let p = b.tmp(self.cx.intern_name("$p"), ptr, span);
        let pp = b.local_place(p, span);
        b.assign(span, pp.clone(), Rvalue::cast(span, ptr, Operand::Copy(rp)));
        b.return_op(span, Some(Operand::Copy(pp)));
        let body = b.finish();
        let ty = self.cx.dyn_up_fn_ty();
        let name = self.cx.intern_name(&format!("$dyn.{stem}.$up.{up}"));
        let id = self.cx.alloc_func(Func {
            id: FuncId(0),
            doc: String::new(),
            module: self.module_names.to_vec(),
            name,
            ty,
            vis: Vis::Private,
            method_of: None,
            kind: FuncKind::J3 { body },
            span,
        });
        self.ir_funcs.push(id);
        Ok(id)
    }
}
