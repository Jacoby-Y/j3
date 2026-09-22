//! Language server. Same parse + check as `j3c`. Stdio JSON-RPC, no crates.io.

mod complete;
mod json;
mod query;

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use crate::parse;

use json::Json;

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

struct Cursor {
    uri: String,
    path: PathBuf,
    path_str: String,
    src: String,
    offset: u32,
}

fn doc_parts(msg: &Json, docs: &HashMap<String, String>) -> Option<(String, PathBuf, String)> {
    let uri = msg
        .get("params")?
        .get("textDocument")?
        .get("uri")?
        .as_str()?
        .to_string();
    let path = uri_to_path(&uri)?;
    let src = docs
        .get(&uri)
        .cloned()
        .or_else(|| crate::stdlib::read(&path))?;
    Some((uri, path, src))
}

fn cursor(msg: &Json, docs: &HashMap<String, String>) -> Option<Cursor> {
    let (uri, path, src) = doc_parts(msg, docs)?;
    let pos = msg.get("params")?.get("position")?;
    let line = pos.get("line").and_then(Json::as_i64)? as u32;
    let col = pos.get("character").and_then(Json::as_i64)? as u32;
    let offset = query::offset_at(&src, line, col);
    let path_str = path.display().to_string();
    Some(Cursor {
        uri,
        path,
        path_str,
        src,
        offset,
    })
}

fn doc_cursor(msg: &Json, docs: &HashMap<String, String>) -> Option<Cursor> {
    let (uri, path, src) = doc_parts(msg, docs)?;
    let path_str = path.display().to_string();
    Some(Cursor {
        uri,
        path,
        path_str,
        src,
        offset: 0,
    })
}

fn cursor_range(msg: &Json, docs: &HashMap<String, String>) -> Option<Cursor> {
    doc_cursor(msg, docs)
}

fn program_for<'a>(
    path: &Path,
    docs: &HashMap<String, String>,
    cache: &'a mut ProgramCache,
) -> Option<&'a crate::check::Program> {
    let key = docs_key(docs);
    let path = path.to_path_buf();
    let overlays = overlays_from(docs);
    cache.get(key, || query::analyze(&path, &overlays).ok())
}

fn has_doc_overlay(docs: &HashMap<String, String>, path: &Path) -> bool {
    docs.keys()
        .filter_map(|uri| uri_to_path(uri))
        .any(|p| paths_match(&p, path))
}

fn completion_json(c: complete::Completion) -> Json {
    let mut pairs = vec![
        ("label".to_string(), Json::Str(c.label)),
        ("kind".to_string(), Json::Num(c.kind as f64)),
        ("detail".to_string(), Json::Str(c.detail)),
    ];
    if !c.insert.is_empty() {
        pairs.push(("insertText".to_string(), Json::Str(c.insert)));
    }
    if c.snippet {
        pairs.push(("insertTextFormat".to_string(), Json::Num(2.0)));
    }
    if !c.documentation.is_empty() {
        pairs.push((
            "documentation".to_string(),
            Json::obj(vec![
                ("kind", Json::Str("markdown".to_string())),
                ("value", Json::Str(c.documentation)),
            ]),
        ));
    }
    if !c.sort.is_empty() {
        pairs.push(("sortText".to_string(), Json::Str(c.sort)));
    }
    if c.commit_dot {
        pairs.push((
            "commitCharacters".to_string(),
            Json::Arr(vec![Json::Str(".".to_string())]),
        ));
    }
    Json::Obj(pairs)
}

fn diag_messages(msg: &Json) -> Vec<String> {
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

fn diag_text(e: &crate::check::CheckError) -> String {
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

fn pos_query<F>(
    msg: &Json,
    docs: &HashMap<String, String>,
    cache: &mut ProgramCache,
    f: F,
) -> Option<Json>
where
    F: FnOnce(&crate::check::Program, &str, u32) -> Option<Json>,
{
    let p = msg.get("params")?;
    let uri = p
        .get("textDocument")
        .and_then(|d| d.get("uri"))
        .and_then(Json::as_str)?;
    let pos = p.get("position")?;
    let line = pos.get("line").and_then(Json::as_i64)? as u32;
    let col = pos.get("character").and_then(Json::as_i64)? as u32;
    let path = uri_to_path(uri)?;
    let text = docs
        .get(uri)
        .cloned()
        .or_else(|| crate::stdlib::read(&path))?;
    let off = query::offset_at(&text, line, col);
    let key = docs_key(docs);
    let path_buf = path.clone();
    let overlays = overlays_from(docs);
    let prog = cache.get(key, || query::analyze(&path_buf, &overlays).ok())?;
    f(prog, &path.display().to_string(), off)
}

fn publish_diags(
    out: &mut impl Write,
    uri: &str,
    text: &str,
    docs: &HashMap<String, String>,
    cache: &mut ProgramCache,
) {
    let Some(path) = uri_to_path(uri) else {
        return;
    };
    let overlays = overlays_from(docs);
    let open = open_docs(docs);
    let mut target = uri.to_string();
    let mut diags = Vec::new();
    let key = docs_key(docs);
    match parse::parse(text, crate::span::FileId(1)) {
        Err(e) => {
            cache.key = key;
            cache.prog = None;
            diags.push(diag_json(text, e.span, &e.message));
        }
        Ok(_) => match crate::compile::analyze_checked(&path, &overlays) {
            Ok(prog) => {
                cache.key = key;
                cache.prog = Some(prog);
            }
            Err(e) => {
                cache.key = key;
                cache.prog = None;
                if let Some(found) = open.iter().find(|(_, p)| paths_match(p, Path::new(&e.path))) {
                    target = found.0.clone();
                }
                let src = if target == uri && paths_match(Path::new(&e.path), &path) {
                    text.to_string()
                } else if let Some(buf) = docs.get(&target) {
                    buf.clone()
                } else {
                    crate::stdlib::read(Path::new(&e.path)).unwrap_or_default()
                };
                diags.push(diag_json(&src, e.span, &diag_text(&e)));
            }
        },
    };
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
        notify_diags(out, uri, diags);
    }
}

fn notify_diags(out: &mut impl Write, uri: &str, diags: Vec<Json>) {
    notify(
        out,
        "textDocument/publishDiagnostics",
        Json::obj(vec![
            ("uri", Json::Str(uri.to_string())),
            ("diagnostics", Json::Arr(diags)),
        ]),
    );
}

fn open_docs(docs: &HashMap<String, String>) -> Vec<(String, PathBuf)> {
    docs.iter()
        .filter_map(|(uri, _)| uri_to_path(uri).map(|p| (uri.clone(), p)))
        .collect()
}

fn diag_json(src: &str, span: crate::span::Span, message: &str) -> Json {
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

fn overlays_from(docs: &HashMap<String, String>) -> Vec<(PathBuf, String)> {
    docs.iter()
        .filter_map(|(uri, text)| uri_to_path(uri).map(|p| (p, text.clone())))
        .collect()
}

fn range_json(docs: &HashMap<String, String>, path: &str, span: crate::span::Span) -> Json {
    let src = src_of(docs, path);
    let ((sl, sc), (el, ec)) = query::range_of(src, span);
    Json::obj(vec![
        ("start", pos_json(sl, sc)),
        ("end", pos_json(el, ec)),
    ])
}

fn src_of<'a>(docs: &'a HashMap<String, String>, path: &str) -> &'a str {
    for (uri, text) in docs {
        if let Some(p) = uri_to_path(uri) {
            if p.display().to_string() == path || paths_match(&p, Path::new(path)) {
                return text;
            }
        }
    }
    ""
}

fn paths_match(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

fn pos_json(line: u32, col: u32) -> Json {
    Json::obj(vec![
        ("line", Json::Num(line as f64)),
        ("character", Json::Num(col as f64)),
    ])
}

fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    let decoded = url_decode(rest);
    Some(PathBuf::from(decoded))
}

fn path_to_uri(path: &str) -> String {
    let abs = Path::new(path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(path));
    format!("file://{}", abs.display())
}

fn url_decode(s: &str) -> String {
    let mut out = String::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hi = from_hex(b[i + 1]);
            let lo = from_hex(b[i + 2]);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as char);
                i += 3;
                continue;
            }
        }
        out.push(b[i] as char);
        i += 1;
    }
    out
}

fn from_hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

fn read_message(stdin: &mut impl BufRead) -> Result<Option<String>, String> {
    let mut headers = String::new();
    loop {
        let mut line = String::new();
        let n = stdin.read_line(&mut line).map_err(|e| e.to_string())?;
        if n == 0 {
            return Ok(None);
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        headers.push_str(&line);
    }
    let mut len = 0usize;
    for line in headers.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("Content-Length:") {
            len = v.trim().parse().map_err(|_| "bad Content-Length")?;
        }
    }
    if len == 0 {
        return Ok(None);
    }
    let mut buf = vec![0u8; len];
    stdin.read_exact(&mut buf).map_err(|e| e.to_string())?;
    String::from_utf8(buf).map(Some).map_err(|e| e.to_string())
}

fn reply(out: &mut impl Write, id: Option<Json>, result: Json) {
    let mut obj = vec![("jsonrpc", Json::Str("2.0".to_string()))];
    if let Some(id) = id {
        obj.push(("id", id));
    }
    obj.push(("result", result));
    write_msg(out, &Json::obj_owned(obj));
}

fn notify(out: &mut impl Write, method: &str, params: Json) {
    write_msg(
        out,
        &Json::obj(vec![
            ("jsonrpc", Json::Str("2.0".to_string())),
            ("method", Json::Str(method.to_string())),
            ("params", params),
        ]),
    );
}

fn write_msg(out: &mut impl Write, v: &Json) {
    let body = v.stringify();
    let _ = write!(
        out,
        "Content-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let _ = out.flush();
}

impl Json {
    fn obj_owned(pairs: Vec<(&str, Json)>) -> Json {
        Json::obj(pairs)
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
