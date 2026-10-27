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

// ---------------------------------------------------------------------------------------------
// completion

#[derive(Debug, PartialEq)]
enum Ctx {
    Script,
    Style,
    /// Inside `{...}` of the markup.
    Hole,
    /// Inside a tag: the text from after `<` to the cursor.
    Tag(String),
    Other,
}

/// Blank `<script>`/`<style>` content (byte for byte, so offsets hold) so the markup scan does
/// not read J3 or CSS as tags.
fn mask(text: &str) -> (Vec<u8>, Option<(usize, usize)>, Option<(usize, usize)>) {
    let mut out = text.as_bytes().to_vec();
    let mut found = [None, None];
    for (k, tag) in ["script", "style"].iter().enumerate() {
        if let Some(open) = text.find(&format!("<{tag}")) {
            if let Some(gt) = text[open..].find('>') {
                let cs = open + gt + 1;
                let ce = text[cs..].find(&format!("</{tag}")).map(|i| cs + i).unwrap_or(text.len());
                found[k] = Some((cs, ce));
                for b in &mut out[cs..ce] {
                    if *b != b'\n' {
                        *b = b' ';
                    }
                }
            }
        }
    }
    (out, found[0], found[1])
}

fn context(text: &str, off: usize) -> Ctx {
    let (m, script, style) = mask(text);
    if script.is_some_and(|(a, b)| off >= a && off <= b) {
        return Ctx::Script;
    }
    if style.is_some_and(|(a, b)| off >= a && off <= b) {
        return Ctx::Style;
    }
    #[derive(PartialEq, Clone, Copy)]
    enum M {
        Brace,
        Quote,
    }
    let b = &m[..];
    let end = off.min(b.len());
    let mut stack: Vec<M> = Vec::new();
    let mut tag: Option<usize> = None;
    let mut i = 0;
    while i < end {
        let c = b[i];
        match stack.last() {
            Some(M::Brace) => match c {
                b'{' => stack.push(M::Brace),
                b'}' => {
                    stack.pop();
                }
                b'"' | b'`' => {
                    i += 1;
                    while i < end && b[i] != c {
                        if b[i] == b'\\' {
                            i += 1;
                        }
                        i += 1;
                    }
                }
                _ => {}
            },
            Some(M::Quote) => match c {
                b'"' => {
                    stack.pop();
                }
                b'{' => stack.push(M::Brace),
                _ => {}
            },
            None => {
                if b[i..].starts_with(b"<!--") {
                    match b[i..].windows(3).position(|w| w == b"-->") {
                        Some(k) => i += k + 2,
                        None => return Ctx::Other,
                    }
                } else if tag.is_some() {
                    match c {
                        b'"' => stack.push(M::Quote),
                        b'{' => stack.push(M::Brace),
                        b'>' => tag = None,
                        _ => {}
                    }
                } else if c == b'<' && b.get(i + 1).is_some_and(|n| n.is_ascii_alphabetic() || *n == b'/') {
                    tag = Some(i + 1);
                } else if c == b'{' {
                    stack.push(M::Brace);
                }
            }
        }
        i += 1;
    }
    match (stack.last(), tag) {
        (Some(M::Brace), _) => Ctx::Hole,
        (Some(M::Quote), _) => Ctx::Other,
        (None, Some(t)) => Ctx::Tag(text[t..end].to_string()),
        _ => Ctx::Other,
    }
}

fn it(label: &str, kind: i32, detail: &str, insert: &str, snippet: bool) -> Completion {
    Completion {
        label: label.to_string(),
        kind,
        detail: detail.to_string(),
        insert: insert.to_string(),
        snippet,
        documentation: String::new(),
        sort: String::new(),
        commit_dot: false,
    }
}

pub(super) fn complete_at(path: &Path, overlays: &[(PathBuf, String)], text: &str, off: usize) -> Vec<Completion> {
    let off = off.min(text.len());
    if is_css(path) {
        return css_items(text, path, overlays, None, off);
    }
    match context(text, off) {
        Ctx::Script => { let mut v = script_snippets(text, off); v.extend(j3_items(path, overlays, text, off)); v },
        Ctx::Hole => { let mut v = block_items(text, off); v.extend(j3_items(path, overlays, text, off)); v },
        Ctx::Style => css_items(text, path, overlays, Some(path), off),
        Ctx::Tag(t) => tag_items(path, overlays, &t),
        Ctx::Other => Vec::new(),
    }
}

/// `{#each` `{:case` `{/if`: block names after `{#`, `{:` or `{/`.
fn block_items(text: &str, off: usize) -> Vec<Completion> {
    let (p, _) = prefix_base(text, off);
    let prefix = &text[p..off];
    let Some(sig) = text[..p].strip_suffix(|c| c == '#' || c == ':' || c == '/').and_then(|h| h.strip_suffix('{')).map(|_| text.as_bytes()[p - 1]) else { return Vec::new() };
    let all: &[(&str, &str, &str)] = match sig {
        b'#' => &[("if", "if ${1:cond}}$0{/if", "block"), ("each", "each ${1:xs} as ${2:x} (${3:key})}$0{/each", "keyed loop"), ("match", "match ${1:x}}{:case ${2:a}}$0{/match", "block"), ("let", "let ${1:name} = ${2:expr}", "local")],
        b':' => &[("else", "else", "branch"), ("else if", "else if ${1:cond}", "branch"), ("case", "case ${1:a, b}", "match arm")],
        _ => &[("if", "if", "close"), ("each", "each", "close"), ("match", "match", "close")],
    };
    all.iter().filter(|(l, _, _)| l.starts_with(prefix)).map(|(l, ins, d)| it(l, 14, d, ins, true)).collect()
}

/// `.j3ui` `<script>` statements and lifecycle functions, at the start of a line.
fn script_snippets(text: &str, off: usize) -> Vec<Completion> {
    let (p, base) = prefix_base(text, off);
    let line = text[..p].rsplit('\n').next().unwrap_or("");
    if base.is_some() || !line.trim().is_empty() {
        return Vec::new();
    }
    let prefix = &text[p..off];
    const S: &[(&str, &str, &str)] = &[
        ("provide", "provide ${1:x};", "share a value with descendants"),
        ("inject", "inject ${1:x}: ${2:T};", "read a provided value"),
        ("emit", "emit ${1:pick}($0);", "fire a declared event"),
        ("events", "events {\n\t${1:pick}(${2:slot}: ${3:int});\n}", "declare events and payloads"),
        ("props", "props {\n\t$0\n}", "component props"),
        ("const", "const ${1:NAME}: ${2:int} = $0;", "script constant"),
        ("onMount", "fn onMount() {\n\t$0\n}", "lifecycle: after first build"),
        ("onFrame", "fn onFrame(dt: f32) {\n\t$0\n}", "lifecycle: every frame"),
        ("onDestroy", "fn onDestroy() {\n\t$0\n}", "lifecycle: before removal"),
    ];
    S.iter().filter(|(l, _, _)| l.starts_with(prefix)).map(|(l, ins, d)| it(l, 15, d, ins, true)).collect()
}

/// `(prefix, base)` before the cursor: `sim.go|` is `("go", Some("sim"))`.
fn prefix_base(text: &str, off: usize) -> (usize, Option<(usize, String)>) {
    let b = text.as_bytes();
    let id = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut p = off;
    while p > 0 && id(b[p - 1]) {
        p -= 1;
    }
    if p > 0 && b[p - 1] == b'.' {
        let dot = p - 1;
        let mut s = dot;
        while s > 0 && id(b[s - 1]) {
            s -= 1;
        }
        if s < dot {
            return (p, Some((dot, text[s..dot].to_string())));
        }
    }
    (p, None)
}

fn j3_items(path: &Path, overlays: &[(PathBuf, String)], text: &str, off: usize) -> Vec<Completion> {
    let (pstart, base) = prefix_base(text, off);
    let prefix = &text[pstart..off];
    if let Some((dot, base)) = base {
        // `sim.` does not parse. Swap `sim.pre` for a value that typechecks in that spot (the
        // spot decides which type fits, so try a few) and ask what `sim` is there.
        // ponytail: four analyses at worst; a typed hole could carry its expected type instead.
        let start = dot - base.len();
        if text[..start].trim_end().ends_with('.') {
            return Vec::new();
        }
        for stand_in in ["0", "\"\"", "false", "0;"] {
            let healed = format!("{}{stand_in}{}", &text[..start], &text[off..]);
            let Some(ui) = analyze(path, &with_text(overlays, path, healed)) else { return Vec::new() };
            if let Some((prog, g, go)) = to_gen(&ui, path, start) {
                return complete::member_items(prog, &g.label, go as u32, &base, prefix);
            }
        }
        return Vec::new();
    }
    let Some(ui) = analyze(path, overlays) else { return Vec::new() };
    // ponytail: no healing for a bare name; while the buffer does not typecheck there are no locals.
    let Some((prog, g, go)) = to_gen(&ui, path, off) else { return Vec::new() };
    complete::complete_at(prog, &g.label, &g.text, go as u32)
}

fn ui_dir(path: &Path) -> Option<(PathBuf, String)> {
    match crate::project::resolve(path) {
        Ok(crate::project::Input::Project(m)) => m.ui.map(|u| (m.root.clone(), u.dir)),
        _ => None,
    }
}

/// Component names (`ui/Name.j3ui`, `ui/game/Bar.j3ui` is `game.Bar`), buffers included.
fn components(path: &Path, overlays: &[(PathBuf, String)]) -> Vec<String> {
    let Some((root, dir)) = ui_dir(path) else { return Vec::new() };
    let extra: Vec<PathBuf> = overlays.iter().map(|(p, _)| p.clone()).collect();
    let found = crate::ui::scan::scan(&root.join(&dir), &dir, &extra).unwrap_or_default();
    let mut names: Vec<String> = found.into_iter().map(|f| f.name).filter(|n| n.rsplit('.').next().is_some_and(|l| l.starts_with(|c: char| c.is_ascii_uppercase()))).collect();
    names.dedup();
    names
}

fn tag_items(path: &Path, overlays: &[(PathBuf, String)], tag_text: &str) -> Vec<Completion> {
    let body = tag_text.strip_prefix('/').unwrap_or(tag_text);
    let name_end = body.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.')).unwrap_or(body.len());
    let name = &body[..name_end];
    let mut out = Vec::new();
    if name_end == body.len() {
        for e in crate::ui::gen::ELEMENTS {
            if e.starts_with(name) {
                out.push(it(e, 14, "element", e, false));
            }
        }
        if "component".starts_with(name) {
            out.push(it("component", 14, "dynamic component from a closed candidate set", "component", false));
        }
        if "slot".starts_with(name) {
            out.push(it("slot", 14, "component content slot", "slot", false));
        }
        for c in components(path, overlays) {
            if c.starts_with(name) {
                // the client replaces only the word after the last `.`
                let insert = &c[name.rfind('.').map_or(0, |d| d + 1)..];
                out.push(it(&c, 22, "component", insert, false));
            }
        }
        return out;
    }
    if tag_text.starts_with('/') {
        return out;
    }
    let rest = &body[name_end..];
    let word_start = rest.rfind(|c: char| c.is_whitespace()).map(|i| i + 1).unwrap_or(0);
    let written: Vec<&str> = rest[..word_start].split_whitespace().map(|w| w.split('=').next().unwrap_or(w)).collect();
    let mut add = |label: &str, detail: &str, insert: &str| {
        if !written.contains(&label) {
            out.push(it(label, 5, detail, insert, true));
        }
    };
    add("slot", "put this child in a named slot", "slot=\"$0\"");
    add("style:name", "dynamic style property", "style:${1:width}={$0}");
    if name == "component" {
        add("this", "int index or candidate-set enum", "this={$0}");
        add("of", "closed component candidate set", "of={${1:TextMsg}, ${2:ImageMsg}}");
    }
    if name == "slot" {
        add("name", "slot name", "name=\"$0\"");
    }
    if name.rsplit('.').next().is_some_and(|l| l.starts_with(|c: char| c.is_ascii_uppercase())) {
        if let Some((props, events)) = component_api(path, overlays, name) {
            for (p, ty) in props {
                add(&p, &format!("prop: {ty}"), &format!("{p}={{$0}}"));
            }
            for e in events {
                add(&format!("on:{e}"), "event", &format!("on:{e}={{$0}}"));
            }
        }
    } else {
        for a in crate::ui::gen::element_attrs(name) {
            let insert = match a.split_once(':') {
                Some((d, n)) if n == "name" => format!("{d}:${{1:name}}={{$0}}"),
                Some((d, n)) => format!("{d}:{n}={{$0}}"),
                None if matches!(a, "disabled") => a.to_string(),
                None => format!("{a}=\"$0\""),
            };
            add(a, "attribute", &insert);
        }
    }
    out
}

/// A component's props (name, type) and events, from its `<script>`.
fn component_api(path: &Path, overlays: &[(PathBuf, String)], name: &str) -> Option<(Vec<(String, String)>, Vec<String>)> {
    let (root, dir) = ui_dir(path)?;
    let text = read_text(&root.join(dir).join(format!("{}.j3ui", name.replace('.', "/"))), overlays)?;
    let mk = crate::ui::markup::parse(&text).ok()?;
    let sc = crate::ui::markup::parse_script(&text, mk.script?).ok()?;
    let props = sc.props.iter().map(|p| (p.name.clone(), text[p.ty.0..p.ty.1].trim().to_string())).collect();
    Some((props, sc.events.iter().map(|e| e.0.clone()).collect()))
}

// ---------------------------------------------------------------------------------------------
// CSS

struct CssTables {
    props: Vec<String>,
    kws: Vec<(String, Vec<String>)>,
}

/// Property and keyword tables, read from the embedded `j3/ui/css.j3` (`PROP_NAMES`), `cssvalue.j3` (`kwList`) and
/// `cssprop.j3` (`groupFirst`) and `cssgrid.j3` (the `grid-*` names its `declare` takes) so they cannot drift.
fn css_tables() -> CssTables {
    let src: String = ["ui/css.j3", "ui/cssvalue.j3", "ui/cssprop.j3"]
        .iter()
        .map(|n| crate::stdlib::files().find(|f| f.0 == *n).map(|f| f.1).unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    let src = src.as_str();
    let quoted = |s: &str| -> Vec<String> { s.split('"').skip(1).step_by(2).map(|x| x.to_string()).collect() };
    let mut props: Vec<String> = Vec::new();
    if let Some(i) = src.find("const PROP_NAMES: string = \"") {
        let rest = &src[i + "const PROP_NAMES: string = \"".len()..];
        props.extend(rest[..rest.find('"').unwrap_or(0)].split_whitespace().filter(|p| !p.starts_with('-')).map(|p| p.to_string()));
    }
    if let Some(i) = src.find("fn groupFirst") {
        let body = &src[i..i + src[i..].find("\n}").unwrap_or(0)];
        for q in quoted(body) {
            if !props.contains(&q) {
                props.push(q);
            }
        }
    }
    if let Some(g) = crate::stdlib::files().find(|f| f.0 == "ui/cssgrid.j3").map(|f| f.1) {
        for q in quoted(g) {
            if (q.starts_with("grid-") || q.starts_with("place-")) && !props.contains(&q) {
                props.push(q);
            }
        }
    }
    let mut kws = Vec::new();
    if let Some(i) = src.find("fn kwList") {
        let body = &src[i..i + src[i..].find("\n}").unwrap_or(0)];
        for line in body.lines() {
            let l = line.trim();
            let vals = quoted(l).first().map(|v| v.split_whitespace().map(|x| x.to_string()).collect::<Vec<_>>());
            let (Some(vals), true) = (vals, l.starts_with("if prop == P_") || l.starts_with("return")) else { continue };
            let name = match l.strip_prefix("if prop == P_") {
                Some(r) => r.split(' ').next().unwrap_or("").to_lowercase().replace('_', "-"),
                None => "text-align".to_string(),
            };
            kws.push((name, vals));
        }
    }
    CssTables { props, kws }
}

fn custom_props(text: &str, out: &mut Vec<String>) {
    let b = text.as_bytes();
    let mut i = 0;
    while let Some(k) = text[i..].find("--") {
        let s = i + k;
        let mut e = s + 2;
        while e < b.len() && (b[e].is_ascii_alphanumeric() || b[e] == b'-' || b[e] == b'_') {
            e += 1;
        }
        if text[e..].trim_start().starts_with(':') && e > s + 2 && !out.contains(&text[s..e].to_string()) {
            out.push(text[s..e].to_string());
        }
        i = e.max(s + 2);
    }
}

fn css_items(text: &str, path: &Path, overlays: &[(PathBuf, String)], ui_file: Option<&Path>, off: usize) -> Vec<Completion> {
    // The CSS the cursor is in: a `<style>` block, or a whole `.css` file.
    let (lo, hi) = if ui_file.is_some() {
        let (_, _, style) = mask(text);
        style.unwrap_or((0, 0))
    } else {
        (0, text.len())
    };
    let head = &text[lo..off.min(text.len()).max(lo)];
    let mut depth = 0i32;
    let mut in_comment = false;
    let mut clean_b: Vec<u8> = Vec::new();
    let hb = head.as_bytes();
    let mut i = 0;
    while i < hb.len() {
        if in_comment {
            if hb[i..].starts_with(b"*/") {
                in_comment = false;
                i += 1;
            }
        } else if hb[i..].starts_with(b"/*") {
            in_comment = true;
            i += 1;
        } else {
            match hb[i] {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
            clean_b.push(hb[i]);
        }
        i += 1;
    }
    if depth <= 0 || in_comment {
        return Vec::new();
    }
    let clean = String::from_utf8_lossy(&clean_b).to_string();
    let seg_at = clean.rfind([';', '{', '}']).map(|i| i + 1).unwrap_or(0);
    let seg = &clean[seg_at..];
    let t = css_tables();
    let mut out = Vec::new();
    match seg.split_once(':') {
        None => {
            let prefix = seg.trim_start();
            for p in &t.props {
                if p.starts_with(prefix) {
                    out.push(it(p, 10, "property", &format!("{p}: "), false));
                }
            }
        }
        Some((prop, value)) => {
            let word = value.rsplit(|c: char| c.is_whitespace() || c == ',').next().unwrap_or("");
            if let Some(v) = word.strip_prefix("var(") {
                let mut names = Vec::new();
                custom_props(&text[lo..hi], &mut names);
                if let Some((root, _)) = ui_dir(path) {
                    if let Some(theme) = crate::project::resolve(path).ok().and_then(|i| match i {
                        crate::project::Input::Project(m) => m.ui.and_then(|u| u.theme),
                        _ => None,
                    }) {
                        if let Some(tt) = read_text(&root.join(theme), overlays) {
                            custom_props(&tt, &mut names);
                        }
                    }
                }
                names.push("--j3-tooltip-delay".to_string());
                for n in names {
                    if n.starts_with(v) {
                        out.push(it(&n, 6, "custom property", &n, false));
                    }
                }
            } else {
                if let Some((_, kws)) = t.kws.iter().find(|(n, _)| n == prop.trim()) {
                    for k in kws {
                        if k.starts_with(word) {
                            out.push(it(k, 12, "keyword", k, false));
                        }
                    }
                }
                for f in ["calc(", "min(", "max(", "clamp("] {
                    if !word.is_empty() && f.starts_with(word) {
                        out.push(it(f, 3, "css function", f, false));
                    }
                }
                let num = word.trim_end_matches(|c: char| c.is_ascii_alphabetic());
                if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit() || c == '.') {
                    for u in ["vw", "vh", "vmin", "vmax"] {
                        if u.starts_with(&word[num.len()..]) && word.len() > num.len() {
                            out.push(it(&format!("{num}{u}"), 12, "viewport unit", &format!("{num}{u}"), false));
                        }
                    }
                }
                if "var(--".starts_with(word) {
                    out.push(it("var(--", 15, "custom property", "var(--", false));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idle() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/idle-breakout").canonicalize().unwrap()
    }

    fn file(name: &str) -> (PathBuf, String) {
        let p = idle().join("ui").join(name);
        let t = std::fs::read_to_string(&p).unwrap();
        (p, t)
    }

    fn labels(v: &[Completion]) -> Vec<&str> {
        v.iter().map(|c| c.label.as_str()).collect()
    }

    #[test]
    fn span_map_round_trips_copied_text() {
        let (p, _) = file("BallCard.j3ui");
        let ui = analyze(&p, &[]).unwrap();
        let g = ui.gen.unwrap();
        let mut n = 0;
        for s in g.map.segs.iter().filter(|s| s.copy) {
            for at in [s.lo, (s.lo + s.hi) / 2, s.hi - 1] {
                let src = g.map.to_src(at, at + 1).unwrap();
                // A source range can be emitted twice (a condition used in two places), so the
                // way back may land on the other copy: same source byte, same text.
                let back = g.map.to_gen(src.file, src.lo).unwrap();
                assert_eq!(g.map.to_src(back, back + 1).unwrap(), src);
                assert_eq!(g.text[back..back + 1], g.text[at..at + 1]);
                n += 1;
            }
        }
        assert!(n > 20);
    }

    #[test]
    fn int_key_hint_is_not_a_diagnostic() {
        let (p, t) = file("Upgrades.j3ui");
        for key in ["(k : int)", "(k:int)", "( k : int )"] {
            let ok = t.replace("{#each 0..KINDS as k}", &format!("{{#each 0..KINDS as k {key}}}"));
            assert_ne!(ok, t);
            let ui = analyze(&p, &[(p.clone(), ok)]).unwrap();
            assert!(ui.err.is_none(), "{key}: {}", ui.err.map(|e| e.message).unwrap_or_default());
        }
    }

    #[test]
    fn diagnostic_lands_on_the_j3ui_line() {
        let (p, t) = file("BallCard.j3ui");
        let bad = t.replace("sim.buyBall(kind)", "sim.buyBal(kind)");
        let ui = analyze(&p, &[(p.clone(), bad.clone())]).unwrap();
        let e = ui.err.expect("error");
        assert_eq!(e.path, "ui/BallCard.j3ui");
        let ((l, c), _) = query::range_of(&bad, e.span);
        assert_eq!((l + 1, c + 1), (31, 48), "{}", e.message);
    }

    #[test]
    fn markup_error_is_structured() {
        let (p, t) = file("BallCard.j3ui");
        let bad = t.replace("<small>", "<blink>");
        let ui = analyze(&p, &[(p.clone(), bad.clone())]).unwrap();
        let e = ui.err.expect("error");
        assert_eq!(e.path, "ui/BallCard.j3ui");
        let ((l, _), _) = query::range_of(&bad, e.span);
        assert_eq!(bad.lines().nth(l as usize).unwrap().trim_start().get(..6), Some("<blink"));
    }

    #[test]
    fn hover_in_a_hole_gives_the_field_type() {
        let (p, t) = file("Upgrades.j3ui");
        let off = t.find("gold={sim.gold}").unwrap() + "gold={sim.g".len();
        let ui = analyze(&p, &[]).unwrap();
        let h = hover(&ui, &p, &t, off, &[]);
        let v = h.get("contents").and_then(|c| c.get("value")).and_then(Json::as_str).unwrap().to_string();
        assert!(v.contains("gold") && v.contains("int"), "{v}");
    }

    #[test]
    fn definition_in_a_hole_lands_in_the_j3ui() {
        let (p, t) = file("BallCard.j3ui");
        let off = t.find("{shop().name}").unwrap() + "{sh".len();
        let ui = analyze(&p, &[]).unwrap();
        let d = definition(&ui, &p, &t, off, &[]);
        let uri = d.get("uri").and_then(Json::as_str).unwrap().to_string();
        assert!(uri.ends_with("BallCard.j3ui"), "{uri}");
        let line = d.get("range").and_then(|r| r.get("start")).and_then(|s| s.get("line")).and_then(Json::as_i64).unwrap();
        assert!(t.lines().nth(line as usize).unwrap().contains("fn shop"), "line {line}");
    }

    #[test]
    fn member_completion_after_sim_dot() {
        let (p, t) = file("BallCard.j3ui");
        let at = t.find("<small>").unwrap();
        let ins = "<span>{sim.}</span>\n";
        let buf = format!("{}{ins}{}", &t[..at], &t[at..]);
        let off = at + "<span>{sim.".len();
        let items = complete_at(&p, &[(p.clone(), buf.clone())], &buf, off);
        let l = labels(&items);
        assert!(l.contains(&"gold") && l.contains(&"ballCount"), "{l:?}");
    }

    #[test]
    fn tag_name_and_prop_completion() {
        let (p, t) = file("Game.j3ui");
        let buf = format!("{t}\n<Ba");
        let items = complete_at(&p, &[(p.clone(), buf.clone())], &buf, buf.len());
        let l = labels(&items);
        assert!(l.contains(&"BallCard") && l.contains(&"BallShop") && !l.contains(&"div"), "{l:?}");
        let buf = format!("{t}\n<UpgradeRow ");
        let items = complete_at(&p, &[(p.clone(), buf.clone())], &buf, buf.len());
        let l = labels(&items);
        for want in ["label", "value", "cost", "gold", "on:buy"] {
            assert!(l.contains(&want), "{want} in {l:?}");
        }
        // BallCard has a non-ASCII `·` before the cursor: offsets are bytes everywhere.
        let (p2, t2) = file("BallCard.j3ui");
        let buf = format!("{t2}\n<UpgradeRow ");
        let items = complete_at(&p2, &[(p2.clone(), buf.clone())], &buf, buf.len());
        assert!(labels(&items).contains(&"on:buy"));
        let buf = format!("{t}\n<button ");
        let l: Vec<String> = complete_at(&p, &[(p.clone(), buf.clone())], &buf, buf.len()).into_iter().map(|c| c.label).collect();
        assert!(l.contains(&"on:click".to_string()) && l.contains(&"disabled".to_string()), "{l:?}");
    }

    fn big(name: &str) -> (PathBuf, String) {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/big_ui_test/ui").join(name);
        let t = std::fs::read_to_string(&p).unwrap();
        (p, t)
    }

    #[test]
    fn nested_components_complete_and_jump() {
        let (p, t) = big("game/Hud.j3ui");
        let off = t.find("<game.Bar").unwrap() + "<game.B".len();
        let ui = analyze(&p, &[]).unwrap();
        assert!(ui.err.is_none() && ui.prog.is_some());
        let d = definition(&ui, &p, &t, off, &[]);
        let uri = d.get("uri").and_then(Json::as_str).unwrap_or("").to_string();
        assert!(uri.ends_with("ui/game/Bar.j3ui"), "{uri}");
        let buf = format!("{t}\n<game.Ba");
        let items = complete_at(&p, &[(p.clone(), buf.clone())], &buf, buf.len());
        let bar = items.iter().find(|c| c.label == "game.Bar").expect("game.Bar offered");
        assert_eq!(bar.insert, "Bar");
        assert!(labels(&items).contains(&"game.Cooldown") == false);
        let buf = format!("{t}\n<game.Bar ");
        let l: Vec<String> = complete_at(&p, &[(p.clone(), buf.clone())], &buf, buf.len()).into_iter().map(|c| c.label).collect();
        assert!(l.contains(&"value".to_string()) && l.contains(&"max".to_string()), "{l:?}");
        // a root file offers the root and nested names alike
        let (sp, st) = big("Shell.j3ui");
        for (pre, want) in [("<pa", "pages.Text"), ("<po", "post.Glass")] {
            let buf = format!("{st}\n{pre}");
            let l: Vec<String> = complete_at(&sp, &[(sp.clone(), buf.clone())], &buf, buf.len()).into_iter().map(|c| c.label).collect();
            assert!(l.contains(&want.to_string()), "{l:?}");
        }
    }

    #[test]
    fn css_completion_from_the_tables() {
        let (p, t) = file("BallCard.j3ui");
        let at = t.find(".card:hover").unwrap();
        let with = |ins: &str| format!("{}{ins}\n{}", &t[..at], &t[at..]);
        let buf = with(".x { flex-dir");
        let items = complete_at(&p, &[(p.clone(), buf.clone())], &buf, at + ".x { flex-dir".len());
        assert_eq!(labels(&items), ["flex-direction"]);
        let buf = with(".x { justify-content: space-b");
        let items = complete_at(&p, &[(p.clone(), buf.clone())], &buf, at + ".x { justify-content: space-b".len());
        assert_eq!(labels(&items), ["space-between"]);
        let buf = with(".x { color: var(--ac");
        let items = complete_at(&p, &[(p.clone(), buf.clone())], &buf, at + ".x { color: var(--ac".len());
        assert!(labels(&items).contains(&"--accent"), "{:?}", labels(&items));
    }

    #[test]
    fn j3ui_feature_completion() {
        let (p, t) = file("BallCard.j3ui");
        let run = |buf: String, off: usize| -> Vec<String> { complete_at(&p, &[(p.clone(), buf.clone())], &buf, off).into_iter().map(|c| c.label).collect() };
        let b = format!("{t}\n<div ");
        let l = run(b.clone(), b.len());
        assert!(l.contains(&"slot".to_string()) && l.contains(&"style:name".to_string()), "{l:?}");
        let b = format!("{t}\n<sl");
        assert!(run(b.clone(), b.len()).contains(&"slot".to_string()));
        let b = format!("{t}\n<comp");
        assert!(run(b.clone(), b.len()).contains(&"component".to_string()));
        let b = format!("{t}\n<component ");
        let l = run(b.clone(), b.len());
        assert!(l.contains(&"this".to_string()) && l.contains(&"of".to_string()), "{l:?}");
        let b = format!("{t}\n<div>{{#ma");
        assert_eq!(run(b.clone(), b.len())[..1], ["match".to_string()]);
        let b = format!("{t}\n<div>{{:ca");
        assert_eq!(run(b.clone(), b.len())[..1], ["case".to_string()]);
        let at = t.find("<script>").unwrap() + "<script>\n".len();
        let b = format!("{}onF\n{}", &t[..at], &t[at..]);
        assert!(run(b.clone(), at + 3).contains(&"onFrame".to_string()));
        let at = t.find(".card:hover").unwrap();
        let with = |ins: &str| format!("{}{ins}\n{}", &t[..at], &t[at..]);
        let b = with(".x { width: cl");
        assert!(run(b.clone(), at + ".x { width: cl".len()).contains(&"clamp(".to_string()));
        let b = with(".x { width: 50vm");
        assert_eq!(run(b.clone(), at + ".x { width: 50vm".len()), ["50vmin", "50vmax"]);
        let b = with(".x { flex-wr");
        assert_eq!(run(b.clone(), at + ".x { flex-wr".len()), ["flex-wrap"]);
        let b = with(".x { grid-auto-fl");
        assert_eq!(run(b.clone(), at + ".x { grid-auto-fl".len()), ["grid-auto-flow"]);
        let b = with(".x { user-select: n");
        assert_eq!(run(b.clone(), at + ".x { user-select: n".len()), ["none"]);
    }
}
