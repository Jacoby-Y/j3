//! Statements. Every statement has type `void`.

use crate::ir::body::Block;
use crate::ir::op::BinOp;
use crate::ir::operand::Operand;
use crate::ir::place::Place;
use crate::ir::rvalue::Rvalue;
use crate::ir::ty::Ty;
use crate::span::Span;

#[derive(Clone, Debug)]
pub struct Stmt {
    pub ty: Ty,
    pub span: Span,
    pub kind: StmtKind,
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    /// Store `rvalue` into `place`. Place is bound once.
    Assign {
        place: Place,
        rvalue: Rvalue,
    },
    /// `place += rhs` (and `*=`, …). Place — including `xs[i]` — is bound once.
    /// Never lowered from `place = place + rhs` in source.
    AssignOp {
        place: Place,
        op: BinOp,
        rhs: Operand,
    },
    /// Runs at end of the current block, LIFO.
    Defer {
        body: Block,
    },
    If {
        cond: Operand,
        then_block: Block,
        else_block: Option<Block>,
    },
    /// `cond` is a value, not a re-tested expression. Source `while` is
    /// rewritten so the test is assigned at the top of `body` each iteration.
    While {
        cond: Operand,
        body: Block,
    },
    Return {
        value: Option<Operand>,
    },
    Break,
    Continue,
    Expr(Rvalue),
    /// `for i in start..end`. `index` is bound once. Step is `+= 1`.
    ForRange {
        index: Place,
        end: Operand,
        body: Block,
    },
    /// Bounds-check / panic abort. `--fast` strips these. Not a Linux syscall.
    Abort,
    /// Nested `{}`. Defers in `body` run at the end of this block.
    Block(Block),
    /// Fill `buf` (`[N]u8`) from interpolate parts; write byte count to `used`.
    /// Printers emit ordinary stmts and `Abort` if `used` would exceed `buf` len.
    InterpFill {
        buf: Place,
        used: Place,
        parts: Vec<crate::ir::rvalue::InterpPart>,
    },
}

impl Stmt {
    pub fn assign(span: Span, void_ty: Ty, place: Place, rvalue: Rvalue) -> Stmt {
        Stmt {
            ty: void_ty,
            span,
            kind: StmtKind::Assign { place, rvalue },
        }
    }

    pub fn assign_op(span: Span, void_ty: Ty, place: Place, op: BinOp, rhs: Operand) -> Stmt {
        Stmt {
            ty: void_ty,
            span,
            kind: StmtKind::AssignOp { place, op, rhs },
        }
    }

    pub fn expr(span: Span, void_ty: Ty, rvalue: Rvalue) -> Stmt {
        Stmt {
            ty: void_ty,
            span,
            kind: StmtKind::Expr(rvalue),
        }
    }

    pub fn return_value(span: Span, void_ty: Ty, value: Option<Operand>) -> Stmt {
        Stmt {
            ty: void_ty,
            span,
            kind: StmtKind::Return { value },
        }
    }

    pub fn if_then(span: Span, void_ty: Ty, cond: Operand, then_block: Block) -> Stmt {
        Stmt {
            ty: void_ty,
            span,
            kind: StmtKind::If {
                cond,
                then_block,
                else_block: None,
            },
        }
    }
}
