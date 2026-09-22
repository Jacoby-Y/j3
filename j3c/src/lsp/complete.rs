//! Scoped completion, signature help, outline, and import quick-fixes.
//!
//! Names come from the typed IR. `use` paths come from [`crate::check::Program::exports`].

use crate::check::{Export, ExportKind, Program};
use crate::ir::body::LocalKind;
use crate::ir::cx::IrCx;
use crate::ir::dump;
use crate::ir::entity::{Func, FuncKind, Vis};
use crate::ir::ids::StructId;
use crate::ir::ty::{Ty, TyKind};
use crate::parse::syntax::{ATTRIBUTES, BUILTIN_TYPES, INTRINSICS};
use crate::span::{FileId, Span};

use super::query;

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

pub fn complete(prog: &Program, path: &str, offset: u32) -> Vec<Completion> {
    let src = file_src(prog, path);
    complete_at(prog, path, &src, offset)
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
            push_file_items(prog, file.id, &prefix, &mut out);
        }
    }
    push_imports(prog, lex, &prefix, &mut out);
    push_builtins(&prefix, &mut out);
    push_keywords(&prefix, &mut out);
    finish(&prefix, out)
}

fn blank_line(src: &str, offset: u32) -> String {
    let offset = (offset as usize).min(src.len());
    let start = src[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let end = src[offset..]
        .find('\n')
        .map(|i| offset + i)
        .unwrap_or(src.len());
    let mut bytes = src.as_bytes().to_vec();
    for b in &mut bytes[start..end] {
        if *b != b'\n' && *b != b'\r' {
            *b = b' ';
        }
    }
    String::from_utf8(bytes).unwrap_or_else(|_| src.to_string())
}

fn set_overlay(overlays: &mut Vec<(std::path::PathBuf, String)>, path: &std::path::Path, src: String) {
    let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if let Some(slot) = overlays.iter_mut().find(|(p, _)| {
        p == path || p.canonicalize().unwrap_or_else(|_| p.clone()) == canon
    }) {
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
                        push_methods(&prog.cx, id, &call.name, &mut found);
                    }
                }
            }
        }
        if found.is_empty() {
            push_methods_named(&prog.cx, recv, &call.name, &mut found);
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
    out.dedup_by(|a, b| a.name == b.name && a.kind == b.kind && a.container == b.container && a.span.start == b.span.start);
    out
}

pub fn import_edits(prog: &Program, src: &str, messages: &[String]) -> Vec<ImportEdit> {
    let mut out = Vec::new();
    let uses = uses_of(prog, src);
    for msg in messages {
        let Some(name) = unknown_name(msg) else {
            continue;
        };
        if !is_ident_name(name) || BUILTIN_TYPES.contains(&name) {
            continue;
        }
        let mut hits: Vec<&Export> = prog.exports.iter().filter(|e| e.name == name).collect();
        hits.sort_by(|a, b| a.module.cmp(&b.module));
        hits.dedup_by(|a, b| a.module == b.module && a.name == b.name);
        for exp in hits {
            if uses.imports.iter().any(|(m, n)| m == &exp.module && n == &exp.name) {
                continue;
            }
            let title = format!("Import {name} from {}", exp.module);
            if out.iter().any(|e: &ImportEdit| e.title == title) {
                continue;
            }
            let anchor = import_anchor(src);
            let mut new_text = format!("use {}.{name};\n", exp.module);
            if anchor > 0 && !src[..anchor].ends_with('\n') {
                new_text.insert(0, '\n');
            }
            out.push(ImportEdit {
                title,
                new_text,
                offset: anchor as u32,
            });
        }
    }
    out
}

fn complete_use(prog: &Program, use_at: &UseAt) -> Vec<Completion> {
    let modules = module_keys(prog);
    let mut out = Vec::new();
    if use_at.brace {
        push_exports(prog, &use_at.path, &use_at.prefix, false, &mut out);
        return finish(&use_at.prefix, out);
    }
    for seg in child_segments(&modules, &use_at.path) {
        if !seg.starts_with(&use_at.prefix) {
            continue;
        }
        out.push(item(&seg, 9, "module", &parent_doc(&use_at.path, &seg), "5", true));
    }
    push_exports(prog, &use_at.path, &use_at.prefix, false, &mut out);
    finish(&use_at.prefix, out)
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
                push_struct_members(cx, id, prefix, &mut out);
                return finish(prefix, out);
            }
        }
    }
    if let Some(s) = query::find_struct(cx, base) {
        if let Some(id) = struct_id(cx, s) {
            push_struct_members(cx, id, prefix, &mut out);
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

fn push_locals(cx: &IrCx, file: FileId, offset: u32, prefix: &str, out: &mut Vec<Completion>) {
    let Some(func) = func_at(cx, file, offset) else {
        return;
    };
    let FuncKind::J3 { body } = &func.kind else {
        return;
    };
    for loc in &body.locals {
        if loc.kind == LocalKind::Tmp {
            continue;
        }
        let name = cx.name_str(loc.name);
        if !is_ident_name(name) || !name.starts_with(prefix) {
            continue;
        }
        let kind = if loc.kind == LocalKind::Const { 21 } else { 6 };
        out.push(item(name, kind, &dump::ty_string(cx, loc.ty), &loc.doc, "0", false));
    }
}

fn push_file_items(prog: &Program, file: FileId, prefix: &str, out: &mut Vec<Completion>) {
    let cx = &prog.cx;
    for func in cx.funcs() {
        if func.span.file != file || func.method_of.is_some() {
            continue;
        }
        let name = cx.name_str(func.name);
        if !is_ident_name(name) || !name.starts_with(prefix) {
            continue;
        }
        out.push(item(name, 3, &dump::ty_string(cx, func.ty), &func.doc, "1", false));
    }
    for s in cx.structs() {
        if s.span.file != file {
            continue;
        }
        let name = cx.name_str(s.name);
        if is_ident_name(name) && name.starts_with(prefix) {
            out.push(item(name, 22, "struct", &s.doc, "1", false));
        }
    }
    for e in cx.enums() {
        if e.span.file != file {
            continue;
        }
        let name = cx.name_str(e.name);
        if is_ident_name(name) && name.starts_with(prefix) {
            out.push(item(name, 13, "enum", &e.doc, "1", false));
        }
    }
    for g in cx.globals() {
        if g.span.file != file {
            continue;
        }
        let name = cx.name_str(g.name);
        if is_ident_name(name) && name.starts_with(prefix) {
            out.push(item(name, 6, &dump::ty_string(cx, g.ty), &g.doc, "1", false));
        }
    }
}

fn push_imports(prog: &Program, src: &str, prefix: &str, out: &mut Vec<Completion>) {
    let uses = uses_of(prog, src);
    for (module, name) in &uses.imports {
        if !name.starts_with(prefix) {
            continue;
        }
        if let Some(exp) = prog.exports.iter().find(|e| e.module == *module && e.name == *name) {
            let (kind, detail) = export_item(prog, exp);
            out.push(item(name, kind, &detail, &export_doc(prog, exp), "2", false));
        } else {
            out.push(item(name, 3, "", module, "2", false));
        }
    }
    for (alias, module) in &uses.aliases {
        if alias.starts_with(prefix) {
            out.push(item(alias, 9, "module", module, "2", true));
        }
    }
}

fn push_exports(prog: &Program, path: &[String], prefix: &str, commit_dot: bool, out: &mut Vec<Completion>) {
    if path.is_empty() {
        return;
    }
    let module = path.join(".");
    if !module_keys(prog).iter().any(|m| m == &module) {
        return;
    }
    for exp in &prog.exports {
        if exp.module != module || !exp.name.starts_with(prefix) {
            continue;
        }
        let (kind, detail) = export_item(prog, exp);
        let doc = export_doc(prog, exp);
        out.push(Completion {
            label: exp.name.clone(),
            kind,
            detail,
            insert: String::new(),
            snippet: false,
            documentation: doc,
            sort: format!("2_{}", exp.name),
            commit_dot,
        });
    }
}

fn push_builtins(prefix: &str, out: &mut Vec<Completion>) {
    for name in BUILTIN_TYPES {
        if name.starts_with(prefix) {
            out.push(item(name, 22, "type", "", "3", false));
        }
    }
    for name in ["true", "false"] {
        if name.starts_with(prefix) {
            out.push(item(name, 21, "bool", "", "3", false));
        }
    }
}

fn push_keywords(prefix: &str, out: &mut Vec<Completion>) {
    for (label, insert) in KEYWORDS {
        if !label.starts_with(prefix) {
            continue;
        }
        let snippet = insert.contains('$');
        out.push(Completion {
            label: (*label).to_string(),
            kind: if snippet { 15 } else { 14 },
            detail: if snippet { "snippet" } else { "keyword" }.to_string(),
            insert: if snippet { (*insert).to_string() } else { String::new() },
            snippet,
            documentation: String::new(),
            sort: format!("4_{label}"),
            commit_dot: false,
        });
    }
}

fn keyed(names: &[&str], prefix: &str, kind: i32, detail: &str, sort: &str) -> Vec<Completion> {
    let mut out = Vec::new();
    for name in names {
        if name.starts_with(prefix) {
            out.push(item(name, kind, detail, "", sort, false));
        }
    }
    finish(prefix, out)
}

fn export_item(prog: &Program, exp: &Export) -> (i32, String) {
    match exp.kind {
        ExportKind::Fn => {
            let detail = func_in_module(prog, &exp.module, &exp.name)
                .map(|f| dump::ty_string(&prog.cx, f.ty))
                .unwrap_or_else(|| "fn".to_string());
            (3, detail)
        }
        ExportKind::Struct => (22, "struct".to_string()),
        ExportKind::Enum => (13, "enum".to_string()),
        ExportKind::Const => {
            let detail = prog
                .cx
                .globals()
                .iter()
                .find(|g| {
                    prog.cx.name_str(g.name) == exp.name
                        && query::module_path(&prog.cx, &g.module) == exp.module
                })
                .map(|g| dump::ty_string(&prog.cx, g.ty))
                .unwrap_or_else(|| "const".to_string());
            (21, detail)
        }
    }
}

fn export_doc(prog: &Program, exp: &Export) -> String {
    let mut doc = String::new();
    if !exp.doc.is_empty() {
        doc.push_str(&exp.doc);
        doc.push_str("\n\n");
    }
    doc.push('`');
    doc.push_str(&exp.module);
    doc.push('`');
    match exp.kind {
        ExportKind::Struct => {
            if let Some(s) = prog.cx.structs().iter().find(|s| {
                prog.cx.name_str(s.name) == exp.name
                    && query::module_path(&prog.cx, &s.module) == exp.module
            }) {
                for f in s.fields.iter().take(12) {
                    doc.push('\n');
                    doc.push_str(prog.cx.name_str(f.name));
                    doc.push_str(": ");
                    doc.push_str(&dump::ty_string(&prog.cx, f.ty));
                }
            }
        }
        ExportKind::Enum => {
            if let Some(e) = prog.cx.enums().iter().find(|e| {
                prog.cx.name_str(e.name) == exp.name
                    && query::module_path(&prog.cx, &e.module) == exp.module
            }) {
                for v in e.variants.iter().take(12) {
                    doc.push('\n');
                    doc.push_str(prog.cx.name_str(v.name));
                }
            }
        }
        _ => {}
    }
    doc
}

fn func_in_module<'a>(prog: &'a Program, module: &str, name: &str) -> Option<&'a Func> {
    prog.cx.funcs().iter().find(|f| {
        f.method_of.is_none()
            && prog.cx.name_str(f.name) == name
            && query::module_path(&prog.cx, &f.module) == module
    })
}

fn push_struct_members(cx: &IrCx, id: StructId, prefix: &str, out: &mut Vec<Completion>) {
    for field in &cx.struct_def(id).fields {
        let name = cx.name_str(field.name);
        if name.is_empty() || !name.starts_with(prefix) {
            continue;
        }
        out.push(item(name, 5, &dump::ty_string(cx, field.ty), &field.doc, "2", false));
    }
    for func in cx.funcs() {
        if func.method_of != Some(id) {
            continue;
        }
        let name = cx.name_str(func.name);
        if !is_ident_name(name) || !name.starts_with(prefix) {
            continue;
        }
        out.push(item(name, 2, &dump::ty_string(cx, func.ty), &func.doc, "2", false));
    }
}

fn push_methods<'a>(cx: &'a IrCx, id: StructId, name: &str, out: &mut Vec<(&'a Func, bool)>) {
    for func in cx.funcs() {
        if func.method_of == Some(id) && cx.name_str(func.name) == name {
            out.push((func, true));
        }
    }
}

fn push_methods_named<'a>(cx: &'a IrCx, struct_name: &str, method: &str, out: &mut Vec<(&'a Func, bool)>) {
    for s in cx.structs() {
        if cx.name_str(s.name) != struct_name {
            continue;
        }
        let Some(id) = struct_id(cx, s) else {
            continue;
        };
        push_methods(cx, id, method, out);
    }
}

fn push_module_fns<'a>(prog: &'a Program, base: &str, name: &str, out: &mut Vec<(&'a Func, bool)>) {
    for func in prog.cx.funcs() {
        if func.method_of.is_some() || prog.cx.name_str(func.name) != name {
            continue;
        }
        if module_tail(&query::module_path(&prog.cx, &func.module), base) {
            out.push((func, false));
        }
    }
}

fn push_free_fns<'a>(
    prog: &'a Program,
    path: &str,
    src: &str,
    name: &str,
    out: &mut Vec<(&'a Func, bool)>,
) {
    let file = query::file_by_path(&prog.cx, path);
    let uses = uses_of(prog, src);
    for func in prog.cx.funcs() {
        if func.method_of.is_some() || prog.cx.name_str(func.name) != name {
            continue;
        }
        let module = query::module_path(&prog.cx, &func.module);
        let here = file.is_some_and(|f| func.span.file == f.id);
        let imported = uses.imports.iter().any(|(m, n)| m == &module && n == name);
        let exported = func.vis == Vis::Pub && prog.exports.iter().any(|e| e.module == module && e.name == name);
        if here || imported || exported {
            out.push((func, false));
        }
    }
}

fn params_of(cx: &IrCx, func: &Func) -> (Vec<String>, String) {
    let ret = match cx.ty_kind(func.ty) {
        TyKind::Fn { ret, .. } if *ret != cx.common.void => dump::ty_string(cx, *ret),
        _ => String::new(),
    };
    if let FuncKind::J3 { body } = &func.kind {
        let params = body
            .params
            .iter()
            .map(|id| {
                let loc = body.local(*id);
                let name = cx.name_str(loc.name);
                let ty = dump::ty_string(cx, loc.ty);
                if name.is_empty() {
                    ty
                } else {
                    format!("{name}: {ty}")
                }
            })
            .collect();
        return (params, ret);
    }
    if let TyKind::Fn { params, .. } = cx.ty_kind(func.ty) {
        let params = params.iter().map(|t| dump::ty_string(cx, *t)).collect();
        return (params, ret);
    }
    (Vec::new(), ret)
}

fn sig_label(name: &str, params: &[String], ret: &str) -> String {
    let mut s = format!("fn {name}(");
    s.push_str(&params.join(", "));
    s.push(')');
    if !ret.is_empty() {
        s.push_str(": ");
        s.push_str(ret);
    }
    s
}

fn item(label: &str, kind: i32, detail: &str, doc: &str, sort: &str, commit_dot: bool) -> Completion {
    Completion {
        label: label.to_string(),
        kind,
        detail: detail.to_string(),
        insert: String::new(),
        snippet: false,
        documentation: doc.to_string(),
        sort: format!("{sort}_{label}"),
        commit_dot,
    }
}

fn finish(prefix: &str, mut out: Vec<Completion>) -> Vec<Completion> {
    out.sort_by(|a, b| a.sort.cmp(&b.sort).then(a.label.cmp(&b.label)));
    out.dedup_by(|a, b| a.label == b.label && a.kind == b.kind);
    if prefix.is_empty() && out.len() > 200 {
        out.truncate(200);
    }
    out
}

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
    cx.funcs().iter().rev().find(|func| {
        func.span.file == file && offset >= func.span.start && offset <= func.span.end
    })
}

fn unknown_name(msg: &str) -> Option<&str> {
    let line = msg.lines().next().unwrap_or(msg);
    let rest = line.strip_prefix("unknown ")?;
    let (_, rest) = rest.split_once(' ')?;
    let rest = rest.trim().strip_prefix('`')?;
    let name = rest.split('`').next()?;
    if name.is_empty() { None } else { Some(name) }
}

fn import_anchor(src: &str) -> usize {
    let mut last_use = None;
    let mut module_end = None;
    let mut i = 0;
    for line in src.split_inclusive('\n') {
        let t = strip_comments(line).trim().to_string();
        if t.starts_with("module ") || t.starts_with("module\t") {
            module_end = Some(i + line.len());
        }
        if t.starts_with("use ") || t == "use" {
            last_use = Some(i + line.len());
        }
        i += line.len();
    }
    last_use.or(module_end).unwrap_or(0)
}

struct Uses {
    imports: Vec<(String, String)>,
    aliases: Vec<(String, String)>,
}

fn next_semi(src: &str, from: usize) -> usize {
    let b = src.as_bytes();
    let mut i = from;
    let mut braces = 0i32;
    let mut mode = Mode::Code;
    while i < b.len() {
        let c = b[i];
        if mode == Mode::Comment {
            if c == b'\n' {
                mode = Mode::Code;
            }
            i += 1;
            continue;
        }
        if mode == Mode::String {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == b'"' {
                mode = Mode::Code;
            }
            i += 1;
            continue;
        }
        if mode == Mode::Tick {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == b'`' {
                mode = Mode::Code;
            }
            i += 1;
            continue;
        }
        if c == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            mode = Mode::Comment;
            i += 2;
            continue;
        }
        match c {
            b'"' => mode = Mode::String,
            b'`' => mode = Mode::Tick,
            b'{' => braces += 1,
            b'}' => braces -= 1,
            b';' if braces == 0 => return i,
            _ => {}
        }
        i += 1;
    }
    b.len()
}

struct UseAt {
    lib: bool,
    brace: bool,
    path: Vec<String>,
    prefix: String,
}

struct ParsedUse {
    lib: bool,
    brace: bool,
    segs: Vec<String>,
    braced: Vec<String>,
    partial: String,
    ended_on_dot: bool,
}

fn use_at(src: &str, offset: usize) -> Option<UseAt> {
    let start = stmt_start(src, offset);
    let stmt = strip_comments(&src[start..offset.min(src.len())]);
    let parsed = parse_use(stmt.trim())?;
    if parsed.lib {
        return Some(UseAt {
            lib: true,
            brace: false,
            path: Vec::new(),
            prefix: String::new(),
        });
    }
    Some(UseAt {
        lib: false,
        brace: parsed.brace,
        path: parsed.segs,
        prefix: parsed.partial,
    })
}

fn finish_use(mut parsed: ParsedUse) -> ParsedUse {
    if parsed.partial.is_empty() {
        return parsed;
    }
    if parsed.brace {
        parsed.braced.push(std::mem::take(&mut parsed.partial));
    } else if !parsed.ended_on_dot {
        parsed.segs.push(std::mem::take(&mut parsed.partial));
    }
    parsed
}

fn classify(modules: &[String], parsed: ParsedUse, uses: &mut Uses) {
    if parsed.lib {
        return;
    }
    if parsed.brace {
        let module = parsed.segs.join(".");
        if modules.iter().any(|m| m == &module) {
            for name in parsed.braced {
                uses.imports.push((module.clone(), name));
            }
        }
        return;
    }
    if parsed.segs.is_empty() {
        return;
    }
    let full = parsed.segs.join(".");
    if modules.iter().any(|m| m == &full) {
        if let Some(alias) = parsed.segs.last() {
            uses.aliases.push((alias.clone(), full));
        }
        return;
    }
    if parsed.segs.len() >= 2 {
        let module = parsed.segs[..parsed.segs.len() - 1].join(".");
        if modules.iter().any(|m| m == &module) {
            if let Some(name) = parsed.segs.last() {
                uses.imports.push((module, name.clone()));
            }
        }
    }
}

fn parse_use(stmt: &str) -> Option<ParsedUse> {
    let rest = stmt.strip_prefix("use")?;
    if !rest.is_empty() {
        let c = rest.chars().next()?;
        if c.is_ascii_alphanumeric() || c == '_' {
            return None;
        }
    }
    let rest = rest.trim_start();
    if rest == "lib" || rest.starts_with("lib ") || rest.starts_with("lib\t") || rest.starts_with("lib\"") {
        return Some(ParsedUse {
            lib: true,
            brace: false,
            segs: Vec::new(),
            braced: Vec::new(),
            partial: String::new(),
            ended_on_dot: false,
        });
    }
    let b = rest.as_bytes();
    let mut i = 0;
    let mut segs = Vec::new();
    let mut braced = Vec::new();
    let mut cur = String::new();
    let mut brace = false;
    let mut ended_on_dot = false;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_ascii_alphanumeric() || c == '_' {
            cur.push(c);
            ended_on_dot = false;
            i += 1;
            continue;
        }
        if c == '.' {
            if !cur.is_empty() {
                if brace {
                    braced.push(std::mem::take(&mut cur));
                } else {
                    segs.push(std::mem::take(&mut cur));
                }
            }
            ended_on_dot = true;
            i += 1;
            let mut j = i;
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < b.len() && b[j] == b'{' {
                brace = true;
                ended_on_dot = false;
                i = j + 1;
            }
            continue;
        }
        if c == '{' {
            if !cur.is_empty() {
                segs.push(std::mem::take(&mut cur));
            }
            brace = true;
            ended_on_dot = false;
            i += 1;
            continue;
        }
        if c == ',' || c == '}' || c.is_ascii_whitespace() {
            if !cur.is_empty() && brace && (c == ',' || c == '}') {
                braced.push(std::mem::take(&mut cur));
            } else if !cur.is_empty() && !brace && c == ',' {
                segs.push(std::mem::take(&mut cur));
            }
            if c != ' ' && c != '\t' && c != '\n' && c != '\r' {
                ended_on_dot = false;
            }
            i += 1;
            continue;
        }
        i += 1;
    }
    Some(ParsedUse {
        lib: false,
        brace,
        segs,
        braced,
        partial: cur,
        ended_on_dot,
    })
}

fn stmt_start(src: &str, offset: usize) -> usize {
    let b = src.as_bytes();
    let mut i = offset.min(b.len());
    let mut braces = 0i32;
    let mut parens = 0i32;
    let mut brackets = 0i32;
    while i > 0 {
        i -= 1;
        if !src.is_char_boundary(i) {
            continue;
        }
        match b[i] {
            b'}' => braces += 1,
            b')' => parens += 1,
            b']' => brackets += 1,
            b'{' if braces == 0 && parens == 0 && brackets == 0 => {
                if !head_is_use(&src[..i]) {
                    return i + 1;
                }
            }
            b'{' if braces > 0 => braces -= 1,
            b'(' if parens > 0 => parens -= 1,
            b'[' if brackets > 0 => brackets -= 1,
            b';' if braces == 0 && parens == 0 && brackets == 0 => return i + 1,
            _ => {}
        }
    }
    0
}

fn head_is_use(before: &str) -> bool {
    let tail = before.rsplit(';').next().unwrap_or(before);
    let t = strip_comments(tail);
    let t = t.trim();
    let Some(rest) = t.strip_prefix("use") else {
        return false;
    };
    if rest.is_empty() {
        return true;
    }
    let c = rest.chars().next().unwrap();
    if c.is_ascii_alphanumeric() || c == '_' {
        return false;
    }
    let rest = rest.trim_start();
    !(rest == "lib" || rest.starts_with("lib ") || rest.starts_with("lib\t") || rest.starts_with("lib\""))
}

fn uses_of(prog: &Program, src: &str) -> Uses {
    let modules = module_keys(prog);
    let mut uses = Uses {
        imports: Vec::new(),
        aliases: Vec::new(),
    };
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let start = i;
        let end = next_semi(src, i);
        let stmt = strip_comments(&src[start..end.min(b.len())]);
        if let Some(parsed) = parse_use(stmt.trim()) {
            classify(&modules, finish_use(parsed), &mut uses);
        }
        if end >= b.len() {
            break;
        }
        i = end + 1;
    }
    uses
}

struct CallSite {
    name: String,
    receiver: Option<String>,
    commas: u32,
}

fn find_call(src: &str, offset: usize) -> Option<CallSite> {
    let b = src.as_bytes();
    let end = offset.min(b.len());
    let mut stack: Vec<Frame> = Vec::new();
    let mut i = 0;
    let mut mode = Mode::Code;
    while i < end {
        if mode == Mode::Comment {
            if b[i] == b'\n' {
                mode = Mode::Code;
            }
            i += 1;
            continue;
        }
        if mode == Mode::String || mode == Mode::Tick {
            if b[i] == b'\\' {
                i += 2;
                continue;
            }
            let quote = if mode == Mode::String { b'"' } else { b'`' };
            if b[i] == quote {
                mode = Mode::Code;
            }
            i += 1;
            continue;
        }
        if b[i] == b'/' && i + 1 < end && b[i + 1] == b'/' {
            mode = Mode::Comment;
            i += 2;
            continue;
        }
        match b[i] {
            b'"' => mode = Mode::String,
            b'`' => mode = Mode::Tick,
            b'(' => stack.push(Frame { kind: b'(', commas: 0, at: i }),
            b')' => {
                if stack.last().is_some_and(|f| f.kind == b'(') {
                    stack.pop();
                }
            }
            b'[' => stack.push(Frame { kind: b'[', commas: 0, at: i }),
            b']' => {
                if stack.last().is_some_and(|f| f.kind == b'[') {
                    stack.pop();
                }
            }
            b'{' => stack.push(Frame { kind: b'{', commas: 0, at: i }),
            b'}' => {
                if stack.last().is_some_and(|f| f.kind == b'{') {
                    stack.pop();
                }
            }
            b',' => {
                if let Some(f) = stack.last_mut() {
                    if f.kind == b'(' {
                        f.commas += 1;
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    let frame = stack.iter().rev().find(|f| f.kind == b'(')?;
    let (name, name_at) = ident_ending(src, skip_ws_before(src, frame.at))?;
    let receiver = receiver_before(src, name_at);
    Some(CallSite {
        name,
        receiver,
        commas: frame.commas,
    })
}

struct Frame {
    kind: u8,
    commas: u32,
    at: usize,
}

fn receiver_before(src: &str, name_at: usize) -> Option<String> {
    let mut i = skip_ws_before(src, name_at);
    let (dot_at, ch) = prev_char(src, i)?;
    if ch != '.' {
        return None;
    }
    i = skip_ws_before(src, dot_at);
    ident_ending(src, i).map(|(n, _)| n)
}

fn ident_ending(src: &str, end: usize) -> Option<(String, usize)> {
    let mut start = end;
    while let Some((j, c)) = prev_char(src, start) {
        if is_ident_continue(c) {
            start = j;
        } else {
            break;
        }
    }
    if start == end {
        return None;
    }
    let name = src.get(start..end)?;
    let first = name.chars().next()?;
    if !is_ident_start(first) {
        return None;
    }
    Some((name.to_string(), start))
}

fn skip_ws_before(src: &str, mut i: usize) -> usize {
    while let Some((j, c)) = prev_char(src, i) {
        if c.is_whitespace() {
            i = j;
        } else {
            break;
        }
    }
    i
}

fn prefix_of(src: &str, offset: usize) -> (String, usize, Option<String>) {
    let end = offset.min(src.len());
    let mut at = end;
    while let Some((j, c)) = prev_char(src, at) {
        if is_ident_continue(c) {
            at = j;
        } else {
            break;
        }
    }
    if at < end {
        if let Some(c) = src[at..].chars().next() {
            if !is_ident_start(c) {
                at = end;
            }
        }
    }
    let prefix = src.get(at..end).unwrap_or("").to_string();
    let mut j = at;
    loop {
        let Some((k, c)) = prev_char(src, j) else {
            return (prefix, at, None);
        };
        if c.is_whitespace() {
            j = k;
            continue;
        }
        if c != '.' {
            return (prefix, at, None);
        }
        j = skip_ws_before(src, k);
        return match ident_ending(src, j) {
            Some((base, _)) => (prefix, at, Some(base)),
            None => (prefix, at, None),
        };
    }
}

fn attr_prefix(src: &str, offset: usize) -> Option<String> {
    let end = offset.min(src.len());
    let mut at = end;
    while let Some((j, c)) = prev_char(src, at) {
        if is_ident_continue(c) {
            at = j;
        } else {
            break;
        }
    }
    let mut j = at;
    loop {
        let Some((k, c)) = prev_char(src, j) else {
            return None;
        };
        if c.is_whitespace() {
            j = k;
            continue;
        }
        if c != '[' {
            return None;
        }
        let Some((_, h)) = prev_char(src, k) else {
            return None;
        };
        if h != '#' {
            return None;
        }
        return Some(src.get(at..end).unwrap_or("").to_string());
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Code,
    String,
    Tick,
    Comment,
}

fn lexical_mode(src: &str, offset: usize) -> Mode {
    let b = src.as_bytes();
    let end = offset.min(b.len());
    let mut i = 0;
    let mut mode = Mode::Code;
    while i < end {
        if mode == Mode::Comment {
            if b[i] == b'\n' {
                mode = Mode::Code;
            }
            i += 1;
            continue;
        }
        if mode == Mode::String || mode == Mode::Tick {
            if b[i] == b'\\' {
                i += 2;
                continue;
            }
            let quote = if mode == Mode::String { b'"' } else { b'`' };
            if b[i] == quote {
                mode = Mode::Code;
            }
            i += 1;
            continue;
        }
        if b[i] == b'/' && i + 1 < end && b[i + 1] == b'/' {
            mode = Mode::Comment;
            i += 2;
            continue;
        }
        match b[i] {
            b'"' => mode = Mode::String,
            b'`' => mode = Mode::Tick,
            _ => {}
        }
        i += 1;
    }
    mode
}

fn strip_comments(s: &str) -> String {
    let mut out = String::new();
    for line in s.split_inclusive('\n') {
        if let Some(i) = comment_at(line) {
            out.push_str(&line[..i]);
            if line.ends_with('\n') {
                out.push('\n');
            }
        } else {
            out.push_str(line);
        }
    }
    out
}

fn comment_at(line: &str) -> Option<usize> {
    let b = line.as_bytes();
    let mut i = 0;
    let mut string = false;
    let mut tick = false;
    while i < b.len() {
        if string {
            if b[i] == b'\\' {
                i += 2;
                continue;
            }
            if b[i] == b'"' {
                string = false;
            }
            i += 1;
            continue;
        }
        if tick {
            if b[i] == b'`' {
                tick = false;
            }
            i += 1;
            continue;
        }
        if b[i] == b'"' {
            string = true;
        } else if b[i] == b'`' {
            tick = true;
        } else if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn prev_char(src: &str, offset: usize) -> Option<(usize, char)> {
    let mut i = offset;
    while i > 0 {
        i -= 1;
        if src.is_char_boundary(i) {
            return src[i..].chars().next().map(|c| (i, c));
        }
    }
    None
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn is_ident_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if is_ident_start(c) => chars.all(is_ident_continue),
        _ => false,
    }
}

const KEYWORDS: &[(&str, &str)] = &[
    ("module", "module ${1:name};"),
    ("use", "use ${1:j3.io}.${2:name};"),
    ("pub", "pub "),
    ("fn", "fn ${1:name}($2) {\n    $0\n}"),
    ("struct", "struct ${1:Name} {\n    $0\n}"),
    ("enum", "enum ${1:Name} {\n    ${2:A},\n}"),
    ("const", "const ${1:NAME}: ${2:int} = ${3:0};"),
    ("let", "let ${1:name} = $0;"),
    ("if", "if ${1:cond} {\n    $0\n}"),
    ("else", "else {\n    $0\n}"),
    ("while", "while ${1:cond} {\n    $0\n}"),
    ("for", "for ${1:i} in ${2:0}..${3:n} {\n    $0\n}"),
    ("in", "in "),
    ("match", "match ${1:value} {\n    ${2:_} => { $0 }\n}"),
    ("return", "return $0;"),
    ("break", "break;"),
    ("continue", "continue;"),
    ("defer", "defer $0;"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::default_hello_path;

    fn hello() -> (Program, String, String) {
        let path = default_hello_path();
        let prog = query::analyze(&path, &[]).expect("analyze hello");
        let src = std::fs::read_to_string(&path).unwrap();
        (prog, path.display().to_string(), src)
    }

    fn labels(items: &[Completion]) -> Vec<&str> {
        items.iter().map(|c| c.label.as_str()).collect()
    }

    #[test]
    fn use_completes_module_then_export() {
        let (prog, path, src) = hello();
        let at = src.find("use j3.").unwrap() + "use j3.".len();
        let items = complete(&prog, &path, at as u32);
        let names = labels(&items);
        assert!(names.contains(&"io"), "{names:?}");
        assert!(names.contains(&"sys"), "{names:?}");
        assert!(!names.contains(&"printLn"), "{names:?}");

        let at = src.find("printLn").unwrap() + 5;
        let items = complete(&prog, &path, at as u32);
        assert!(items.iter().any(|c| c.label == "printLn"), "{:?}", labels(&items));
    }

    #[test]
    fn bare_completion_is_in_scope() {
        let (prog, path, src) = hello();
        let at = src.find("printLn(\"").unwrap();
        let items = complete(&prog, &path, (at + 2) as u32);
        assert!(items.iter().any(|c| c.label == "printLn"));
        assert!(items.iter().all(|c| c.label != "exit"));
        assert!(items.iter().all(|c| c.label != "readFile"));

        let items = complete(&prog, &path, at as u32);
        assert!(items.iter().any(|c| c.label == "let"));
        assert!(items.iter().any(|c| c.label == "printLn"));
        assert!(items.iter().all(|c| c.label != "exit"));
    }

    #[test]
    fn signature_of_print_ln() {
        let (prog, path, src) = hello();
        let at = src.find("printLn(\"").unwrap() + "printLn(".len();
        let sig = signature(&prog, &path, &src, at as u32).expect("sig");
        assert!(sig.signatures[0].label.contains("string"), "{}", sig.signatures[0].label);
        assert_eq!(sig.signatures[0].params.len(), 1);
        assert!(sig.signatures[0].params[0].contains('s'), "{:?}", sig.signatures[0].params);
        assert_eq!(sig.active, 0);
    }

    #[test]
    fn import_edit_for_unimported_export() {
        let (prog, _, src) = hello();
        let edits = import_edits(&prog, &src, &["unknown function `readFile`".to_string()]);
        assert_eq!(edits.len(), 1, "{:?}", edits.iter().map(|e| e.title.clone()).collect::<Vec<_>>());
        assert!(edits[0].new_text.contains("use j3.io.readFile;"), "{}", edits[0].new_text);
        let at = edits[0].offset as usize;
        assert!(src[..at].contains("use j3.io.printLn"), "{}", &src[..at]);
        let none = import_edits(&prog, &src, &["unknown name `noSuchName`".to_string()]);
        assert!(none.is_empty());
    }

    #[test]
    fn signature_of_method_skips_this() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/mesh.j3");
        let prog = query::analyze(&path, &[]).expect("mesh");
        let src = std::fs::read_to_string(&path).unwrap();
        let at = src.find("q.rotate(").unwrap() + "q.rotate(".len();
        let sig = signature(&prog, &path.display().to_string(), &src, at as u32).expect("sig");
        assert!(
            sig.signatures.iter().any(|s| s.label.contains("Vec3") && s.params.iter().all(|p| !p.starts_with("this:"))),
            "{:?}",
            sig.signatures.iter().map(|s| s.label.clone()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn outline_lists_main() {
        let (prog, path, _) = hello();
        let syms = symbols(&prog, &path);
        assert!(syms.iter().any(|s| s.name == "main" && s.kind == 12), "{:?}", syms.iter().map(|s| &s.name).collect::<Vec<_>>());
    }

    #[test]
    fn module_dot_lists_exported_type() {
        let path = default_hello_path();
        let src = "\
module hello;

use j3.math;
use j3.io.printLn;

fn main() {
    math.sqrt(f32(4));
    printLn(\"ok\");
}
";
        let overlay = vec![(path.clone(), src.to_string())];
        let prog = query::analyze(&path, &overlay).expect("analyze");
        let shown = path.display().to_string();
        let at = src.find("math.sqrt").unwrap() + "math.".len();
        let items = complete_at(&prog, &shown, src, at as u32);
        let names = labels(&items);
        assert!(names.contains(&"sqrt"), "{names:?}");
        assert!(names.contains(&"Quat"), "{names:?}");
        assert!(names.contains(&"Vec2"), "{names:?}");
    }

    #[test]
    fn incomplete_app_dot_lists_methods() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../examples/minecraft/src/main.j3");
        let mut src = std::fs::read_to_string(&path).unwrap();
        let at_run = src.find("    app.run();\n").expect("run");
        src.insert_str(at_run, "    app.\n");
        let at = at_run + "    app.".len();
        let items = complete_buffer(&path, &[(path.clone(), src.clone())], &src, at as u32);
        let names = labels(&items);
        assert!(names.contains(&"useGl"), "{names:?}");
        assert!(names.contains(&"run"), "{names:?}");
        assert!(names.contains(&"startup"), "{names:?}");
        assert!(!names.contains(&"App"), "{names:?}");
    }
}
