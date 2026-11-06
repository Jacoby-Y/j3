//! `.j3ui` in the editor. J3 regions (`<script>`, `{...}`) go through the same typed program as
//! `.j3`: the buffer is generated into J3 (`crate::ui`), the query runs on the generated text, and
//! the span map moves positions both ways. Markup and CSS completion are text scans over the
//! tables the compiler already has (`ui::gen::ELEMENTS`, `j3/ui/css.j3`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::complete::{self, Completion};
use super::json::Json;
use super::query;
use super::{docs_key, overlays_from, paths_match, pos_json, uri_to_path};
use crate::check::{CheckError, Program};
use crate::span::{FileId, Span};
use crate::ui::Generated;

mod css;
mod items;
#[cfg(test)]
mod tests;

use css::*;
use items::*;

pub(super) struct Ui {
    pub prog: Option<Program>,
    pub gen: Option<Generated>,
    pub err: Option<CheckError>,
    pub root: PathBuf,
}

pub(super) struct UiCache {
    key: u64,
    val: Option<Ui>,
    set: bool,
}

impl UiCache {
    pub fn new() -> UiCache {
        UiCache { key: 0, val: None, set: false }
    }
    fn get(&mut self, key: u64, load: impl FnOnce() -> Option<Ui>) -> Option<&Ui> {
        if !self.set || self.key != key {
            self.key = key;
            self.val = load();
            self.set = true;
        }
        self.val.as_ref()
    }
}

/// Analyze the project around `path` (a `.j3ui` or the theme). `None` when it has no `[ui]`.
pub(super) fn analyze(path: &Path, overlays: &[(PathBuf, String)]) -> Option<Ui> {
    let m = match crate::project::resolve(path) {
        Ok(crate::project::Input::Project(m)) => m,
        _ => return None,
    };
    m.ui.as_ref()?;
    let (r, gen) = crate::compile::analyze_ui(&m.entry, overlays);
    let (prog, err) = match r {
        Ok(p) => (Some(p), None),
        Err(e) => (None, Some(e)),
    };
    Some(Ui { prog, gen, err, root: m.root.clone() })
}

pub(super) fn is_ui_path(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()) == Some("j3ui")
}

/// A `.css` file next to the components (`ui/theme.css`): only CSS completion applies.
fn is_css(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()) == Some("css")
}

pub(super) fn is_ui_request(msg: &Json) -> bool {
    msg.get("params")
        .and_then(|p| p.get("textDocument"))
        .and_then(|d| d.get("uri"))
        .and_then(Json::as_str)
        .and_then(uri_to_path)
        .is_some_and(|p| is_ui_path(&p) || is_css(&p))
}

fn read_text(path: &Path, overlays: &[(PathBuf, String)]) -> Option<String> {
    overlays
        .iter()
        .find(|(p, _)| paths_match(p, path))
        .map(|(_, t)| t.clone())
        .or_else(|| std::fs::read_to_string(path).ok())
}

fn with_text(overlays: &[(PathBuf, String)], path: &Path, text: String) -> Vec<(PathBuf, String)> {
    let mut v: Vec<_> = overlays.iter().filter(|(p, _)| !paths_match(p, path)).cloned().collect();
    v.push((path.to_path_buf(), text));
    v
}

fn file_index(ui: &Ui, path: &Path) -> Option<usize> {
    ui.gen.as_ref()?.files.iter().position(|f| paths_match(&ui.root.join(&f.path), path))
}

fn range_json(text: &str, lo: usize, hi: usize) -> Json {
    let ((sl, sc), (el, ec)) = query::range_of(text, Span { file: FileId(1), start: lo as u32, end: hi as u32 });
    Json::obj(vec![("start", pos_json(sl, sc)), ("end", pos_json(el, ec))])
}

fn ident_range(text: &str, off: usize) -> (usize, usize) {
    let b = text.as_bytes();
    let id = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut lo = off.min(b.len());
    while lo > 0 && id(b[lo - 1]) {
        lo -= 1;
    }
    let mut hi = off.min(b.len());
    while hi < b.len() && id(b[hi]) {
        hi += 1;
    }
    (lo, hi)
}

/// Request entry: hover, definition, completion, signature help for a `.j3ui` (or theme `.css`).
/// Everything else answers empty.
pub(super) fn handle(method: &str, msg: &Json, docs: &HashMap<String, String>, cache: &mut UiCache) -> Json {
    let empty = if matches!(method, "textDocument/documentSymbol" | "textDocument/codeAction" | "textDocument/formatting") {
        Json::Arr(Vec::new())
    } else {
        Json::Null
    };
    let Some(p) = msg.get("params") else { return empty };
    let Some(uri) = p.get("textDocument").and_then(|d| d.get("uri")).and_then(Json::as_str) else { return empty };
    let Some(path) = uri_to_path(uri) else { return empty };
    let overlays = overlays_from(docs);
    let Some(text) = docs.get(uri).cloned().or_else(|| std::fs::read_to_string(&path).ok()) else { return empty };
    let off = match p.get("position") {
        Some(pos) => {
            let line = pos.get("line").and_then(Json::as_i64).unwrap_or(0) as u32;
            let col = pos.get("character").and_then(Json::as_i64).unwrap_or(0) as u32;
            query::offset_at(&text, line, col) as usize
        }
        None => 0,
    };
    match method {
        "textDocument/completion" => {
            let items = complete_at(&path, &overlays, &text, off);
            let items: Vec<Json> = items.into_iter().map(super::completion_json).collect();
            Json::obj(vec![("isIncomplete", Json::Bool(false)), ("items", Json::Arr(items))])
        }
        "textDocument/hover" | "textDocument/definition" | "textDocument/signatureHelp" => {
            if !is_ui_path(&path) {
                return Json::Null;
            }
            let key = docs_key(docs);
            let Some(ui) = cache.get(key, || analyze(&path, &overlays)) else { return Json::Null };
            match method {
                "textDocument/hover" => hover(ui, &path, &text, off, &overlays),
                "textDocument/definition" => definition(ui, &path, &text, off, &overlays),
                _ => signature(ui, &path, off),
            }
        }
        _ => empty,
    }
}

/// The generated offset of source offset `off` and the generated file's text.
fn to_gen<'a>(ui: &'a Ui, path: &Path, off: usize) -> Option<(&'a Program, &'a Generated, usize)> {
    let (prog, g) = (ui.prog.as_ref()?, ui.gen.as_ref()?);
    let fi = file_index(ui, path)?;
    Some((prog, g, g.map.to_gen(fi, off)?))
}

fn hover(ui: &Ui, path: &Path, text: &str, off: usize, _o: &[(PathBuf, String)]) -> Json {
    let Some((prog, g, go)) = to_gen(ui, path, off) else { return Json::Null };
    let Some(h) = query::hover(prog, &g.label, go as u32) else { return Json::Null };
    let (lo, hi) = ident_range(text, off);
    Json::obj(vec![
        ("contents", Json::obj(vec![("kind", Json::Str("markdown".into())), ("value", Json::Str(h.text))])),
        ("range", range_json(text, lo, hi)),
    ])
}

/// `<game.Bar` under the cursor: the file of that component (names are full paths from `ui/`).
fn component_file(ui: &Ui, text: &str, off: usize) -> Option<Json> {
    let b = text.as_bytes();
    let ok = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'.';
    let (mut lo, mut hi) = (off.min(b.len()), off.min(b.len()));
    while lo > 0 && ok(b[lo - 1]) {
        lo -= 1;
    }
    while hi < b.len() && ok(b[hi]) {
        hi += 1;
    }
    if lo == 0 || !(b[lo - 1] == b'<' || (b[lo - 1] == b'/' && lo > 1 && b[lo - 2] == b'<')) {
        return None;
    }
    let f = ui.gen.as_ref()?.files.iter().find(|f| f.name == text[lo..hi])?;
    let uri = super::path_to_uri(&ui.root.join(&f.path).display().to_string());
    Some(Json::obj(vec![("uri", Json::Str(uri)), ("range", range_json(&f.text, 0, 0))]))
}

fn definition(ui: &Ui, path: &Path, text: &str, off: usize, overlays: &[(PathBuf, String)]) -> Json {
    if let Some(j) = component_file(ui, text, off) {
        return j;
    }
    let Some((prog, g, go)) = to_gen(ui, path, off) else { return Json::Null };
    let Some(loc) = query::definition(prog, &g.label, go as u32) else { return Json::Null };
    let (target, text, lo, hi) = if loc.path == g.label {
        // A result in generated text lands on the element or script that produced it.
        match g.map.to_src(loc.span.start as usize, loc.span.end as usize) {
            Some(s) => (ui.root.join(&g.files[s.file].path), g.files[s.file].text.clone(), s.lo, s.hi),
            None => return Json::Null,
        }
    } else {
        let p = PathBuf::from(&loc.path);
        let t = read_text(&p, overlays).unwrap_or_default();
        (p, t, loc.span.start as usize, loc.span.end as usize)
    };
    Json::obj(vec![("uri", Json::Str(super::path_to_uri(&target.display().to_string()))), ("range", range_json(&text, lo, hi))])
}

fn signature(ui: &Ui, path: &Path, off: usize) -> Json {
    // ponytail: only when the buffer typechecks as typed; a half-typed call has no program.
    let Some((prog, g, go)) = to_gen(ui, path, off) else { return Json::Null };
    let Some(help) = complete::signature(prog, &g.label, &g.text, go as u32) else { return Json::Null };
    let sigs = help
        .signatures
        .into_iter()
        .map(|s| {
            let params = s.params.into_iter().map(|p| Json::obj(vec![("label", Json::Str(p))])).collect();
            Json::obj(vec![
                ("label", Json::Str(s.label)),
                ("parameters", Json::Arr(params)),
                ("documentation", Json::obj(vec![("kind", Json::Str("markdown".into())), ("value", Json::Str(s.documentation))])),
            ])
        })
        .collect();
    Json::obj(vec![("signatures", Json::Arr(sigs)), ("activeSignature", Json::Num(0.0)), ("activeParameter", Json::Num(help.active as f64))])
}
