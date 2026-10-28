//! Online assets: search and fetch CC0 models, materials, and skies from Poly Haven,
//! ambientCG, and Kenney. HTTP goes through the host's `curl` (no crates), zips are read here
//! with `png::inflate`, and downloads are cached under `~/.cache/j3/assets/<source>/`. Every
//! asset that lands in a project gets one line in its `assets/CREDITS.txt`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::lsp::json::{self, Json};

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

/// Hits in a Poly Haven `/assets?type=...` list (an object keyed by id).
pub(crate) fn polyhaven_hits(list_json: &str, kind: &'static str, query: &str) -> Result<Vec<Hit>, String> {
    let Json::Obj(items) = json::parse(list_json)? else { return Err("Poly Haven: expected an object of assets".into()) };
    let mut hits = Vec::new();
    for (id, a) in &items {
        if safe_id(id).is_err() { continue; }
        let name = a.get("name").and_then(Json::as_str).unwrap_or(id);
        let mut fields = vec![id.as_str(), name];
        fields.extend(strs(a.get("tags")));
        fields.extend(strs(a.get("categories")));
        if !matches(query, &fields) { continue; }
        let thumb = a.get("thumbnail_url").and_then(Json::as_str).and_then(|u| https(u).ok()).unwrap_or_default();
        hits.push(Hit { source: "polyhaven", id: id.clone(), kind, name: name.into(), author: authors(a.get("authors")), thumb });
    }
    Ok(finish(hits))
}

/// A Poly Haven model's 1k glTF files as (relative path, url), the `.gltf` first.
pub(crate) fn polyhaven_model_files(files_json: &str) -> Result<Vec<(String, String)>, String> {
    let j = json::parse(files_json)?;
    let g = j.get("gltf").and_then(|g| g.get("1k")).and_then(|g| g.get("gltf")).ok_or("Poly Haven: no 1k glTF for this asset")?;
    let url = https(g.get("url").and_then(Json::as_str).ok_or("Poly Haven: glTF without a url")?)?;
    let name = url.rsplit('/').next().unwrap_or("");
    let mut out = vec![(safe_rel(name)?.to_string(), url.clone())];
    if let Some(Json::Obj(inc)) = g.get("include") {
        for (rel, f) in inc {
            let url = f.get("url").and_then(Json::as_str).ok_or("Poly Haven: include without a url")?;
            out.push((safe_rel(rel)?.to_string(), https(url)?));
        }
    }
    Ok(out)
}

/// The 1k `.hdr` of a Poly Haven sky.
pub(crate) fn polyhaven_hdr_url(files_json: &str) -> Result<String, String> {
    let j = json::parse(files_json)?;
    let url = ["hdri", "1k", "hdr", "url"].iter().try_fold(&j, |j, k| j.get(k)).and_then(Json::as_str);
    https(url.ok_or("Poly Haven: no 1k .hdr for this sky")?)
}

/// Hits in an ambientCG `full_json` reply (the API already applied the query).
pub(crate) fn ambientcg_hits(json: &str) -> Result<Vec<Hit>, String> {
    let j = json::parse(json)?;
    let Some(Json::Arr(found)) = j.get("foundAssets") else { return Err("ambientCG: no foundAssets in the reply".into()) };
    let mut hits = Vec::new();
    for a in found {
        let Some(id) = a.get("assetId").and_then(Json::as_str).filter(|id| safe_id(id).is_ok()) else { continue };
        let thumb = a.get("previewImage").and_then(|p| p.get("256-PNG")).and_then(Json::as_str).and_then(|u| https(u).ok());
        hits.push(Hit {
            source: "ambientcg",
            id: id.into(),
            kind: "material",
            name: a.get("displayName").and_then(Json::as_str).unwrap_or(id).into(),
            author: "ambientCG".into(),
            thumb: thumb.unwrap_or_default(),
        });
    }
    Ok(finish(hits))
}

/// The `1K-JPG` zip of ambientCG asset `id`.
pub(crate) fn ambientcg_zip_url(json: &str, id: &str) -> Result<String, String> {
    let j = json::parse(json)?;
    let Some(Json::Arr(found)) = j.get("foundAssets") else { return Err("ambientCG: no foundAssets in the reply".into()) };
    let a = found.iter().find(|a| a.get("assetId").and_then(Json::as_str) == Some(id)).ok_or(format!("ambientCG has no asset `{id}`"))?;
    let path = ["downloadFolders", "default", "downloadFiletypeCategories", "zip", "downloads"];
    let Some(Json::Arr(downloads)) = path.iter().try_fold(a, |j, k| j.get(k)) else {
        return Err(format!("ambientCG: `{id}` has no zip downloads"));
    };
    let d = downloads.iter().find(|d| d.get("attribute").and_then(Json::as_str) == Some("1K-JPG")).ok_or(format!("ambientCG: `{id}` has no 1K-JPG zip"))?;
    https(d.get("downloadLink").and_then(Json::as_str).ok_or("ambientCG: download without a link")?)
}

pub(crate) fn kenney_hits(query: &str) -> Vec<Hit> {
    let hits = KENNEY.iter().filter(|(slug, name)| matches(query, &[slug, name])).map(|(slug, name)| Hit {
        source: "kenney",
        id: slug.to_string(),
        kind: "pack",
        name: name.to_string(),
        author: "Kenney".into(),
        thumb: String::new(),
    });
    finish(hits.collect())
}

/// The one pack zip linked from `https://kenney.nl/assets/<slug>`.
pub(crate) fn kenney_zip_url(html: &str, slug: &str) -> Result<String, String> {
    let prefix = format!("https://kenney.nl/media/pages/assets/{slug}/");
    let mut zips: Vec<&str> = html
        .match_indices(&prefix)
        .map(|(i, _)| {
            let rest = &html[i..];
            &rest[..rest.find(|c: char| c == '"' || c == '\'' || c.is_whitespace()).unwrap_or(rest.len())]
        })
        .filter(|u| u.ends_with(".zip"))
        .collect();
    zips.sort();
    zips.dedup();
    match zips[..] {
        [one] => Ok(one.to_string()),
        [] => Err(format!("kenney.nl/assets/{slug} has no download link (a renamed pack?)")),
        _ => Err(format!("kenney.nl/assets/{slug} links {} zips; expected one", zips.len())),
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

/// Unpack `zip` into `dir`; returns how many files were written.
pub(crate) fn unzip(zip: &[u8], dir: &Path) -> Result<usize, String> {
    let bad = || "not a zip file (zip64 isn't supported)".to_string();
    let u16at = |i: usize| zip.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize).ok_or_else(bad);
    let u32at = |i: usize| zip.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize).ok_or_else(bad);
    let eocd = (0..zip.len().saturating_sub(21)).rev().take(65536 + 22).find(|&i| zip[i..i + 4] == [0x50, 0x4b, 5, 6]).ok_or_else(bad)?;
    let count = u16at(eocd + 10)?;
    let mut at = u32at(eocd + 16)?;
    if count == 0xffff || at == 0xffff_ffff { return Err(bad()); }
    let mut written = 0;
    for _ in 0..count {
        if zip.get(at..at + 4) != Some(&[0x50, 0x4b, 1, 2]) { return Err(bad()); }
        let (method, csize, size) = (u16at(at + 10)?, u32at(at + 20)?, u32at(at + 24)?);
        let (nlen, elen, clen, local) = (u16at(at + 28)?, u16at(at + 30)?, u16at(at + 32)?, u32at(at + 42)?);
        let name = String::from_utf8_lossy(zip.get(at + 46..at + 46 + nlen).ok_or_else(bad)?).into_owned();
        at += 46 + nlen + elen + clen;
        if name.ends_with('/') { continue; }
        safe_rel(&name)?;
        let start = local + 30 + u16at(local + 26)? + u16at(local + 28)?;
        let data = zip.get(start..start + csize).ok_or_else(bad)?;
        let bytes = match method {
            0 => data.to_vec(),
            8 => crate::png::inflate(data, size).filter(|b| b.len() == size).ok_or(format!("{name}: bad compressed data"))?,
            m => return Err(format!("{name}: zip method {m} isn't supported")),
        };
        let dest = dir.join(&name);
        if let Some(parent) = dest.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
        fs::write(&dest, bytes).map_err(|e| format!("{}: {e}", dest.display()))?;
        written += 1;
    }
    Ok(written)
}

/// Radiance `.hdr` (flat or new-style RLE, `-Y h +X w`) tone-mapped (Reinhard, gamma 2.2) to
/// an RGBA8 PNG.
pub(crate) fn hdr_to_png(b: &[u8]) -> Result<Vec<u8>, String> {
    let (w, h, px) = hdr_decode(b)?;
    Ok(crate::png::encode_rgba(w, h, &px))
}

fn hdr_decode(b: &[u8]) -> Result<(usize, usize, Vec<u8>), String> {
    let bad = |m: &str| format!(".hdr: {m}");
    if !b.starts_with(b"#?") { return Err(bad("not a Radiance file")); }
    let mut at = 0;
    let mut line = || -> Result<&[u8], String> {
        let end = at + b[at..].iter().position(|&c| c == b'\n').ok_or_else(|| bad("header never ends"))?;
        let l = &b[at..end];
        at = end + 1;
        Ok(l)
    };
    while !line()?.is_empty() {}
    let res = String::from_utf8_lossy(line()?).into_owned();
    let parts: Vec<&str> = res.split_whitespace().collect();
    let (w, h) = match parts[..] {
        ["-Y", h, "+X", w] => (w.parse::<usize>().map_err(|_| bad("bad width"))?, h.parse::<usize>().map_err(|_| bad("bad height"))?),
        _ => return Err(bad("only `-Y H +X W` images are supported")),
    };
    if w == 0 || h == 0 || w * h > 16384 * 8192 { return Err(bad("bad size")); }
    let mut rgbe = vec![0u8; w * h * 4];
    let short = || bad("data ends early");
    for y in 0..h {
        let row = &mut rgbe[y * w * 4..(y + 1) * w * 4];
        let rle = (8..32768).contains(&w) && b.get(at..at + 2) == Some(&[2, 2]) && b.get(at + 2).is_some_and(|&c| c & 0x80 == 0);
        if !rle {
            row.copy_from_slice(b.get(at..at + w * 4).ok_or_else(short)?);
            at += w * 4;
            continue;
        }
        if ((b[at + 2] as usize) << 8 | *b.get(at + 3).ok_or_else(short)? as usize) != w { return Err(bad("scanline width mismatch")); }
        at += 4;
        for c in 0..4 {
            let mut x = 0;
            while x < w {
                let n = *b.get(at).ok_or_else(short)? as usize;
                at += 1;
                if n > 128 {
                    let v = *b.get(at).ok_or_else(short)?;
                    at += 1;
                    if x + n - 128 > w { return Err(bad("run past the scanline")); }
                    for _ in 0..n - 128 { row[x * 4 + c] = v; x += 1; }
                } else {
                    if n == 0 || x + n > w { return Err(bad("bad run")); }
                    for &v in b.get(at..at + n).ok_or_else(short)? { row[x * 4 + c] = v; x += 1; }
                    at += n;
                }
            }
        }
    }
    let mut out = Vec::with_capacity(w * h * 4);
    for p in rgbe.chunks(4) {
        let scale = if p[3] == 0 { 0.0 } else { 2f32.powi(p[3] as i32 - 136) };
        for &m in &p[..3] {
            let v = m as f32 * scale;
            out.push(((v / (1.0 + v)).powf(1.0 / 2.2) * 255.0).round() as u8);
        }
        out.push(255);
    }
    Ok((w, h, out))
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

/// The hit's thumbnail PNG in the cache, downloaded if needed.
pub(crate) fn thumb(hit: &Hit) -> Option<PathBuf> {
    if hit.thumb.is_empty() { return None; }
    let dest = cache(hit.source).ok()?.join("thumbs").join(format!("{}.png", hit.id));
    download(&hit.thumb, &dest).ok().map(|_| dest)
}

/// Small previews for a pack's `models`, from the pictures the pack ships with the same
/// names (Kenney: `Isometric/` or `Previews/`), cached as 128px PNGs.
pub(crate) fn pack_thumbs(source: &str, id: &str, models: &[PathBuf]) -> Vec<Option<PathBuf>> {
    let (Some(first), Ok(dir)) = (models.first(), cache(source)) else { return vec![None; models.len()] };
    let pack = first.ancestors().find(|a| a.file_name().is_some_and(|n| n == "Models")).and_then(Path::parent);
    let mut pictures = std::collections::HashMap::new();
    for p in pack.map(walk).unwrap_or_default() {
        let folder = p.parent().and_then(|d| d.file_name()).and_then(|n| n.to_str()).unwrap_or("");
        if p.extension().is_some_and(|e| e == "png") && matches!(folder, "Isometric" | "Previews") {
            pictures.insert(p.file_stem().unwrap_or_default().to_os_string(), p);
        }
    }
    let out = dir.join(id).join("thumbs");
    let _ = fs::create_dir_all(&out);
    let one = |model: &PathBuf| -> Option<PathBuf> {
        let stem = model.file_stem()?;
        let dest = out.join(stem).with_extension("png");
        if dest.is_file() { return Some(dest); }
        let (w, h, px) = crate::png::decode(&fs::read(pictures.get(stem)?).ok()?)?;
        let (tw, th) = shrink(w, h, 128);
        fs::write(&dest, crate::png::encode_rgba(tw, th, &box_scale(&px, w, h, tw, th))).ok()?;
        Some(dest)
    };
    // ponytail: 8 threads over a few hundred pictures, once per pack; cached after that.
    std::thread::scope(|s| {
        let chunk = models.len().div_ceil(8).max(1);
        let jobs: Vec<_> = models.chunks(chunk).map(|c| s.spawn(move || c.iter().map(one).collect::<Vec<_>>())).collect();
        jobs.into_iter().flat_map(|j| j.join().unwrap_or_default()).collect()
    })
}

/// `w`x`h` scaled to fit in `max`, at least 1x1.
fn shrink(w: usize, h: usize, max: usize) -> (usize, usize) {
    if w <= max && h <= max { return (w, h); }
    let s = max as f32 / w.max(h) as f32;
    (((w as f32 * s) as usize).max(1), ((h as f32 * s) as usize).max(1))
}

/// RGBA `px` (`w`x`h`) averaged down to `tw`x`th`.
fn box_scale(px: &[u8], w: usize, h: usize, tw: usize, th: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(tw * th * 4);
    for y in 0..th {
        let (y0, y1) = (y * h / th, ((y + 1) * h / th).max(y * h / th + 1));
        for x in 0..tw {
            let (x0, x1) = (x * w / tw, ((x + 1) * w / tw).max(x * w / tw + 1));
            let mut sum = [0u32; 4];
            for sy in y0..y1 {
                for sx in x0..x1 {
                    for c in 0..4 { sum[c] += px[(sy * w + sx) * 4 + c] as u32; }
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as u32;
            out.extend(sum.map(|v| (v / n) as u8));
        }
    }
    out
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
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("j3_online_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn ids_and_paths_are_checked() {
        assert!(safe_id("ArmChair_01").is_ok());
        for bad in ["", "../x", "a/b", "a b", &"x".repeat(81)] { assert!(safe_id(bad).is_err(), "{bad}"); }
        assert!(safe_rel("textures/a.jpg").is_ok());
        for bad in ["../a", "a/../../b", "/etc/x", "a\\b", "C:x"] { assert!(safe_rel(bad).is_err(), "{bad}"); }
        assert_eq!(url_encode("red brick/ü"), "red%20brick%2F%C3%BC");
    }

    #[test]
    fn polyhaven_list_and_files() {
        let list = r#"{"ArmChair_01":{"name":"Arm Chair 01","type":2,"tags":["gothic","chair"],"categories":["furniture"],
            "authors":{"Kirill Sannikov":"All"},"thumbnail_url":"https://cdn.polyhaven.com/asset_img/thumbs/ArmChair_01.png?width=256"},
            "Barrel_01":{"name":"Barrel 01","tags":[],"categories":["props"],"authors":{"A":"All","B":"Mesh"},"thumbnail_url":"http://x"}}"#;
        let hits = polyhaven_hits(list, "model", "FURNITURE").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!((hits[0].id.as_str(), hits[0].name.as_str(), hits[0].author.as_str()), ("ArmChair_01", "Arm Chair 01", "Kirill Sannikov"));
        let all = polyhaven_hits(list, "model", "").unwrap();
        assert_eq!((all.len(), all[1].author.as_str(), all[1].thumb.as_str()), (2, "A, B", ""));

        let files = r#"{"gltf":{"1k":{"gltf":{"url":"https://dl.polyhaven.org/x/ArmChair_01_1k.gltf","include":{
            "ArmChair_01.bin":{"url":"https://dl.polyhaven.org/x/ArmChair_01.bin"},
            "textures/a_diff_1k.jpg":{"url":"https://dl.polyhaven.org/x/a_diff_1k.jpg"}}}}}}"#;
        let got = polyhaven_model_files(files).unwrap();
        assert_eq!(got[0].0, "ArmChair_01_1k.gltf");
        assert_eq!(got.len(), 3);
        assert!(polyhaven_model_files(&files.replace("textures/a", "../a")).is_err());
        assert!(polyhaven_model_files(&files.replace("https://dl.polyhaven.org/x/a_", "http://dl/a_")).is_err());
        let hdr = r#"{"hdri":{"1k":{"hdr":{"url":"https://dl.polyhaven.org/sky_1k.hdr"}}},"tonemapped":{}}"#;
        assert_eq!(polyhaven_hdr_url(hdr).unwrap(), "https://dl.polyhaven.org/sky_1k.hdr");
    }

    #[test]
    fn ambientcg_reply() {
        let json = r#"{"foundAssets":[{"assetId":"Ground111","displayName":"Ground 111",
            "previewImage":{"256-PNG":"https://acg-media.example/Ground111.png"},
            "downloadFolders":{"default":{"downloadFiletypeCategories":{"zip":{"downloads":[
              {"attribute":"2K-JPG","downloadLink":"https://ambientcg.com/get?file=Ground111_2K-JPG.zip"},
              {"attribute":"1K-JPG","downloadLink":"https://ambientcg.com/get?file=Ground111_1K-JPG.zip"}]}}}}}]}"#;
        let hits = ambientcg_hits(json).unwrap();
        assert_eq!((hits[0].id.as_str(), hits[0].name.as_str(), hits[0].kind), ("Ground111", "Ground 111", "material"));
        assert_eq!(ambientcg_zip_url(json, "Ground111").unwrap(), "https://ambientcg.com/get?file=Ground111_1K-JPG.zip");
        assert!(ambientcg_zip_url(json, "Bricks001").is_err());
    }

    #[test]
    fn kenney_list_and_page() {
        assert_eq!(kenney_hits("town")[0].id, "fantasy-town-kit");
        assert_eq!(kenney_hits("").len(), KENNEY.len());
        let html = r#"<img src="https://kenney.nl/media/pages/assets/nature-kit/2d81fc716e-1677698893/preview.png">
            <a href="https://kenney.nl/media/pages/assets/nature-kit/37ac38a37b-1677698939/kenney_nature-kit.zip">Download</a>
            <a href='https://kenney.nl/media/pages/assets/nature-kit/37ac38a37b-1677698939/kenney_nature-kit.zip'>again</a>"#;
        assert_eq!(kenney_zip_url(html, "nature-kit").unwrap(), "https://kenney.nl/media/pages/assets/nature-kit/37ac38a37b-1677698939/kenney_nature-kit.zip");
        assert!(kenney_zip_url(html, "castle-kit").is_err());
    }

    #[test]
    fn credit_once_per_key() {
        let root = temp("credit");
        credit(&root, "kenney:nature-kit  Nature Kit — Kenney — CC0 — https://kenney.nl/assets/nature-kit").unwrap();
        credit(&root, "kenney:nature-kit  again").unwrap();
        credit(&root, "kenney:nature-kit-2  other").unwrap();
        let text = fs::read_to_string(root.join("assets/CREDITS.txt")).unwrap();
        assert_eq!(text.lines().count(), 2, "{text}");
        let _ = fs::remove_dir_all(&root);
    }

    /// A zip with one stored and one deflated entry, built by hand.
    fn zip_of(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, data, deflate) in entries {
            let body = if *deflate {
                // Stored DEFLATE block: BFINAL=1, BTYPE=00, then LEN/NLEN.
                let mut z = vec![1u8];
                z.extend((data.len() as u16).to_le_bytes());
                z.extend((!(data.len() as u16)).to_le_bytes());
                z.extend_from_slice(data);
                z
            } else {
                data.to_vec()
            };
            let method: u16 = if *deflate { 8 } else { 0 };
            let offset = out.len() as u32;
            out.extend([0x50, 0x4b, 3, 4]);
            out.extend([0u8; 4]);
            out.extend(method.to_le_bytes());
            out.extend([0u8; 8]);
            out.extend((body.len() as u32).to_le_bytes());
            out.extend((data.len() as u32).to_le_bytes());
            out.extend((name.len() as u16).to_le_bytes());
            out.extend([0u8; 2]);
            out.extend(name.as_bytes());
            out.extend(&body);
            central.extend([0x50, 0x4b, 1, 2]);
            central.extend([0u8; 6]);
            central.extend(method.to_le_bytes());
            central.extend([0u8; 8]);
            central.extend((body.len() as u32).to_le_bytes());
            central.extend((data.len() as u32).to_le_bytes());
            central.extend((name.len() as u16).to_le_bytes());
            central.extend([0u8; 12]);
            central.extend(offset.to_le_bytes());
            central.extend(name.as_bytes());
        }
        let at = out.len() as u32;
        out.extend(&central);
        out.extend([0x50, 0x4b, 5, 6, 0, 0, 0, 0]);
        out.extend((entries.len() as u16).to_le_bytes());
        out.extend((entries.len() as u16).to_le_bytes());
        out.extend((central.len() as u32).to_le_bytes());
        out.extend(at.to_le_bytes());
        out.extend([0u8; 2]);
        out
    }

    #[test]
    fn unzip_writes_files_and_refuses_escapes() {
        let dir = temp("unzip");
        let zip = zip_of(&[("Models/GLB format/a.glb", b"glb", false), ("License.txt", b"CC0", true), ("Models/", b"", false)]);
        assert_eq!(unzip(&zip, &dir).unwrap(), 2);
        assert_eq!(fs::read(dir.join("License.txt")).unwrap(), b"CC0");
        assert_eq!(pack_models(&dir), vec![dir.join("Models/GLB format/a.glb")]);
        assert!(unzip(&zip_of(&[("../evil.txt", b"x", false)]), &dir).is_err());
        assert!(!dir.parent().unwrap().join("evil.txt").exists());
        assert!(unzip(b"not a zip at all, just some text here", &dir).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    fn hdr_file(w: usize, body: &[u8]) -> Vec<u8> {
        let mut b = format!("#?RADIANCE\nFORMAT=32-bit_rle_rgbe\n\n-Y 1 +X {w}\n").into_bytes();
        b.extend_from_slice(body);
        b
    }

    #[test]
    fn hdr_flat_and_rle() {
        // (128, 0, 64, e=129): v = m * 2^-7, so 1.0, 0, 0.5 -> Reinhard 0.5, 0, 1/3.
        let (w, h, px) = hdr_decode(&hdr_file(2, &[128, 0, 64, 129, 0, 0, 0, 0])).unwrap();
        let g = |v: f32| (v.powf(1.0 / 2.2) * 255.0).round() as u8;
        assert_eq!((w, h), (2, 1));
        assert_eq!(px, vec![g(0.5), 0, g(1.0 / 3.0), 255, 0, 0, 0, 255]);

        let pixels: Vec<[u8; 4]> = (0..10).map(|i| [if i < 6 { 200 } else { 10 * i as u8 }, 7, 7, 130]).collect();
        let flat: Vec<u8> = pixels.iter().flatten().copied().collect();
        let mut rle = vec![2, 2, 0, 10];
        rle.extend([128 + 6, 200, 4, 60, 70, 80, 90]); // R: a run of 6, then 4 literals
        for c in [7u8, 7, 130] { rle.extend([128 + 10, c]); }
        assert_eq!(hdr_decode(&hdr_file(10, &rle)).unwrap(), hdr_decode(&hdr_file(10, &flat)).unwrap());
        assert!(hdr_decode(b"P6\n1 1\n").is_err());
    }

    #[test]
    fn scaling_down() {
        assert_eq!(shrink(512, 256, 128), (128, 64));
        assert_eq!(shrink(64, 64, 128), (64, 64));
        let px = [0, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255];
        assert_eq!(box_scale(&px, 2, 2, 1, 1), vec![127, 127, 127, 255]);
    }

    #[test]
    #[ignore]
    fn network() {
        let cache = temp("net_cache");
        std::env::set_var("XDG_CACHE_HOME", &cache);
        let root = temp("net_project");
        assert!(search("kenney", "town", "any").unwrap().iter().any(|h| h.id == "fantasy-town-kit"));
        let Fetched::Pack(models) = fetch(&root, "kenney", "fantasy-town-kit", None).unwrap() else { panic!("not a pack") };
        assert!(models.len() >= 50, "{} models", models.len());
        let thumbs = pack_thumbs("kenney", "fantasy-town-kit", &models);
        assert!(thumbs.iter().filter(|t| t.is_some()).count() * 2 > models.len(), "most models have a preview");
        assert!(!search("polyhaven", "chair", "model").unwrap().is_empty());
        let Fetched::Model(gltf) = fetch(&root, "polyhaven", "ArmChair_01", None).unwrap() else { panic!("not a model") };
        assert!(gltf.with_file_name("ArmChair_01.bin").is_file(), "{}", gltf.display());
        assert!(!search("ambientcg", "brick", "any").unwrap().is_empty());
        let credits = fs::read_to_string(root.join("assets/CREDITS.txt")).unwrap();
        assert!(credits.contains("polyhaven:ArmChair_01  Arm Chair 01 — Kirill Sannikov — CC0"), "{credits}");
        let _ = fs::remove_dir_all(&cache);
        let _ = fs::remove_dir_all(&root);
    }
}
