//! Places: a slot you can load from or store to.
//!
//! Field and index of `&T` stay places. Auto-deref is `Proj::Deref`.
//! Local `use` aliases are the same `local + proj` as the field they name.

use crate::ir::ids::{FieldId, GlobalId, LocalId, VariantId};
use crate::ir::operand::Operand;
use crate::ir::ty::Ty;
use crate::span::Span;

/// What you may do with a place. Not a type; the type is `Place.ty`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Access {
    /// Store / `+=` allowed (`let`, or through `&T`).
    pub assign: bool,
    /// `&place` allowed. False for `const` bindings.
    pub take_ref: bool,
}

impl Access {
    pub fn let_binding() -> Access {
        Access {
            assign: true,
            take_ref: true,
        }
    }

    pub fn const_binding() -> Access {
        Access {
            assign: false,
            take_ref: false,
        }
    }

    /// Read-only view (`string`, slice of a const). Index is not an lvalue.
    pub fn read_only_view() -> Access {
        Access {
            assign: false,
            take_ref: true,
        }
    }

    /// Through `&T`: writable, can take another `&`.
    pub fn through_ref() -> Access {
        Access {
            assign: true,
            take_ref: true,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Proj {
    /// `*p`. `p` must have type `&T`. Result type is `T`.
    Deref,
    Field {
        field: FieldId,
        ty: Ty,
    },
    /// `xs[i]`. `i` is evaluated when the place is bound (once for `+=`).
    Index {
        index: Operand,
        ty: Ty,
    },
    /// `.ptr` on `[]T` / `string`. Type is `&T`.
    Ptr,
    /// `.len` on `[]T` / `string`. Type is `int`.
    Len,
    /// Tag of a tagged-union enum. Type is `i32`.
    EnumTag,
    /// Payload of a tagged-union variant.
    EnumPayload {
        variant: VariantId,
        ty: Ty,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum PlaceRoot {
    Local(LocalId),
    Global(GlobalId),
}

#[derive(Clone, Debug)]
pub struct Place {
    /// Type of the value in the slot (`Arena`, not `&Arena`, after deref).
    pub ty: Ty,
    pub access: Access,
    pub span: Span,
    pub root: PlaceRoot,
    pub proj: Vec<Proj>,
}

impl Place {
    pub fn local(local: LocalId, ty: Ty, access: Access, span: Span) -> Place {
        Place {
            ty,
            access,
            span,
            root: PlaceRoot::Local(local),
            proj: Vec::new(),
        }
    }

    pub fn global(global: GlobalId, ty: Ty, access: Access, span: Span) -> Place {
        Place {
            ty,
            access,
            span,
            root: PlaceRoot::Global(global),
            proj: Vec::new(),
        }
    }

    pub fn project(mut self, proj: Proj, ty: Ty, access: Access, span: Span) -> Place {
        self.proj.push(proj);
        self.ty = ty;
        self.access = access;
        self.span = span;
        self
    }
}
