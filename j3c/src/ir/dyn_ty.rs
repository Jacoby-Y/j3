//! Fat pointers. A fat pointer is an ordinary two-field struct `{ $data: ptr, $vt: ptr }` (16 bytes,
//! by value), so layout, both backends and the validator need nothing new. `dyn Trait` is one,
//! interned per trait; `$vt` points at a method table: a struct of `fn(ptr, P..): R` fields, one
//! global per (type, trait) for the whole program (`check/lower/dyn_call.rs`).

use std::collections::HashMap;

use crate::intern::Name;
use crate::ir::cx::IrCx;
use crate::ir::entity::{FieldDef, StructDef};
use crate::ir::ids::{GlobalId, StructId};
use crate::ir::ty::{Ty, TyKind};
use crate::span::Span;

/// One method of a `dyn` trait: its name and the type of its table entry, `fn(ptr, P..): R`.
#[derive(Clone)]
pub struct DynSlot {
    pub name: String,
    pub fn_ty: Ty,
    /// The trait (key) that declares it: a supertrait's methods are in the table too.
    pub from: String,
}

/// A trait's table layout, made at the first conversion or call.
#[derive(Clone)]
pub struct DynTable {
    /// The table struct; slot `i` is field `i`, then up `j` is field `slots.len() + j`.
    pub ty: Ty,
    pub slots: Vec<DynSlot>,
    /// Every supertrait (key, transitively): its entry is a `fn(): ptr` giving the same type's table
    /// for that trait, so a `dyn Sub` converts to a `dyn Super` (`check/lower/dyn_up.rs`).
    pub ups: Vec<String>,
}

#[derive(Default, Clone)]
pub struct DynTables {
    by_trait: HashMap<String, Ty>,
    traits: HashMap<StructId, String>,
    pub(crate) tables: HashMap<String, DynTable>,
    /// The table global of each (type, trait key): one per program, whichever module made it first.
    pub(crate) vtables: HashMap<(Ty, String), GlobalId>,
    /// Structs made since the last `take_new_dyn_structs`, for a module's struct list.
    fresh: Vec<StructId>,
    /// Tables made since the last `take_new_vtables`, for the lowering module's global list.
    fresh_globals: Vec<GlobalId>,
}

impl IrCx {
    /// A two-word fat pointer struct, `{ $data: ptr, $vt: ptr }`, named `name`.
    pub fn fat_ptr_struct(&mut self, name: &str, module: Vec<Name>, span: Span) -> Ty {
        let ptr = self.common.ptr;
        let fields = vec![self.plain_field("$data", ptr), self.plain_field("$vt", ptr)];
        self.anon_struct(name, module, fields, span)
    }

    /// `dyn Name` for the trait `key` (`module::Name`); one type per trait.
    pub fn dyn_ty(&mut self, key: &str) -> Ty {
        if let Some(&t) = self.dyns.by_trait.get(key) {
            return t;
        }
        let (module, short) = key.rsplit_once("::").unwrap_or(("", key));
        let names: Vec<Name> = module.split('.').filter(|p| !p.is_empty()).map(|p| self.intern_name(p)).collect();
        let ty = self.fat_ptr_struct(&format!("dyn {short}"), names, Span::dummy());
        if let TyKind::Struct(id) = *self.ty_kind(ty) {
            self.dyns.traits.insert(id, key.to_string());
        }
        self.dyns.by_trait.insert(key.to_string(), ty);
        ty
    }

    /// The trait key when `ty` is a `dyn Trait`.
    pub fn dyn_trait(&self, ty: Ty) -> Option<&str> {
        match self.ty_kind(ty) {
            TyKind::Struct(id) => self.dyns.traits.get(id).map(|s| s.as_str()),
            _ => None,
        }
    }

    /// A method table struct: field `i` is `slots[i]`, typed `fn(ptr, P..): R`, then one
    /// `$up.Trait: fn(): ptr` per supertrait in `ups`.
    pub fn dyn_table_struct(&mut self, key: &str, slots: &[DynSlot], ups: &[String]) -> Ty {
        let mut fields: Vec<FieldDef> = slots.iter().map(|s| self.plain_field(&s.name, s.fn_ty)).collect();
        let up_ty = self.dyn_up_fn_ty();
        for u in ups {
            fields.push(self.plain_field(&format!("$up.{u}"), up_ty));
        }
        let (module, short) = key.rsplit_once("::").unwrap_or(("", key));
        let names: Vec<Name> = module.split('.').filter(|p| !p.is_empty()).map(|p| self.intern_name(p)).collect();
        self.anon_struct(&format!("vtable {short}"), names, fields, Span::dummy())
    }

    /// The type of a table's supertrait entry: `fn(): ptr`.
    pub fn dyn_up_fn_ty(&mut self) -> Ty {
        let ptr = self.common.ptr;
        self.ty_fn(Vec::new(), ptr)
    }

    pub fn note_vtable(&mut self, g: GlobalId) {
        self.dyns.fresh_globals.push(g);
    }

    /// Fat-pointer and table structs made since the last call, for a module's struct list.
    pub fn take_new_dyn_structs(&mut self) -> Vec<StructId> {
        std::mem::take(&mut self.dyns.fresh)
    }

    /// Table globals made since the last call; the module being lowered owns (defines) them.
    pub fn take_new_vtables(&mut self) -> Vec<GlobalId> {
        std::mem::take(&mut self.dyns.fresh_globals)
    }

    fn plain_field(&mut self, name: &str, ty: Ty) -> FieldDef {
        FieldDef { name: self.intern_name(name), doc: String::new(), ty, embedded: false, default: None }
    }

    fn anon_struct(&mut self, name: &str, module: Vec<Name>, fields: Vec<FieldDef>, span: Span) -> Ty {
        let sname = self.intern_name(name);
        let id = self.alloc_struct(StructDef {
            module,
            name: sname,
            doc: String::new(),
            fields,
            packed: false,
            debug: false,
            span,
            ty_args: Vec::new(),
            script: None,
        });
        self.dyns.fresh.push(id);
        self.ty_struct(id)
    }
}
