use super::*;

pub(super) fn check_cmd(rest: &[String]) -> i32 {
    let mut json = false;
    let mut strict = false;
    let mut path = None;
    for a in rest {
        if a == "--json" {
            json = true;
        } else if a == "--strict" {
            strict = true;
        } else if a == "--help" || a == "-h" {
            println!("usage: j3c check [--json] [--strict] [path]\n\ntypechecks the project, then reports missing libraries and files, and (with [ui]) CSS lint\nwarnings for theme.css and every component <style> (`j3c doc css`, Lint).\n  --json    one JSON object: ok/entry (and css / i18n arrays of warnings) or the error\n  --strict  exit 1 when the CSS lint or i18n check finds anything (default: warnings only)\nAlso warns `file:line:col: message [i18n-missing]` when a `t(\"key\")`/`tf` key is not in the default locale or a locale has a key the default lacks (`j3c doc i18n`).\nSilence: /* j3-lint-ignore rule */ before a line, or [ui] lint = false / lint-off = [\"rule\"] in j3.toml.");
            return 0;
        } else if a.starts_with('-') {
            eprintln!("j3c: unknown flag `{a}`");
            eprintln!("  = help: `j3c check [--json] [--strict] [path]`");
            return 1;
        } else if path.is_none() {
            path = Some(a.as_str());
        } else {
            eprintln!("j3c: unexpected argument `{a}`");
            return 1;
        }
    }
    let t_check = std::time::Instant::now();
    let res = compile_user(path);
    timing("check total", t_check);
    match res {
        Ok((prog, manifest)) => {
            if let Err(e) = crate::preflight::check(&prog, manifest.as_ref()) {
                if json {
                    println!("{}", json_err(&e));
                } else {
                    print_fail(&e);
                }
                return 1;
            }
            let entry = match &manifest {
                Some(m) => m.entry.display().to_string(),
                None => path.unwrap_or(".").to_string(),
            };
            // CSS lint: warnings only (exit code unchanged unless --strict)
            let css = match manifest.as_ref().map(crate::csslint::lint_project) {
                Some(Ok(w)) => w,
                Some(Err(e)) => {
                    eprintln!("note: css lint skipped: {e}");
                    Vec::new()
                }
                None => Vec::new(),
            };
            let i18n = manifest.as_ref().map(|m| crate::i18n::lint(&m.root)).unwrap_or_default();
            if json {
                println!("{}", check_json(&entry, &css, &i18n));
            } else {
                for w in css.iter().chain(&i18n) {
                    println!("{w}");
                }
                println!("checked {entry}");
            }
            if strict && !(css.is_empty() && i18n.is_empty()) { 1 } else { 0 }
        }
        Err(e) => {
            if json {
                println!("{}", json_err(&e));
            } else {
                print_fail(&e);
            }
            1
        }
    }
}

/// The `check --json` line for a project that typechecks; `css` and `i18n` are the warning arrays (omitted when empty).
pub(super) fn check_json(entry: &str, css: &[String], i18n: &[String]) -> String {
    let arr = |key: &str, v: &[String]| if v.is_empty() { String::new() } else { format!(",\"{key}\":[{}]", v.iter().map(|w| json_str(w)).collect::<Vec<_>>().join(",")) };
    format!("{{\"ok\":true,\"entry\":{}{}{}}}", json_str(entry), arr("css", css), arr("i18n", i18n))
}

pub(super) fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub(super) fn json_err(text: &str) -> String {
    let mut message = String::new();
    let mut path = String::new();
    let mut line = 0usize;
    let mut column = 0usize;
    let mut notes = Vec::new();
    let mut helps = Vec::new();
    for raw in text.lines() {
        let t = raw.trim();
        if let Some(rest) = t.strip_prefix("error:") {
            if message.is_empty() {
                message = rest.trim().to_string();
            }
        } else if let Some(rest) = t.strip_prefix("-->") {
            let rest = rest.trim();
            if let Some((p, l, c)) = split_loc(rest) {
                path = p;
                line = l;
                column = c;
            } else if path.is_empty() {
                path = rest.to_string();
            }
        } else if let Some(rest) = t.strip_prefix("= note:") {
            notes.push(rest.trim().to_string());
        } else if let Some(rest) = t.strip_prefix("= help:") {
            helps.push(rest.trim().to_string());
        }
    }
    if message.is_empty() {
        message = text.trim().to_string();
    }
    let notes = json_list(&notes);
    let helps = json_list(&helps);
    format!(
        "{{\"ok\":false,\"message\":{},\"path\":{},\"line\":{line},\"column\":{column},\"notes\":{notes},\"helps\":{helps}}}",
        json_str(&message),
        json_str(&path),
    )
}

pub(super) fn json_list(items: &[String]) -> String {
    let mut out = String::from("[");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&json_str(item));
    }
    out.push(']');
    out
}

pub(super) fn split_loc(s: &str) -> Option<(String, usize, usize)> {
    let (rest, col) = s.rsplit_once(':')?;
    let (path, line) = rest.rsplit_once(':')?;
    Some((path.to_string(), line.parse().ok()?, col.parse().ok()?))
}

pub(super) fn fmt_cmd(rest: &[String]) -> i32 {
    let mut write = false;
    let mut check = false;
    let mut path = None;
    for a in rest {
        if a == "--write" || a == "-w" {
            write = true;
        } else if a == "--check" {
            check = true;
        } else if !a.starts_with('-') && path.is_none() {
            path = Some(a.as_str());
        }
    }
    let files = match collect_j3(path) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("j3c: {e}");
            return 1;
        }
    };
    if files.is_empty() {
        eprintln!("j3c: no `.j3` or `.j3s` files to format");
        return 1;
    }
    let mut dirty = 0;
    for file in &files {
        let src = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("j3c: read {}: {e}", file.display());
                return 1;
            }
        };
        let out = match crate::fmt::format_src(&src) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("j3c: fmt {}: {e}", file.display());
                return 1;
            }
        };
        if out == src {
            if path.is_some() && !write && !check && files.len() == 1 {
                print!("{out}");
            }
            continue;
        }
        dirty += 1;
        if check {
            eprintln!("j3c: {} needs formatting", file.display());
            continue;
        }
        if write {
            if let Err(e) = std::fs::write(file, &out) {
                eprintln!("j3c: write {}: {e}", file.display());
                return 1;
            }
        } else if files.len() == 1 {
            print!("{out}");
        } else {
            println!("// --- {} ---", file.display());
            print!("{out}");
        }
    }
    if check && dirty > 0 {
        1
    } else {
        0
    }
}

pub(super) fn collect_j3(path: Option<&str>) -> Result<Vec<PathBuf>, String> {
    match path {
        None => {
            let here = std::env::current_dir().map_err(|e| format!("cwd: {e}"))?;
            let src = here.join("src");
            if src.is_dir() {
                list_project_sources(&here)
            } else {
                list_j3(&here)
            }
        }
        Some(p) => {
            let p = PathBuf::from(p);
            if p.is_dir() {
                let src = p.join("src");
                if src.is_dir() {
                    list_project_sources(&p)
                } else {
                    list_j3(&p)
                }
            } else {
                Ok(vec![p])
            }
        }
    }
}

/// `src/**/*.j3` plus every `.j3s` under the project root (scripts can live anywhere).
pub(super) fn list_project_sources(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = list_j3(&root.join("src"))?;
    for rel in crate::compile::script_paths(root) {
        let path = root.join(rel);
        if !out.contains(&path) {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

pub(super) fn list_j3(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    walk_j3(dir, &mut out)?;
    out.sort();
    Ok(out)
}

pub(super) fn walk_j3(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let ents = std::fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
    for ent in ents.flatten() {
        let p = ent.path();
        if p.is_dir() {
            walk_j3(&p, out)?;
        } else if matches!(p.extension().and_then(|e| e.to_str()), Some("j3" | "j3s")) {
            out.push(p);
        }
    }
    Ok(())
}
