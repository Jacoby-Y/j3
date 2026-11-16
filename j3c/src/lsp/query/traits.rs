//! Hover and goto for what has no IR entity of its own: a trait name (also after `dyn`) and a char
//! literal (its code point and type).

use crate::check::{Program, TraitDoc};
use crate::parse::TokenKind;
use crate::span::{FileId, Span};

/// The trait `name` names here: the one declared in this file's module first, else the first.
pub(super) fn find_trait<'a>(prog: &'a Program, file: FileId, name: &str) -> Option<&'a TraitDoc> {
    let hits = prog.traits.iter().filter(|t| t.name == name);
    hits.clone().find(|t| t.span.file == file).or_else(|| hits.into_iter().next())
}

pub(super) fn trait_hover(t: &TraitDoc, dyn_kw: bool) -> String {
    let mut text = format!("```j3\n{}\n```\n*{}.{}*", t.sig, t.module, t.name);
    if dyn_kw {
        text.push_str("\n\n`dyn` value: a pointer to any value whose type has these methods, plus that type's method table");
    }
    if !t.doc.is_empty() {
        text.push_str("\n\n");
        text.push_str(&t.doc);
    }
    text
}

/// `dyn Name`: the name after the `dyn` keyword at `span`.
pub(super) fn after_dyn(src: &str, span: Span) -> Option<String> {
    let rest = src.get(span.end as usize..)?.trim_start();
    let end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(rest.len());
    (end > 0).then(|| rest[..end].to_string())
}

/// The char literal (`'a'`, `'\n'`, `'\u{1F600}'`) under `offset`: its hover text and span.
pub(super) fn char_hover(src: &str, file: FileId, offset: u32) -> Option<(String, Span)> {
    // ponytail: lexes the whole file per hover; cache tokens per version if hover gets slow
    let toks = crate::parse::lex::lex(src, file).ok()?;
    let t = toks.iter().find(|t| t.kind == TokenKind::Char && t.span.start <= offset && offset < t.span.end)?;
    let v: u32 = t.text.parse().ok()?;
    let ty = if v < 0x80 { "u8" } else { "u32" };
    let shown = src.get(t.span.start as usize..t.span.end as usize).unwrap_or("");
    Some((format!("```j3\n{shown}: {ty}\n```\ncode point {v} (U+{v:04X})"), t.span))
}
