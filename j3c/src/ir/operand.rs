//! Operands: a value ready to use. Either a constant or a copy (memcpy) of a place.

use crate::ir::ids::FuncId;
use crate::ir::place::Place;
use crate::ir::ty::Ty;
use crate::span::Span;

#[derive(Clone, Debug)]
pub struct Const {
    pub ty: Ty,
    pub span: Span,
    pub kind: ConstKind,
}

#[derive(Clone, Debug)]
pub enum ConstKind {
    Void,
    Bool(bool),
    /// Bit pattern. Width/sign come from `Const.ty`.
    Int(u64),
    F32Bits(u32),
    F64Bits(u64),
    FnAddr(FuncId),
    Null,
    /// UTF-8 bytes for a `string` lang-item constant.
    Bytes(Vec<u8>),
}

#[derive(Clone, Debug)]
pub enum Operand {
    Const(Const),
    /// Load / memcpy from a place. Copying `&T` is a pointer copy (fine).
    /// Copying `Arena` by value is the header-alias bug — don't put `Arena` in `Game`.
    Copy(Place),
}

impl Operand {
    pub fn ty(&self) -> Ty {
        match self {
            Operand::Const(c) => c.ty,
            Operand::Copy(place) => place.ty,
        }
    }

    pub fn unit(span: Span, void_ty: Ty) -> Operand {
        Operand::Const(Const {
            ty: void_ty,
            span,
            kind: ConstKind::Void,
        })
    }

    pub fn int(span: Span, ty: Ty, bits: u64) -> Operand {
        Operand::Const(Const {
            ty,
            span,
            kind: ConstKind::Int(bits),
        })
    }

    pub fn fn_addr(span: Span, ty: Ty, id: FuncId) -> Operand {
        Operand::Const(Const {
            ty,
            span,
            kind: ConstKind::FnAddr(id),
        })
    }

    pub fn bool_const(span: Span, ty: Ty, value: bool) -> Operand {
        Operand::Const(Const {
            ty,
            span,
            kind: ConstKind::Bool(value),
        })
    }

    pub fn f32_const(span: Span, ty: Ty, value: f32) -> Operand {
        Operand::Const(Const {
            ty,
            span,
            kind: ConstKind::F32Bits(value.to_bits()),
        })
    }

    pub fn f64_const(span: Span, ty: Ty, value: f64) -> Operand {
        Operand::Const(Const {
            ty,
            span,
            kind: ConstKind::F64Bits(value.to_bits()),
        })
    }

    pub fn null(span: Span, ty: Ty) -> Operand {
        Operand::Const(Const {
            ty,
            span,
            kind: ConstKind::Null,
        })
    }

    pub fn bytes(span: Span, ty: Ty, bytes: &[u8]) -> Operand {
        Operand::Const(Const {
            ty,
            span,
            kind: ConstKind::Bytes(bytes.to_vec()),
        })
    }
}
