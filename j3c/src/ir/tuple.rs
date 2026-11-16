//! Tuple types. `(A, B)` is an ordinary struct with fields `0`, `1` (C layout), so layout, both
//! backends and the validator need nothing new. Equal element lists intern to one struct for the
//! whole program; `check_files_inner` hands each new one to a module so the emitters declare it.

use std::collections::HashMap;

use crate::ir::cx::IrCx;
use crate::ir::dump::dump_ty;
use crate::ir::entity::{FieldDef, StructDef};
use crate::ir::ids::StructId;
use crate::ir::ty::{Ty, TyKind};
use crate::span::Span;

#[derive(Default, Clone)]
pub struct TupleTables {
    by_elems: HashMap<Vec<Ty>, Ty>,
    elems: HashMap<StructId, Vec<Ty>>,
    /// Made since the last `take_new_tuples`, not yet on any module's struct list.
    fresh: Vec<StructId>,
}

impl IrCx {
    /// The tuple type of `elems`, when it already exists.
    pub fn tuple_lookup(&self, elems: &[Ty]) -> Option<Ty> {
        self.tuples.by_elems.get(elems).copied()
    }

    /// Element types when `ty` is a tuple.
    pub fn tuple_elems(&self, ty: Ty) -> Option<&[Ty]> {
        match self.ty_kind(ty) {
            TyKind::Struct(id) => self.tuples.elems.get(id).map(|v| v.as_slice()),
            _ => None,
        }
    }

    pub fn is_tuple_struct(&self, id: StructId) -> bool {
        self.tuples.elems.contains_key(&id)
    }

    /// Interns `(elems..)` (two or more).
    pub fn tuple_ty(&mut self, elems: Vec<Ty>) -> Ty {
        if let Some(t) = self.tuple_lookup(&elems) {
            return t;
        }
        let mut name = String::from("(");
        for (i, e) in elems.iter().enumerate() {
            if i > 0 {
                name.push_str(", ");
            }
            dump_ty(self, *e, &mut name);
        }
        name.push(')');
        let fields = elems
            .iter()
            .enumerate()
            .map(|(k, ty)| FieldDef { name: self.intern_name(&k.to_string()), doc: String::new(), ty: *ty, embedded: false, default: None })
            .collect();
        let sname = self.intern_name(&name);
        let module = vec![self.intern_name("tuple")];
        let id = self.alloc_struct(StructDef {
            module,
            name: sname,
            doc: String::new(),
            fields,
            packed: false,
            debug: false,
            span: Span::dummy(),
            ty_args: Vec::new(),
            script: None,
        });
        let ty = self.ty_struct(id);
        self.tuples.by_elems.insert(elems.clone(), ty);
        self.tuples.elems.insert(id, elems);
        self.tuples.fresh.push(id);
        ty
    }

    /// Tuple structs made since the last call, for a module's struct list.
    pub fn take_new_tuples(&mut self) -> Vec<StructId> {
        std::mem::take(&mut self.tuples.fresh)
    }
}
