//! Open documents, the cursor of a request, and the shared program lookup.

use super::*;

pub(super) struct Cursor {
    pub(super) uri: String,
    pub(super) path: PathBuf,
    pub(super) path_str: String,
    pub(super) src: String,
    pub(super) offset: u32,
}

pub(super) fn doc_parts(msg: &Json, docs: &HashMap<String, String>) -> Option<(String, PathBuf, String)> {
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

pub(super) fn cursor(msg: &Json, docs: &HashMap<String, String>) -> Option<Cursor> {
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

pub(super) fn doc_cursor(msg: &Json, docs: &HashMap<String, String>) -> Option<Cursor> {
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

pub(super) fn cursor_range(msg: &Json, docs: &HashMap<String, String>) -> Option<Cursor> {
    doc_cursor(msg, docs)
}

pub(super) fn program_for<'a>(
    path: &Path,
    docs: &HashMap<String, String>,
    cache: &'a mut ProgramCache,
) -> Option<&'a crate::check::Program> {
    let key = docs_key(docs);
    let path = path.to_path_buf();
    let overlays = overlays_from(docs);
    cache.get(key, || query::analyze(&path, &overlays).ok())
}

pub(super) fn has_doc_overlay(docs: &HashMap<String, String>, path: &Path) -> bool {
    docs.keys()
        .filter_map(|uri| uri_to_path(uri))
        .any(|p| paths_match(&p, path))
}

pub(super) fn pos_query<F>(
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

/// An error path from the checker; `.j3ui` errors are relative to the project root.

pub(super) fn open_docs(docs: &HashMap<String, String>) -> Vec<(String, PathBuf)> {
    docs.iter()
        .filter_map(|(uri, _)| uri_to_path(uri).map(|p| (uri.clone(), p)))
        .collect()
}

pub(super) fn overlays_from(docs: &HashMap<String, String>) -> Vec<(PathBuf, String)> {
    docs.iter()
        .filter_map(|(uri, text)| uri_to_path(uri).map(|p| (p, text.clone())))
        .collect()
}

pub(super) fn src_of<'a>(docs: &'a HashMap<String, String>, path: &str) -> &'a str {
    for (uri, text) in docs {
        if let Some(p) = uri_to_path(uri) {
            if p.display().to_string() == path || paths_match(&p, Path::new(path)) {
                return text;
            }
        }
    }
    ""
}

pub(super) fn paths_match(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}
