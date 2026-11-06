//! Code warnings on the user's own modules (not `j3.*`, not generated `.j3ui` code): unused
//! locals, unused `use`s, and statements after a `return` / `break` / `continue`.
//! Built to never warn wrongly: when unsure, stay quiet.

use std::collections::{HashMap, HashSet};

use super::ParsedFile;
use crate::diag::Diagnostic;
use crate::parse::{Block, Item, Stmt, TokenKind};
use crate::span::Span;

/// A source edit `check --autofix` can apply: replace `src[start..end]` of `path` with `text`.
#[derive(Clone, Debug)]
pub struct Fix {
    pub path: String,
    pub start: usize,
    pub end: usize,
    pub text: String,
}

pub(crate) fn lint(files: &[ParsedFile]) -> (Vec<Diagnostic>, Vec<Fix>) {
    let mut out = Vec::new();
    let mut fixes = Vec::new();
    // module key -> its files (a part file shares its module's `use`s)
    let mut by_mod: HashMap<String, Vec<&ParsedFile>> = HashMap::new();
    for f in files.iter().filter(|f| is_user(f)) {
        by_mod.entry(f.ast.module_key()).or_default().push(f);
    }
    let mut keys: Vec<&String> = by_mod.keys().collect();
    keys.sort();
    for key in keys {
        let group = &by_mod[key];
        for f in group {
            for item in &f.ast.items {
                match item {
                    Item::Fn(func) => body(&f.path, &func.body, &mut out),
                    Item::Struct(s) => {
                        for m in &s.methods {
                            body(&f.path, &m.body, &mut out);
                        }
                    }
                    _ => {}
                }
            }
        }
        unused_uses(group, &mut out, &mut fixes);
    }
    (out, fixes)
}

fn is_user(f: &ParsedFile) -> bool {
    let key = f.ast.module_key();
    !f.ast.is_script && key != "j3" && !key.starts_with("j3.") && !f.path.ends_with(".generated")
}

fn warn(path: &str, span: Span, msg: String) -> Diagnostic {
    Diagnostic::warning(path, span, msg)
}

/// Unused locals and unreachable statements of one fn body.
fn body(path: &str, block: &Block, out: &mut Vec<Diagnostic>) {
    let mut used = HashSet::new();
    crate::parse::walk::names_in_block(block, &mut |id| {
        used.insert(id.name.clone());
    });
    let mut lets = Vec::new();
    walk_blocks(block, &mut |b| {
        for s in &b.stmts {
            if let Stmt::Let(l) = s {
                lets.push((l.name.name.clone(), l.name.span));
            }
        }
        unreachable(path, b, out);
    });
    for (name, span) in lets {
        if !name.starts_with('_') && !used.contains(&name) {
            out.push(
                warn(path, span, format!("unused local `{name}`"))
                    .help(format!("remove it, or name it `_{name}` to keep it")),
            );
        }
    }
}

fn unreachable(path: &str, b: &Block, out: &mut Vec<Diagnostic>) {
    let mut iter = b.stmts.iter();
    while let Some(s) = iter.next() {
        let what = match s {
            Stmt::Return(_) => "return",
            Stmt::Break(..) => "break",
            Stmt::Continue(..) => "continue",
            _ => continue,
        };
        if let Some(next) = iter.next() {
            out.push(warn(path, stmt_span(next), format!("unreachable code after `{what}`")));
        }
        return;
    }
}

/// Calls `f` on `block` and every block nested in its statements (closures excluded).
fn walk_blocks(block: &Block, f: &mut dyn FnMut(&Block)) {
    f(block);
    for s in &block.stmts {
        walk_stmt(s, f);
    }
}

fn walk_stmt(s: &Stmt, f: &mut dyn FnMut(&Block)) {
    match s {
        Stmt::If(s) => {
            walk_blocks(&s.then_block, f);
            if let Some(b) = &s.else_block {
                walk_blocks(b, f);
            }
        }
        Stmt::While(w) => walk_blocks(&w.body, f),
        Stmt::For(x) => walk_blocks(&x.body, f),
        Stmt::ForEach(x) => walk_blocks(&x.body, f),
        Stmt::Match(m) => {
            for a in &m.arms {
                walk_blocks(&a.body, f);
            }
        }
        Stmt::Defer(d) => walk_blocks(&d.body, f),
        Stmt::Labeled(l) => walk_stmt(&l.body, f),
        Stmt::Block(b) => walk_blocks(b, f),
        _ => {}
    }
}

fn stmt_span(s: &Stmt) -> Span {
    match s {
        Stmt::Let(x) => x.span,
        Stmt::Return(x) => x.span,
        Stmt::Assign(x) => x.span,
        Stmt::AssignOp(x) => x.span,
        Stmt::Expr(e) => e.span,
        Stmt::If(x) => x.span,
        Stmt::While(x) => x.span,
        Stmt::For(x) => x.span,
        Stmt::ForEach(x) => x.span,
        Stmt::UseLocal(x) => x.span,
        Stmt::Match(x) => x.span,
        Stmt::Defer(x) => x.span,
        Stmt::Labeled(x) => x.span,
        Stmt::Break(s, _) | Stmt::Continue(s, _) => *s,
        Stmt::Block(b) => b.span,
    }
}

/// A `use`d name that no token of the module mentions (outside `use` items), nor any string.
fn unused_uses(group: &[&ParsedFile], out: &mut Vec<Diagnostic>, fixes: &mut Vec<Fix>) {
    let mut idents: HashSet<String> = HashSet::new();
    let mut strings = String::new();
    for f in group {
        let Ok(toks) = crate::parse::lex::lex(&f.src, f.file_id) else { return };
        let mut in_use = false;
        for t in &toks {
            match t.kind {
                TokenKind::Use => in_use = true,
                TokenKind::Semi if in_use => in_use = false,
                TokenKind::Ident if !in_use => {
                    idents.insert(t.text.clone());
                }
                TokenKind::String | TokenKind::Interpolate => {
                    strings.push_str(&t.text);
                    strings.push('\n');
                }
                _ => {}
            }
        }
    }
    for f in group {
        for item in &f.ast.items {
            let Item::Use(u) = item else { continue };
            let names: Vec<&crate::parse::Ident> = match &u.names {
                Some(ns) => ns.iter().collect(),
                None => u.path.last().into_iter().collect(),
            };
            let total = names.len();
            let (unused, kept): (Vec<&crate::parse::Ident>, Vec<&crate::parse::Ident>) =
                names.into_iter().partition(|n| !idents.contains(&n.name) && !strings.contains(n.name.as_str()));
            if !unused.is_empty() {
                fixes.push(use_fix(f, u, &kept, total));
            }
            // one warning per `use` line, however many of its names are unused
            match unused.as_slice() {
                [] => {}
                [n] => out.push(warn(&f.path, n.span, format!("unused import `{}`", n.name)).help("remove it from the `use`")),
                [first, .., last] => {
                    let list = unused.iter().map(|n| format!("`{}`", n.name)).collect::<Vec<_>>().join(", ");
                    let span = Span { file: first.span.file, start: first.span.start, end: last.span.end };
                    out.push(warn(&f.path, span, format!("unused imports {list}")).help("remove them from the `use`"));
                }
            }
        }
    }
}

/// The edit that drops the unused names of `u`: the whole line when none is kept, else the
/// `use` rewritten with the kept names (`use m.{ a, b };`, or `use m.a;` for one).
fn use_fix(f: &ParsedFile, u: &crate::parse::UseItem, kept: &[&crate::parse::Ident], total: usize) -> Fix {
    let (mut start, mut end) = (u.span.start as usize, u.span.end as usize);
    let src = f.src.as_bytes();
    if kept.is_empty() || total == 0 {
        // the whole line, when the `use` is alone on it
        let ls = f.src[..start].rfind('\n').map_or(0, |i| i + 1);
        let le = f.src[end..].find('\n').map_or(f.src.len(), |i| end + i + 1);
        if f.src[ls..start].trim().is_empty() && f.src[end..le].trim().is_empty() {
            start = ls;
            end = le;
        } else {
            while end < src.len() && src[end] == b' ' {
                end += 1;
            }
        }
        return Fix { path: f.path.clone(), start, end, text: String::new() };
    }
    let path = u.path.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(".");
    let text = match kept {
        [one] => format!("use {path}.{};", one.name),
        _ => format!("use {path}.{{ {} }};", kept.iter().map(|n| n.name.as_str()).collect::<Vec<_>>().join(", ")),
    };
    Fix { path: f.path.clone(), start, end, text }
}

/// `fixes` applied to `src` (all for one file; they do not overlap).
pub fn apply_fixes(src: &str, fixes: &[&Fix]) -> String {
    let mut sorted: Vec<&&Fix> = fixes.iter().collect();
    sorted.sort_by_key(|f| std::cmp::Reverse(f.start));
    let mut out = src.to_string();
    for f in sorted {
        if f.end <= out.len() && f.start <= f.end {
            out.replace_range(f.start..f.end, &f.text);
        }
    }
    out
}
