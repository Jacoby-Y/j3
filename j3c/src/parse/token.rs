use crate::span::Span;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TokenKind {
    Eof,
    Ident,
    Int,
    Float,
    String,
    Module,
    Use,
    Lib,
    As,
    Pub,
    Fn,
    Struct,
    Enum,
    Const,
    Let,
    If,
    Else,
    While,
    For,
    In,
    Match,
    Return,
    Break,
    Continue,
    Defer,
    Operator,
    This,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Semi,
    Colon,
    ColonColon,
    Dot,
    DotDot,
    DotDotEq,
    At,
    Hash,
    Question,
    Amp,
    Eq,
    EqEq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    AndAnd,
    OrOr,
    Not,
    Plus,
    PlusEq,
    Minus,
    MinusEq,
    Star,
    StarEq,
    Slash,
    SlashEq,
    Percent,
    Interpolate,
    /// `///` line. `text` is the body, without the slashes or one leading space.
    Doc,
}

/// Lexer keyword table. Highlighter (`dump-syntax`) iterates this; do not fork a second list.
pub const KEYWORDS: &[(&str, TokenKind)] = &[
    ("module", TokenKind::Module),
    ("use", TokenKind::Use),
    ("lib", TokenKind::Lib),
    ("as", TokenKind::As),
    ("pub", TokenKind::Pub),
    ("fn", TokenKind::Fn),
    ("struct", TokenKind::Struct),
    ("enum", TokenKind::Enum),
    ("const", TokenKind::Const),
    ("let", TokenKind::Let),
    ("if", TokenKind::If),
    ("else", TokenKind::Else),
    ("while", TokenKind::While),
    ("for", TokenKind::For),
    ("in", TokenKind::In),
    ("match", TokenKind::Match),
    ("return", TokenKind::Return),
    ("break", TokenKind::Break),
    ("continue", TokenKind::Continue),
    ("defer", TokenKind::Defer),
    ("operator", TokenKind::Operator),
    ("this", TokenKind::This),
];

/// Parsed as ident; check treats them as bool lits. Not keywords.
pub const BOOL_LITS: &[&str] = &["true", "false"];

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    pub text: String,
}

impl Token {
    pub fn eof(span: Span) -> Token {
        Token {
            kind: TokenKind::Eof,
            span,
            text: String::new(),
        }
    }
}

pub fn keyword(name: &str) -> Option<TokenKind> {
    if BOOL_LITS.iter().any(|&s| s == name) {
        return None;
    }
    for &(s, k) in KEYWORDS {
        if s == name {
            return Some(k);
        }
    }
    None
}

impl TokenKind {
    pub fn highlight_scope(self) -> Option<&'static str> {
        match self {
            TokenKind::If
            | TokenKind::Else
            | TokenKind::While
            | TokenKind::For
            | TokenKind::In
            | TokenKind::Match
            | TokenKind::Return
            | TokenKind::Break
            | TokenKind::Continue
            | TokenKind::Defer => Some("keyword.control.j3"),
            TokenKind::Fn
            | TokenKind::Struct
            | TokenKind::Enum
            | TokenKind::Const
            | TokenKind::Let => Some("storage.type.j3"),
            TokenKind::Pub => Some("storage.modifier.j3"),
            TokenKind::This => Some("variable.language.j3"),
            TokenKind::Module
            | TokenKind::Use
            | TokenKind::Lib
            | TokenKind::As
            | TokenKind::Operator => Some("keyword.other.j3"),
            _ => None,
        }
    }

    pub fn operator_class(self) -> Option<&'static str> {
        match self {
            TokenKind::Eq
            | TokenKind::PlusEq
            | TokenKind::MinusEq
            | TokenKind::StarEq
            | TokenKind::SlashEq => Some("keyword.operator.assignment.j3"),
            TokenKind::EqEq
            | TokenKind::Ne
            | TokenKind::Lt
            | TokenKind::Le
            | TokenKind::Gt
            | TokenKind::Ge => Some("keyword.operator.comparison.j3"),
            TokenKind::AndAnd | TokenKind::OrOr | TokenKind::Not => {
                Some("keyword.operator.logical.j3")
            }
            TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::Star
            | TokenKind::Slash
            | TokenKind::Percent => {
                Some("keyword.operator.arithmetic.j3")
            }
            TokenKind::ColonColon
            | TokenKind::Dot
            | TokenKind::DotDot
            | TokenKind::DotDotEq
            | TokenKind::Question
            | TokenKind::Amp => Some("keyword.operator.j3"),
            _ => None,
        }
    }

    pub fn operator_src(self) -> Option<&'static str> {
        match self {
            TokenKind::ColonColon => Some("::"),
            TokenKind::Dot => Some("."),
            TokenKind::DotDot => Some(".."),
            TokenKind::DotDotEq => Some("..="),
            TokenKind::Question => Some("?"),
            TokenKind::Amp => Some("&"),
            TokenKind::Eq => Some("="),
            TokenKind::EqEq => Some("=="),
            TokenKind::Ne => Some("!="),
            TokenKind::Lt => Some("<"),
            TokenKind::Le => Some("<="),
            TokenKind::Gt => Some(">"),
            TokenKind::Ge => Some(">="),
            TokenKind::AndAnd => Some("&&"),
            TokenKind::OrOr => Some("||"),
            TokenKind::Not => Some("!"),
            TokenKind::Plus => Some("+"),
            TokenKind::PlusEq => Some("+="),
            TokenKind::Minus => Some("-"),
            TokenKind::MinusEq => Some("-="),
            TokenKind::Star => Some("*"),
            TokenKind::StarEq => Some("*="),
            TokenKind::Slash => Some("/"),
            TokenKind::SlashEq => Some("/="),
            TokenKind::Percent => Some("%"),
            _ => None,
        }
    }
}
