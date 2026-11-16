//! Expression syntax: `Expr` and its kinds, closure literals, string interpolation parts, and
//! operators. Re-exported from `ast.rs`.

use super::ast::{Block, Ident, MatchStmt, Type};
use crate::span::Span;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EmbedKind {
    Text,
    Bytes,
    Dir,
}

#[derive(Clone, Debug)]
pub struct Expr {
    pub span: Span,
    pub kind: ExprKind,
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Name(Ident),
    Int(i64),
    /// An int literal above `i64::MAX` (`0xFFFFFFFFFFFFFFFF`): only a `u64` takes it.
    BigInt(u64),
    /// `let buf: [64]u8;`: the `let`'s type, zero-filled. Only a typed `let` makes it.
    Zeroed,
    /// `'a'` (u8 when ASCII, else a u32 code point; takes the other operand's int type).
    Char(u32),
    Float(f64),
    Bool(bool),
    String(Vec<u8>),
    StructLit {
        name: Ident,
        type_args: Vec<Type>,
        fields: Vec<(Ident, Expr)>,
    },
    TypePath {
        name: Ident,
        args: Vec<Type>,
    },
    ArrayLit {
        elems: Vec<Expr>,
    },
    /// `[]T { ptr: …, len: … }`
    SliceLit {
        elem: Type,
        fields: Vec<(Ident, Expr)>,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Slice {
        base: Box<Expr>,
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
    },
    Field {
        base: Box<Expr>,
        field: Ident,
    },
    Call {
        func: Box<Expr>,
        type_args: Vec<Type>,
        args: Vec<Expr>,
    },
    Syscall {
        args: Vec<Expr>,
    },
    /// `@atomicLoad(p)`, `@atomicStore(p, v)`, `@atomicAdd(p, v)`, `@atomicCas(p, old, new)`, each with
    /// an optional trailing order word (`acquire`); `order` is already validated for `op`.
    Atomic {
        op: crate::ir::op::AtomicOp,
        order: crate::ir::op::AtomicOrder,
        args: Vec<Expr>,
    },
    /// The type of a project-local `.j3s` file.
    ScriptType(String),
    /// `@embed("p")`, `@embedBytes("p")`, `@embedDir("dir", "glob")`: file contents at compile time;
    /// the loader reads them (`ParsedFile.embeds`, keyed by this expression's span).
    Embed { kind: EmbedKind, path: String, glob: String },
    /// `@script("path.j3s") { prop: value }`.
    ScriptLit {
        path: String,
        fields: Vec<(Ident, Expr)>,
    },
    SizeOf(Type),
    /// Compiler UI lowering: test an int index or a candidate-set enum against one arm.
    UiChoice { set: Type, value: Box<Expr>, index: Box<Expr> },
    SliceFrom {
        ptr: Box<Expr>,
        len: Box<Expr>,
    },
    Cast {
        expr: Box<Expr>,
        ty: Type,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Interpolate {
        parts: Vec<InterpPart>,
    },
    /// `(a, b)`: a tuple value (two or more elements).
    Tuple(Vec<Expr>),
    If {
        cond: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
    },
    Question {
        expr: Box<Expr>,
    },
    /// `match x { A { 1 } B { 2 } }` (and `if let`) used as a value.
    Match(Box<MatchStmt>),
    /// `fn(a: int, b): T { body }`, a closure literal.
    Closure(Box<ClosureLit>),
}

#[derive(Clone, Debug)]
pub struct ClosureLit {
    pub arena: Option<Expr>, // `fn[mem](..)`: the arena a boxed closure copies its captures into
    pub params: Vec<ClosureParam>,
    pub ret: Option<Type>,
    pub body: Block,
}

/// A closure parameter; the type may be left out when the expected type gives it.
#[derive(Clone, Debug)]
pub struct ClosureParam {
    pub name: Ident,
    pub ty: Option<Type>,
}

#[derive(Clone, Debug)]
pub enum InterpPart {
    Lit(Vec<u8>),
    Expr(Expr),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Shl,
    Shr,
    BitAnd,
    BitOr,
    BitXor,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnaryOp {
    Neg,
    BitNot,
    Ref,
    Deref,
    Not,
}

impl Expr {
    /// `-3` and `-0.5` as the literals they are (the parser keeps them as negation); a char
    /// literal as its code point.
    pub fn folded_literal(&self) -> Expr {
        if let ExprKind::Char(v) = self.kind {
            return Expr { span: self.span, kind: ExprKind::Int(v as i64) };
        }
        if let ExprKind::Unary {
            op: UnaryOp::Neg,
            expr,
        } = &self.kind
        {
            let kind = match expr.kind {
                ExprKind::Int(v) => Some(ExprKind::Int(v.wrapping_neg())),
                ExprKind::Float(v) => Some(ExprKind::Float(-v)),
                _ => None,
            };
            if let Some(kind) = kind {
                return Expr {
                    span: self.span,
                    kind,
                };
            }
        }
        self.clone()
    }
}
