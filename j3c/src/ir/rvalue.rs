//! Rvalues: something that produces a value. Always has a `ty`.

use crate::ir::ids::VariantId;
use crate::ir::op::{AtomicOp, BinOp, UnaryOp};
use crate::ir::operand::Operand;
use crate::ir::place::Place;
use crate::ir::ty::Ty;
use crate::span::Span;

#[derive(Clone, Debug)]
pub struct Rvalue {
    pub ty: Ty,
    pub span: Span,
    pub kind: RvalueKind,
}

#[derive(Clone, Debug)]
pub enum RvalueKind {
    Use(Operand),
    /// `&place`. Place is bound once. Requires `place.access.take_ref`.
    Ref {
        place: Place,
    },
    Binary {
        op: BinOp,
        lhs: Operand,
        rhs: Operand,
    },
    Unary {
        op: UnaryOp,
        arg: Operand,
    },
    Call {
        func: Operand,
        args: Vec<Operand>,
    },
    /// `@syscall(nr, args...)`. Type is `int`.
    Syscall {
        nr: Operand,
        args: Vec<Operand>,
    },
    /// `@atomicLoad(p)` and friends: `args[0]` is the pointer, the rest are `int` values.
    /// Type is `int`, or `bool` for `Cas`.
    Atomic {
        op: AtomicOp,
        args: Vec<Operand>,
    },
    Aggregate {
        fields: Vec<Operand>,
    },
    /// Enum construction. C-like variants have `payload: None`.
    Enum {
        variant: VariantId,
        payload: Option<Operand>,
    },
    Cast {
        from: Operand,
    },
    /// `` `hello {name}` `` debug dump. Type is `string`.
    Interpolate {
        parts: Vec<InterpPart>,
    },
}

#[derive(Clone, Debug)]
pub enum InterpPart {
    Lit(Vec<u8>),
    Value(Operand),
}

impl Rvalue {
    pub fn use_op(span: Span, operand: Operand) -> Rvalue {
        Rvalue {
            ty: operand.ty(),
            span,
            kind: RvalueKind::Use(operand),
        }
    }

    pub fn ref_place(span: Span, ref_ty: Ty, place: Place) -> Rvalue {
        Rvalue {
            ty: ref_ty,
            span,
            kind: RvalueKind::Ref { place },
        }
    }

    pub fn binary(span: Span, ty: Ty, op: BinOp, lhs: Operand, rhs: Operand) -> Rvalue {
        Rvalue {
            ty,
            span,
            kind: RvalueKind::Binary { op, lhs, rhs },
        }
    }

    pub fn call(span: Span, ty: Ty, func: Operand, args: Vec<Operand>) -> Rvalue {
        Rvalue {
            ty,
            span,
            kind: RvalueKind::Call { func, args },
        }
    }

    pub fn syscall(span: Span, int_ty: Ty, nr: Operand, args: Vec<Operand>) -> Rvalue {
        Rvalue {
            ty: int_ty,
            span,
            kind: RvalueKind::Syscall { nr, args },
        }
    }

    pub fn atomic(span: Span, ty: Ty, op: AtomicOp, args: Vec<Operand>) -> Rvalue {
        Rvalue {
            ty,
            span,
            kind: RvalueKind::Atomic { op, args },
        }
    }

    pub fn cast(span: Span, ty: Ty, from: Operand) -> Rvalue {
        Rvalue {
            ty,
            span,
            kind: RvalueKind::Cast { from },
        }
    }

    pub fn aggregate(span: Span, ty: Ty, fields: Vec<Operand>) -> Rvalue {
        Rvalue {
            ty,
            span,
            kind: RvalueKind::Aggregate { fields },
        }
    }

    pub fn enum_new(span: Span, ty: Ty, variant: VariantId, payload: Option<Operand>) -> Rvalue {
        Rvalue {
            ty,
            span,
            kind: RvalueKind::Enum { variant, payload },
        }
    }
}
