//! `[dependencies]`: path dependencies, resolved once when the manifest loads, so every command
//! (check, lsp, run, build, editor) sees the same list through `Manifest::deps`.
//!
//! `name = { path = "relative/dir" }` (or `name = "relative/dir"`); the path is relative to the
//! `j3.toml` that declares it. A dep's own `j3.toml` is read for `[package] name` (must equal the
//! key), `[package] j3` (the oldest j3c it builds with) and `[dependencies]` only: its main, `[link]`
//! and the rest belong to its own builds.
//!
//! `name = { git = "URL", tag | rev | branch = "..." }` clones with the system `git` into
//! `~/.cache/j3/pkgs/<name>-<commit12>/` (`J3_PKG_DIR` overrides), then is a path dep. The project's
//! `j3.lock` pins each git dep (transitive ones too) to a commit; with the lock and the cache present
//! no git runs. A checkout dir is never written after it is renamed into place.

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
    src: Src,
    line: usize,
}

enum Src {
    Path(String),
    /// URL (a local path is made absolute against the declaring `j3.toml`), and `tag:`/`rev:`/
    /// `branch:`/`head` (the form `j3.lock` stores).
    Git(String, String),
}

/// One `j3.lock` entry.
#[derive(Clone, PartialEq)]
struct Locked {
    name: String,
    git: String,
    at: String,
    commit: String,
}

/// The LSP sets this: never run git while typing; a missing dep says to run `j3c fetch`.
pub static NO_FETCH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

struct Walk {
    out: Vec<Dep>,
    /// The `j3.toml` that first declared `out[i]`, for the clash message.
    from: Vec<PathBuf>,
    /// (name, root) from the project down to the dep being read, for cycles.
    stack: Vec<(String, PathBuf)>,
    lock_old: Vec<Locked>,
    lock_new: Vec<Locked>,
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
    let lock_old = read_lock(&canon)?;
    let mut w = Walk { out: Vec::new(), from: Vec::new(), stack: vec![(package.to_string(), canon.clone())], lock_old, lock_new: Vec::new() };
    walk(&canon, &toml, list, &mut w)?;
    if !w.lock_new.is_empty() && w.lock_new != w.lock_old {
        write_lock(&canon, &w.lock_new)?;
    }
    Ok(w.out)
}

/// `j3c update [name]`: forget the locked commit of one git dep (or all), so the next load resolves
/// it again. The caller reloads the manifest, which writes the new lock.
pub(crate) fn unlock(root: &Path, name: Option<&str>) -> Result<(), String> {
    let old = read_lock(root)?;
    if let Some(n) = name {
        if !old.iter().any(|l| l.name == n) {
            return Err(format!("j3.lock has no `{n}` (only git deps are locked)"));
        }
    }
    let keep: Vec<Locked> = old.into_iter().filter(|l| name.is_some_and(|n| l.name != n)).collect();
    if keep.is_empty() {
        let _ = std::fs::remove_file(root.join("j3.lock"));
        Ok(())
    } else {
        write_lock(root, &keep)
    }
}

fn read_lock(root: &Path) -> Result<Vec<Locked>, String> {
    let path = root.join("j3.lock");
    let Ok(text) = std::fs::read_to_string(&path) else { return Ok(Vec::new()) };
    let tbl = parse_toml(&text).map_err(|e| e.replacen("j3.toml", &path.display().to_string(), 1))?;
    let mut out: Vec<Locked> = Vec::new();
    for (section, _, _) in &tbl.vals {
        if out.iter().any(|l| l.name == *section) {
            continue;
        }
        let get = |k: &str| tbl.get(section, k).and_then(TomlVal::as_str).unwrap_or("").to_string();
        let l = Locked { name: section.clone(), git: get("git"), at: get("ref"), commit: get("commit") };
        if l.commit.len() < 12 || !l.commit.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!("{}: [{section}] has no commit; delete the file or run `j3c update`", path.display()));
        }
        out.push(l);
    }
    Ok(out)
}

fn write_lock(root: &Path, list: &[Locked]) -> Result<(), String> {
    let mut sorted = list.to_vec();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    let mut text = String::from("# Written by j3c: the commit each git dependency builds from. Commit this file.\n# `j3c update [name]` moves deps to the newest commit of their tag/branch.\n");
    for l in &sorted {
        text += &format!("\n[{}]\ngit = \"{}\"\nref = \"{}\"\ncommit = \"{}\"\n", l.name, l.git, l.at, l.commit);
    }
    let path = root.join("j3.lock");
    std::fs::write(&path, text).map_err(|e| format!("write {}: {e}", path.display()))
}

fn pkg_dir() -> Result<PathBuf, String> {
    if let Some(d) = std::env::var_os("J3_PKG_DIR").filter(|s| !s.is_empty()) {
        return Ok(PathBuf::from(d));
    }
    crate::editor::cache_root().map(|c| c.join("pkgs")).ok_or_else(|| "no cache directory for git deps (set HOME or J3_PKG_DIR)".into())
}

fn git(args: &[&str]) -> Result<String, String> {
    let out = std::process::Command::new("git")
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("cannot run `git` ({e}); install git, or use a path dep (`name = \"../dir\"`)"))?;
    if !out.status.success() {
        return Err(format!("`git {}` failed: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The checkout of a git dep: from the lock and the cache when both have it, else cloned now.
fn git_checkout(name: &str, url: &str, at: &str, w: &mut Walk) -> Result<PathBuf, String> {
    let cache = pkg_dir()?;
    let locked = w.lock_old.iter().find(|l| l.name == name && l.git == url && l.at == at).map(|l| l.commit.clone());
    if let Some(c) = &locked {
        let dir = cache.join(format!("{name}-{}", &c[..12]));
        if dir.is_dir() {
            lock_push(w, Locked { name: name.into(), git: url.into(), at: at.into(), commit: c.clone() });
            return Ok(dir);
        }
    }
    if NO_FETCH.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(format!("`{name}` is not downloaded yet; run `j3c fetch`"));
    }
    std::fs::create_dir_all(&cache).map_err(|e| format!("create {}: {e}", cache.display()))?;
    eprintln!("j3c: fetching {name} ({url}{})", if at == "head" { String::new() } else { format!(" {at}") });
    let tmp = cache.join(format!(".{name}-{}.tmp", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let tmp_s = tmp.to_string_lossy().to_string();
    let res = (|| {
        git(&["clone", "--quiet", url, &tmp_s])?;
        let target = match (&locked, at.split_once(':')) {
            (Some(c), _) => Some(c.clone()),
            (None, Some(("tag", t))) => Some(format!("refs/tags/{t}")),
            (None, Some(("branch", b))) => Some(format!("origin/{b}")),
            (None, Some((_, r))) => Some(r.to_string()),
            (None, None) => None,
        };
        if let Some(t) = target {
            let what = if locked.is_some() { "locked commit" } else { at.split_once(':').map_or("ref", |(k, _)| k) };
            let commit = git(&["-C", &tmp_s, "rev-parse", "--verify", "--quiet", &format!("{t}^{{commit}}")])
                .map_err(|_| format!("{url} has no {what} `{}`", at.split_once(':').map_or(at, |(_, v)| v)))?;
            git(&["-C", &tmp_s, "-c", "advice.detachedHead=false", "checkout", "--quiet", "--detach", &commit])?;
        }
        git(&["-C", &tmp_s, "rev-parse", "HEAD"])
    })();
    let commit = match res {
        Ok(c) => c,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(e);
        }
    };
    let dir = cache.join(format!("{name}-{}", &commit[..12.min(commit.len())]));
    if dir.is_dir() {
        let _ = std::fs::remove_dir_all(&tmp);
    } else {
        let _ = std::fs::remove_dir_all(tmp.join(".git"));
        std::fs::rename(&tmp, &dir).map_err(|e| format!("move {} to {}: {e}", tmp.display(), dir.display()))?;
    }
    lock_push(w, Locked { name: name.into(), git: url.into(), at: at.into(), commit });
    Ok(dir)
}

fn lock_push(w: &mut Walk, l: Locked) {
    if !w.lock_new.iter().any(|x| x.name == l.name) {
        w.lock_new.push(l);
    }
}

/// `[package] j3 = ">=0.4"`: the oldest j3c the package builds with.
fn check_j3_bound(want: &str) -> Result<(), String> {
    let Some(min) = want.trim().strip_prefix(">=") else {
        return Err(format!("`j3 = \"{want}\"`: write `>=X.Y` (the oldest j3c it needs)"));
    };
    let parse = |v: &str| -> Option<Vec<u64>> { v.trim().split('.').map(|p| p.parse().ok()).collect() };
    let (Some(min_v), Some(have)) = (parse(min), parse(env!("CARGO_PKG_VERSION"))) else {
        return Err(format!("`j3 = \"{want}\"`: not a version"));
    };
    if have < min_v {
        return Err(format!("needs j3c {want}, this is {}; update j3c", env!("CARGO_PKG_VERSION")));
    }
    Ok(())
}

fn entries(toml: &Path, tbl: &TomlTable) -> Result<Vec<Entry>, String> {
    let mut out: Vec<Entry> = Vec::new();
    for ((section, key, val), line) in tbl.vals.iter().zip(&tbl.lines) {
        if section != "dependencies" {
            continue;
        }
        let at = format!("{}:{}: [dependencies] {}", toml.display(), line, key);
        let field = |k: &str| match val {
            TomlVal::Map(m) => m.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone()),
            _ => None,
        };
        let src = match val {
            TomlVal::Str(s) => Src::Path(s.clone()),
            TomlVal::Map(m) if m.len() == 1 && m[0].0 == "path" => Src::Path(m[0].1.clone()),
            TomlVal::Map(m) if field("git").is_some() => {
                let refs: Vec<&(String, String)> = m.iter().filter(|(k, _)| matches!(k.as_str(), "tag" | "rev" | "branch")).collect();
                if refs.len() > 1 || m.len() > 1 + refs.len() {
                    return Err(format!("{at}: write `{key} = {{ git = \"URL\", tag = \"v1.0\" }}` (one of tag, rev, branch; or none for the default branch)"));
                }
                let mut url = field("git").unwrap_or_default();
                let local = !url.contains("://") && !url.contains('@');
                if local {
                    let dir = toml.parent().unwrap_or(Path::new(".")).join(&url);
                    url = dir.canonicalize().map_err(|e| format!("{at}: git `{url}`: {e}"))?.display().to_string();
                }
                Src::Git(url, refs.first().map(|(k, v)| format!("{k}:{v}")).unwrap_or_else(|| "head".into()))
            }
            _ => return Err(format!("{at}: write `{key} = {{ path = \"path/to/dir\" }}` or `{key} = {{ git = \"URL\", tag = \"v1.0\" }}`")),
        };
        if key == "j3" || !is_ident(key) || key.contains('-') {
            return Err(format!("{at}: `{key}` is not a usable module name (letters, digits, `_`; not `j3`)"));
        }
        if out.iter().any(|e| e.name == *key) {
            return Err(format!("{at}: listed twice"));
        }
        out.push(Entry { name: key.clone(), src, line: *line });
    }
    Ok(out)
}

fn walk(dir: &Path, toml: &Path, list: Vec<Entry>, w: &mut Walk) -> Result<(), String> {
    for e in list {
        let at = format!("{}:{}: [dependencies] {}", toml.display(), e.line, e.name);
        let joined = match &e.src {
            Src::Path(rel) => {
                let joined = dir.join(rel);
                if !joined.is_dir() {
                    return Err(format!("{at}: path `{rel}` is not a directory (looked for {})", joined.display()));
                }
                joined
            }
            Src::Git(url, r) => git_checkout(&e.name, url, r, w).map_err(|err| format!("{at}: {err}"))?,
        };
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
            if let Some(want) = tbl.get("package", "j3").and_then(TomlVal::as_str) {
                check_j3_bound(want).map_err(|err| format!("{at}: {err}"))?;
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
