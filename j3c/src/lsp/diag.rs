//! Diagnostics: checker errors to LSP ranges, publishing to every open buffer.

use super::*;

pub(super) fn diag_messages(msg: &Json) -> Vec<String> {
    let Some(arr) = msg
        .get("params")
        .and_then(|p| p.get("context"))
        .and_then(|c| c.get("diagnostics"))
    else {
        return Vec::new();
    };
    let Json::Arr(xs) = arr else {
        return Vec::new();
    };
    xs.iter()
        .filter_map(|d| d.get("message").and_then(Json::as_str).map(|s| s.to_string()))
        .collect()
}

pub(super) fn diag_text(e: &crate::check::CheckError) -> String {
    let mut m = e.message.clone();
    for n in &e.notes {
        m.push_str("\nnote: ");
        m.push_str(n);
    }
    for h in &e.helps {
        m.push_str("\nhelp: ");
        m.push_str(h);
    }
    m
}

pub(super) fn err_path(from: &Path, p: &str) -> PathBuf {
    let p = PathBuf::from(p);
    if p.is_relative() && ui::is_ui_path(&p) {
        if let Ok(crate::project::Input::Project(m)) = crate::project::resolve(from) {
            return m.root.join(p);
        }
    }
    p
}

pub(super) fn publish_diags(
    out: &mut impl Write,
    uri: &str,
    text: &str,
    docs: &HashMap<String, String>,
    cache: &mut ProgramCache,
) {
    let Some(path) = uri_to_path(uri) else {
        return;
    };
    if path.extension().and_then(|e| e.to_str()) == Some("css") {
        return; // CSS has completion only (lsp/ui.rs), no diagnostics
    }
    let overlays = overlays_from(docs);
    let open = open_docs(docs);
    let mut target = uri.to_string();
    let mut diags = Vec::new();
    let key = docs_key(docs);
    let pick = |e_path: &Path, target: &mut String| {
        if let Some(found) = open.iter().find(|(_, p)| paths_match(p, e_path)) {
            *target = found.0.clone();
        }
    };
    let read = |e_path: &Path, target: &str| -> String {
        match docs.get(target) {
            Some(buf) => buf.clone(),
            None => crate::stdlib::read(e_path).unwrap_or_default(),
        }
    };
    if ui::is_ui_path(&path) {
        // A `.j3ui` buffer: generate, check, and publish on whichever `.j3ui` the error is in.
        if let Some(u) = ui::analyze(&path, &overlays) {
            if let Some(e) = u.err {
                let ep = err_path(&path, &e.path);
                let ep = if ep.is_relative() { u.root.join(&ep) } else { ep };
                pick(&ep, &mut target);
                let src = read(&ep, &target);
                diags.push(diag_json(&src, e.span, &diag_text(&e)));
            }
        }
    } else {
        match parse::parse(text, crate::span::FileId(1)) {
            Err(e) => {
                cache.key = key;
                cache.prog = None;
                diags.push(diag_json(text, e.span, &e.message));
            }
            Ok(_) => match crate::compile::analyze_checked(&path, &overlays) {
                Ok(prog) => {
                    for w in prog.warnings.iter().filter(|w| paths_match(Path::new(&w.path), &path)) {
                        let mut d = diag_json(text, w.span, &w.message);
                        if let Json::Obj(fields) = &mut d {
                            for (k, v) in fields.iter_mut() {
                                if k == "severity" {
                                    *v = Json::Num(2.0);
                                }
                            }
                        }
                        diags.push(d);
                    }
                    cache.key = key;
                    cache.prog = Some(prog);
                }
                Err(e) => {
                    cache.key = key;
                    cache.prog = None;
                    let ep = err_path(&path, &e.path);
                    pick(&ep, &mut target);
                    let src = if target == uri && paths_match(&ep, &path) {
                        text.to_string()
                    } else {
                        read(&ep, &target)
                    };
                    // the other errors in that same file (they are published on one buffer)
                    for x in e.all().filter(|x| x.path == e.path) {
                        diags.push(diag_json(&src, x.span, &diag_text(x)));
                    }
                }
            },
        }
    }
    let mut sent = false;
    for (u, _) in &open {
        let items = if u == &target {
            sent = true;
            diags.clone()
        } else {
            Vec::new()
        };
        notify_diags(out, u, items);
    }
    if !sent {
        notify_diags(out, &target, diags);
    }
}

pub(super) fn notify_diags(out: &mut impl Write, uri: &str, diags: Vec<Json>) {
    notify(
        out,
        "textDocument/publishDiagnostics",
        Json::obj(vec![
            ("uri", Json::Str(uri.to_string())),
            ("diagnostics", Json::Arr(diags)),
        ]),
    );
}

pub(super) fn diag_json(src: &str, span: crate::span::Span, message: &str) -> Json {
    let ((sl, sc), (el, ec)) = query::range_of(src, span);
    Json::obj(vec![
        (
            "range",
            Json::obj(vec![
                ("start", pos_json(sl, sc)),
                ("end", pos_json(el, ec)),
            ]),
        ),
        ("severity", Json::Num(1.0)),
        ("source", Json::Str("j3c".to_string())),
        ("message", Json::Str(message.to_string())),
    ])
}

pub(super) fn range_json(docs: &HashMap<String, String>, path: &str, span: crate::span::Span) -> Json {
    let src = src_of(docs, path);
    let ((sl, sc), (el, ec)) = query::range_of(src, span);
    Json::obj(vec![
        ("start", pos_json(sl, sc)),
        ("end", pos_json(el, ec)),
    ])
}
