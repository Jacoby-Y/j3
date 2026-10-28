//! Scoped completion, signature help, outline, and import quick-fixes.
//!
//! Names come from the typed IR. `use` paths come from [`crate::check::Program::exports`].

use crate::check::{Export, ExportKind, Program};
use crate::ir::body::LocalKind;
use crate::ir::cx::IrCx;
use crate::ir::dump;
use crate::ir::entity::{Func, FuncKind, Vis};
use crate::ir::ids::StructId;
use crate::ir::operand::{ConstKind, Operand};
use crate::ir::ty::{Ty, TyKind};
use crate::parse::syntax::{ATTRIBUTES, BUILTIN_TYPES, INTRINSICS};
use crate::span::{FileId, Span};

use super::query;

mod items;
mod scan;
mod script;
mod uses;

use items::*;
use scan::*;
use script::*;
pub use uses::*;

pub struct Completion {
    pub label: String,
    pub kind: i32,
    pub detail: String,
    pub insert: String,
    pub snippet: bool,
    pub documentation: String,
    pub sort: String,
    pub commit_dot: bool,
}

pub struct SignatureHelp {
    pub signatures: Vec<Signature>,
    pub active: u32,
}

pub struct Signature {
    pub label: String,
    pub params: Vec<String>,
    pub documentation: String,
}

pub struct Symbol {
    pub name: String,
    pub kind: i32,
    pub container: String,
    pub span: Span,
}

pub struct ImportEdit {
    pub title: String,
    pub new_text: String,
    pub offset: u32,
}

#[cfg(test)]
pub fn complete(prog: &Program, path: &str, offset: u32) -> Vec<Completion> {
    let src = file_src(prog, path);
    complete_at(prog, path, &src, offset)
}

/// Members of `base` for `base.prefix`, looked up from `offset` in `path` (the `.j3ui` bridge).
pub(super) fn member_items(prog: &Program, path: &str, offset: u32, base: &str, prefix: &str) -> Vec<Completion> {
    match query::file_by_path(&prog.cx, path) {
        Some(f) => members(prog, f.id, offset, base, prefix, true),
        None => Vec::new(),
    }
}

pub fn complete_at(prog: &Program, path: &str, src: &str, offset: u32) -> Vec<Completion> {
    complete_with(prog, path, src, src, offset)
}

/// Completion for an editor buffer. A half-typed line like `app.` does not
/// parse, so the typed program would otherwise be the last good file on disk
/// and member completion would miss locals. Blank that line (same length, so
/// spans still match) and typecheck again.
pub fn complete_buffer(
    path: &std::path::Path,
    overlays: &[(std::path::PathBuf, String)],
    src: &str,
    offset: u32,
) -> Vec<Completion> {
    if let Some(prefix) = script_path_prefix(src, offset as usize) {
        return script_path_items(path, &prefix);
    }
    let shown = path.display().to_string();
    if let Ok(prog) = query::analyze(path, overlays) {
        return complete_at(&prog, &shown, src, offset);
    }
    let healed = blank_line(src, offset);
    if healed != src {
        let mut next = overlays.to_vec();
        set_overlay(&mut next, path, healed.clone());
        if let Ok(prog) = query::analyze(path, &next) {
            return complete_with(&prog, &shown, src, &healed, offset);
        }
    }
    if let Ok(prog) = query::analyze(path, &[]) {
        return complete_at(&prog, &shown, src, offset);
    }
    Vec::new()
}

fn complete_with(
    prog: &Program,
    path: &str,
    lex: &str,
    spanned: &str,
    offset: u32,
) -> Vec<Completion> {
    if let Some(prefix) = script_path_prefix(lex, offset as usize) {
        return script_path_items(std::path::Path::new(path), &prefix);
    }
    if lexical_mode(lex, offset as usize) != Mode::Code {
        return Vec::new();
    }
    if let Some(prefix) = attr_prefix(lex, offset as usize) {
        return keyed(&ATTRIBUTES, &prefix, 14, "attribute", "4");
    }
    let (prefix, prefix_at, base) = prefix_of(lex, offset as usize);
    if prefix_at > 0 && lex.as_bytes()[prefix_at - 1] == b'@' {
        return keyed(&INTRINSICS, &prefix, 3, "intrinsic", "4");
    }
    if let Some(use_at) = use_at(lex, offset as usize) {
        if use_at.lib {
            return Vec::new();
        }
        return complete_use(prog, &use_at);
    }
    if let Some(items) = script_prop_items(prog, path, lex, prefix_at, &prefix) {
        return items;
    }
    let file = query::file_by_path(&prog.cx, path);
    let trusted = file
        .map(|f| f.src == spanned && lex.len() == spanned.len())
        .unwrap_or(false);
    if let Some(base) = base {
        if let Some(file) = file {
            return members(prog, file.id, offset, &base, &prefix, trusted);
        }
        return Vec::new();
    }
    let mut out = Vec::new();
    if trusted {
        if let Some(file) = file {
            push_locals(&prog.cx, file.id, offset, &prefix, &mut out);
            push_script_names(&prog.cx, file.id, &prefix, &mut out);
            push_file_items(prog, file.id, &prefix, &mut out);
        }
    }
    push_imports(prog, lex, &prefix, &mut out);
    push_builtins(&prefix, &mut out);
    push_keywords(&prefix, &mut out);
    finish(&prefix, out)
}

fn set_overlay(overlays: &mut Vec<(std::path::PathBuf, String)>, path: &std::path::Path, src: String) {
    let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if let Some(slot) = overlays
        .iter_mut()
        .find(|(p, _)| p == path || p.canonicalize().unwrap_or_else(|_| p.clone()) == canon)
    {
        slot.1 = src;
    } else {
        overlays.push((path.to_path_buf(), src));
    }
}

pub fn signature(prog: &Program, path: &str, src: &str, offset: u32) -> Option<SignatureHelp> {
    if lexical_mode(src, offset as usize) != Mode::Code {
        return None;
    }
    let call = find_call(src, offset as usize)?;
    let file = query::file_by_path(&prog.cx, path);
    let trusted = file.map(|f| f.src == src).unwrap_or(false);
    let mut found: Vec<(&Func, bool)> = Vec::new();
    if let Some(recv) = &call.receiver {
        if trusted {
            if let Some(file) = file {
                if let Some(local) = query::find_local(&prog.cx, file.id, offset, recv) {
                    if let Some(id) = struct_of(&prog.cx, local.ty) {
                        push_methods(&prog.cx, id, &call.name, file.id, &mut found);
                    }
                }
            }
        }
        if found.is_empty() {
            push_methods_named(&prog.cx, recv, &call.name, file.map(|f| f.id), &mut found);
        }
        if found.is_empty() {
            push_module_fns(prog, recv, &call.name, &mut found);
        }
    }
    if found.is_empty() {
        push_free_fns(prog, path, src, &call.name, &mut found);
    }
    if found.is_empty() {
        return None;
    }
    let mut signatures = Vec::new();
    for (func, method) in found {
        let (mut params, ret) = params_of(&prog.cx, func);
        if method {
            if params.first().is_some_and(|p| p.starts_with("this:")) {
                params.remove(0);
            }
        }
        let name = prog.cx.name_str(func.name);
        let label = sig_label(name, &params, &ret);
        let documentation = if func.doc.is_empty() {
            query::module_path(&prog.cx, &func.module)
        } else {
            func.doc.clone()
        };
        if signatures.iter().any(|s: &Signature| s.label == label) {
            continue;
        }
        signatures.push(Signature {
            label,
            params,
            documentation,
        });
        if signatures.len() == 8 {
            break;
        }
    }
    let n = signatures.first().map(|s| s.params.len()).unwrap_or(0) as u32;
    let active = if n == 0 { 0 } else { call.commas.min(n - 1) };
    Some(SignatureHelp { signatures, active })
}

pub fn symbols(prog: &Program, path: &str) -> Vec<Symbol> {
    let Some(file) = query::file_by_path(&prog.cx, path) else {
        return Vec::new();
    };
    if file.path.ends_with(".j3s") {
        let module = ["script".to_string(), "outline".to_string()];
        let Ok(ast) = crate::parse::parse_script(&file.src, file.id, &module) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for item in ast.items {
            match item {
                crate::parse::Item::Props(b) | crate::parse::Item::Shared(b) => {
                    let name = if file
                        .src
                        .get(b.span.start as usize..)
                        .is_some_and(|s| s.starts_with("props"))
                    {
                        "props"
                    } else {
                        "shared"
                    };
                    out.push(Symbol {
                        name: name.to_string(),
                        kind: 3,
                        container: String::new(),
                        span: b.span,
                    });
                    for field in b.fields {
                        out.push(Symbol {
                            name: field.name.name,
                            kind: 8,
                            container: name.to_string(),
                            span: field.span,
                        });
                    }
                }
                crate::parse::Item::State(s) => out.push(Symbol {
                    name: s.name.name,
                    kind: 8,
                    container: "state".to_string(),
                    span: s.span,
                }),
                crate::parse::Item::Fn(f) => out.push(Symbol {
                    name: f.name.name,
                    kind: 12,
                    container: String::new(),
                    span: f.span,
                }),
                crate::parse::Item::Const(c) => out.push(Symbol {
                    name: c.name.name,
                    kind: 14,
                    container: String::new(),
                    span: c.span,
                }),
                _ => {}
            }
        }
        return out;
    }
    let cx = &prog.cx;
    let mut out = Vec::new();
    for s in cx.structs() {
        if s.span.file != file.id || s.span.is_dummy() {
            continue;
        }
        let name = cx.name_str(s.name).to_string();
        if !is_ident_name(&name) {
            continue;
        }
        out.push(Symbol {
            name: name.clone(),
            kind: 23,
            container: String::new(),
            span: s.span,
        });
        for f in &s.fields {
            let fname = cx.name_str(f.name);
            if fname.is_empty() {
                continue;
            }
            out.push(Symbol {
                name: fname.to_string(),
                kind: 8,
                container: name.clone(),
                span: s.span,
            });
        }
    }
    for e in cx.enums() {
        if e.span.file != file.id || e.span.is_dummy() {
            continue;
        }
        let name = cx.name_str(e.name).to_string();
        if !is_ident_name(&name) {
            continue;
        }
        out.push(Symbol {
            name: name.clone(),
            kind: 10,
            container: String::new(),
            span: e.span,
        });
        for v in &e.variants {
            out.push(Symbol {
                name: cx.name_str(v.name).to_string(),
                kind: 22,
                container: name.clone(),
                span: e.span,
            });
        }
    }
    for func in cx.funcs() {
        if func.span.file != file.id || func.span.is_dummy() {
            continue;
        }
        let name = cx.name_str(func.name);
        if !is_ident_name(name) {
            continue;
        }
        let (kind, container) = if let Some(id) = func.method_of {
            (6, cx.name_str(cx.struct_def(id).name).to_string())
        } else {
            (12, String::new())
        };
        out.push(Symbol {
            name: name.to_string(),
            kind,
            container,
            span: func.span,
        });
    }
    for g in cx.globals() {
        if g.span.file != file.id || g.span.is_dummy() {
            continue;
        }
        let name = cx.name_str(g.name);
        if !is_ident_name(name) {
            continue;
        }
        out.push(Symbol {
            name: name.to_string(),
            kind: 13,
            container: String::new(),
            span: g.span,
        });
    }
    out.sort_by(|a, b| a.span.start.cmp(&b.span.start).then(a.name.cmp(&b.name)));
    out.dedup_by(|a, b| {
        a.name == b.name
            && a.kind == b.kind
            && a.container == b.container
            && a.span.start == b.span.start
    });
    out
}

fn members(
    prog: &Program,
    file: FileId,
    offset: u32,
    base: &str,
    prefix: &str,
    trusted: bool,
) -> Vec<Completion> {
    let cx = &prog.cx;
    let mut out = Vec::new();
    if trusted {
        if let Some(local) = query::find_local(cx, file, offset, base) {
            if let Some(id) = struct_of(cx, local.ty) {
                push_struct_members(cx, id, file, prefix, &mut out);
                return finish(prefix, out);
            }
        }
    }
    if let Some(s) = query::find_struct(cx, base) {
        if let Some(id) = struct_id(cx, s) {
            push_struct_members(cx, id, file, prefix, &mut out);
            if !out.is_empty() {
                return finish(prefix, out);
            }
        }
    }
    if let Some(e) = query::find_enum(cx, base) {
        for v in &e.variants {
            let name = cx.name_str(v.name);
            if name.starts_with(prefix) {
                out.push(item(name, 20, "variant", "", "2", false));
            }
        }
        if !out.is_empty() {
            return finish(prefix, out);
        }
    }
    for func in cx.funcs() {
        if func.method_of.is_some() || !module_tail(&query::module_path(cx, &func.module), base) {
            continue;
        }
        if func.vis != Vis::Pub && func.span.file != file {
            continue;
        }
        let name = cx.name_str(func.name);
        if !is_ident_name(name) || !name.starts_with(prefix) {
            continue;
        }
        let doc = if func.doc.is_empty() {
            query::module_path(cx, &func.module)
        } else {
            func.doc.clone()
        };
        out.push(item(name, 3, &dump::ty_string(cx, func.ty), &doc, "2", false));
    }
    for exp in &prog.exports {
        if exp.kind != ExportKind::Struct && exp.kind != ExportKind::Enum {
            continue;
        }
        if !module_tail(&exp.module, base) || !exp.name.starts_with(prefix) {
            continue;
        }
        let detail = match exp.kind {
            ExportKind::Struct => "struct",
            ExportKind::Enum => "enum",
            _ => "",
        };
        let doc = if exp.doc.is_empty() { exp.module.clone() } else { exp.doc.clone() };
        out.push(item(&exp.name, if exp.kind == ExportKind::Enum { 13 } else { 22 }, detail, &doc, "2", false));
    }
    finish(prefix, out)
}

#[cfg(test)]
fn file_src(prog: &Program, path: &str) -> String {
    query::file_by_path(&prog.cx, path)
        .map(|f| f.src.clone())
        .unwrap_or_default()
}

fn module_keys(prog: &Program) -> Vec<String> {
    let mut keys: Vec<String> = prog
        .modules
        .iter()
        .map(|m| query::module_path(&prog.cx, &m.name))
        .filter(|s| !s.is_empty())
        .collect();
    keys.sort();
    keys.dedup();
    keys
}

fn child_segments(modules: &[String], parent: &[String]) -> Vec<String> {
    let prefix = if parent.is_empty() {
        String::new()
    } else {
        format!("{}.", parent.join("."))
    };
    let mut out = Vec::new();
    for m in modules {
        let rest = if parent.is_empty() {
            m.as_str()
        } else if let Some(r) = m.strip_prefix(&prefix) {
            r
        } else {
            continue;
        };
        let seg = rest.split('.').next().unwrap_or("");
        if !seg.is_empty() && !out.iter().any(|s: &String| s == seg) {
            out.push(seg.to_string());
        }
    }
    out
}

fn module_tail(module: &str, base: &str) -> bool {
    module == base || module.ends_with(&format!(".{base}"))
}

fn parent_doc(path: &[String], seg: &str) -> String {
    if path.is_empty() {
        seg.to_string()
    } else {
        format!("{}.{}", path.join("."), seg)
    }
}

fn struct_of(cx: &IrCx, mut ty: Ty) -> Option<StructId> {
    loop {
        match cx.ty_kind(ty) {
            TyKind::Struct(id) => return Some(*id),
            TyKind::Ref(inner) => ty = *inner,
            _ => return None,
        }
    }
}

fn struct_id(cx: &IrCx, def: &crate::ir::entity::StructDef) -> Option<StructId> {
    cx.structs()
        .iter()
        .position(|s| std::ptr::eq(s, def))
        .map(|i| StructId(i as u32))
}

fn func_at(cx: &IrCx, file: FileId, offset: u32) -> Option<&Func> {
    cx.funcs()
        .iter()
        .rev()
        .find(|func| func.span.file == file && offset >= func.span.start && offset <= func.span.end)
}

fn unknown_name(msg: &str) -> Option<&str> {
    let line = msg.lines().next().unwrap_or(msg);
    let rest = line.strip_prefix("unknown ")?;
    let (_, rest) = rest.split_once(' ')?;
    let rest = rest.trim().strip_prefix('`')?;
    let name = rest.split('`').next()?;
    if name.is_empty() { None } else { Some(name) }
}

#[cfg(test)]
mod tests;
