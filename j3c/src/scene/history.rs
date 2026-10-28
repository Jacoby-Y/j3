use super::*;

pub(super) const KEEP: usize = 20;

/// Backups of `rel`, oldest first: `target/j3-editor/history/<rel, / as __>.<unix millis>`.
pub(super) fn history(root: &Path, rel: &str) -> (PathBuf, Vec<PathBuf>) {
    let dir = root.join("target/j3-editor/history");
    let prefix = format!("{}.", rel.trim_start_matches("./").replace('/', "__"));
    let mut found: Vec<(u128, PathBuf)> = fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let stamp = name.strip_prefix(&prefix)?.parse().ok()?;
            Some((stamp, e.path()))
        })
        .collect();
    found.sort();
    (dir.join(prefix), found.into_iter().map(|(_, p)| p).collect())
}

/// Copy the scene aside before a change; keeps the newest `KEEP`.
// ponytail: whole-file copies (scenes are small text), no redo.
pub(super) fn backup(root: &Path, rel: &str) -> Option<PathBuf> {
    let (base, old) = history(root, rel);
    fs::create_dir_all(base.parent()?).ok()?;
    let mut millis = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_millis();
    if let Some(last) = old.last() {
        let prev: u128 = last.extension()?.to_str()?.parse().ok()?;
        millis = millis.max(prev + 1);
    }
    let path = PathBuf::from(format!("{}{millis}", base.display()));
    fs::copy(root.join(rel), &path).ok()?;
    for extra in old.iter().take((old.len() + 1).saturating_sub(KEEP)) { let _ = fs::remove_file(extra); }
    Some(path)
}

/// Put the newest backup of `rel` back over it.
pub(crate) fn undo(root: &Path, rel: &str) -> Result<String, String> {
    let (_, old) = history(root, rel);
    let last = old.last().ok_or_else(|| format!("no earlier version of {rel} to go back to"))?;
    fs::copy(last, root.join(rel)).map_err(|e| e.to_string())?;
    fs::remove_file(last).map_err(|e| e.to_string())?;
    Ok(format!("{rel} is back to how it was before the last change ({} earlier versions left)", old.len() - 1))
}
