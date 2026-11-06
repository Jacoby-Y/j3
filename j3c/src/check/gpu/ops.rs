//! Operator typing: GLSL's rules for `+ - * /` between scalars, vectors and matrices, comparison, logic, bits,
//! and how an untyped literal takes the type of the other side.

use crate::emit::glsl::num::is_int_text;
use crate::parse::BinOp;
use crate::span::Span;

use super::gen::Gen;
use super::ty::*;
use super::R;

pub fn sym(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Rem => "%",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
        BinOp::BitAnd => "&",
        BinOp::BitOr => "|",
        BinOp::BitXor => "^",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}

fn is_lit(t: &GTy) -> bool {
    matches!(t, GTy::LitInt | GTy::LitFloat)
}

/// The scalar type a literal beside `t` becomes.
fn lit_target(t: &GTy) -> Option<GTy> {
    match t {
        GTy::Mat(_) => Some(F32),
        t if is_lit(t) => None,
        t => t.elem(),
    }
}

fn numeric(t: &GTy) -> bool {
    matches!(t, GTy::Scalar(Base::F | Base::I | Base::U) | GTy::Vec(Base::F | Base::I | Base::U, _) | GTy::Mat(_) | GTy::LitInt | GTy::LitFloat)
}

impl<'a> Gen<'a> {
    /// `v` as type `want`: a literal takes it, an exact type passes, anything else is the error every
    /// mismatch in shader code reports.
    pub fn coerce(&self, v: (String, GTy), want: &GTy, span: Span) -> R<String> {
        let (text, got) = v;
        if got == *want {
            return Ok(text);
        }
        match (&got, want) {
            (GTy::LitInt, GTy::Scalar(Base::F)) => Ok(if is_int_text(&text) { format!("{text}.0") } else { format!("float({text})") }),
            (GTy::LitInt, GTy::Scalar(Base::I)) => Ok(text),
            (GTy::LitInt, GTy::Scalar(Base::U)) if is_int_text(&text) && !text.starts_with('-') => Ok(format!("{text}u")),
            (GTy::LitInt, GTy::Scalar(Base::U)) => Ok(format!("uint({text})")),
            (GTy::LitFloat, GTy::Scalar(Base::F)) => Ok(text),
            _ => {
                let help = match (&got, want) {
                    (GTy::Scalar(_), GTy::Scalar(_)) => format!("convert it: `x as {}`", want.name()),
                    (GTy::LitFloat, GTy::Scalar(_)) => "an integer literal has no decimal point".to_string(),
                    _ => String::new(),
                };
                let e = self.e(span, format!("expected {}, found {}", want.name(), got.name()));
                Err(if help.is_empty() { e } else { e.help(help) })
            }
        }
    }

    /// Both operands of `op` made concrete, and the result type: `(left text, right text, result)`.
    pub fn arith(&mut self, op: BinOp, l: (String, GTy), r: (String, GTy), span: Span) -> R<(String, String, GTy)> {
        let (mut l, mut r) = (l, r);
        if is_lit(&l.1) && !is_lit(&r.1) {
            if let Some(t) = lit_target(&r.1) {
                l = (self.coerce(l, &t, span)?, t);
            }
        } else if is_lit(&r.1) && !is_lit(&l.1) {
            if let Some(t) = lit_target(&l.1) {
                r = (self.coerce(r, &t, span)?, t);
            }
        } else if is_lit(&l.1) && l.1 != r.1 {
            return self.err_help(span, "an integer literal and a float literal in one expression", "write both as floats (`2.0`) or both as integers");
        }
        let (a, b) = (l.1.clone(), r.1.clone());
        let bad = |s: &Self| -> R<GTy> {
            let help = match (&a, &b) {
                (GTy::Scalar(x), GTy::Scalar(y)) if x != y => format!("convert one side: `x as {}`", a.name()),
                (GTy::Vec(..), GTy::Vec(..)) => "vectors of different sizes or kinds do not combine; build one with a constructor".to_string(),
                _ => String::new(),
            };
            let e = s.e(span, format!("cannot apply `{}` to {} and {}", sym(op), a.name(), b.name()));
            Err(if help.is_empty() { e } else { e.help(help) })
        };
        use BinOp::*;
        let res = match op {
            Add | Sub | Mul | Div => match (&a, &b) {
                (x, y) if x == y && numeric(x) => x.clone(),
                (GTy::Vec(bs, _), GTy::Scalar(c)) | (GTy::Scalar(c), GTy::Vec(bs, _)) if bs == c && *bs != Base::B => if matches!(a, GTy::Vec(..)) { a.clone() } else { b.clone() },
                (GTy::Mat(_), GTy::Scalar(Base::F)) => a.clone(),
                (GTy::Scalar(Base::F), GTy::Mat(_)) => b.clone(),
                (GTy::Mat(n), GTy::Vec(Base::F, m)) if op == Mul && n == m => b.clone(),
                (GTy::Vec(Base::F, m), GTy::Mat(n)) if op == Mul && n == m => a.clone(),
                _ => bad(self)?,
            },
            Rem => match (&a, &b) {
                (x, y) if x == y && x.is_int() => x.clone(),
                (GTy::Vec(bs @ (Base::I | Base::U), _), GTy::Scalar(c)) if bs == c => a.clone(),
                _ if a.is_float() => return self.err_help(span, "`%` is for integers", "use `@mod(x, y)` for floats"),
                _ => bad(self)?,
            },
            BitAnd | BitOr | BitXor | Shl | Shr => match (&a, &b) {
                (x, y) if x == y && matches!(x, GTy::Scalar(Base::I | Base::U) | GTy::LitInt) => x.clone(),
                _ => bad(self)?,
            },
            Lt | Le | Gt | Ge => match (&a, &b) {
                (x, y) if x == y && x.is_scalar() && *x != BOOL => BOOL,
                _ => bad(self)?,
            },
            Eq | Ne => match (&a, &b) {
                (x, y) if x == y && !matches!(x, GTy::Sampler(_) | GTy::Array(..) | GTy::Void) => BOOL,
                _ => bad(self)?,
            },
            And | Or => match (&a, &b) {
                (x, y) if *x == BOOL && *y == BOOL => BOOL,
                _ => bad(self)?,
            },
        };
        Ok((l.0, r.0, res))
    }
}
