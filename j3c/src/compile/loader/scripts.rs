//! `.j3s` scripts and scene scripts joining the program.

use super::*;

impl Loader {
    /// When a scene attaches scripts, they and the registry join the program.
    pub(in crate::compile) fn load_scene_scripts(&mut self) -> Result<(), String> {
        let root = self.script_root.clone();
        let scan = self.scene_scan.get_or_insert_with(|| scene_scripts::scan(&root));
        if scan.attachments.is_empty() {
            return Ok(());
        }
        self.load_registry()?;
        self.load_imports()
    }

    pub(super) fn load_script(
        &mut self,
        rel: &str,
        source: usize,
        span: crate::span::Span,
    ) -> Result<(), String> {
        let parts = script_module_parts(rel)?;
        let key = parts.join(".");
        if self.loaded.contains(&key) {
            return Ok(());
        }
        let path = self.script_root.join(rel);
        if !path.is_file() && !self.overlays.iter().any(|(p, _)| same_path(p, &path)) {
            let source_file = &self.files[source];
            let mut error = crate::check::CheckError::new(
                &source_file.path,
                span,
                format!("no script at {rel}"),
            );
            if let Some(suggested) =
                crate::diag::did_you_mean(rel, &script_paths(&self.script_root))
            {
                error = error.help(format!("did you mean `{suggested}`?"));
            }
            return Err(error.render(&[(source_file.path.clone(), source_file.src.clone())]));
        }
        self.load_path(&path)
    }
}

/// Every `.j3s` under `root` (skipping `target/` and `.git/`), relative, `/`-separated.
pub fn script_paths(root: &Path) -> Vec<String> {
    fn visit(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && !path.is_symlink() {
                let name = entry.file_name();
                if name != ".git" && name != "target" {
                    visit(root, &path, out);
                }
            } else if path.extension().and_then(|s| s.to_str()) == Some("j3s") {
                if let Ok(rel) = path.strip_prefix(root) {
                    out.push(
                        rel.to_string_lossy()
                            .replace(std::path::MAIN_SEPARATOR, "/"),
                    );
                }
            }
        }
    }
    let mut out = Vec::new();
    visit(root, root, &mut out);
    out.sort();
    out
}

pub(in crate::compile) fn script_module_parts(rel: &str) -> Result<Vec<String>, String> {
    let path = Path::new(rel);
    if path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(format!(
            "script path must be relative to the project root: {rel}"
        ));
    }
    let Some(stem) = rel.strip_suffix(".j3s") else {
        return Err(format!("script path must end in `.j3s`: {rel}"));
    };
    let mut parts = vec!["script".to_string()];
    for part in stem.split('/') {
        if part.is_empty()
            || !part
                .bytes()
                .enumerate()
                .all(|(i, b)| b.is_ascii_alphabetic() || b == b'_' || (i > 0 && b.is_ascii_digit()))
        {
            return Err(format!(
                "script path has an invalid module component: {rel}"
            ));
        }
        parts.push(part.to_string());
    }
    Ok(parts)
}
