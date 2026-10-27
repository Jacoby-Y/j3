//! `j3c check` i18n warnings and `j3c i18n extract`. Keys are the string-literal first arguments of `t`, `tf`, `tf2`,
//! `tf3`, and `tfn` in `.j3` and `.j3ui` files, found with the compiler's lexer (`.j3ui` expression spans come from
//! `ui::markup::parse`; no second parser); locale files are `key = text` lines (`j3c doc i18n`). Warnings read
//! `file:line:col: message [i18n-missing]`.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::parse::TokenKind;
use crate::ui::markup::{self, Node, Part, Value};

const CALLS: &[&str] = &["t", "tf", "tf2", "tf3", "tfn"];
const PLURAL: &[&str] = &["zero", "one", "two", "few", "many", "other"];

/// A key used in code: file (relative to the project), 1-based line and column of the string.
pub struct Use {
    pub key: String,
    pub file: String,
    pub line: usize,
    pub col: usize,
}

/// The module `j3.i18n` imports: the project's `[i18n]` settings as constants (made in memory by the loader).
pub const MODULE: &str = "j3.i18nconfig";
pub const LABEL: &str = "<j3.i18nconfig>";

pub fn config_module(root: &Path) -> String {
    let (dir, default) = config(root);
    let text = fs::read_to_string(root.join("j3.toml")).unwrap_or_default();
    let fallback = crate::project::toml_str(&text, "i18n", "fallback").unwrap_or_else(|| "en".to_string());
    let q = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        "/// `[i18n]` of j3.toml, baked in at build time.\nmodule j3.i18nconfig;\npub const DIR: string = \"{}\";\npub const DEFAULT: string = \"{}\";\npub const FALLBACK: string = \"{}\";\n",
        q(&dir),
        q(&default),
        q(&fallback)
    )
}

/// `[i18n] dir` and `default` (`locales`, `en`).
pub fn config(root: &Path) -> (String, String) {
    let text = fs::read_to_string(root.join("j3.toml")).unwrap_or_default();
    let get = |k: &str, d: &str| crate::project::toml_str(&text, "i18n", k).unwrap_or_else(|| d.to_string());
    (get("dir", "locales"), get("default", "en"))
}

fn pos(text: &str, at: usize) -> (usize, usize) {
    let before = &text[..at];
    let line = before.matches('\n').count() + 1;
    let start = before.rfind('\n').map_or(0, |i| i + 1);
    (line, before[start..].chars().count() + 1)
}

/// Keys in `text[lo..hi]` (J3 source), lexed with the compiler's lexer so strings and comments are right.
fn scan_span(text: &str, lo: usize, hi: usize, file: &str, out: &mut Vec<Use>) {
    let Ok(toks) = crate::parse::lex::lex(&text[lo..hi], crate::span::FileId(0)) else { return };
    for w in toks.windows(3) {
        if w[0].kind == TokenKind::Ident && CALLS.contains(&w[0].text.as_str()) && w[1].kind == TokenKind::LParen && w[2].kind == TokenKind::String {
            let (line, col) = pos(text, lo + w[2].span.start as usize);
            out.push(Use { key: w[2].text.clone(), file: file.to_string(), line, col });
        }
    }
}

fn scan_nodes(nodes: &[Node], text: &str, file: &str, out: &mut Vec<Use>) {
    let mut span = |r: (usize, usize), out: &mut Vec<Use>| scan_span(text, r.0, r.1, file, out);
    for n in nodes {
        match n {
            Node::Let(l) => span(l.expr, out),
            Node::Text { parts, .. } => {
                for p in parts {
                    if let Part::Hole(r) = p {
                        span(*r, out);
                    }
                }
            }
            Node::Elem(e) => {
                for a in &e.attrs {
                    match &a.value {
                        Value::Expr(r) => span(*r, out),
                        Value::Str(parts) => {
                            for p in parts {
                                if let Part::Hole(r) = p {
                                    span(*r, out);
                                }
                            }
                        }
                        Value::None => {}
                    }
                }
                scan_nodes(&e.children, text, file, out);
            }
            Node::If(i) => {
                if let Some(r) = i.scrut {
                    span(r, out);
                }
                for b in &i.branches {
                    if let Some(r) = b.cond {
                        span(r, out);
                    }
                    for r in &b.vals {
                        span(*r, out);
                    }
                    scan_nodes(&b.nodes, text, file, out);
                }
            }
            Node::Each(e) => {
                span(e.head, out);
                scan_nodes(&e.body, text, file, out);
            }
        }
    }
}

/// Keys used in one source text: all of a `.j3` file, or the `<script>` and every `{...}` of a `.j3ui` (found with
/// `ui::markup::parse`). A file that does not parse is skipped (`j3c check` reports it).
pub fn scan_text(text: &str, file: &str) -> Vec<Use> {
    let mut out = Vec::new();
    if file.ends_with(".j3ui") {
        if let Ok(mk) = markup::parse(text) {
            if let Some((a, b)) = mk.script {
                scan_span(text, a, b, file, &mut out);
            }
            scan_nodes(&mk.nodes, text, file, &mut out);
        }
    } else {
        scan_span(text, 0, text.len(), file, &mut out);
    }
    out
}

fn walk(dir: &Path, root: &Path, skip: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut paths: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    paths.sort();
    for p in paths {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') || name == "target" || p == skip {
            continue;
        }
        if p.is_dir() {
            walk(&p, root, skip, out);
        } else if matches!(p.extension().and_then(|e| e.to_str()), Some("j3" | "j3ui")) {
            out.push(p);
        }
    }
}

/// Every key used in the project's `.j3` and `.j3ui` files (not the locales folder).
pub fn uses(root: &Path) -> Vec<Use> {
    let (dir, _) = config(root);
    let mut files = Vec::new();
    walk(root, root, &root.join(dir), &mut files);
    let mut out = Vec::new();
    for f in files {
        let rel = f.strip_prefix(root).unwrap_or(&f).display().to_string();
        if let Ok(text) = fs::read_to_string(&f) {
            out.extend(scan_text(&text, &rel));
        }
    }
    out
}

/// `(key, line)` of a locale file.
pub fn parse_locale(text: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    for (n, l) in text.lines().enumerate() {
        let t = l.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if let Some((k, _)) = t.split_once('=') {
            if !k.trim().is_empty() {
                out.push((k.trim().to_string(), n + 1));
            }
        }
    }
    out
}

fn base(key: &str) -> &str {
    for p in PLURAL {
        if let Some(b) = key.strip_suffix(p).and_then(|b| b.strip_suffix('.')) {
            return b;
        }
    }
    key
}

/// `warning: file:line:col: message [i18n-missing]` lines for the project at `root`; empty when there is no locales folder.
pub fn lint(root: &Path) -> Vec<String> {
    let (dir, def) = config(root);
    let ldir = root.join(&dir);
    if !ldir.is_dir() {
        return Vec::new();
    }
    let dpath = ldir.join(format!("{def}.txt"));
    let shown = |p: &Path| p.strip_prefix(root).unwrap_or(p).display().to_string();
    let mut out = Vec::new();
    let Ok(dtext) = fs::read_to_string(&dpath) else {
        out.push(format!("warning: {}:1:1: default locale file is missing [i18n-missing]", shown(&dpath)));
        return out;
    };
    let mut have: HashSet<String> = HashSet::new();
    for (k, _) in parse_locale(&dtext) {
        have.insert(k.clone());
        have.insert(base(&k).to_string());
    }
    let mut seen = HashSet::new();
    for u in uses(root) {
        if !have.contains(&u.key) && seen.insert((u.file.clone(), u.line, u.col)) {
            out.push(format!("warning: {}:{}:{}: key `{}` is not in {} [i18n-missing]", u.file, u.line, u.col, u.key, shown(&dpath)));
        }
    }
    let mut files: Vec<PathBuf> = fs::read_dir(&ldir).map(|r| r.filter_map(|e| e.ok()).map(|e| e.path()).collect()).unwrap_or_default();
    files.sort();
    for f in files {
        if f == dpath || f.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&f) else { continue };
        for (k, line) in parse_locale(&text) {
            if !have.contains(&k) && !have.contains(base(&k)) {
                out.push(format!("warning: {}:{}:1: key `{}` is not in the default locale {} [i18n-missing]", shown(&f), line, k, shown(&dpath)));
            }
        }
    }
    out
}

/// `j3c i18n extract [path]`: the keys used in code, one per line with where.
pub fn cmd(rest: &[String]) -> i32 {
    let mut args = rest.iter().map(String::as_str);
    if args.next() != Some("extract") {
        eprintln!("usage: j3c i18n extract [project dir]");
        return 1;
    }
    let root = PathBuf::from(args.next().unwrap_or("."));
    let mut seen = HashSet::new();
    for u in uses(&root) {
        if seen.insert(u.key.clone()) {
            println!("{}\t{}:{}:{}", u.key, u.file, u.line, u.col);
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_calls_and_skips_lookalikes() {
        let src = "let a = t(\"a.b\");\n// t(\"commented\")\nlet s = \"t(\\\"instr\\\")\"; let b = t (\"spaced\");\n";
        let got: Vec<(String, usize, usize)> = scan_text(src, "f.j3").into_iter().map(|u| (u.key, u.line, u.col)).collect();
        assert_eq!(got, vec![("a.b".to_string(), 1, 11), ("spaced".to_string(), 3, 36)]);
        let ui = "<script>\n    fn f(): string { return t(\"in.script\"); }\n</script>\n<p title={t(\"attr.key\")}>{tf(\"x.y\", num(2))} {i18n.t( \"z\")} {set(\"no\")} {t(k)}</p>\n";
        let got: Vec<(String, usize)> = scan_text(ui, "A.j3ui").into_iter().map(|u| (u.key, u.line)).collect();
        assert_eq!(got, vec![("in.script".to_string(), 2), ("attr.key".to_string(), 4), ("x.y".to_string(), 4), ("z".to_string(), 4)]);
    }

    #[test]
    fn config_module_bakes_the_toml_keys() {
        let root = std::env::temp_dir().join(format!("j3_i18n_cfg_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        assert!(config_module(&root).contains("pub const DIR: string = \"locales\";"));
        fs::write(root.join("j3.toml"), "[i18n]\ndir = \"lang\"\ndefault = \"de\"\nfallback = \"fr\"\n").unwrap();
        let m = config_module(&root);
        let _ = fs::remove_dir_all(&root);
        assert!(m.contains("DIR: string = \"lang\"") && m.contains("DEFAULT: string = \"de\"") && m.contains("FALLBACK: string = \"fr\""), "{m}");
    }

    #[test]
    fn lint_reports_missing_keys_in_a_temp_project() {
        let root = std::env::temp_dir().join(format!("j3_i18n_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("locales")).unwrap();
        fs::create_dir_all(root.join("ui")).unwrap();
        fs::write(root.join("locales/en.txt"), "# c\nhello = Hi\nitems.one = {count} item\nitems.other = {count} items\n").unwrap();
        fs::write(root.join("locales/de.txt"), "hello = Hallo\nitems.other = x\nextra = nur de\n").unwrap();
        fs::write(root.join("main.j3"), "fn f() { t(\"hello\"); t(\"gone\"); }\n").unwrap();
        fs::write(root.join("ui/A.j3ui"), "<p>{tf(\"items\", num(1))}</p>\n<p>{t(\"nope\")}</p>\n").unwrap();
        let w = lint(&root);
        let _ = fs::remove_dir_all(&root);
        assert_eq!(w.len(), 3, "{w:?}");
        assert!(w.iter().any(|l| l.starts_with("warning: main.j3:1:") && l.contains("`gone`") && l.ends_with("[i18n-missing]")), "{w:?}");
        assert!(w.iter().any(|l| l.starts_with("warning: ui/A.j3ui:2:") && l.contains("`nope`")), "{w:?}");
        assert!(w.iter().any(|l| l.starts_with("warning: locales/de.txt:3:1:") && l.contains("`extra`")), "{w:?}");
    }
}
