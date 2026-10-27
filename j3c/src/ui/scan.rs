//! Finding `.j3ui` files: `ui/` is scanned recursively, and `ui/game/hud/Cool.j3ui` is the
//! component `game.hud.Cool` (written with its full path in markup).

use std::path::{Component, Path, PathBuf};

#[derive(Debug)]
pub struct Found {
    pub path: PathBuf,
    /// Dotted component name, `game.hud.Cool`.
    pub name: String,
    /// As shown in errors, `/`-separated: `ui/game/hud/Cool.j3ui`.
    pub shown: String,
}

const MAX_DEPTH: usize = 16;

fn hidden(n: &std::ffi::OsStr) -> bool {
    n.to_string_lossy().starts_with('.')
}

fn walk(dir: &Path, rel: &Path, depth: usize, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err(format!("{} is nested deeper than {MAX_DEPTH} folders", dir.display()));
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return Ok(()) };
    for e in rd.flatten() {
        let n = e.file_name();
        if hidden(&n) {
            continue;
        }
        let (p, r) = (e.path(), rel.join(&n));
        // symlinked folders are not followed (loops); a symlinked file is fine
        let ft = e.file_type().map_err(|x| x.to_string())?;
        if ft.is_dir() {
            walk(&p, &r, depth + 1, out)?;
        } else if p.extension().and_then(|x| x.to_str()) == Some("j3ui") && p.is_file() {
            out.push(r);
        }
    }
    Ok(())
}

/// Every `.j3ui` under `dir` plus `extra` paths (unsaved editor buffers), sorted by path.
/// `shown_dir` is how `dir` is written in errors (`ui`).
pub fn scan(dir: &Path, shown_dir: &str, extra: &[PathBuf]) -> Result<Vec<Found>, String> {
    let mut rels = Vec::new();
    walk(dir, Path::new(""), 0, &mut rels)?;
    for p in extra {
        let Ok(r) = p.strip_prefix(dir) else { continue };
        let ok = r.extension().and_then(|x| x.to_str()) == Some("j3ui") && r.components().all(|c| matches!(c, Component::Normal(n) if !hidden(n)));
        if ok && !rels.iter().any(|x| x == r) {
            rels.push(r.to_path_buf());
        }
    }
    let mut found = Vec::new();
    for r in rels {
        let mut segs: Vec<String> = r.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
        let file = segs.pop().unwrap_or_default();
        let stem = file.strip_suffix(".j3ui").unwrap_or(&file).to_string();
        let shown = format!("{shown_dir}/{}", segs.iter().chain([&file]).cloned().collect::<Vec<_>>().join("/"));
        if stem.contains('.') {
            return Err(format!("{shown}: a component file name cannot contain `.` (dots are folder separators in component names)"));
        }
        segs.push(stem);
        found.push(Found { path: dir.join(&r), name: segs.join("."), shown });
    }
    found.sort_by(|a, b| a.shown.cmp(&b.shown));
    Ok(found)
}
