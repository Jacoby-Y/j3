//! Closure types. `closure(P..): R` is an ordinary two-field struct `{ $f: fn(ptr, P..): R, $ctx: ptr }`
//! (16 bytes, by value), so the backends and the validator need nothing new. Plain `fn(..)` stays one word.
//! Equal signatures intern to the same struct.

use std::collections::HashMap;

use crate::intern::Name;
use crate::ir::cx::IrCx;
use crate::ir::dump::dump_ty;
use crate::ir::entity::{FieldDef, StructDef};
use crate::ir::ids::{FuncId, StructId};
use crate::ir::ty::{Ty, TyKind};
use crate::span::Span;

#[derive(Default)]
pub struct ClosureTables {
    by_sig: HashMap<(Vec<Ty>, Ty), Ty>,
    sigs: HashMap<StructId, (Vec<Ty>, Ty)>,
    /// Plain fn to the `(ctx: ptr, ..)` thunk that lets it stand in for a closure.
    pub thunks: HashMap<FuncId, FuncId>,
}

impl IrCx {
    /// The struct for `closure(params): ret`, when it already exists.
    pub fn closure_lookup(&self, params: &[Ty], ret: Ty) -> Option<Ty> {
        self.closures.by_sig.get(&(params.to_vec(), ret)).copied()
    }

    /// `(params, ret)` when `ty` is a closure type.
    pub fn closure_sig(&self, ty: Ty) -> Option<&(Vec<Ty>, Ty)> {
        match self.ty_kind(ty) {
            TyKind::Struct(id) => self.closures.sigs.get(id),
            _ => None,
        }
    }

    /// `fn(ptr, params..): ret`, the type of a closure's body function.
    pub fn closure_fn_ty(&mut self, params: &[Ty], ret: Ty) -> Ty {
        let mut ps = vec![self.common.ptr];
        ps.extend_from_slice(params);
        self.ty_fn(ps, ret)
    }

    /// Interns `closure(params): ret`; a new struct is pushed on `structs` (the creating module's list).
    pub fn closure_ty(
        &mut self,
        params: Vec<Ty>,
        ret: Ty,
        module: &[Name],
        span: Span,
        structs: &mut Vec<StructId>,
    ) -> Ty {
        if let Some(t) = self.closure_lookup(&params, ret) {
            return t;
        }
        let mut name = String::from("closure(");
        for (i, p) in params.iter().enumerate() {
            if i > 0 {
                name.push_str(", ");
            }
            dump_ty(self, *p, &mut name);
        }
        name.push(')');
        if ret != self.common.void {
            name.push_str(": ");
            dump_ty(self, ret, &mut name);
        }
        let fn_ty = self.closure_fn_ty(&params, ret);
        let field = |cx: &mut IrCx, n: &str, ty: Ty| FieldDef {
            name: cx.intern_name(n),
            doc: String::new(),
            ty,
            embedded: false,
            default: None,
        };
        let fields = vec![field(self, "$f", fn_ty), field(self, "$ctx", self.common.ptr)];
        let sname = self.intern_name(&name);
        let id = self.alloc_struct(StructDef {
            module: module.to_vec(),
            name: sname,
            doc: String::new(),
            fields,
            packed: false,
            debug: false,
            span,
            ty_args: Vec::new(),
            script: None,
        });
        structs.push(id);
        let ty = self.ty_struct(id);
        self.closures.by_sig.insert((params.clone(), ret), ty);
        self.closures.sigs.insert(id, (params, ret));
        ty
    }
}
