//! Hover / goto-def / completion / diagnostics from the same typed IR `j3c` uses.

use crate::check::Program;
use crate::compile;
use crate::diag;
use crate::fmt;
use crate::ir::body::LocalKind;
use crate::ir::cx::IrCx;
use crate::ir::dump;
use crate::ir::entity::FuncKind;
use crate::span::{FileId, Span};

use std::path::{Path, PathBuf};

pub struct Hover {
    pub text: String,
    pub span: Span,
}

pub struct Location {
    pub path: String,
    pub span: Span,
}

pub fn analyze(path: &Path, overlays: &[(PathBuf, String)]) -> Result<Program, String> {
    compile::analyze_path(path, overlays)
}

pub fn hover(prog: &Program, path: &str, offset: u32) -> Option<Hover> {
    let file = file_by_path(&prog.cx, path)?;
    let (name, span) = ident_at(&file.src, file.id, offset)?;
    if let Some(local) = find_local(&prog.cx, file.id, offset, &name) {
        let ty = dump::ty_string(&prog.cx, local.ty);
        let kind = match local.kind {
            LocalKind::Param => "param",
            LocalKind::Let => "let",
            LocalKind::Const => "const",
            LocalKind::Tmp => "tmp",
        };
        return Some(Hover {
            text: with_doc(format!("```j3\n{kind} {name}: {ty}\n```"), &local.doc),
            span: local.span,
        });
    }
    if let Some(func) = find_func(&prog.cx, &name, file.id) {
        let mut sig = String::from("```j3\n");
        dump::dump_ty(&prog.cx, func.ty, &mut sig);
        sig.push_str("\n```\n");
        let mod_path = module_path(&prog.cx, &func.module);
        if !mod_path.is_empty() {
            sig.push_str(&format!("*{mod_path}.{name}*"));
        }
        return Some(Hover {
            text: with_doc(sig, &func.doc),
            span,
        });
    }
    if let Some(s) = find_struct(&prog.cx, &name) {
        let mut text = format!("```j3\nstruct {}\n```\n", prog.cx.name_str(s.name));
        text.push_str(&module_path(&prog.cx, &s.module));
        return Some(Hover {
            text: with_doc(text, &s.doc),
            span,
        });
    }
    if let Some(e) = find_enum(&prog.cx, &name) {
        let mut text = format!("```j3\nenum {}\n```\n", prog.cx.name_str(e.name));
        text.push_str(&module_path(&prog.cx, &e.module));
        return Some(Hover {
            text: with_doc(text, &e.doc),
            span,
        });
    }
    if let Some(g) = find_global(&prog.cx, &name) {
        let ty = dump::ty_string(&prog.cx, g.ty);
        return Some(Hover {
            text: with_doc(
                format!("```j3\nlet {}: {ty}\n```", prog.cx.name_str(g.name)),
                &g.doc,
            ),
            span,
        });
    }
    if let Some(c) = find_const(prog, &file.src, &name) {
        return Some(Hover {
            text: with_doc(
                format!("```j3\nconst {name}\n```\n*{}*", c.module),
                &c.doc,
            ),
            span,
        });
    }
    if let Some(field) = find_field(&prog.cx, &name) {
        let ty = dump::ty_string(&prog.cx, field.1);
        let owner = &field.0;
        return Some(Hover {
            text: with_doc(format!("```j3\n{owner}.{name}: {ty}\n```"), &field.2),
            span,
        });
    }
    Some(Hover {
        text: format!("`{name}`"),
        span,
    })
}

pub fn definition(prog: &Program, path: &str, offset: u32) -> Option<Location> {
    let file = file_by_path(&prog.cx, path)?;
    let (name, _) = ident_at(&file.src, file.id, offset)?;
    if let Some(local) = find_local(&prog.cx, file.id, offset, &name) {
        return loc_of(&prog.cx, local.span);
    }
    if let Some(func) = find_func(&prog.cx, &name, file.id) {
        return loc_of(&prog.cx, func.span);
    }
    if let Some(s) = find_struct(&prog.cx, &name) {
        return loc_of(&prog.cx, s.span);
    }
    if let Some(e) = find_enum(&prog.cx, &name) {
        return loc_of(&prog.cx, e.span);
    }
    if let Some(g) = find_global(&prog.cx, &name) {
        return loc_of(&prog.cx, g.span);
    }
    None
}

pub use super::complete::complete;

pub fn format_file(src: &str) -> Result<String, String> {
    fmt::format_src(src)
}

pub fn range_of(src: &str, span: Span) -> ((u32, u32), (u32, u32)) {
    let (sl, sc) = diag::loc(src, span.start);
    let (el, ec) = diag::loc(src, span.end);
    ((sl as u32 - 1, sc as u32 - 1), (el as u32 - 1, ec as u32 - 1))
}

pub fn offset_at(src: &str, line: u32, col: u32) -> u32 {
    if line == 0 {
        return col.min(src.len() as u32);
    }
    let mut cur_line = 0u32;
    let mut i = 0u32;
    for ch in src.chars() {
        i += ch.len_utf8() as u32;
        if ch == '\n' {
            cur_line += 1;
            if cur_line == line {
                return (i + col).min(src.len() as u32);
            }
        }
    }
    src.len() as u32
}

pub(super) fn file_by_path<'a>(cx: &'a IrCx, path: &str) -> Option<&'a crate::ir::cx::SourceFile> {
    cx.files.iter().find(|f| paths_eq(&f.path, path))
}

fn paths_eq(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let pa = Path::new(a);
    let pb = Path::new(b);
    match (pa.canonicalize(), pb.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

fn ident_at(src: &str, file: FileId, offset: u32) -> Option<(String, Span)> {
    let bytes = src.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let mut i = (offset as usize).min(bytes.len().saturating_sub(1));
    if !is_ident_byte(bytes[i]) {
        if i > 0 && is_ident_byte(bytes[i - 1]) {
            i -= 1;
        } else {
            return None;
        }
    }
    let mut lo = i;
    while lo > 0 && is_ident_byte(bytes[lo - 1]) {
        lo -= 1;
    }
    let mut hi = i + 1;
    while hi < bytes.len() && is_ident_byte(bytes[hi]) {
        hi += 1;
    }
    let name = src.get(lo..hi)?.to_string();
    if name.is_empty() {
        return None;
    }
    Some((
        name,
        Span {
            file,
            start: lo as u32,
            end: hi as u32,
        },
    ))
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

pub(super) fn find_local<'a>(
    cx: &'a IrCx,
    file: FileId,
    offset: u32,
    name: &str,
) -> Option<&'a crate::ir::body::LocalDecl> {
    let mut best = None;
    for func in cx.funcs() {
        if func.span.file != file {
            continue;
        }
        if offset < func.span.start || offset > func.span.end {
            continue;
        }
        let FuncKind::J3 { body } = &func.kind else {
            continue;
        };
        for loc in &body.locals {
            if loc.kind == LocalKind::Tmp {
                continue;
            }
            if cx.name_str(loc.name) == name {
                best = Some(loc);
            }
        }
    }
    best
}

fn find_func<'a>(cx: &'a IrCx, name: &str, prefer: FileId) -> Option<&'a crate::ir::entity::Func> {
    let mut fallback = None;
    for func in cx.funcs() {
        if cx.name_str(func.name) != name {
            continue;
        }
        if func.span.file == prefer {
            return Some(func);
        }
        if fallback.is_none() {
            fallback = Some(func);
        }
    }
    fallback
}

pub(super) fn find_struct<'a>(cx: &'a IrCx, name: &str) -> Option<&'a crate::ir::entity::StructDef> {
    cx.structs().iter().find(|s| cx.name_str(s.name) == name)
}

pub(super) fn find_enum<'a>(cx: &'a IrCx, name: &str) -> Option<&'a crate::ir::entity::EnumDef> {
    cx.enums().iter().find(|e| cx.name_str(e.name) == name)
}

fn find_global<'a>(cx: &'a IrCx, name: &str) -> Option<&'a crate::ir::entity::GlobalDef> {
    cx.globals().iter().find(|g| cx.name_str(g.name) == name)
}

fn find_field(cx: &IrCx, name: &str) -> Option<(String, crate::ir::ty::Ty, String)> {
    for s in cx.structs() {
        for f in &s.fields {
            if cx.name_str(f.name) == name {
                return Some((cx.name_str(s.name).to_string(), f.ty, f.doc.clone()));
            }
        }
    }
    None
}

fn find_const<'a>(prog: &'a Program, src: &str, name: &str) -> Option<&'a crate::check::Export> {
    let hits: Vec<_> = prog
        .const_docs
        .iter()
        .filter(|c| c.name == name)
        .collect();
    if hits.is_empty() {
        return None;
    }
    hits.into_iter()
        .find(|c| src.contains(&c.module))
        .or_else(|| prog.const_docs.iter().find(|c| c.name == name))
}

fn with_doc(mut text: String, doc: &str) -> String {
    if doc.is_empty() {
        return text;
    }
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text.push('\n');
    text.push_str(doc);
    text
}

pub(super) fn module_path(cx: &IrCx, parts: &[crate::intern::Name]) -> String {
    let mut s = String::new();
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            s.push('.');
        }
        s.push_str(cx.name_str(*p));
    }
    s
}

fn loc_of(cx: &IrCx, span: Span) -> Option<Location> {
    if span.is_dummy() {
        return None;
    }
    let path = cx.file(span.file)?.path.clone();
    Some(Location { path, span })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::default_hello_path;
    use std::path::PathBuf;

    #[test]
    fn hover_print_ln_on_hello() {
        let path = default_hello_path();
        let prog = analyze(&path, &[]).expect("analyze hello");
        let src = std::fs::read_to_string(&path).unwrap();
        let off = src.find("printLn").expect("printLn") as u32;
        let h = hover(&prog, &path.display().to_string(), off).expect("hover");
        assert!(h.text.contains("fn") || h.text.contains("printLn"), "{}", h.text);
        let d = definition(&prog, &path.display().to_string(), off).expect("def");
        assert!(d.path.contains("io.j3"), "{}", d.path);
        let items = complete(&prog, &path.display().to_string(), off + 5);
        assert!(
            items.iter().any(|c| c.label == "printLn"),
            "{:?}",
            items.iter().map(|c| c.label.clone()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doc_comment_shows_on_hover_and_completion() {
        let path = default_hello_path();
        let src = "\
module hello;

/// Prints a line.
fn greet(name: string) {
    let _ = name;
}

fn main() {
    /// the player
    let player = 1;
    greet(\"a\");
    let _ = player;
}
";
        let overlay = vec![(path.clone(), src.to_string())];
        let prog = analyze(&path, &overlay).expect("analyze");
        let shown = path.display().to_string();
        let greet = src.find("greet(\"a\")").unwrap() as u32;
        let h = hover(&prog, &shown, greet).expect("hover");
        assert!(h.text.contains("Prints a line."), "{}", h.text);
        let at = src.find("greet(\"a\")").unwrap();
        let items = complete(&prog, &shown, at as u32 + 2);
        let greet_item = items.iter().find(|c| c.label == "greet").expect("greet");
        assert!(greet_item.documentation.contains("Prints a line."), "{}", greet_item.documentation);
        let player = src.rfind("player").unwrap() as u32;
        let hp = hover(&prog, &shown, player).expect("player");
        assert!(hp.text.contains("the player"), "{}", hp.text);
    }

    #[test]
    fn const_doc_shows_on_hover() {
        let path = default_hello_path();
        let src = "\
module hello;

use j3.math.PI;

fn main() {
    let x = PI;
    let _ = x;
}
";
        let overlay = vec![(path.clone(), src.to_string())];
        let prog = analyze(&path, &overlay).expect("analyze");
        let at = src.find("PI;").unwrap() as u32;
        let h = hover(&prog, &path.display().to_string(), at).expect("hover");
        assert!(h.text.contains("circumference"), "{}", h.text);
    }

    #[test]
    fn complete_quat_rotate() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/mesh.j3");
        let prog = analyze(&path, &[]).expect("analyze mesh");
        let src = std::fs::read_to_string(&path).unwrap();
        let at = src.find("q.rotate").expect("q.rotate") as u32;
        let items = complete(&prog, &path.display().to_string(), at + 2);
        assert!(
            items.iter().any(|c| c.label == "rotate"),
            "{:?}",
            items.iter().map(|c| c.label.clone()).collect::<Vec<_>>()
        );
    }
}
