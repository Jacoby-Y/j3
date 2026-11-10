//! Verbose helpers for building places and bodies by hand.

use crate::intern::Name;
use crate::ir::body::{Block, Body, LocalDecl, LocalKind};
use crate::ir::cx::IrCx;
use crate::ir::ids::{FieldId, LocalId, VariantId};
use crate::ir::op::BinOp;
use crate::ir::operand::Operand;
use crate::ir::place::{Access, Place, Proj};
use crate::ir::rvalue::Rvalue;
use crate::ir::stmt::Stmt;
use crate::ir::ty::Ty;
use crate::span::Span;

pub struct BodyBuilder {
    locals: Vec<LocalDecl>,
    params: Vec<LocalId>,
    stmts: Vec<Stmt>,
    void: Ty,
}

impl BodyBuilder {
    pub fn new(void: Ty) -> BodyBuilder {
        BodyBuilder {
            locals: Vec::new(),
            params: Vec::new(),
            stmts: Vec::new(),
            void,
        }
    }

    pub fn alloc_local(
        &mut self,
        name: Name,
        ty: Ty,
        access: Access,
        kind: LocalKind,
        span: Span,
    ) -> LocalId {
        let id = LocalId(self.locals.len() as u32);
        self.locals.push(LocalDecl {
            id,
            doc: String::new(),
            name,
            ty,
            access,
            kind,
            span,
        });
        if kind == LocalKind::Param {
            self.params.push(id);
        }
        id
    }

    /// Locals allocated so far; the next `LocalId` is this.
    pub fn local_count(&self) -> u32 {
        self.locals.len() as u32
    }

    pub fn param(&mut self, name: Name, ty: Ty, span: Span) -> LocalId {
        self.alloc_local(name, ty, Access::let_binding(), LocalKind::Param, span)
    }

    pub fn tmp(&mut self, name: Name, ty: Ty, span: Span) -> LocalId {
        self.alloc_local(name, ty, Access::let_binding(), LocalKind::Tmp, span)
    }

    pub fn let_local(&mut self, name: Name, ty: Ty, span: Span) -> LocalId {
        self.alloc_local(name, ty, Access::let_binding(), LocalKind::Let, span)
    }

    pub fn set_doc(&mut self, id: LocalId, doc: String) {
        if !doc.is_empty() {
            self.locals[id.0 as usize].doc = doc;
        }
    }

    pub fn local_place(&self, id: LocalId, span: Span) -> Place {
        let decl = &self.locals[id.0 as usize];
        Place::local(id, decl.ty, decl.access, span)
    }

    pub fn deref(&self, cx: &IrCx, place: Place, span: Span) -> Place {
        let inner = cx.ref_inner(place.ty).expect("deref of non-ref type");
        place.project(Proj::Deref, inner, Access::through_ref(), span)
    }

    pub fn field(&self, cx: &IrCx, place: Place, field: FieldId, span: Span) -> Place {
        let field_ty = cx.field_def(field).ty;
        let access = place.access;
        place.project(
            Proj::Field {
                field,
                ty: field_ty,
            },
            field_ty,
            access,
            span,
        )
    }

    pub fn index(&self, cx: &IrCx, place: Place, index: Operand, span: Span) -> Place {
        let elem = cx.elem_ty(place.ty).expect("index of non-array/slice");
        let mut access = place.access;
        match cx.ty_kind(place.ty) {
            crate::ir::ty::TyKind::String => {
                access = Access::read_only_view();
            }
            _ => {}
        }
        place.project(Proj::Index { index, ty: elem }, elem, access, span)
    }

    pub fn slice_ptr(&self, cx: &mut IrCx, place: Place, span: Span) -> Place {
        let elem = cx.elem_ty(place.ty).expect("ptr of non-slice/string");
        let ptr_ty = cx.ty_ref(elem);
        let mut access = place.access;
        match cx.ty_kind(place.ty) {
            crate::ir::ty::TyKind::String => {
                access = Access::read_only_view();
            }
            _ => {}
        }
        place.project(Proj::Ptr, ptr_ty, access, span)
    }

    /// Lane `lane` (0..3) of a vector place; the lane is writable when the vector is.
    pub fn vec_lane(&self, cx: &IrCx, place: Place, lane: u8, span: Span) -> Place {
        let ty = cx.vec_lane(place.ty).expect("lane of non-vector");
        let access = place.access;
        place.project(Proj::Lane { lane, ty }, ty, access, span)
    }

    pub fn enum_tag(&self, cx: &IrCx, place: Place, span: Span) -> Place {
        let access = place.access;
        place.project(Proj::EnumTag, cx.common.i32, access, span)
    }

    pub fn enum_payload(&self, place: Place, variant: VariantId, ty: Ty, span: Span) -> Place {
        let access = place.access;
        place.project(Proj::EnumPayload { variant, ty }, ty, access, span)
    }

    pub fn slice_len(&self, cx: &IrCx, place: Place, span: Span) -> Place {
        let mut access = place.access;
        match cx.ty_kind(place.ty) {
            crate::ir::ty::TyKind::String => {
                access = Access::read_only_view();
            }
            _ => {}
        }
        place.project(Proj::Len, cx.common.int, access, span)
    }

    pub fn push(&mut self, stmt: Stmt) {
        self.stmts.push(stmt);
    }

    pub fn assign(&mut self, span: Span, place: Place, rvalue: Rvalue) {
        self.push(Stmt::assign(span, self.void, place, rvalue));
    }

    pub fn assign_op(&mut self, span: Span, place: Place, op: BinOp, rhs: Operand) {
        self.push(Stmt::assign_op(span, self.void, place, op, rhs));
    }

    pub fn expr(&mut self, span: Span, rvalue: Rvalue) {
        self.push(Stmt::expr(span, self.void, rvalue));
    }

    pub fn return_op(&mut self, span: Span, value: Option<Operand>) {
        self.push(Stmt::return_value(span, self.void, value));
    }

    pub fn if_then(&mut self, span: Span, cond: Operand, then_block: Block) {
        self.push(Stmt::if_then(span, self.void, cond, then_block));
    }

    /// `_tmp = &place`; returns `copy _tmp`. Place is bound once.
    pub fn ref_tmp(&mut self, cx: &mut IrCx, name: Name, place: Place, span: Span) -> Operand {
        let ref_ty = cx.ty_ref(place.ty);
        let tmp = self.tmp(name, ref_ty, span);
        let tmp_place = self.local_place(tmp, span);
        let rvalue = Rvalue::ref_place(span, ref_ty, place);
        self.assign(span, tmp_place, rvalue);
        Operand::Copy(self.local_place(tmp, span))
    }

    pub fn begin_block(&mut self) -> Vec<Stmt> {
        std::mem::take(&mut self.stmts)
    }

    pub fn end_block(&mut self, saved: Vec<Stmt>) -> Block {
        let inner = std::mem::take(&mut self.stmts);
        self.stmts = saved;
        Block { stmts: inner }
    }

    pub fn finish(self) -> Body {
        Body {
            locals: self.locals,
            params: self.params,
            block: Block { stmts: self.stmts },
        }
    }
}
