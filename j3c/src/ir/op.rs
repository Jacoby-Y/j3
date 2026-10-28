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

/// `@atomicLoad` / `@atomicStore` / `@atomicAdd` / `@atomicCas` on an `int`, sequentially consistent.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AtomicOp {
    Load,
    Store,
    Add,
    Cas,
}

impl AtomicOp {
    /// Name after the `@`.
    pub fn name(self) -> &'static str {
        match self {
            AtomicOp::Load => "atomicLoad",
            AtomicOp::Store => "atomicStore",
            AtomicOp::Add => "atomicAdd",
            AtomicOp::Cas => "atomicCas",
        }
    }

    pub fn from_name(s: &str) -> Option<AtomicOp> {
        [AtomicOp::Load, AtomicOp::Store, AtomicOp::Add, AtomicOp::Cas]
            .into_iter()
            .find(|o| o.name() == s)
    }

    /// Operands: the pointer, then this many `int` values.
    pub fn values(self) -> usize {
        match self {
            AtomicOp::Load => 0,
            AtomicOp::Store | AtomicOp::Add => 1,
            AtomicOp::Cas => 2,
        }
    }
}
