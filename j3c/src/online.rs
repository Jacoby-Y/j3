//! Online assets: search and fetch CC0 models, materials, and skies from Poly Haven,
//! ambientCG, and Kenney. HTTP goes through the host's `curl` (no crates), zips are read here
//! with `png::inflate`, and downloads are cached under `~/.cache/j3/assets/<source>/`. Every
//! asset that lands in a project gets one line in its `assets/CREDITS.txt`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::lsp::json::{self, Json};

mod pack;
mod sources;
pub(crate) use pack::{hdr_to_png, pack_thumbs, thumb, unzip};
pub(crate) use sources::*;

pub(crate) struct Hit {
    pub source: &'static str,
    pub id: String,
    pub kind: &'static str,
    pub name: String,
    pub author: String,
    pub thumb: String,
}

pub(crate) enum Fetched {
    /// A model file in the cache, ready for `editor::import_asset`.
    Model(PathBuf),
    /// A project-relative `.j3mat`.
    Material(String),
    /// A Kenney pack's model files in the cache; fetch again with `file` to import one.
    Pack(Vec<PathBuf>),
    /// A project-relative equirect PNG for a `builtin:sky` mesh.
    Sky(String),
}

pub(crate) const SOURCES: &[&str] = &["polyhaven", "ambientcg", "kenney"];
pub(crate) const KINDS: &[&str] = &["any", "model", "material", "sky"];

/// Kenney has no API, so its packs are listed by hand (slug, name).
pub(crate) const KENNEY: &[(&str, &str)] = &[
    ("fantasy-town-kit", "Fantasy Town Kit"),
    ("nature-kit", "Nature Kit"),
    ("castle-kit", "Castle Kit"),
    ("furniture-kit", "Furniture Kit"),
    ("platformer-kit", "Platformer Kit"),
    ("city-kit-suburban", "City Kit (Suburban)"),
    ("city-kit-commercial", "City Kit (Commercial)"),
    ("survival-kit", "Survival Kit"),
    ("pirate-kit", "Pirate Kit"),
    ("graveyard-kit", "Graveyard Kit"),
    ("tower-defense-kit", "Tower Defense Kit"),
    ("car-kit", "Car Kit"),
    ("food-kit", "Food Kit"),
    ("mini-dungeon", "Mini Dungeon"),
    ("space-kit", "Space Kit"),
];

const MAX_HITS: usize = 40;
const DAY: Duration = Duration::from_secs(24 * 3600);
const UA: &str = concat!("j3c/", env!("CARGO_PKG_VERSION"), " (+https://github.com/Jacoby-Y)");

/// `id` when it is 1-80 of `[A-Za-z0-9_-]`, so it is safe in a path and a URL.
pub(crate) fn safe_id(id: &str) -> Result<&str, String> {
    let ok = (1..=80).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if ok { Ok(id) } else { Err(format!("bad asset id `{id}`: letters, digits, _ and - only")) }
}

/// A relative path from the network that stays inside the directory it is joined to.
fn safe_rel(rel: &str) -> Result<&str, String> {
    let bad = rel.is_empty() || rel.starts_with('/') || rel.contains(['\\', ':']) || rel.split('/').any(|c| c == "..");
    if bad { Err(format!("unsafe path `{rel}` in download")) } else { Ok(rel) }
}

fn https(url: &str) -> Result<String, String> {
    if url.starts_with("https://") { Ok(url.to_string()) } else { Err(format!("not an https URL: {url}")) }
}

fn matches(query: &str, fields: &[&str]) -> bool {
    let q = query.trim().to_lowercase();
    q.is_empty() || fields.iter().any(|f| f.to_lowercase().contains(&q))
}

fn finish(mut hits: Vec<Hit>) -> Vec<Hit> {
    hits.sort_by_key(|h| h.name.to_lowercase());
    hits.truncate(MAX_HITS);
    hits
}

fn strs(j: Option<&Json>) -> Vec<&str> {
    match j {
        Some(Json::Arr(xs)) => xs.iter().filter_map(Json::as_str).collect(),
        _ => Vec::new(),
    }
}

/// Poly Haven's `authors` object, its keys joined.
fn authors(j: Option<&Json>) -> String {
    match j {
        Some(Json::Obj(ps)) if !ps.is_empty() => ps.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>().join(", "),
        _ => "Poly Haven".into(),
    }
}


/// Percent-encode all but RFC 3986 unreserved characters.
pub(crate) fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Append `line` to `<root>/assets/CREDITS.txt` unless a line with the same first word
/// (`source:id`) is there.
pub(crate) fn credit(root: &Path, line: &str) -> Result<(), String> {
    let path = root.join("assets/CREDITS.txt");
    let key = line.split_whitespace().next().unwrap_or("");
    let old = fs::read_to_string(&path).unwrap_or_default();
    if old.lines().any(|l| l.split_whitespace().next() == Some(key)) { return Ok(()); }
    fs::create_dir_all(root.join("assets")).map_err(|e| e.to_string())?;
    let sep = if old.is_empty() || old.ends_with('\n') { "" } else { "\n" };
    fs::write(&path, format!("{old}{sep}{line}\n")).map_err(|e| format!("{}: {e}", path.display()))
}


// ---- network ----

fn cache(source: &str) -> Result<PathBuf, String> {
    crate::editor::cache_root().map(|r| r.join("assets").join(source)).ok_or_else(|| "no cache directory (set HOME)".into())
}

/// Download `url` to `dest` with curl, unless `dest` exists.
pub(crate) fn download(url: &str, dest: &Path) -> Result<(), String> {
    if dest.is_file() { return Ok(()); }
    https(url)?;
    if let Some(parent) = dest.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
    let mut part = dest.as_os_str().to_owned();
    part.push(".part");
    let out = Command::new("curl")
        .args(["-fsSL", "--proto", "=https", "--proto-redir", "=https", "--connect-timeout", "15", "--max-time", "600", "-A", UA, "-o"])
        .arg(&part)
        .arg(url)
        .output();
    match out {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err("curl not found; install curl".into()),
        Err(e) => Err(format!("curl: {e}")),
        Ok(o) if !o.status.success() => {
            let _ = fs::remove_file(&part);
            let err: String = String::from_utf8_lossy(&o.stderr).trim().chars().take(300).collect();
            Err(format!("download {url}: {err}"))
        }
        Ok(_) => fs::rename(&part, dest).map_err(|e| e.to_string()),
    }
}

/// `url`'s text, cached at `dest` for `max_age`.
fn get(url: &str, dest: &Path, max_age: Duration) -> Result<String, String> {
    let fresh = fs::metadata(dest).and_then(|m| m.modified()).is_ok_and(|t| t.elapsed().is_ok_and(|a| a < max_age));
    if !fresh {
        let _ = fs::remove_file(dest);
        download(url, dest)?;
    }
    fs::read_to_string(dest).map_err(|e| e.to_string())
}

/// Up to 40 hits. `source` is `any` or one of `SOURCES`; `kind` one of `KINDS`. `any` skips
/// sources that fail unless all do.
pub(crate) fn search(source: &str, query: &str, kind: &str) -> Result<Vec<Hit>, String> {
    if !KINDS.contains(&kind) { return Err(format!("kind `{kind}`: expected any, model, material, or sky")); }
    let sources = match source {
        "any" => SOURCES,
        s if SOURCES.contains(&s) => std::slice::from_ref(SOURCES.iter().find(|x| **x == s).unwrap()),
        s => return Err(format!("source `{s}`: expected any, polyhaven, ambientcg, or kenney")),
    };
    let mut hits = Vec::new();
    let mut errors = Vec::new();
    for s in sources {
        match search_one(s, query, kind) {
            Ok(h) => hits.extend(h),
            Err(e) => errors.push(format!("{s}: {e}")),
        }
    }
    if errors.len() == sources.len() { return Err(errors.join("; ")); }
    Ok(finish(hits))
}

fn search_one(source: &str, query: &str, kind: &str) -> Result<Vec<Hit>, String> {
    let want = |k: &str| kind == "any" || kind == k;
    match source {
        "polyhaven" => {
            let mut hits = Vec::new();
            for (k, list) in [("model", "models"), ("sky", "hdris")] {
                if !want(k) { continue; }
                let url = format!("https://api.polyhaven.com/assets?type={list}");
                hits.extend(polyhaven_hits(&get(&url, &cache(source)?.join(format!("{list}.json")), DAY)?, k, query)?);
            }
            Ok(hits)
        }
        "ambientcg" if want("material") => {
            let url = format!(
                "https://ambientcg.com/api/v2/full_json?q={}&type=Material&limit={MAX_HITS}&include=previewData,downloadData",
                url_encode(query.trim())
            );
            ambientcg_hits(&get(&url, &cache(source)?.join("search.json"), Duration::ZERO)?)
        }
        "kenney" if want("model") => Ok(kenney_hits(query)),
        _ => Ok(Vec::new()),
    }
}


/// Download (or reuse) asset `id` and put what the project needs into it; see `Fetched`.
/// `id` may be `<pack>/<file>`, the same as passing `file`.
pub(crate) fn fetch(root: &Path, source: &str, id: &str, file: Option<&str>) -> Result<Fetched, String> {
    let (id, file) = match (id.split_once('/'), file) {
        (Some((pack, f)), None) => (pack, Some(f)),
        _ => (id, file),
    };
    safe_id(id)?;
    let dir = cache(source)?.join(id);
    let write = |rel: &str, bytes: &[u8]| -> Result<(), String> {
        let path = root.join(rel);
        if let Some(parent) = path.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
        fs::write(&path, bytes).map_err(|e| format!("{rel}: {e}"))
    };
    match source {
        "polyhaven" => {
            let files = get(&format!("https://api.polyhaven.com/files/{id}"), &dir.join("files.json"), DAY)?;
            let info = json::parse(&get(&format!("https://api.polyhaven.com/info/{id}"), &dir.join("info.json"), DAY)?)?;
            let name = info.get("name").and_then(Json::as_str).unwrap_or(id);
            let line = format!("polyhaven:{id}  {name} — {} — CC0 — https://polyhaven.com/a/{id}", authors(info.get("authors")));
            let j = json::parse(&files)?;
            if j.get("hdri").is_some() {
                let hdr = dir.join(format!("{id}.hdr"));
                download(&polyhaven_hdr_url(&files)?, &hdr)?;
                let rel = format!("assets/skies/{id}.png");
                write(&rel, &hdr_to_png(&fs::read(&hdr).map_err(|e| e.to_string())?)?)?;
                credit(root, &line)?;
                Ok(Fetched::Sky(rel))
            } else if j.get("gltf").is_some() {
                // Each file lands whole (curl writes .part, then renames), so a retry resumes.
                let files = polyhaven_model_files(&files)?;
                for (rel, url) in &files { download(url, &dir.join(rel))?; }
                credit(root, &line)?;
                Ok(Fetched::Model(dir.join(&files[0].0)))
            } else {
                Err(format!("polyhaven:{id} is a texture set; use an ambientCG material instead"))
            }
        }
        "ambientcg" => {
            let json = get(&format!("https://ambientcg.com/api/v2/full_json?id={id}&include=downloadData"), &dir.join("info.json"), DAY)?;
            let name = ambientcg_hits(&json)?.into_iter().find(|h| h.id == id).map_or(id.to_string(), |h| h.name);
            let pack = unpack(&ambientcg_zip_url(&json, id)?, &dir)?;
            let color = walk(&pack).into_iter().find(|p| p.to_string_lossy().to_lowercase().ends_with("_color.jpg"));
            let color = color.ok_or(format!("ambientcg:{id} has no _Color.jpg"))?;
            let tex = format!("assets/textures/{id}_color.jpg");
            write(&tex, &fs::read(&color).map_err(|e| e.to_string())?)?;
            let rel = format!("assets/materials/{id}.j3mat");
            // An existing material may have the person's edits; keep it.
            if !root.join(&rel).exists() { write(&rel, format!("texture s={tex}\nshine f=0.1\n").as_bytes())?; }
            credit(root, &format!("ambientcg:{id}  {name} — ambientCG — CC0 — https://ambientcg.com/view?id={id}"))?;
            Ok(Fetched::Material(rel))
        }
        "kenney" => {
            let name = KENNEY.iter().find(|k| k.0 == id).map_or(id, |k| k.1);
            let pack = if dir.join("done").is_file() {
                dir.join("files")
            } else {
                let html = get(&format!("https://kenney.nl/assets/{id}"), &dir.join("page.html"), DAY)?;
                unpack(&kenney_zip_url(&html, id)?, &dir)?
            };
            let models = pack_models(&pack);
            if models.is_empty() { return Err(format!("kenney:{id} has no .glb or .gltf models")); }
            let Some(file) = file else { return Ok(Fetched::Pack(models)) };
            let named = |f: &dyn Fn(&Path) -> bool| models.iter().filter(|p| f(p)).cloned().collect::<Vec<_>>();
            let mut found = named(&|p| p.file_name().is_some_and(|n| n == file));
            if found.is_empty() { found = named(&|p| p.file_stem().is_some_and(|n| n == file)); }
            let [model] = &found[..] else {
                return Err(format!("kenney:{id} has {} models named `{file}`; call without file to list them", found.len()));
            };
            credit(root, &format!("kenney:{id}  {name} — Kenney — CC0 — https://kenney.nl/assets/{id}"))?;
            Ok(Fetched::Model(model.clone()))
        }
        s => Err(format!("source `{s}`: expected polyhaven, ambientcg, or kenney")),
    }
}

/// Download and unpack the zip at `url` into `<dir>/files` once (`<dir>/done` marks it whole).
fn unpack(url: &str, dir: &Path) -> Result<PathBuf, String> {
    let files = dir.join("files");
    if dir.join("done").is_file() { return Ok(files); }
    let zip = dir.join("pack.zip");
    download(url, &zip)?;
    let _ = fs::remove_dir_all(&files);
    unzip(&fs::read(&zip).map_err(|e| e.to_string())?, &files)?;
    let _ = fs::remove_file(&zip);
    fs::write(dir.join("done"), "").map_err(|e| e.to_string())?;
    Ok(files)
}

/// Every file under `dir`, sorted; symlinks aren't followed.
fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in fs::read_dir(dir).into_iter().flatten().flatten() {
        if e.file_type().is_ok_and(|t| t.is_dir()) { out.extend(walk(&e.path())); } else { out.push(e.path()); }
    }
    out.sort();
    out
}

/// A pack's models under `Models/`, only the `GLB format` folder when it has one.
fn pack_models(pack: &Path) -> Vec<PathBuf> {
    let has = |p: &Path, name: &str| p.components().any(|c| c.as_os_str() == name);
    let models: Vec<PathBuf> = walk(pack)
        .into_iter()
        .filter(|p| has(p, "Models") && matches!(p.extension().and_then(|e| e.to_str()), Some("glb" | "gltf")))
        .collect();
    if models.iter().any(|p| has(p, "GLB format")) { models.into_iter().filter(|p| has(p, "GLB format")).collect() } else { models }
}

#[cfg(test)]
mod tests;
