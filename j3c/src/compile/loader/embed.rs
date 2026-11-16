//! `@embed*`: read the files a parsed file embeds, so the checker sees their bytes.
//! A path is relative to the source file's folder, else to the package root (`@script`'s root).

use super::*;
use crate::parse::{EmbedFile, EmbedKind};
use std::sync::Arc;

impl Loader {
    pub(super) fn read_embeds(&self, path: &str, src: &str, ast: &mut parse::File) -> Result<(), String> {
        for r in script_deps::embeds(ast) {
            let fail = |msg: String| {
                crate::check::CheckError::new(path, r.span, msg).render(&[(path.to_string(), src.to_string())])
            };
            let dir = Path::new(path).parent().unwrap_or(Path::new("."));
            let want_dir = r.kind == EmbedKind::Dir;
            let found = [dir.join(&r.path), self.script_root.join(&r.path)]
                .into_iter()
                .find(|p| if want_dir { p.is_dir() } else { p.is_file() || self.overlay(p).is_some() });
            let Some(found) = found else {
                let what = if want_dir { "folder" } else { "file" };
                return Err(fail(format!(
                    "`@{}`: no {what} `{}` (looked beside {} and in {})",
                    kind_name(r.kind),
                    r.path,
                    dir.display(),
                    self.script_root.display()
                )));
            };
            let files = if want_dir {
                let mut names = Vec::new();
                walk(&found, "", &r.glob, &mut names);
                names.sort();
                if names.is_empty() {
                    return Err(fail(format!("`@embedDir`: no `{}` files under `{}`", r.glob, found.display())));
                }
                let mut files = Vec::with_capacity(names.len());
                for name in names {
                    files.push(self.read_embed(&found.join(&name), name).map_err(&fail)?);
                }
                files
            } else {
                vec![self.read_embed(&found, r.path.clone()).map_err(&fail)?]
            };
            ast.embeds.push((r.span, Arc::new(files)));
        }
        Ok(())
    }

    fn overlay(&self, p: &Path) -> Option<&String> {
        self.overlays.iter().find(|(o, _)| same_path(o, p)).map(|(_, s)| s)
    }

    fn read_embed(&self, disk: &Path, name: String) -> Result<EmbedFile, String> {
        let data = match self.overlay(disk) {
            Some(s) => s.as_bytes().to_vec(),
            None => std::fs::read(disk).map_err(|e| format!("cannot read `{}`: {e}", disk.display()))?,
        };
        Ok(EmbedFile { name, disk: disk.display().to_string(), data })
    }
}

fn kind_name(kind: EmbedKind) -> &'static str {
    match kind {
        EmbedKind::Text => "embed",
        EmbedKind::Bytes => "embedBytes",
        EmbedKind::Dir => "embedDir",
    }
}

/// Files under `dir` whose name matches `glob` (`*` is any run), as `/`-separated paths below the
/// starting folder. Dot entries are skipped; a `glob` with `/` matches the whole relative path.
fn walk(dir: &Path, rel: &str, glob: &str, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let sub = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
        let path = e.path();
        if path.is_dir() {
            walk(&path, &sub, glob, out);
        } else if crate::mcp::wild(glob, if glob.contains('/') { &sub } else { &name }) {
            out.push(sub);
        }
    }
}
