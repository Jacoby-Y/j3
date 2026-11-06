//! Project-owned scene files and a bundled J3 desktop editor.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Write};
#[cfg(unix)]
use std::os::fd::AsFd;
#[cfg(windows)]
use std::os::windows::io::AsHandle;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use crate::project::Manifest;

mod assets;
mod frontend;
mod play;
mod templates;

pub(crate) use assets::*;
pub(crate) use frontend::*;
use play::*;
pub(crate) use templates::*;

/// The editor's `src/` files, by name.
const FRONTEND: &[(&str, &str)] = &[
    ("assets.j3", include_str!("../../editor/src/assets.j3")),
    ("cameras.j3", include_str!("../../editor/src/cameras.j3")),
    ("commands.j3", include_str!("../../editor/src/commands.j3")),
    ("console.j3", include_str!("../../editor/src/console.j3")),
    ("files.j3", include_str!("../../editor/src/files.j3")),
    ("gizmo.j3", include_str!("../../editor/src/gizmo.j3")),
    ("history.j3", include_str!("../../editor/src/history.j3")),
    ("host.j3", include_str!("../../editor/src/host.j3")),
    ("input.j3", include_str!("../../editor/src/input.j3")),
    ("layout.j3", include_str!("../../editor/src/layout.j3")),
    ("live.j3", include_str!("../../editor/src/live.j3")),
    ("main.j3", include_str!("../../editor/src/main.j3")),
    ("materials.j3", include_str!("../../editor/src/materials.j3")),
    ("menu.j3", include_str!("../../editor/src/menu.j3")),
    ("nodes.j3", include_str!("../../editor/src/nodes.j3")),
    ("overlay.j3", include_str!("../../editor/src/overlay.j3")),
    ("overrides.j3", include_str!("../../editor/src/overrides.j3")),
    ("picker.j3", include_str!("../../editor/src/picker.j3")),
    ("prefs.j3", include_str!("../../editor/src/prefs.j3")),
    ("render.j3", include_str!("../../editor/src/render.j3")),
    ("scenes.j3", include_str!("../../editor/src/scenes.j3")),
    ("selection.j3", include_str!("../../editor/src/selection.j3")),
    ("session.j3", include_str!("../../editor/src/session.j3")),
    ("shared.j3", include_str!("../../editor/src/shared.j3")),
    ("tabs.j3", include_str!("../../editor/src/tabs.j3")),
    ("tool.j3", include_str!("../../editor/src/tool.j3")),
    ("tool/catalog.j3", include_str!("../../editor/src/tool/catalog.j3")),
    ("tool/check.j3", include_str!("../../editor/src/tool/check.j3")),
    ("tool/edit.j3", include_str!("../../editor/src/tool/edit.j3")),
    ("tool/place.j3", include_str!("../../editor/src/tool/place.j3")),
    ("tool/read.j3", include_str!("../../editor/src/tool/read.j3")),
    ("tool/refs.j3", include_str!("../../editor/src/tool/refs.j3")),
    ("tool/split.j3", include_str!("../../editor/src/tool/split.j3")),
    ("tool/stats.j3", include_str!("../../editor/src/tool/stats.j3")),
    ("types.j3", include_str!("../../editor/src/types.j3")),
    ("ui_dialogs.j3", include_str!("../../editor/src/ui_dialogs.j3")),
    ("ui_inspector.j3", include_str!("../../editor/src/ui_inspector.j3")),
    ("ui_material.j3", include_str!("../../editor/src/ui_material.j3")),
    ("ui_panes.j3", include_str!("../../editor/src/ui_panes.j3")),
    ("ui_props.j3", include_str!("../../editor/src/ui_props.j3")),
    ("ui_top.j3", include_str!("../../editor/src/ui_top.j3")),
    ("ui_tree.j3", include_str!("../../editor/src/ui_tree.j3")),
    ("ui_online.j3", include_str!("../../editor/src/ui_online.j3")),
    ("util.j3", include_str!("../../editor/src/util.j3")),
    ("viewport.j3", include_str!("../../editor/src/viewport.j3")),
    ("watch.j3", include_str!("../../editor/src/watch.j3")),
    ("widgets.j3", include_str!("../../editor/src/widgets.j3")),
];
pub(crate) const FONT: &[u8] = include_bytes!("../../editor/font.ttf");

pub fn cmd(rest: &[String]) -> i32 {
    match execute(rest) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("j3c editor: {e}");
            1
        }
    }
}

fn execute(rest: &[String]) -> Result<(), String> {
    if rest.iter().any(|s| s == "--help" || s == "-h") {
        println!("usage: j3c editor [--new] [project]");
        return Ok(());
    }
    let mut create = false;
    let mut path: Option<&str> = None;
    for arg in rest {
        if arg == "--new" {
            create = true;
        } else if arg.starts_with('-') {
            return Err(format!("unknown option `{arg}`"));
        } else if path.replace(arg).is_some() {
            return Err("expected one project path".into());
        }
    }
    let requested = PathBuf::from(path.unwrap_or("."));
    let root = if requested.is_absolute() {
        requested
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(requested)
    };
    if create {
        create_project(&root, false)?;
    } else {
        let fresh = !root.join("j3.editor.toml").exists();
        add_editor_files(&root)?;
        crate::scaffold::add_scenes_section(&root);
        if fresh {
            println!(
                "Added j3.editor.toml and scenes/main.j3scene to {}",
                root.display()
            );
            println!("To use the scene in this game's code:");
            println!("  use j3.scene3d;");
            println!("  let scene = scene3d.load(\"scenes/main.j3scene\", mem);");
            println!("  scene3d.upload(&scene); // after gfx.init()");
            println!("  scene3d.draw(&scene);   // in shadow and camera passes");
            println!("The existing entry point was left as it was.");
        }
    }
    let manifest = Manifest::load(&root.join("j3.toml"))?;
    let scene = scene_path(&root)?;
    validate_scene(&scene)?;
    let _project_lock = project_lock(&root)?;
    note_open(&root);
    let _ = &manifest;
    generate_catalog(&root)?;
    for line in crate::gen::run_all(&root, false).failed {
        eprintln!("warning: mesh generator failed: {line}");
    }
    generate_assets_catalog(&root)?;
    launch(&root, &scene)
}

/// A quoted `key = "value"` from `j3.editor.toml` (any section), if present and non-empty.
pub(crate) fn config_value(root: &Path, key: &str) -> Option<String> {
    let config = fs::read_to_string(root.join("j3.editor.toml")).ok()?;
    let value = config.lines().filter_map(|line| line.split_once('=')).find(|(k, _)| k.trim() == key)?.1.trim();
    let value = value.strip_prefix('"')?.strip_suffix('"')?;
    (!value.is_empty()).then(|| value.to_string())
}

pub(crate) fn scene_path(root: &Path) -> Result<PathBuf, String> {
    let config = fs::read_to_string(root.join("j3.editor.toml"))
        .map_err(|e| format!("read j3.editor.toml: {e}"))?;
    let value = config
        .lines()
        .filter_map(|line| line.split_once('='))
        .find(|(key, _)| key.trim() == "scene")
        .map(|(_, value)| value.trim())
        .ok_or("j3.editor.toml: missing scene path")?;
    let value = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or("j3.editor.toml: scene path must be quoted")?;
    let rel = Path::new(value);
    if rel.is_absolute()
        || rel
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("scene must be a project-relative path".into());
    }
    Ok(root.join(rel))
}

fn validate_scene(path: &Path) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !text.starts_with("j3scene") && !text.lines().any(|l| l == "scene.version=1") {
        return Err("scene: not a j3scene file (expected `j3scene 2` or `scene.version=1`)".into());
    }
    let (objects, error) = crate::scenefile::parse(&text);
    if let Some(error) = error {
        return Err(format!("scene: {error}"));
    }
    let mut parents = HashMap::new();
    for o in &objects {
        if o.id <= 0 || parents.insert(o.id, o.parent).is_some() {
            return Err(format!("scene: duplicate or invalid object ID {}", o.id));
        }
        if !(0..=4).contains(&o.kind) {
            return Err(format!("scene: object {} has invalid kind {}", o.id, o.kind));
        }
    }
    for (&id, &parent) in &parents {
        if parent != 0 && !parents.contains_key(&parent) {
            return Err(format!("scene: object {id} has missing parent {parent}"));
        }
    }
    let mut safe = HashSet::new();
    for &id in parents.keys() {
        let mut chain = HashSet::new();
        let mut cursor = id;
        while cursor != 0 && !safe.contains(&cursor) {
            if !chain.insert(cursor) {
                return Err(format!("scene: parenting cycle involving object {id}"));
            }
            cursor = parents[&cursor];
        }
        safe.extend(chain);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
