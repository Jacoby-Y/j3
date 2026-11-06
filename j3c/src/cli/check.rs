use super::*;

pub(super) fn check_cmd(rest: &[String]) -> i32 {
    let mut json = false;
    let mut strict = false;
    let mut autofix = false;
    let mut path = None;
    for a in rest {
        if a == "--json" {
            json = true;
        } else if a == "--autofix" || a == "--fix" {
            autofix = true;
        } else if a == "--strict" {
            strict = true;
        } else if a == "--host" {
            crate::project::HOST.store(true, std::sync::atomic::Ordering::Relaxed);
        } else if a == "--help" || a == "-h" {
            println!("usage: j3c check [--json] [--strict] [--autofix] [--host] [path]\n\ntypechecks the project, then reports missing libraries and files, and (with [ui]) CSS lint\nwarnings for theme.css and every component <style> (`j3c doc css`, Lint).\n  --json    one JSON object: ok/entry (and warnings / css / i18n arrays) or errors[] (the first one's fields also at the top level)\n  --autofix remove unused imports in place, re-check, and put the files back if that breaks the build\n  --strict  exit 1 on any warning: code (unused locals and imports, unreachable code), CSS lint, i18n (default: warnings only)\n  --host    ignore [emit] target and check for this machine (a host build of an Android package)\nAlso warns `file:line:col: message [i18n-missing]` when a `t(\"key\")`/`tf` key is not in the default locale or a locale has a key the default lacks (`j3c doc i18n`).\nSilence: /* j3-lint-ignore rule */ before a line, or [ui] lint = false / lint-off = [\"rule\"] in j3.toml.");
            return 0;
        } else if a.starts_with('-') {
            eprintln!("j3c: unknown flag `{a}`");
            eprintln!("  = help: `j3c check [--json] [--strict] [--autofix] [path]`");
            return 1;
        } else if path.is_none() {
            path = Some(a.as_str());
        } else {
            eprintln!("j3c: unexpected argument `{a}`");
            return 1;
        }
    }
    let t_check = std::time::Instant::now();
    let mut res = compile_user(path);
    if autofix {
        if let Ok((prog, _)) = &res {
            if !prog.fixes.is_empty() {
                let (msg, again) = autofix_run(prog, path);
                eprintln!("{msg}");
                if let Some(r) = again {
                    res = r;
                }
            }
        }
    }
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
            let code = prog.rendered_warnings();
            if json {
                let warns: Vec<String> = code.iter().map(|w| diag_json(w, "warning")).collect();
                println!("{}", check_json(&entry, &css, &i18n, &warns));
            } else {
                for w in &code {
                    eprintln!("{w}");
                }
                for w in css.iter().chain(&i18n) {
                    println!("{w}");
                }
                let n = code.len();
                match n {
                    0 => println!("checked {entry}"),
                    1 => println!("checked {entry} (1 warning)"),
                    _ => println!("checked {entry} ({n} warnings)"),
                }
                let k = prog.fixes.len();
                if k > 0 {
                    let arg = path.map(|p| format!(" {p}")).unwrap_or_default();
                    let what = if k == 1 { "the unused import".to_string() } else { format!("unused imports on {k} `use` lines") };
                    println!("help: run `j3c check --autofix{arg}` to remove {what}");
                }
            }
            if strict && !(css.is_empty() && i18n.is_empty() && code.is_empty()) { 1 } else { 0 }
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

/// Applies `prog.fixes`, then checks again. When the edited sources no longer typecheck, every
/// file is written back as it was. Returns the line to print and the new result (None: reverted).
fn autofix_run(
    prog: &compile::Program,
    path: Option<&str>,
) -> (String, Option<Result<(compile::Program, Option<Manifest>), String>>) {
    let mut by_file: Vec<(String, Vec<&crate::check::lint::Fix>)> = Vec::new();
    for f in &prog.fixes {
        match by_file.iter_mut().find(|(p, _)| *p == f.path) {
            Some((_, v)) => v.push(f),
            None => by_file.push((f.path.clone(), vec![f])),
        }
    }
    let mut saved: Vec<(String, String)> = Vec::new();
    for (file, fixes) in &by_file {
        let Ok(src) = std::fs::read_to_string(file) else { continue };
        let new = crate::check::lint::apply_fixes(&src, fixes);
        if std::fs::write(file, &new).is_ok() {
            saved.push((file.clone(), src));
        }
    }
    let again = compile_user(path);
    if let Err(e) = &again {
        for (file, src) in &saved {
            let _ = std::fs::write(file, src);
        }
        return (format!("autofix: reverted, the edited code did not check:\n{e}"), None);
    }
    let n = prog.fixes.len();
    let files = saved.len();
    (format!("autofix: removed unused imports ({n} `use` line{}, {files} file{})", if n == 1 { "" } else { "s" }, if files == 1 { "" } else { "s" }), Some(again))
}

/// The `check --json` line for a project that typechecks; `css` and `i18n` are the warning arrays
/// (strings), `warnings` the code warnings (objects from `diag_json`); each omitted when empty.
pub(super) fn check_json(entry: &str, css: &[String], i18n: &[String], warnings: &[String]) -> String {
    let arr = |key: &str, v: &[String]| if v.is_empty() { String::new() } else { format!(",\"{key}\":[{}]", v.iter().map(|w| json_str(w)).collect::<Vec<_>>().join(",")) };
    let warns = if warnings.is_empty() { String::new() } else { format!(",\"warnings\":[{}]", warnings.join(",")) };
    format!("{{\"ok\":true,\"entry\":{}{}{}{warns}}}", json_str(entry), arr("css", css), arr("i18n", i18n))
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

/// The `check --json` line for a failure: the first error's fields at the top level (older readers),
/// and every error in `errors`.
pub(super) fn json_err(text: &str) -> String {
    let blocks = split_blocks(text, "error:");
    let blocks: Vec<&str> = blocks.into_iter().filter(|b| !is_count_line(b)).collect();
    let errors: Vec<String> = if blocks.is_empty() { vec![diag_json(text, "error")] } else { blocks.iter().map(|b| diag_json(b, "error")).collect() };
    let first = errors[0].trim_start_matches('{').trim_end_matches('}');
    format!("{{\"ok\":false,{first},\"errors\":[{}]}}", errors.join(","))
}

/// The `error: N errors` summary line under several errors.
fn is_count_line(block: &str) -> bool {
    let t = block.trim();
    t.lines().count() == 1 && t.strip_prefix("error:").is_some_and(|r| r.trim().ends_with(" errors"))
}

/// Rendered diagnostics, one block per `head` line (`error:` / `warning:` at the start of a line).
fn split_blocks<'a>(text: &'a str, head: &str) -> Vec<&'a str> {
    let mut starts: Vec<usize> = Vec::new();
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if line.starts_with(head) {
            starts.push(at);
        }
        at += line.len();
    }
    starts.iter().enumerate().map(|(i, &s)| &text[s..starts.get(i + 1).copied().unwrap_or(text.len())]).collect()
}

/// One rendered diagnostic as `{"severity","message","path","line","column","notes","helps"}`.
pub(super) fn diag_json(text: &str, severity: &str) -> String {
    let mut message = String::new();
    let mut path = String::new();
    let mut line = 0usize;
    let mut column = 0usize;
    let mut notes = Vec::new();
    let mut helps = Vec::new();
    for raw in text.lines() {
        let t = raw.trim();
        if let Some(rest) = t.strip_prefix("error:").or_else(|| t.strip_prefix("warning:")) {
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
        "{{\"severity\":{},\"message\":{},\"path\":{},\"line\":{line},\"column\":{column},\"notes\":{notes},\"helps\":{helps}}}",
        json_str(severity),
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
    const USAGE: &str = "usage: j3c fmt [--write | --check] [path]";
    if rest.iter().any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        println!("  path         a `.j3` / `.j3s` file, or a directory (default: the current one)");
        println!("  -w, --write  rewrite the files that change");
        println!("  --check      exit 1 if formatting would change a file");
        return 0;
    }
    if let Some(bad) = rest.iter().find(|a| a.starts_with('-') && !matches!(a.as_str(), "--write" | "-w" | "--check")) {
        eprintln!("j3c fmt: unknown flag `{bad}`; {USAGE}");
        return 1;
    }
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
