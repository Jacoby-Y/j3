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
    /// Integer `match`: runs the case whose value equals `scrut` (bit pattern of its int type),
    /// else `default`. Values are distinct. Printers emit a `switch`.
    Switch {
        scrut: Operand,
        cases: Vec<(u64, Block)>,
        default: Block,
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
    /// Leaves the loop `n` levels out (0 is the innermost); `break 'outer;` sets it.
    Break(u32),
    Continue(u32),
    Expr(Rvalue),
    /// `for i in start..end`. `index` is bound once. Step is `+= 1`.
    ForRange {
        index: Place,
        end: Operand,
        body: Block,
    },
    /// Bounds-check / panic abort. `--fast` strips these. Not a Linux syscall. The operands are
    /// `int` values shown in the message (`AbortKind::detail`): index and len, or a slice's ends.
    Abort(AbortKind, Vec<Operand>),
    /// Integer division by zero. `--fast` does not strip this.
    Div0,
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

/// Why a runtime check failed; picks the message `j3c` prints before exit 102.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbortKind {
    Index,
    Slice,
    Div0,
    Fmt,
    NoReturn,
    /// `xs[a..b]` with `b < a`.
    SliceOrder,
}

impl AbortKind {
    /// Every kind, in `code` order (the packed-abort table in the LLVM printer).
    pub const ALL: [AbortKind; 6] = [
        AbortKind::Index,
        AbortKind::Slice,
        AbortKind::Div0,
        AbortKind::Fmt,
        AbortKind::NoReturn,
        AbortKind::SliceOrder,
    ];

    pub fn code(self) -> u64 {
        self as u64
    }

    pub fn message(self) -> &'static str {
        match self {
            AbortKind::Index => "index out of bounds",
            AbortKind::Slice => "slice range out of bounds",
            AbortKind::Div0 => "integer divide by zero",
            AbortKind::Fmt => "string interpolation overflowed its buffer",
            AbortKind::NoReturn => "function ended without returning a value",
            AbortKind::SliceOrder => "slice range out of bounds",
        }
    }

    /// The text around the two values after the message: `[": index ", ", len "]` reads
    /// `index out of bounds: index 5, len 3`. Empty for a kind without values.
    pub fn detail(self) -> [&'static str; 2] {
        match self {
            AbortKind::Index => [": index ", ", len "],
            AbortKind::Slice => [": ", " is past len "],
            AbortKind::SliceOrder => [": end ", " is before start "],
            _ => ["", ""],
        }
    }

    /// `message` plus the values, as a backend that formats at run time prints it.
    pub fn text_with(self, vals: &[i64]) -> String {
        let [a, b] = self.detail();
        match vals {
            [x, y] if !a.is_empty() => format!("{}{a}{x}{b}{y}", self.message()),
            _ => self.message().to_string(),
        }
    }
}
