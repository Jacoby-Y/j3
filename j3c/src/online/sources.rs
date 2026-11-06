//! Poly Haven, ambientCG and Kenney reply parsers: search hits and download URLs.

use super::*;

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
