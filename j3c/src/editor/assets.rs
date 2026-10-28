use super::*;

/// One script prop, as the inspector shows it.
pub(crate) struct ScriptProp {
    pub name: String,
    /// Inspector type: `f32`, `int`, `bool`, `string`, `vec3`, or `object`.
    pub kind: String,
    /// The default as written in the script ("" when the prop is required).
    pub default: String,
    pub required: bool,
}

/// The props of the script at `rel` (e.g. `scripts/door.j3s`) the editor can set. Props of
/// other types stay at their defaults.
pub(crate) fn script_props(root: &Path, rel: &str) -> Result<Vec<ScriptProp>, String> {
    let text = fs::read_to_string(root.join(rel)).map_err(|e| format!("{rel}: {e}"))?;
    let parts: Vec<String> = std::iter::once("script".to_string())
        .chain(rel.trim_end_matches(".j3s").split('/').map(str::to_string))
        .collect();
    let file = crate::parse::parse_script(&text, crate::span::FileId(0), &parts)
        .map_err(|e| crate::parse::format_error(rel, &text, &e))?;
    let mut out = Vec::new();
    for item in &file.items {
        let crate::parse::Item::Props(block) = item else { continue };
        for field in &block.fields {
            let crate::parse::TypeKind::Named(ty) = &field.ty.kind else { continue };
            let kind = match ty.rsplit('.').next().unwrap_or(ty) {
                "f32" | "f64" | "float" => "f32",
                "int" | "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" => "int",
                "bool" => "bool",
                "string" => "string",
                "Vec3" => "vec3",
                "NodeId" => "object",
                _ => continue,
            };
            let folded = field.default.as_ref().map(|e| e.folded_literal());
            let default = match folded.as_ref().map(|e| &e.kind) {
                Some(crate::parse::ExprKind::Int(v)) => v.to_string(),
                Some(crate::parse::ExprKind::Float(v)) => v.to_string(),
                Some(crate::parse::ExprKind::Bool(v)) => v.to_string(),
                Some(crate::parse::ExprKind::String(b)) => String::from_utf8_lossy(b).into_owned(),
                _ => String::new(),
            };
            out.push(ScriptProp {
                name: field.name.name.clone(),
                kind: kind.to_string(),
                default,
                // `node: NodeId` falls back to the object the script is attached to.
                required: field.default.is_none() && !(kind == "object" && field.name.name == "node"),
            });
        }
    }
    Ok(out)
}

/// `target/j3-editor/catalog`: every `.j3s` in the project and the props the inspector edits.
/// A script that doesn't parse is listed with no props (`j3c check` reports it).
pub(crate) fn generate_catalog(root: &Path) -> Result<(), String> {
    let scripts = crate::compile::script_paths(root);
    let mut out = format!("catalog.count={}\n", scripts.len());
    for (i, rel) in scripts.iter().enumerate() {
        let props = script_props(root, rel).unwrap_or_default();
        out.push_str(&format!("catalog.{i}.name s={rel}\ncatalog.{i}.count={}\n", props.len()));
        for (j, prop) in props.iter().enumerate() {
            out.push_str(&format!(
                "catalog.{i}.{j}.name s={}\ncatalog.{i}.{j}.type s={}\ncatalog.{i}.{j}.default s={}\ncatalog.{i}.{j}.required={}\n",
                prop.name,
                prop.kind,
                prop.default,
                u8::from(prop.required)
            ));
            let value = match prop.kind.as_str() {
                "int" => format!("catalog.{i}.{j}.value={}\n", prop.default.parse::<i64>().unwrap_or(0)),
                "f32" => format!("catalog.{i}.{j}.value f={}\n", prop.default.parse::<f32>().unwrap_or(0.0)),
                "bool" => format!("catalog.{i}.{j}.value={}\n", u8::from(prop.default == "true")),
                "vec3" => format!("catalog.{i}.{j}.x f=0\ncatalog.{i}.{j}.y f=0\ncatalog.{i}.{j}.z f=0\n"),
                _ => String::new(),
            };
            out.push_str(&value);
        }
    }
    let dir = root.join("target/j3-editor");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(dir.join("catalog"), out).map_err(|e| e.to_string())
}

/// `j3c -e script-props <path>`: a script's editable props as JSON.
pub(crate) fn script_props_json(root: &Path, rel: &str) -> Result<String, String> {
    let props = script_props(root, rel)?;
    let rows: Vec<String> = props
        .iter()
        .map(|p| {
            format!(
                "{{\"name\":{:?},\"type\":{:?},\"default\":{:?},\"required\":{}}}",
                p.name, p.kind, p.default, p.required
            )
        })
        .collect();
    Ok(format!("[{}]", rows.join(",")))
}

pub(super) fn collect_assets(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            collect_assets(root, &path, out)?;
        } else if ["j3mesh", "png", "jpg", "jpeg", "j3scene", "j3mat", "j3fx"].contains(
            &path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str(),
        ) {
            let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
            out.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

/// `rel` as a file under the project's `assets/` or `scenes/`, refusing anything that could
/// point outside them.
pub(super) fn asset_file(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let ok = (rel.starts_with("assets/") || rel.starts_with("scenes/"))
        && !rel.contains('\\')
        && rel.split('/').all(|part| !part.is_empty() && part != "." && part != "..");
    if !ok {
        return Err(format!("`{rel}` isn't a path under assets/ or scenes/"));
    }
    Ok(root.join(rel))
}

pub(crate) fn is_main_scene(root: &Path, rel: &str) -> bool {
    scene_path(root).map(|main| main == root.join(rel)).unwrap_or(false)
}

/// Rename or move asset `old` to `new`, and point every scene file's references at `new`.
/// Returns how many scene files changed.
pub(super) fn rename_asset(root: &Path, old: &str, new: &str) -> Result<usize, String> {
    let from = asset_file(root, old)?;
    let to = asset_file(root, new)?;
    if !from.is_file() {
        return Err(format!("{old} doesn't exist"));
    }
    if to.exists() {
        return Err(format!("{new} already exists"));
    }
    if from.extension() != to.extension() {
        return Err("Keep the file extension when renaming".into());
    }
    if is_main_scene(root, old) {
        return Err("That's the scene in j3.editor.toml; change it there first".into());
    }
    if let Some(dir) = to.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::rename(&from, &to).map_err(|e| e.to_string())?;
    let mut scenes = Vec::new();
    collect_assets(root, &root.join("assets"), &mut scenes)?;
    collect_assets(root, &root.join("scenes"), &mut scenes)?;
    let old_value = format!(" s={old}");
    let mut changed = 0;
    for scene in scenes.iter().filter(|p| p.ends_with(".j3scene")) {
        let path = root.join(scene);
        let Ok(text) = fs::read_to_string(&path) else { continue };
        if text.starts_with("j3scene") {
            if let Some(out) = crate::scenefile::rename_v2(&text, old, new) {
                let temp = path.with_extension("j3tmp");
                fs::write(&temp, out).map_err(|e| e.to_string())?;
                fs::rename(&temp, &path).map_err(|e| e.to_string())?;
                changed += 1;
            }
            continue;
        }
        let mut hit = false;
        // Node paths and asset fields are whole values, so only exact matches change.
        let lines: Vec<String> = text
            .lines()
            .map(|line| match line.strip_suffix(&old_value) {
                Some(head) if line.starts_with("node.") || line.starts_with("param.") => {
                    hit = true;
                    format!("{head} s={new}")
                }
                _ => line.to_string(),
            })
            .collect();
        if hit {
            let temp = path.with_extension("j3tmp");
            fs::write(&temp, lines.join("\n") + "\n").map_err(|e| e.to_string())?;
            fs::rename(&temp, &path).map_err(|e| e.to_string())?;
            changed += 1;
        }
    }
    generate_assets_catalog(root)?;
    Ok(changed)
}

/// Move asset `rel` into `target/j3-editor/trash/`, where it can be recovered by hand.
/// References to it stay, and show as missing.
pub(super) fn delete_asset(root: &Path, rel: &str) -> Result<String, String> {
    let from = asset_file(root, rel)?;
    if !from.is_file() {
        return Err(format!("{rel} doesn't exist"));
    }
    if is_main_scene(root, rel) {
        return Err("That's the scene in j3.editor.toml; change it there first".into());
    }
    let trash = root.join("target/j3-editor/trash");
    fs::create_dir_all(&trash).map_err(|e| e.to_string())?;
    let name = from.file_name().and_then(|n| n.to_str()).unwrap_or("asset").to_string();
    let mut to = trash.join(&name);
    let mut n = 2;
    while to.exists() {
        to = trash.join(format!("{n}-{name}"));
        n += 1;
    }
    fs::rename(&from, &to).map_err(|e| e.to_string())?;
    generate_assets_catalog(root)?;
    Ok(to.strip_prefix(root).unwrap_or(&to).to_string_lossy().into_owned())
}

pub(crate) fn generate_assets_catalog(root: &Path) -> Result<(), String> {
    let mut assets = Vec::new();
    collect_assets(root, &root.join("assets"), &mut assets)?;
    collect_assets(root, &root.join("scenes"), &mut assets)?;
    assets.sort();
    let mut out = format!("assets.count={}\n", assets.len());
    for (i, path) in assets.iter().enumerate() {
        out.push_str(&format!("assets.{i}.path s={path}\n"));
    }
    let dir = root.join("target/j3-editor");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(dir.join("assets-catalog"), out).map_err(|e| e.to_string())
}

/// Write `scripts/<name>.j3s` with a `props` block and the two hooks the scene calls.
pub(super) fn create_script(root: &Path, name: &str) -> Result<String, String> {
    if name.is_empty()
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        || name.as_bytes()[0].is_ascii_digit()
        || crate::parse::keyword(name).is_some()
    {
        return Err("script name must be a J3 identifier".into());
    }
    let rel = format!("scripts/{name}.j3s");
    let path = root.join(&rel);
    if path.exists() {
        return Err(format!("{rel} already exists"));
    }
    fs::create_dir_all(root.join("scripts")).map_err(|e| e.to_string())?;
    let stub = "use j3.scene3d.{ Scene, NodeId };\n\nprops {\n    node: NodeId;\n}\n\n\
pub fn start(scene: &Scene) {\n}\n\npub fn update(scene: &Scene, dt: f32) {\n}\n";
    fs::write(&path, stub).map_err(|e| e.to_string())?;
    generate_catalog(root)?;
    Ok(rel)
}

/// Append `kind<TAB>scene<TAB>message` to the log the editor polls.
pub(crate) fn event(root: &Path, kind: &str, scene: &str, message: &str) {
    let clean = |s: &str| s.replace(['\n', '\r', '\t'], " ");
    let line = format!("{kind}\t{}\t{}\n", clean(scene), clean(message));
    append_event(root, &line);
}

/// The import event carries its own fragment name, so queued imports never share a file.
pub(super) fn import_event(root: &Path, scene: &str, file: &str, message: &str) {
    let clean = |s: &str| s.replace(['\n', '\r', '\t'], " ");
    let line = format!("import\t{}\t{file}\t{}\n", clean(scene), clean(message));
    append_event(root, &line);
}

pub(crate) fn append_event(root: &Path, line: &str) {
    // One small O_APPEND write, so lines from the Play thread and the main loop don't interleave.
    if let Ok(mut f) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("target/j3-editor/events"))
    {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Move an import's scene out of `target/` into `assets/imported/<name>.j3scene`, named after
/// the source file, and return that project-relative path.
pub(crate) fn keep_import(root: &Path, fragment: &Path, source: &str) -> Result<String, String> {
    let clean = import_name(source);
    let dir = root.join("assets/imported");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut name = format!("{clean}.j3scene");
    let mut n = 2;
    while dir.join(&name).exists() {
        name = format!("{clean}_{n}.j3scene");
        n += 1;
    }
    fs::rename(fragment, dir.join(&name)).map_err(|e| e.to_string())?;
    Ok(format!("assets/imported/{name}"))
}

/// The base name `keep_import` gives `source`: `assets/imported/<this>.j3scene` when free.
pub(crate) fn import_name(source: &str) -> String {
    let stem = Path::new(source).file_stem().and_then(|s| s.to_str()).unwrap_or("model");
    stem.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect()
}

/// `bake`: see `gltf::import`; `.j3mesh` ignores it.
pub(crate) fn import_asset(root: &Path, scene: &Path, raw: &str, bake: bool) -> Result<Vec<String>, String> {
    let source = Path::new(raw.trim());
    if raw.trim().is_empty() {
        return Err("enter an asset path first".into());
    }
    let source = if source.is_absolute() {
        source.to_path_buf()
    } else {
        root.join(source)
    };
    match source.extension().and_then(|s| s.to_str()).unwrap_or("") {
        "gltf" | "glb" => crate::gltf::import(root, scene, &source, bake),
        "j3mesh" => crate::gltf::import_mesh(root, scene, &source),
        _ => Err("expected .gltf, .glb, or .j3mesh".into()),
    }
}
