//! `j3c fetch`, `j3c update`, `j3c add`: package commands. Resolving lives in `project/deps.rs`;
//! these only load the manifest (which fetches and writes `j3.lock`) or edit `j3.toml`.

use super::*;

/// The project around the cwd (or `path`): its `j3.toml`.
fn project_toml(path: Option<&str>) -> Result<PathBuf, String> {
    let start = match path {
        Some(p) => PathBuf::from(p),
        None => std::env::current_dir().map_err(|e| format!("cwd: {e}"))?,
    };
    project::find_toml_up(&start).ok_or_else(|| format!("no j3.toml in {} or above it", start.display()))
}

fn report(m: &Manifest) {
    if m.deps.is_empty() {
        println!("no dependencies");
    }
    for d in &m.deps {
        println!("{}  {}", d.name, d.root.display());
    }
}

/// `j3c fetch [path]`: download every git dep, write `j3.lock`, list where each dep lives.
pub(super) fn fetch_cmd(rest: &[String]) -> i32 {
    if rest.iter().any(|a| a == "--help" || a == "-h") {
        println!("usage: j3c fetch [path]\n  download the project's git dependencies (and theirs) and write j3.lock; prints each dep's folder");
        return 0;
    }
    match project_toml(rest.first().map(String::as_str)).and_then(|t| Manifest::load(&t)) {
        Ok(m) => {
            report(&m);
            0
        }
        Err(e) => {
            eprintln!("j3c fetch: {e}");
            1
        }
    }
}

/// `j3c update [name]`: drop the locked commit (one dep or all) and resolve again.
pub(super) fn update_cmd(rest: &[String]) -> i32 {
    if rest.iter().any(|a| a == "--help" || a == "-h") {
        println!("usage: j3c update [name]\n  move git deps (or just `name`) to the newest commit of their tag/branch and rewrite j3.lock");
        return 0;
    }
    let res = project_toml(None).and_then(|t| {
        let root = t.parent().unwrap_or(Path::new(".")).to_path_buf();
        let saved = std::fs::read_to_string(root.join("j3.lock")).ok();
        project::unlock(&root, rest.first().map(String::as_str))?;
        Manifest::load(&t).inspect_err(|_| {
            if let Some(text) = &saved {
                let _ = std::fs::write(root.join("j3.lock"), text);
            }
        })
    });
    match res {
        Ok(m) => {
            report(&m);
            0
        }
        Err(e) => {
            eprintln!("j3c update: {e}");
            1
        }
    }
}

/// `j3c add <name> (--path DIR | --git URL [--tag T | --rev R | --branch B])`: one line under
/// `[dependencies]` in the project's `j3.toml`, then fetch. The file is put back if that fails.
pub(super) fn add_cmd(rest: &[String]) -> i32 {
    const USAGE: &str = "usage: j3c add <name> --path DIR\n       j3c add <name> --git URL [--tag T | --rev R | --branch B]";
    let mut name = None;
    let mut fields: Vec<(String, String)> = Vec::new();
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--help" | "-h" => {
                println!("{USAGE}\n  writes the dependency into j3.toml and fetches it; then `use <name>;` in your code");
                return 0;
            }
            "--path" | "--git" | "--tag" | "--rev" | "--branch" => match it.next() {
                Some(v) => fields.push((a[2..].to_string(), v.clone())),
                None => {
                    eprintln!("j3c add: {a} needs a value\n{USAGE}");
                    return 1;
                }
            },
            s if !s.starts_with('-') && name.is_none() => name = Some(s.to_string()),
            _ => {
                eprintln!("j3c add: unexpected `{a}`\n{USAGE}");
                return 1;
            }
        }
    }
    let src = fields.iter().filter(|(k, _)| k == "path" || k == "git").count();
    let (Some(name), 1) = (name, src) else {
        eprintln!("{USAGE}");
        return 1;
    };
    let inline: Vec<String> = fields.iter().map(|(k, v)| format!("{k} = \"{v}\"")).collect();
    let line = format!("{name} = {{ {} }}", inline.join(", "));
    let toml = match project_toml(None) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("j3c add: {e}");
            return 1;
        }
    };
    let old = match std::fs::read_to_string(&toml) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("j3c add: read {}: {e}", toml.display());
            return 1;
        }
    };
    let new = add_dep_line(&old, &name, &line);
    if let Err(e) = std::fs::write(&toml, &new) {
        eprintln!("j3c add: write {}: {e}", toml.display());
        return 1;
    }
    match Manifest::load(&toml) {
        Ok(m) => {
            println!("added {line}");
            report(&m);
            0
        }
        Err(e) => {
            let _ = std::fs::write(&toml, old);
            eprintln!("j3c add: {e}\n  (j3.toml left as it was)");
            1
        }
    }
}

/// `text` with `line` as the last key of `[dependencies]` (an existing `name = ...` is replaced),
/// or a new section at the end.
pub(super) fn add_dep_line(text: &str, name: &str, line: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let Some(head) = lines.iter().position(|l| l.trim() == "[dependencies]") else {
        let sep = if text.is_empty() || text.ends_with("\n\n") { "" } else if text.ends_with('\n') { "\n" } else { "\n\n" };
        return format!("{text}{sep}[dependencies]\n{line}\n");
    };
    let end = lines[head + 1..].iter().position(|l| l.trim_start().starts_with('[')).map_or(lines.len(), |i| head + 1 + i);
    let key = |l: &str| l.split('=').next().map(|k| k.trim().to_string());
    if let Some(i) = (head + 1..end).find(|&i| key(&lines[i]).as_deref() == Some(name)) {
        lines[i] = line.to_string();
    } else {
        let mut at = end;
        while at > head + 1 && lines[at - 1].trim().is_empty() {
            at -= 1;
        }
        lines.insert(at, line.to_string());
    }
    lines.join("\n") + "\n"
}

/// `run` / `build` on a `[lib]` project: say what to do instead.
pub(super) fn refuse_lib(path: Option<&str>, cmd: &str) -> Option<i32> {
    match load_input(path) {
        Ok(Input::Project(m)) if m.lib => {
            eprintln!("j3c {cmd}: {} is a library ([lib] in j3.toml), not a program", m.name);
            let demo = m.root.join("examples/demo");
            if demo.join("j3.toml").is_file() {
                eprintln!("  = help: `j3c {cmd} {}`, or `j3c check` the library", demo.display());
            } else {
                eprintln!("  = help: `j3c check` it; run it from a project that depends on it");
            }
            Some(1)
        }
        _ => None,
    }
}
