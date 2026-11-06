//! Operators. Primitive math is compiler-known; user `operator+` is a call.
//! Integer `BinOp` wraps (two's complement). No traps.

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

impl BinOp {
    pub fn as_str(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
        }
    }

    pub fn assign_op_str(self) -> Option<&'static str> {
        match self {
            BinOp::Add => Some("+="),
            BinOp::Sub => Some("-="),
            BinOp::Mul => Some("*="),
            BinOp::Div => Some("/="),
            BinOp::Rem => Some("%="),
            BinOp::BitAnd => Some("&="),
            BinOp::BitOr => Some("|="),
            BinOp::BitXor => Some("^="),
            BinOp::Shl => Some("<<="),
            BinOp::Shr => Some(">>="),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum UnaryOp {
    Neg,
    Not,
    /// Bitwise not.
    BitNot,
}

impl UnaryOp {
    pub fn as_str(self) -> &'static str {
        match self {
            UnaryOp::Neg => "-",
            UnaryOp::Not => "!",
            UnaryOp::BitNot => "~",
        }
    }
}

/// `@atomicLoad` / `@atomicStore` / `@atomicAdd` / `@atomicSub` / `@atomicExchange` / `@atomicCas`
/// on `int`, `i32`, `u32`, `u64`, `bool` or `ptr` (add and sub: integers only), through a `&T`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AtomicOp {
    Load,
    Store,
    Add,
    Sub,
    Xchg,
    Cas,
}

impl AtomicOp {
    const ALL: [AtomicOp; 6] = [
        AtomicOp::Load,
        AtomicOp::Store,
        AtomicOp::Add,
        AtomicOp::Sub,
        AtomicOp::Xchg,
        AtomicOp::Cas,
    ];

    /// Name after the `@`.
    pub fn name(self) -> &'static str {
        match self {
            AtomicOp::Load => "atomicLoad",
            AtomicOp::Store => "atomicStore",
            AtomicOp::Add => "atomicAdd",
            AtomicOp::Sub => "atomicSub",
            AtomicOp::Xchg => "atomicExchange",
            AtomicOp::Cas => "atomicCas",
        }
    }

    pub fn from_name(s: &str) -> Option<AtomicOp> {
        Self::ALL.into_iter().find(|o| o.name() == s)
    }

    /// Operands: the pointer, then this many values of the pointee type.
    pub fn values(self) -> usize {
        match self {
            AtomicOp::Load => 0,
            AtomicOp::Store | AtomicOp::Add | AtomicOp::Sub | AtomicOp::Xchg => 1,
            AtomicOp::Cas => 2,
        }
    }

    /// Orders this op accepts (the C11 rule: a load cannot release, a store cannot acquire).
    pub fn orders(self) -> &'static [AtomicOrder] {
        use AtomicOrder::*;
        match self {
            AtomicOp::Load => &[Relaxed, Acquire, SeqCst],
            AtomicOp::Store => &[Relaxed, Release, SeqCst],
            _ => &[Relaxed, Acquire, Release, AcqRel, SeqCst],
        }
    }
}

/// Memory order of an atomic op. Default `SeqCst`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AtomicOrder {
    Relaxed,
    Acquire,
    Release,
    AcqRel,
    SeqCst,
}

impl AtomicOrder {
    /// The spelling in source.
    pub fn name(self) -> &'static str {
        match self {
            AtomicOrder::Relaxed => "relaxed",
            AtomicOrder::Acquire => "acquire",
            AtomicOrder::Release => "release",
            AtomicOrder::AcqRel => "acqrel",
            AtomicOrder::SeqCst => "seqcst",
        }
    }

    pub fn from_name(s: &str) -> Option<AtomicOrder> {
        [AtomicOrder::Relaxed, AtomicOrder::Acquire, AtomicOrder::Release, AtomicOrder::AcqRel, AtomicOrder::SeqCst]
            .into_iter()
            .find(|o| o.name() == s)
    }

    /// LLVM keyword.
    pub fn llvm(self) -> &'static str {
        match self {
            AtomicOrder::Relaxed => "monotonic",
            AtomicOrder::Acquire => "acquire",
            AtomicOrder::Release => "release",
            AtomicOrder::AcqRel => "acq_rel",
            AtomicOrder::SeqCst => "seq_cst",
        }
    }

    /// C11 `__ATOMIC_*` constant.
    pub fn c11(self) -> &'static str {
        match self {
            AtomicOrder::Relaxed => "__ATOMIC_RELAXED",
            AtomicOrder::Acquire => "__ATOMIC_ACQUIRE",
            AtomicOrder::Release => "__ATOMIC_RELEASE",
            AtomicOrder::AcqRel => "__ATOMIC_ACQ_REL",
            AtomicOrder::SeqCst => "__ATOMIC_SEQ_CST",
        }
    }

    /// Order for the failure path of a compare-exchange: no release part, no stronger than `self`.
    pub fn cas_failure(self) -> AtomicOrder {
        match self {
            AtomicOrder::Relaxed | AtomicOrder::Release => AtomicOrder::Relaxed,
            AtomicOrder::Acquire | AtomicOrder::AcqRel => AtomicOrder::Acquire,
            AtomicOrder::SeqCst => AtomicOrder::SeqCst,
        }
    }
}
