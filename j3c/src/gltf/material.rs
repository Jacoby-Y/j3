use super::*;

pub(super) fn safe_stem(path: &Path) -> String {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("model");
    let text: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if text.is_empty() {
        "model".into()
    } else {
        text
    }
}

pub(super) fn unique_prefix(project: &Path, path: &Path) -> Result<String, String> {
    let stem = safe_stem(path);
    let folder = project.join("assets/imported");
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let existing: Vec<String> = fs::read_dir(&folder)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| entry.ok().and_then(|e| e.file_name().into_string().ok()))
        .collect();
    for n in 0..10000 {
        let prefix = if n == 0 {
            stem.clone()
        } else {
            format!("{stem}_{n}")
        };
        if !existing
            .iter()
            .any(|name| name.starts_with(&format!("{prefix}_")))
        {
            return Ok(prefix);
        }
    }
    Err("glTF: could not allocate an import name".into())
}

pub(super) fn write_images(
    root: &Json,
    path: &Path,
    buffers: &[Vec<u8>],
    project: &Path,
    prefix: &str,
    warnings: &mut Vec<String>,
) -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    let mut needed = HashSet::new();
    for material in root.get("materials").map(arr).unwrap_or(&[]) {
        if let Some(info) = material
            .get("pbrMetallicRoughness")
            .and_then(|pbr| pbr.get("baseColorTexture"))
        {
            let texture = item(root, "textures", usize_field(info, "index")?)?;
            needed.insert(usize_field(texture, "source")?);
        }
    }
    let images = root.get("images").map(arr).unwrap_or(&[]);
    for (i, image) in images.iter().enumerate() {
        if !needed.contains(&i) {
            result.push(String::new());
            continue;
        }
        let data = if let Some(uri) = optional_str(image, "uri") {
            match uri_bytes(path.parent().unwrap_or(Path::new(".")), uri) {
                Ok(bytes) => bytes,
                Err(_) => {
                    warn(warnings, "some textures could not be read");
                    result.push(String::new());
                    continue;
                }
            }
        } else if let Some(view_index) = optional_usize(image, "bufferView") {
            let view = item(root, "bufferViews", view_index)?;
            let bytes = buffers
                .get(usize_field(view, "buffer")?)
                .ok_or("glTF: image buffer missing")?;
            let offset = optional_usize(view, "byteOffset").unwrap_or(0);
            let len = usize_field(view, "byteLength")?;
            bytes
                .get(offset..offset + len)
                .ok_or("glTF: image data truncated")?
                .to_vec()
        } else {
            return Err(format!("glTF: image {i} has no data"));
        };
        let extension = if data.starts_with(b"\x89PNG\r\n\x1a\n") {
            "png"
        } else if data.starts_with(&[0xff, 0xd8]) {
            "jpg"
        } else {
            warn(warnings, "images that aren't PNG or JPEG were skipped");
            result.push(String::new());
            continue;
        };
        let rel = format!("assets/imported/{prefix}_image{i}.{extension}");
        fs::write(project.join(&rel), data).map_err(|e| e.to_string())?;
        result.push(rel);
    }
    Ok(result)
}

pub(super) fn material(
    root: &Json,
    primitive: &Json,
    images: &[String],
    warnings: &mut Vec<String>,
) -> Result<([f32; 3], String, usize, bool, i64, f32), String> {
    let Some(index) = optional_usize(primitive, "material") else {
        return Ok(([1.0; 3], String::new(), 0, false, 0, 1.0));
    };
    let mat = item(root, "materials", index)?;
    let alpha = match optional_str(mat, "alphaMode").unwrap_or("OPAQUE") {
        "MASK" => 1,
        "BLEND" => 2,
        _ => 0,
    };
    if alpha == 1 && mat.get("alphaCutoff").and_then(|c| c.as_f64()).is_some_and(|c| (c - 0.5).abs() > 0.001) {
        warn(warnings, "alpha cutoffs other than 0.5 were imported as 0.5");
    }
    let double_sided = matches!(mat.get("doubleSided"), Some(Json::Bool(true)));
    let pbr = mat.get("pbrMetallicRoughness").unwrap_or(&Json::Null);
    // ponytail: the renderer has no tangents or PBR maps; see "Rendering gaps" in EDITOR.md.
    if mat.get("normalTexture").or(mat.get("occlusionTexture")).or(pbr.get("metallicRoughnessTexture")).is_some() {
        warn(warnings, "normal, metal/roughness, and occlusion maps were ignored");
    }
    let factor = pbr
        .get("baseColorFactor")
        .map(|v| vector(Some(v), 4, 1.0))
        .unwrap_or(vec![1.0; 4]);
    let mut uv_set = 0;
    let texture = if let Some(info) = pbr.get("baseColorTexture") {
        uv_set = optional_usize(info, "texCoord").unwrap_or(0);
        // Only offset, rotation, or scale change the UVs; a bare texCoord doesn't.
        let transform = info.get("extensions").and_then(|e| e.get("KHR_texture_transform"));
        if transform.is_some_and(|t| ["offset", "rotation", "scale"].iter().any(|k| t.get(k).is_some())) {
            warn(warnings, "texture transforms were ignored");
        }
        let tex = item(root, "textures", usize_field(info, "index")?)?;
        let source = usize_field(tex, "source")?;
        images
            .get(source)
            .cloned()
            .ok_or("glTF: texture image missing")?
    } else {
        String::new()
    };
    Ok((
        [factor[0], factor[1], factor[2]],
        texture,
        uv_set,
        double_sided,
        alpha,
        factor[3],
    ))
}
