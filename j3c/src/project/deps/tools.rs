//! A package's tools, from its own `j3.toml`: `[commands]` (run by `j3c pkg NAME CMD`) and `[dist_outputs]`
//! (run by `j3c build --release` when the game's `[dist] build_outputs` names one). Each entry is
//! `name = "tools/x.j3"` or `name = { file = "tools/x.j3", help = "one line" }`; the file is relative to the
//! package root (beside `j3.toml`) and must exist.

use super::super::toml::{TomlTable, TomlVal};
use super::Dep;
use std::path::{Path, PathBuf};

/// One command or dist output of a package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tool {
    /// A TOML key (`is_ident`: a letter or `_`, then letters, digits, `-`, `_`).
    pub name: String,
    /// Absolute path of the entry `.j3` (under the dep's `pkg_root`).
    pub file: PathBuf,
    /// One line for listings; None when not given.
    pub help: Option<String>,
}

/// The tools under `[section]` of the dep `dep` (its `j3.toml` at `toml`, package root `pkg_root`).
pub(super) fn read(dep: &str, toml: &Path, tbl: &TomlTable, pkg_root: &Path, section: &str) -> Result<Vec<Tool>, String> {
    if let Some((head, line)) = tbl.heads.iter().find(|(h, _)| h.strip_prefix(section).is_some_and(|r| r.starts_with('.'))) {
        let name = &head[section.len() + 1..];
        return Err(format!(
            "{}:{line}: package `{dep}` [{head}]: sub-tables are not read; under [{section}] write `{name} = {{ file = \"tools/{name}.j3\", help = \"one line\" }}`",
            toml.display()
        ));
    }
    let mut out: Vec<Tool> = Vec::new();
    for ((s, key, val), line) in tbl.vals.iter().zip(&tbl.lines) {
        if s != section {
            continue;
        }
        let at = format!("{}:{line}: package `{dep}` [{section}] {key}", toml.display());
        let usage = format!("write `{key} = \"tools/{key}.j3\"` or `{key} = {{ file = \"tools/{key}.j3\", help = \"one line\" }}`");
        let (rel, help) = match val {
            TomlVal::Str(f) | TomlVal::Bare(f) => (f.clone(), None),
            TomlVal::Map(kv) => {
                let get = |k: &str| kv.iter().rev().find(|(n, _)| n == k).map(|(_, v)| v.clone());
                if let Some((bad, _)) = kv.iter().find(|(n, _)| n != "file" && n != "help") {
                    return Err(format!("{at}: unknown key `{bad}`; {usage}"));
                }
                (get("file").ok_or_else(|| format!("{at}: no `file`; {usage}"))?, get("help"))
            }
            TomlVal::List(_) => return Err(format!("{at}: must be a string (a file) or an inline table; {usage}")),
        };
        let file = pkg_root.join(&rel);
        if !file.is_file() {
            return Err(format!("{at}: no file `{rel}` (looked for {})", file.display()));
        }
        if out.iter().any(|t| t.name == *key) {
            return Err(format!("{at}: listed twice"));
        }
        out.push(Tool { name: key.clone(), file, help });
    }
    Ok(out)
}

/// The dep (and its tool) whose `[dist_outputs]` has `name`. `check_dist_unique` makes it at most one.
pub fn dist_provider<'a>(deps: &'a [Dep], name: &str) -> Option<(&'a Dep, &'a Tool)> {
    deps.iter().find_map(|d| d.dist_outputs.iter().find(|t| t.name == name).map(|t| (d, t)))
}

/// Two deps providing the same dist output is an error: `build_outputs = ["x"]` must name one tool.
pub(super) fn check_dist_unique(deps: &[Dep]) -> Result<(), String> {
    for (i, d) in deps.iter().enumerate() {
        for t in &d.dist_outputs {
            if let Some(other) = deps[..i].iter().find(|o| o.dist_outputs.iter().any(|x| x.name == t.name)) {
                return Err(format!(
                    "[dist_outputs] {}: provided by both `{}` ({}) and `{}` ({}); depend on only one of them",
                    t.name,
                    other.name,
                    other.pkg_root.join("j3.toml").display(),
                    d.name,
                    d.pkg_root.join("j3.toml").display()
                ));
            }
        }
    }
    Ok(())
}
