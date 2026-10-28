use super::*;

pub(super) fn push_locals(cx: &IrCx, file: FileId, offset: u32, prefix: &str, out: &mut Vec<Completion>) {
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
        out.push(item(
            name,
            kind,
            &dump::ty_string(cx, loc.ty),
            &loc.doc,
            "0",
            false,
        ));
    }
}

pub(super) fn push_script_names(cx: &IrCx, file: FileId, prefix: &str, out: &mut Vec<Completion>) {
    let Some((id, script)) = cx
        .structs()
        .iter()
        .enumerate()
        .find(|(_, s)| s.span.file == file && s.script.is_some())
    else {
        return;
    };
    let sid = StructId(id as u32);
    let prop_count = script.script.as_ref().unwrap().prop_spans.len();
    for (index, field) in script.fields.iter().enumerate() {
        let name = cx.name_str(field.name);
        if !name.starts_with(prefix) {
            continue;
        }
        let detail = if index < prop_count { "prop" } else { "state" };
        out.push(item(
            name,
            6,
            &format!("{detail}: {}", dump::ty_string(cx, field.ty)),
            &field.doc,
            "1",
            false,
        ));
    }
    for global in cx.globals().iter().filter(|g| g.span.file == file) {
        let name = cx.name_str(global.name);
        if name.starts_with(prefix) {
            out.push(item(
                name,
                6,
                &format!("shared: {}", dump::ty_string(cx, global.ty)),
                &global.doc,
                "1",
                false,
            ));
        }
    }
    for func in cx.funcs().iter().filter(|f| f.method_of == Some(sid)) {
        let name = cx.name_str(func.name);
        if name.starts_with(prefix) {
            out.push(item(
                name,
                3,
                &format!("script fn: {}", dump::ty_string(cx, func.ty)),
                &func.doc,
                "1",
                false,
            ));
        }
    }
}

pub(super) fn push_file_items(prog: &Program, file: FileId, prefix: &str, out: &mut Vec<Completion>) {
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

pub(super) fn push_imports(prog: &Program, src: &str, prefix: &str, out: &mut Vec<Completion>) {
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

pub(super) fn push_exports(prog: &Program, path: &[String], prefix: &str, commit_dot: bool, out: &mut Vec<Completion>) {
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

pub(super) fn push_builtins(prefix: &str, out: &mut Vec<Completion>) {
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

pub(super) fn push_keywords(prefix: &str, out: &mut Vec<Completion>) {
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

pub(super) fn keyed(names: &[&str], prefix: &str, kind: i32, detail: &str, sort: &str) -> Vec<Completion> {
    let mut out = Vec::new();
    for name in names {
        if name.starts_with(prefix) {
            out.push(item(name, kind, detail, "", sort, false));
        }
    }
    finish(prefix, out)
}

pub(super) fn export_item(prog: &Program, exp: &Export) -> (i32, String) {
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

pub(super) fn export_doc(prog: &Program, exp: &Export) -> String {
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

pub(super) fn func_in_module<'a>(prog: &'a Program, module: &str, name: &str) -> Option<&'a Func> {
    prog.cx.funcs().iter().find(|f| {
        f.method_of.is_none()
            && prog.cx.name_str(f.name) == name
            && query::module_path(&prog.cx, &f.module) == module
    })
}

pub(super) fn push_struct_members(
    cx: &IrCx,
    id: StructId,
    file: FileId,
    prefix: &str,
    out: &mut Vec<Completion>,
) {
    let script_outside = cx.struct_def(id).script.is_some() && cx.struct_def(id).span.file != file;
    for field in &cx.struct_def(id).fields {
        if script_outside {
            break;
        }
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
        if script_outside && func.vis != Vis::Pub {
            continue;
        }
        let name = cx.name_str(func.name);
        if !is_ident_name(name) || !name.starts_with(prefix) {
            continue;
        }
        out.push(item(name, 2, &dump::ty_string(cx, func.ty), &func.doc, "2", false));
    }
}

pub(super) fn push_methods<'a>(
    cx: &'a IrCx,
    id: StructId,
    name: &str,
    file: FileId,
    out: &mut Vec<(&'a Func, bool)>,
) {
    for func in cx.funcs() {
        if func.method_of == Some(id) && cx.name_str(func.name) == name {
            if cx.struct_def(id).script.is_some()
                && cx.struct_def(id).span.file != file
                && func.vis != Vis::Pub
            {
                continue;
            }
            out.push((func, true));
        }
    }
}

pub(super) fn push_methods_named<'a>(
    cx: &'a IrCx,
    struct_name: &str,
    method: &str,
    file: Option<FileId>,
    out: &mut Vec<(&'a Func, bool)>,
) {
    for s in cx.structs() {
        if cx.name_str(s.name) != struct_name {
            continue;
        }
        let Some(id) = struct_id(cx, s) else {
            continue;
        };
        push_methods(cx, id, method, file.unwrap_or(FileId(0)), out);
    }
}

pub(super) fn push_module_fns<'a>(prog: &'a Program, base: &str, name: &str, out: &mut Vec<(&'a Func, bool)>) {
    for func in prog.cx.funcs() {
        if func.method_of.is_some() || prog.cx.name_str(func.name) != name {
            continue;
        }
        if module_tail(&query::module_path(&prog.cx, &func.module), base) {
            out.push((func, false));
        }
    }
}

pub(super) fn push_free_fns<'a>(
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

pub(super) fn params_of(cx: &IrCx, func: &Func) -> (Vec<String>, String) {
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

pub(super) fn sig_label(name: &str, params: &[String], ret: &str) -> String {
    let mut s = format!("fn {name}(");
    s.push_str(&params.join(", "));
    s.push(')');
    if !ret.is_empty() {
        s.push_str(": ");
        s.push_str(ret);
    }
    s
}

pub(super) fn item(label: &str, kind: i32, detail: &str, doc: &str, sort: &str, commit_dot: bool) -> Completion {
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

pub(super) fn finish(prefix: &str, mut out: Vec<Completion>) -> Vec<Completion> {
    out.sort_by(|a, b| a.sort.cmp(&b.sort).then(a.label.cmp(&b.label)));
    out.dedup_by(|a, b| a.label == b.label && a.kind == b.kind);
    if prefix.is_empty() && out.len() > 200 {
        out.truncate(200);
    }
    out
}
