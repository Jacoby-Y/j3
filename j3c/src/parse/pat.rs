//! `match` (a statement, or an expression: `ExprKind::Match`), destructuring `let` and
//! `let .. else`, and their patterns. `if let` is parsed as a `match` with a `_` arm.

use super::ast::{Block, Expr, ExprKind, Ident};
use crate::span::Span;

#[derive(Clone, Debug)]
pub struct MatchStmt {
    pub span: Span,
    pub expr: Expr,
    pub arms: Vec<MatchArm>,
}

/// `A | B if guard { body }`: one or more alternatives, an optional guard, a body.
#[derive(Clone, Debug)]
pub struct MatchArm {
    pub span: Span,
    /// The `|` alternatives, at least one.
    pub pats: Vec<Pat>,
    /// `if cond`: checked after the pattern's names are bound; when false, later arms are tried.
    pub guard: Option<Expr>,
    pub body: Block,
    /// The arm's value: an expression after the statements with no `;` (`Op.Add { 1 }`). In a
    /// statement `match` it runs as an expression statement.
    pub tail: Option<Expr>,
}

/// `let Span { lo, hi } = s;`: the pattern always matches (checked), and binds its names.
/// `let E.A(x) = e else { return; };`: a pattern that may not match, and an `else` that leaves.
#[derive(Clone, Debug)]
pub struct LetPatStmt {
    pub span: Span,
    pub pat: Pat,
    pub value: Expr,
    pub else_block: Option<Block>,
}

#[derive(Clone, Debug)]
pub enum Pat {
    /// A name (only inside a payload or tuple: a top-level single name is a `Variant` path or a `const`).
    Bind(Ident),
    Wild(Span),
    /// `Enum.Name(a, b)` (one sub-pattern per field, none without parens), or in an int match a
    /// `const` (`LIMIT`, `lex.T_EOF`).
    Variant {
        span: Span,
        path: Vec<Ident>,
        payload: Vec<Pat>,
    },
    /// `Span { lo, hi: h, .. }`: `lo` is short for `lo: lo`; `rest` is the trailing `..`, without
    /// which every field is listed.
    Struct {
        span: Span,
        path: Vec<Ident>,
        fields: Vec<(Ident, Pat)>,
        rest: bool,
    },
    /// `(a, b)`: a tuple's elements; names inside bind, as in a payload.
    Tuple { span: Span, elems: Vec<Pat> },
    /// An int or char literal: `3`, `-1`, `'a'`.
    Lit(Expr),
    /// `lo..=hi`, both ends included; each end a literal or a `const`.
    Range { lo: Expr, hi: Expr },
}

impl Pat {
    pub fn span(&self) -> Span {
        match self {
            Pat::Bind(n) => n.span,
            Pat::Wild(s) | Pat::Variant { span: s, .. } | Pat::Struct { span: s, .. } | Pat::Tuple { span: s, .. } => *s,
            Pat::Lit(e) => e.span,
            Pat::Range { lo, hi } => Span { file: lo.span.file, start: lo.span.start, end: hi.span.end },
        }
    }

    /// Matches every value of its type: `_`, a name, or a struct pattern of such patterns.
    /// A variant pattern is refutable even on a one-variant enum.
    pub fn irrefutable(&self) -> bool {
        match self {
            Pat::Bind(_) | Pat::Wild(_) => true,
            Pat::Struct { fields, .. } => fields.iter().all(|(_, p)| p.irrefutable()),
            Pat::Tuple { elems, .. } => elems.iter().all(Pat::irrefutable),
            Pat::Variant { .. } | Pat::Lit(_) | Pat::Range { .. } => false,
        }
    }
}

/// `a.b.c` as a name/field expression: an int `const` named in a pattern.
pub fn path_expr(path: &[Ident], span: Span) -> Expr {
    let mut e = Expr { span: path[0].span, kind: ExprKind::Name(path[0].clone()) };
    for seg in &path[1..] {
        e = Expr { span: seg.span, kind: ExprKind::Field { base: Box::new(e), field: seg.clone() } };
    }
    Expr { span, ..e }
}
