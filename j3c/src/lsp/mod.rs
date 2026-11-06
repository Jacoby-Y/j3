//! Language server. Same parse + check as `j3c`. Stdio JSON-RPC, no crates.io.

mod complete;
mod diag;
mod docs;
pub(crate) mod json;
mod proto;
mod query;
mod ui;

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use crate::parse;

use diag::*;
use docs::*;
use json::Json;
use proto::*;

struct ProgramCache {
    key: u64,
    prog: Option<crate::check::Program>,
}

fn docs_key(docs: &HashMap<String, String>) -> u64 {
    let mut keys: Vec<&String> = docs.keys().collect();
    keys.sort();
    let mut hasher = DefaultHasher::new();
    for k in keys {
        k.hash(&mut hasher);
        docs.get(k).unwrap().hash(&mut hasher);
    }
    hasher.finish()
}

impl ProgramCache {
    fn get<F>(&mut self, key: u64, load: F) -> Option<&crate::check::Program>
    where
        F: FnOnce() -> Option<crate::check::Program>,
    {
        if self.prog.is_none() || self.key != key {
            self.key = key;
            self.prog = load();
        }
        self.prog.as_ref()
    }
}

pub fn serve() -> i32 {
    let stdin = io::stdin();
    let mut stdin = stdin.lock();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    let mut docs: HashMap<String, String> = HashMap::new();
    let mut cache = ProgramCache {
        key: 0,
        prog: None,
    };
    let mut ui_cache = ui::UiCache::new();
    let mut shutdown = false;
    loop {
        let raw = match read_message(&mut stdin) {
            Ok(Some(s)) => s,
            Ok(None) => break,
            Err(e) => {
                eprintln!("j3c lsp: {e}");
                return 1;
            }
        };
        let msg = match Json::parse_str(&raw) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("j3c lsp: bad json: {e}");
                continue;
            }
        };
        let method = msg.get("method").and_then(Json::as_str).unwrap_or("");
        let id = msg.get("id").cloned();
        if matches!(
            method,
            "textDocument/hover"
                | "textDocument/definition"
                | "textDocument/completion"
                | "textDocument/signatureHelp"
                | "textDocument/documentSymbol"
                | "textDocument/codeAction"
                | "textDocument/formatting"
        ) && ui::is_ui_request(&msg)
        {
            let result = ui::handle(method, &msg, &docs, &mut ui_cache);
            reply(&mut stdout, id, result);
            continue;
        }
        match method {
            "initialize" => {
                let result = Json::obj(vec![(
                    "capabilities",
                    Json::obj(vec![
                        ("textDocumentSync", Json::Num(1.0)),
                        ("hoverProvider", Json::Bool(true)),
                        ("definitionProvider", Json::Bool(true)),
                        ("documentFormattingProvider", Json::Bool(true)),
                        (
                            "completionProvider",
                            Json::obj(vec![
                                (
                                    "triggerCharacters",
                                    Json::Arr(vec![
                                        Json::Str(".".to_string()),
                                        Json::Str("{".to_string()),
                                        Json::Str("@".to_string()),
                                        Json::Str("[".to_string()),
                                        Json::Str("<".to_string()),
                                        Json::Str(" ".to_string()),
                                        Json::Str(":".to_string()),
                                    ]),
                                ),
                                ("resolveProvider", Json::Bool(false)),
                            ]),
                        ),
                        (
                            "signatureHelpProvider",
                            Json::obj(vec![(
                                "triggerCharacters",
                                Json::Arr(vec![
                                    Json::Str("(".to_string()),
                                    Json::Str(",".to_string()),
                                ]),
                            )]),
                        ),
                        ("documentSymbolProvider", Json::Bool(true)),
                        (
                            "codeActionProvider",
                            Json::obj(vec![(
                                "codeActionKinds",
                                Json::Arr(vec![Json::Str("quickfix".to_string())]),
                            )]),
                        ),
                    ]),
                ), (
                    "serverInfo",
                    Json::obj(vec![
                        ("name", Json::Str("j3c".to_string())),
                        ("version", Json::Str("0.0.1".to_string())),
                    ]),
                )]);
                reply(&mut stdout, id, result);
            }
            "initialized" | "textDocument/didSave" => {}
            "shutdown" => {
                shutdown = true;
                reply(&mut stdout, id, Json::Null);
            }
            "exit" => return if shutdown { 0 } else { 1 },
            "textDocument/didOpen" => {
                if let Some(doc) = msg.get("params").and_then(|p| p.get("textDocument")) {
                    if let (Some(uri), Some(text)) = (
                        doc.get("uri").and_then(Json::as_str),
                        doc.get("text").and_then(Json::as_str),
                    ) {
                        docs.insert(uri.to_string(), text.to_string());
                        publish_diags(&mut stdout, uri, text, &docs, &mut cache);
                    }
                }
            }
            "textDocument/didChange" => {
                if let Some(p) = msg.get("params") {
                    if let (Some(uri), Some(changes)) = (
                        p.get("textDocument")
                            .and_then(|d| d.get("uri"))
                            .and_then(Json::as_str),
                        p.get("contentChanges"),
                    ) {
                        if let Json::Arr(xs) = changes {
                            if let Some(text) = xs.last().and_then(|c| c.get("text")).and_then(Json::as_str)
                            {
                                docs.insert(uri.to_string(), text.to_string());
                                publish_diags(&mut stdout, uri, text, &docs, &mut cache);
                            }
                        }
                    }
                }
            }
            "textDocument/didClose" => {
                if let Some(uri) = msg
                    .get("params")
                    .and_then(|p| p.get("textDocument"))
                    .and_then(|d| d.get("uri"))
                    .and_then(Json::as_str)
                {
                    docs.remove(uri);
                }
            }
            "textDocument/hover" => {
                let result = pos_query(&msg, &docs, &mut cache, |prog, path, off| {
                    query::hover(prog, path, off).map(|h| {
                        Json::obj(vec![
                            (
                                "contents",
                                Json::obj(vec![
                                    ("kind", Json::Str("markdown".to_string())),
                                    ("value", Json::Str(h.text)),
                                ]),
                            ),
                            ("range", range_json(&docs, path, h.span)),
                        ])
                    })
                });
                reply(&mut stdout, id, result.unwrap_or(Json::Null));
            }
            "textDocument/definition" => {
                let result = pos_query(&msg, &docs, &mut cache, |prog, path, off| {
                    query::definition(prog, path, off).map(|loc| {
                        let src = src_of(&docs, &loc.path);
                        let ((sl, sc), (el, ec)) = query::range_of(src, loc.span);
                        Json::obj(vec![
                            ("uri", Json::Str(path_to_uri(&loc.path))),
                            (
                                "range",
                                Json::obj(vec![
                                    ("start", pos_json(sl, sc)),
                                    ("end", pos_json(el, ec)),
                                ]),
                            ),
                        ])
                    })
                });
                reply(&mut stdout, id, result.unwrap_or(Json::Null));
            }
            "textDocument/completion" => {
                let items = match cursor(&msg, &docs) {
                    Some(cur) => {
                        let key = docs_key(&docs);
                        let path = cur.path.clone();
                        let overlays = overlays_from(&docs);
                        match cache.get(key, || query::analyze(&path, &overlays).ok()) {
                            Some(prog) => complete::complete_at(
                                prog,
                                &cur.path_str,
                                &cur.src,
                                cur.offset,
                            ),
                            None => {
                                // Healed-line analyze must not write into cache.
                                complete::complete_buffer(
                                    &cur.path,
                                    &overlays,
                                    &cur.src,
                                    cur.offset,
                                )
                            }
                        }
                    }
                    None => Vec::new(),
                };
                let items = items.into_iter().map(completion_json).collect();
                reply(
                    &mut stdout,
                    id,
                    Json::obj(vec![
                        ("isIncomplete", Json::Bool(false)),
                        ("items", Json::Arr(items)),
                    ]),
                );
            }
            "textDocument/signatureHelp" => {
                let result = match cursor(&msg, &docs) {
                    Some(cur) => {
                        let help = match program_for(&cur.path, &docs, &mut cache) {
                            Some(prog) => {
                                complete::signature(prog, &cur.path_str, &cur.src, cur.offset)
                            }
                            None if !has_doc_overlay(&docs, &cur.path) => {
                                query::analyze(&cur.path, &[]).ok().and_then(|prog| {
                                    complete::signature(
                                        &prog,
                                        &cur.path_str,
                                        &cur.src,
                                        cur.offset,
                                    )
                                })
                            }
                            None => None,
                        };
                        help.map(|help| {
                            let sigs = help
                                .signatures
                                .into_iter()
                                .map(|s| {
                                    let params = s
                                        .params
                                        .into_iter()
                                        .map(|p| Json::obj(vec![("label", Json::Str(p))]))
                                        .collect();
                                    Json::obj(vec![
                                        ("label", Json::Str(s.label)),
                                        ("parameters", Json::Arr(params)),
                                        (
                                            "documentation",
                                            Json::obj(vec![
                                                ("kind", Json::Str("markdown".to_string())),
                                                ("value", Json::Str(s.documentation)),
                                            ]),
                                        ),
                                    ])
                                })
                                .collect();
                            Json::obj(vec![
                                ("signatures", Json::Arr(sigs)),
                                ("activeSignature", Json::Num(0.0)),
                                ("activeParameter", Json::Num(help.active as f64)),
                            ])
                        })
                    }
                    None => None,
                };
                reply(&mut stdout, id, result.unwrap_or(Json::Null));
            }
            "textDocument/documentSymbol" => {
                let items = match doc_cursor(&msg, &docs) {
                    Some(cur) => {
                        let key = docs_key(&docs);
                        let path = cur.path.clone();
                        let overlays = overlays_from(&docs);
                        let syms = match cache.get(key, || query::analyze(&path, &overlays).ok()) {
                            Some(prog) => complete::symbols(prog, &cur.path_str),
                            None if !has_doc_overlay(&docs, &cur.path) => {
                                // Disk-only analyze must not write into cache.
                                query::analyze(&cur.path, &[])
                                    .ok()
                                    .map(|prog| complete::symbols(&prog, &cur.path_str))
                                    .unwrap_or_default()
                            }
                            None => Vec::new(),
                        };
                        syms.into_iter()
                            .map(|s| {
                                let ((sl, sc), (el, ec)) = query::range_of(&cur.src, s.span);
                                Json::obj(vec![
                                    ("name", Json::Str(s.name)),
                                    ("kind", Json::Num(s.kind as f64)),
                                    ("containerName", Json::Str(s.container)),
                                    (
                                        "location",
                                        Json::obj(vec![
                                            ("uri", Json::Str(cur.uri.clone())),
                                            (
                                                "range",
                                                Json::obj(vec![
                                                    ("start", pos_json(sl, sc)),
                                                    ("end", pos_json(el, ec)),
                                                ]),
                                            ),
                                        ]),
                                    ),
                                ])
                            })
                            .collect()
                    }
                    None => Vec::new(),
                };
                reply(&mut stdout, id, Json::Arr(items));
            }
            "textDocument/codeAction" => {
                let result = match cursor_range(&msg, &docs) {
                    Some(cur) => {
                        let messages = diag_messages(&msg);
                        let edits = match program_for(&cur.path, &docs, &mut cache) {
                            Some(prog) => complete::import_edits(prog, &cur.src, &messages),
                            None if !has_doc_overlay(&docs, &cur.path) => {
                                query::analyze(&cur.path, &[])
                                    .ok()
                                    .map(|prog| {
                                        complete::import_edits(&prog, &cur.src, &messages)
                                    })
                                    .unwrap_or_default()
                            }
                            None => Vec::new(),
                        };
                        let uri = cur.uri.clone();
                        Json::Arr(
                            edits
                                .into_iter()
                                .map(|e| {
                                    let ((sl, sc), _) = query::range_of(
                                        &cur.src,
                                        crate::span::Span {
                                            file: crate::span::FileId(0),
                                            start: e.offset,
                                            end: e.offset,
                                        },
                                    );
                                    Json::obj(vec![
                                        ("title", Json::Str(e.title)),
                                        ("kind", Json::Str("quickfix".to_string())),
                                        (
                                            "edit",
                                            Json::Obj(vec![(
                                                "changes".to_string(),
                                                Json::Obj(vec![(
                                                    uri.clone(),
                                                    Json::Arr(vec![Json::obj(vec![
                                                        (
                                                            "range",
                                                            Json::obj(vec![
                                                                ("start", pos_json(sl, sc)),
                                                                ("end", pos_json(sl, sc)),
                                                            ]),
                                                        ),
                                                        ("newText", Json::Str(e.new_text)),
                                                    ])]),
                                                )]),
                                            )]),
                                        ),
                                    ])
                                })
                                .collect(),
                        )
                    }
                    None => Json::Arr(Vec::new()),
                };
                reply(&mut stdout, id, result);
            }
            "textDocument/formatting" => {
                let uri = msg
                    .get("params")
                    .and_then(|p| p.get("textDocument"))
                    .and_then(|d| d.get("uri"))
                    .and_then(Json::as_str)
                    .unwrap_or("");
                let text = docs.get(uri).cloned().unwrap_or_default();
                let result = match query::format_file(&text) {
                    Ok(formatted) if formatted != text => {
                        let end = query::range_of(&text, crate::span::Span {
                            file: crate::span::FileId(1),
                            start: 0,
                            end: text.len() as u32,
                        });
                        Json::Arr(vec![Json::obj(vec![
                            (
                                "range",
                                Json::obj(vec![
                                    ("start", pos_json(0, 0)),
                                    ("end", pos_json(end.1.0, end.1.1)),
                                ]),
                            ),
                            ("newText", Json::Str(formatted)),
                        ])])
                    }
                    _ => Json::Arr(Vec::new()),
                };
                reply(&mut stdout, id, result);
            }
            _ => {
                if id.is_some() {
                    reply(&mut stdout, id, Json::Null);
                }
            }
        }
    }
    0
}

impl Json {
    fn parse_str(s: &str) -> Result<Json, String> {
        json::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn docs_key_equal_for_same_pairs() {
        let mut a = HashMap::new();
        a.insert("file:///a.j3".to_string(), "module a;".to_string());
        a.insert("file:///b.j3".to_string(), "module b;".to_string());
        let mut b = HashMap::new();
        b.insert("file:///b.j3".to_string(), "module b;".to_string());
        b.insert("file:///a.j3".to_string(), "module a;".to_string());
        assert_eq!(docs_key(&a), docs_key(&b));
    }

    #[test]
    fn docs_key_changes_with_text() {
        let mut a = HashMap::new();
        a.insert("file:///a.j3".to_string(), "module a;".to_string());
        let mut b = HashMap::new();
        b.insert("file:///a.j3".to_string(), "module a; fn main() {}".to_string());
        assert_ne!(docs_key(&a), docs_key(&b));
    }

    #[test]
    fn program_cache_get_loads_once_per_key() {
        let path = crate::compile::default_hello_path();
        let src = "module hello;\nfn main() {}\n";
        let overlay = vec![(path.clone(), src.to_string())];
        let calls = Cell::new(0);
        let mut cache = ProgramCache {
            key: 0,
            prog: None,
        };
        assert!(cache
            .get(1, || {
                calls.set(calls.get() + 1);
                query::analyze(&path, &overlay).ok()
            })
            .is_some());
        assert!(cache
            .get(1, || {
                calls.set(calls.get() + 1);
                query::analyze(&path, &overlay).ok()
            })
            .is_some());
        assert_eq!(calls.get(), 1);
        assert!(cache
            .get(2, || {
                calls.set(calls.get() + 1);
                query::analyze(&path, &overlay).ok()
            })
            .is_some());
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn program_cache_failed_load_not_sticky() {
        let calls = Cell::new(0);
        let mut cache = ProgramCache {
            key: 0,
            prog: None,
        };
        assert!(cache
            .get(7, || {
                calls.set(calls.get() + 1);
                None
            })
            .is_none());
        assert!(cache
            .get(7, || {
                calls.set(calls.get() + 1);
                None
            })
            .is_none());
        assert_eq!(calls.get(), 2);
    }
}
