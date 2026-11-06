use crate::parse::ast::{
    AssignOpStmt, AssignStmt, BinOp, Block, ConstItem, DeferStmt, EnumItem, Expr, ExprKind,
    FieldItem, File, FnItem, GpuStage, GpuStruct, ForEachStmt, ForStmt, Ident, IfStmt, InterpPart, Item, LabeledStmt, LetStmt,
    LibFnDecl, MatchArm, MatchStmt, ModLetItem, Param, Pat, ReturnStmt, ScriptBlockItem,
    ScriptLetItem, Stmt, StructItem, TraitItem, TraitMethod, Type, TypeKind, UnaryOp, UseItem, UseLibItem, UseLocalStmt,
    VariantItem, WhileStmt,
};

struct ItemAttrs {
    debug: bool,
    try_catch: Option<(Ident, Ident)>,
    singleton: bool,
    thread: bool,
    impl_fn: bool,
    /// `#[shader]`, `#[shader(vertex)]`, `#[shader(fragment)]` (see `shader_attrs.rs`).
    shader: Option<GpuStage>,
    gpu_struct: GpuStruct,
    uniform: bool,
    flat: bool,
}
use crate::parse::error::ParseError;
use crate::parse::syntax::{
    is_j3ui_widget, INTRINSIC_CLOSURES, INTRINSIC_GPU, INTRINSIC_SIMD, ATTR_DEBUG, ATTR_IMPL, ITEM_EXTEND, ITEM_TRAIT, ATTR_SINGLETON, ATTR_THREAD, ATTR_TRY_CATCH, INTRINSIC_SCRIPT, INTRINSIC_SIZEOF,
    INTRINSIC_SLICE, INTRINSIC_SYSCALL, INTRINSIC_UI_CHOICE, J3UI_END, J3UI_IF, J3UI_POP, SCRIPT_PROPS, SCRIPT_SHARED,
};
use crate::parse::token::{Token, TokenKind};
use crate::span::Span;

mod atomic;
mod attrs;
mod bitops;
mod closure;
mod expr;
mod extend;
mod generics;
mod items;
mod primary;
mod shader_attrs;
mod script_ui;
mod stmt;
mod traits;

pub fn parse_tokens(tokens: &[Token]) -> Result<File, ParseError> {
    let mut p = Parser {
        tokens,
        i: 0,
        j3ui_recv: None,
        script_module: None,
    };
    p.file()
}

pub fn parse_script_tokens(tokens: &[Token], module: &[String]) -> Result<File, ParseError> {
    let mut p = Parser {
        tokens,
        i: 0,
        j3ui_recv: None,
        script_module: Some(module.to_vec()),
    };
    p.file()
}

struct Parser<'a> {
    tokens: &'a [Token],
    i: usize,
    /// First parameter name while parsing an `@j3ui` body.
    j3ui_recv: Option<String>,
    script_module: Option<Vec<String>>,
}

impl<'a> Parser<'a> {
    fn ident(&mut self) -> Result<Ident, ParseError> {
        let tok = self.cur().clone();
        if tok.kind != TokenKind::Ident && tok.kind != TokenKind::This {
            return Err(self.err("expected identifier"));
        }
        let _ = self.bump();
        Ok(Ident {
            name: tok.text,
            span: tok.span,
        })
    }

    fn expect(&mut self, kind: TokenKind, message: &str) -> Result<&Token, ParseError> {
        if self.cur().kind == kind {
            Ok(self.bump())
        } else if kind == TokenKind::Semi && self.i > 0 {
            // A missing `;` belongs at the end of the line before, not on the next statement.
            let end = self.tokens[self.i - 1].span.end;
            Err(ParseError::new(Span { file: self.cur().span.file, start: end, end: end + 1 }, message))
        } else {
            Err(self.err(message))
        }
    }

    fn err(&self, message: &str) -> ParseError {
        ParseError::new(self.cur().span, message)
    }

    fn cur(&self) -> &Token {
        &self.tokens[self.i]
    }

    fn look(&self, n: usize) -> &Token {
        let i = self.i + n;
        if i < self.tokens.len() {
            &self.tokens[i]
        } else {
            self.tokens.last().unwrap()
        }
    }

    fn bump(&mut self) -> &Token {
        let i = self.i;
        if self.tokens[i].kind != TokenKind::Eof {
            self.i += 1;
        }
        &self.tokens[i]
    }

    fn prev_end(&self, fallback: Span) -> Span {
        if self.i == 0 {
            return fallback;
        }
        self.tokens[self.i - 1].span
    }
}

fn join(a: Span, b: Span) -> Span {
    Span {
        file: a.file,
        start: a.start,
        end: b.end.max(a.end),
    }
}

fn stmt_span(stmt: &Stmt) -> Span {
    match stmt {
        Stmt::Let(s) => s.span,
        Stmt::Return(s) => s.span,
        Stmt::Assign(s) => s.span,
        Stmt::AssignOp(s) => s.span,
        Stmt::Expr(e) => e.span,
        Stmt::If(s) => s.span,
        Stmt::While(s) => s.span,
        Stmt::For(s) => s.span,
        Stmt::ForEach(s) => s.span,
        Stmt::UseLocal(s) => s.span,
        Stmt::Match(s) => s.span,
        Stmt::Defer(s) => s.span,
        Stmt::Break(s, _) | Stmt::Continue(s, _) => *s,
        Stmt::Labeled(s) => s.span,
        Stmt::Block(b) => b.span,
    }
}

fn parse_int(text: &str) -> Result<i64, String> {
    if let Some(hex) = text.strip_prefix("0x") {
        match i64::from_str_radix(hex, 16) {
            Ok(v) => Ok(v),
            Err(_) => Err(format!("invalid hex literal `{text}`")),
        }
    } else if let Some(bin) = text.strip_prefix("0b") {
        i64::from_str_radix(bin, 2).map_err(|_| format!("invalid binary literal `{text}`"))
    } else {
        match text.parse::<i64>() {
            Ok(v) => Ok(v),
            Err(_) => Err(format!("invalid integer `{text}`")),
        }
    }
}

fn split_interp(text: &str, span: Span) -> Result<Vec<InterpPart>, ParseError> {
    let mut parts = Vec::new();
    let mut lit = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    // byte offset of each char, so a hole's spans point into the file (the token starts at a backtick)
    let offs: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '{' {
            if !lit.is_empty() {
                parts.push(InterpPart::Lit(std::mem::take(&mut lit)));
            }
            i += 1;
            let hole_start = i;
            let mut depth = 1;
            while i < chars.len() && depth > 0 {
                if chars[i] == '{' {
                    depth += 1;
                } else if chars[i] == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                i += 1;
            }
            if depth != 0 {
                return Err(ParseError::new(span, "unterminated interpolate hole"));
            }
            if hole_start == i {
                return Err(ParseError::new(span, "empty interpolate hole"));
            }
            let hole: String = chars[hole_start..i].iter().collect();
            let expr = parse_hole_expr(&hole, span, span.start + 1 + offs[hole_start] as u32)?;
            parts.push(InterpPart::Expr(expr));
            i += 1;
            continue;
        }
        let ch = chars[i];
        let mut buf = [0u8; 4];
        let s = ch.encode_utf8(&mut buf);
        lit.extend(s.as_bytes());
        i += 1;
    }
    if !lit.is_empty() {
        parts.push(InterpPart::Lit(lit));
    }
    Ok(parts)
}

fn ident_at(name: &str, span: Span) -> Ident {
    Ident {
        name: name.to_string(),
        span,
    }
}

fn parse_hole_expr(src: &str, span: Span, base: u32) -> Result<Expr, ParseError> {
    let mut tokens = crate::parse::lex::lex(src, span.file)?;
    for t in tokens.iter_mut() {
        t.span.start += base;
        t.span.end += base;
    }
    let mut p = Parser {
        tokens: &tokens,
        i: 0,
        j3ui_recv: None,
        script_module: None,
    };
    let expr = p.expr()?;
    if p.cur().kind != TokenKind::Eof {
        return Err(ParseError::new(span, "trailing junk in interpolate hole"));
    }
    Ok(expr)
}
