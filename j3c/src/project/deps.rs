//! `[dependencies]`: path dependencies, resolved once when the manifest loads, so every command
//! (check, lsp, run, build, editor) sees the same list through `Manifest::deps`.
//!
//! `name = { path = "relative/dir" }` (or `name = "relative/dir"`); the path is relative to the
//! `j3.toml` that declares it. A dep's own `j3.toml` is read for `[package] name` (must equal the
//! key) and `[dependencies]` only: its main, `[link]` and the rest belong to its own builds.

use super::toml::{is_ident, parse_toml, TomlTable, TomlVal};
use std::path::{Path, PathBuf};

/// A resolved dependency: `use <name>.a.b;` loads `<root>/a/b.j3`. `root` is the dep's `src/` when
/// it is a package (a `j3.toml` beside a `src/`), else the folder itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dep {
    pub name: String,
    /// Canonical absolute directory of its modules.
    pub root: PathBuf,
}

struct Entry {
    name: String,
    rel: String,
    line: usize,
}

struct Walk {
    out: Vec<Dep>,
    /// The `j3.toml` that first declared `out[i]`, for the clash message.
    from: Vec<PathBuf>,
    /// (name, root) from the project down to the dep being read, for cycles.
    stack: Vec<(String, PathBuf)>,
}

/// Every dep of the project at `root` (direct and transitive), each name once.
pub(super) fn resolve(root: &Path, tbl: &TomlTable, package: &str) -> Result<Vec<Dep>, String> {
    let toml = root.join("j3.toml");
    let list = entries(&toml, tbl)?;
    if list.is_empty() {
        return Ok(Vec::new());
    }
    for e in &list {
        let src = root.join("src");
        if e.name == package || src.join(format!("{}.j3", e.name)).is_file() || src.join(&e.name).is_dir() {
            return Err(format!("{}:{}: [dependencies] {}: clashes with a module of this project (`use {}...`); rename the dep", toml.display(), e.line, e.name, e.name));
        }
    }
    let canon = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut w = Walk { out: Vec::new(), from: Vec::new(), stack: vec![(package.to_string(), canon.clone())] };
    walk(&canon, &toml, list, &mut w)?;
    Ok(w.out)
}

fn entries(toml: &Path, tbl: &TomlTable) -> Result<Vec<Entry>, String> {
    let mut out: Vec<Entry> = Vec::new();
    for ((section, key, val), line) in tbl.vals.iter().zip(&tbl.lines) {
        if section != "dependencies" {
            continue;
        }
        let at = format!("{}:{}: [dependencies] {}", toml.display(), line, key);
        let rel = match val {
            TomlVal::Str(s) => s,
            TomlVal::Map(m) if m.len() == 1 && m[0].0 == "path" => &m[0].1,
            _ => return Err(format!("{at}: write `{key} = {{ path = \"path/to/dir\" }}`")),
        };
        if key == "j3" || !is_ident(key) || key.contains('-') {
            return Err(format!("{at}: `{key}` is not a usable module name (letters, digits, `_`; not `j3`)"));
        }
        if out.iter().any(|e| e.name == *key) {
            return Err(format!("{at}: listed twice"));
        }
        out.push(Entry { name: key.clone(), rel: rel.clone(), line: *line });
    }
    Ok(out)
}

fn walk(dir: &Path, toml: &Path, list: Vec<Entry>, w: &mut Walk) -> Result<(), String> {
    for e in list {
        let at = format!("{}:{}: [dependencies] {}", toml.display(), e.line, e.name);
        let joined = dir.join(&e.rel);
        if !joined.is_dir() {
            return Err(format!("{at}: path `{}` is not a directory (looked for {})", e.rel, joined.display()));
        }
        let root = joined.canonicalize().map_err(|err| format!("{at}: {}: {err}", joined.display()))?;
        if let Some(i) = w.stack.iter().position(|(_, r)| *r == root) {
            let mut chain: Vec<&str> = w.stack[i..].iter().map(|(n, _)| n.as_str()).collect();
            chain.push(&e.name);
            return Err(format!("{at}: dependency cycle {}", chain.join(" -> ")));
        }
        let dep_toml = root.join("j3.toml");
        let src = root.join("src");
        let mods = if dep_toml.is_file() && src.is_dir() { src } else { root.clone() };
        if let Some(i) = w.out.iter().position(|d| d.name == e.name) {
            if w.out[i].root == mods {
                continue;
            }
            return Err(format!(
                "{at}: `{}` is {} here but {} in {}; one name needs one path",
                e.name, mods.display(), w.out[i].root.display(), w.from[i].display()
            ));
        }
        let mut sub = Vec::new();
        if dep_toml.is_file() {
            let text = std::fs::read_to_string(&dep_toml).map_err(|err| format!("{at}: read {}: {err}", dep_toml.display()))?;
            let tbl = parse_toml(&text).map_err(|err| err.replacen("j3.toml", &dep_toml.display().to_string(), 1))?;
            if let Some(n) = tbl.get("package", "name").and_then(TomlVal::as_str) {
                if n != e.name {
                    return Err(format!("{at}: {} says [package] name = \"{n}\"; the key here must match it (or drop the name)", dep_toml.display()));
                }
            }
            sub = entries(&dep_toml, &tbl)?;
        }
        w.out.push(Dep { name: e.name.clone(), root: mods });
        w.from.push(toml.to_path_buf());
        w.stack.push((e.name, root.clone()));
        walk(&root, &dep_toml, sub, w)?;
        w.stack.pop();
    }
    Ok(())
}
