//! `j3c check` CSS lint. The CSS parser is J3 stdlib code and there is no second one, so the lint runs it: this side reads
//! the markup of each component (the tags, classes and ids its elements can have), writes them with the style sources to a
//! spec file, and runs a small J3 program (`csslint_main.j3` over `stdlib/j3/ui/csslint.j3`) that parses every sheet and
//! reports. The program is built once per `j3c` binary and cached (`~/.cache/j3/csslint`), so a check costs a few ms.
//! Parse warnings come out too. Silence: `/* j3-lint-ignore rule */` on the line before (no rule: all), `[ui] lint = false`
//! or `lint-off = ["rule"]` in `j3.toml`.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::process::Command;

use crate::project::Manifest;
use crate::ui::markup::{self, Attr, Node, Part, Value};
use crate::ui::scan;

static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Unique per call in this process (tests run lint in parallel).
fn uniq() -> String {
    format!("{}-{}", std::process::id(), SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}

const MAIN: &str = include_str!("csslint_main.j3");

/// The rules `csslint.j3` and this file report (the `[rule]` at the end of a warning).
pub const RULES: &[&str] = &["never-matches", "ellipsis-without-overflow", "offset-without-position", "duplicate-decl", "undefined-var", "broad-has", "has-before-combinator", "style-revert"];

struct Source {
    scope: usize,
    shown: String,
    line0: usize,
    css: String,
}

fn line_col(text: &str, at: usize) -> (usize, usize) {
    let at = at.min(text.len());
    let line = text[..at].matches('\n').count() + 1;
    let col = at - text[..at].rfind('\n').map_or(0, |i| i + 1) + 1;
    (line, col)
}

/// The class words of an attribute value: `a b{x}` gives `a`, `b*`; `{x}` alone gives `*`.
fn words(v: &Value, out: &mut Vec<String>, prefix: &str) {
    match v {
        Value::None => {}
        Value::Expr(_) => out.push(format!("{prefix}*")),
        Value::Str(parts) => {
            let mut s = String::new();
            for p in parts {
                match p {
                    Part::Lit(l) => s.push_str(l),
                    Part::Hole(_) => s.push('\u{1}'),
                }
            }
            for w in s.split_whitespace() {
                out.push(format!("{prefix}{}", w.replace('\u{1}', "*")));
            }
        }
    }
}

fn element_words(attrs: &[Attr]) -> Vec<String> {
    let mut w = Vec::new();
    for a in attrs {
        if a.name == "class" {
            words(&a.value, &mut w, "");
        } else if let Some(c) = a.name.strip_prefix("class:") {
            w.push(c.to_string());
        } else if a.name == "id" {
            words(&a.value, &mut w, "#");
        }
    }
    w
}

/// One `E` line per element the markup can create; a component use that passes a class is an element of unknown tag.
fn facts(nodes: &[Node], scope: usize, src: &str, shown: &str, out: &mut String, found: &mut Vec<(String, usize, usize, String)>) {
    for n in nodes {
        match n {
            Node::Elem(e) => {
                for a in e.attrs.iter().filter(|a| a.name.starts_with("style:")) {
                    if let Value::Str(p) = &a.value {
                        if let [Part::Lit(l)] = p.as_slice() {
                            if matches!(l.trim(), "revert" | "revert-layer") {
                                let (line, col) = line_col(src, a.range.0);
                                found.push((shown.to_string(), line, col, format!("`{}=\"{}\"` acts as `unset` (inline styles have no tag defaults to roll back to) [style-revert]", a.name, l.trim())));
                            }
                        }
                    }
                }
                let w = element_words(&e.attrs);
                // `<component this of>` is a use of a component picked at run time
                if e.comp || e.tag == "component" {
                    if !w.is_empty() {
                        out.push_str(&format!("E\t{scope}\t*\t{}\n", w.join(" ")));
                    }
                } else if e.tag != "slot" {
                    out.push_str(&format!("E\t{scope}\t{}\t{}\n", e.tag, w.join(" ")));
                }
                facts(&e.children, scope, src, shown, out, found);
            }
            Node::If(i) => {
                for b in &i.branches {
                    facts(&b.nodes, scope, src, shown, out, found);
                }
            }
            Node::Each(e) => facts(&e.body, scope, src, shown, out, found),
            Node::Let(_) | Node::Text { .. } => {}
        }
    }
}

/// The lint program, built if this `j3c` has not yet (cached by binary).
fn tool() -> Result<(PathBuf, Option<PathBuf>), String> {
    let host = crate::editor::host_exe()?;
    let mut h = DefaultHasher::new();
    MAIN.hash(&mut h);
    let meta = fs::metadata(&host).map_err(|e| e.to_string())?;
    (meta.len(), meta.modified().ok()).hash(&mut h);
    let overridden = crate::stdlib::using_override();
    let base = crate::editor::cache_root().ok_or("no XDG_CACHE_HOME or HOME")?.join("csslint");
    let key = format!("{:016x}", h.finish());
    // J3_STDLIB (stdlib work) is not hashed: build fresh every time instead of caching
    let dir = if overridden { base.join(format!("tmp-{}", uniq())) } else { base.join(&key) };
    let exe = dir.join("target/j3_csslint");
    if exe.is_file() {
        return Ok((exe, None));
    }
    let temp = base.join(format!("{key}.tmp-{}", uniq()));
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(temp.join("src")).map_err(|e| e.to_string())?;
    fs::write(temp.join("src/main.j3"), MAIN).map_err(|e| e.to_string())?;
    fs::write(temp.join("j3.toml"), "[package]\nname = \"j3_csslint\"\nentry = \"src/main.j3\"\n").map_err(|e| e.to_string())?;
    // in the unit tests the running binary is the test harness, not `j3c`
    #[cfg(test)]
    let (ok, msg) = (crate::cli::run(["build".into(), temp.display().to_string()]) == 0, String::new());
    #[cfg(not(test))]
    let (ok, msg) = {
        let out = Command::new(&host).arg("build").arg(&temp).output().map_err(|e| format!("build css lint: {e}"))?;
        (out.status.success(), String::from_utf8_lossy(&out.stderr).into_owned() + &String::from_utf8_lossy(&out.stdout))
    };
    if !ok || !temp.join("target/j3_csslint").is_file() {
        let _ = fs::remove_dir_all(&temp);
        return Err(format!("css lint program did not build: {}", msg.lines().last().unwrap_or("")));
    }
    // publish by rename: a concurrent build that got there first stays untouched
    if let Err(e) = fs::rename(&temp, &dir) {
        let _ = fs::remove_dir_all(&temp);
        if !exe.is_file() {
            return Err(format!("css lint cache: {e}"));
        }
    }
    // the J3_STDLIB build is private to this run: the caller removes it
    Ok((exe, overridden.then_some(dir)))
}

/// Is `rule` at `line` of `text` silenced by a `j3-lint-ignore` comment on that line or the one before?
fn ignored(text: &str, line: usize, rule: &str) -> bool {
    let lines: Vec<&str> = text.lines().collect();
    [line.wrapping_sub(2), line.wrapping_sub(1)].iter().filter_map(|&i| lines.get(i)).any(|l| {
        l.find("j3-lint-ignore").is_some_and(|at| {
            let rest = l[at + "j3-lint-ignore".len()..].split("*/").next().unwrap_or("");
            let names: Vec<&str> = rest.split(|c: char| c.is_whitespace() || c == ',').filter(|s| !s.is_empty()).collect();
            names.is_empty() || names.contains(&rule)
        })
    })
}

/// `warning: file:line:col: message [rule]` lines for the project's theme and component styles (none without `[ui]`).
pub fn lint_project(m: &Manifest) -> Result<Vec<String>, String> {
    let Some(u) = &m.ui else { return Ok(Vec::new()) };
    if u.lint_off.iter().any(|r| r == "*") {
        return Ok(Vec::new());
    }
    let dir = m.root.join(&u.dir);
    let mut spec = String::new();
    let mut sources: Vec<Source> = Vec::new();
    let mut texts: Vec<(String, String)> = Vec::new();
    let mut found: Vec<(String, usize, usize, String)> = Vec::new();
    if let Some(t) = &u.theme {
        let css = fs::read_to_string(m.root.join(t)).map_err(|e| format!("read {t}: {e}"))?;
        texts.push((t.clone(), css.clone()));
        sources.push(Source { scope: 0, shown: t.clone(), line0: 0, css });
    }
    for (i, f) in scan::scan(&dir, &u.dir, &[])?.into_iter().enumerate() {
        let text = fs::read_to_string(&f.path).map_err(|e| format!("read {}: {e}", f.path.display()))?;
        // a markup error is `j3c check`'s to report (it ran first)
        let Ok(mk) = markup::parse(&text) else { continue };
        let scope = i + 1;
        if let Some((a, b)) = mk.style {
            let top = a + (text[a..b].len() - text[a..b].trim_start_matches('\n').len());
            sources.push(Source { scope, shown: f.shown.clone(), line0: text[..top].matches('\n').count(), css: text[a..b].trim_matches('\n').to_string() });
        }
        facts(&mk.nodes, scope, &text, &f.shown, &mut spec, &mut found);
        texts.push((f.shown.clone(), text));
    }
    if sources.is_empty() && spec.is_empty() {
        return Ok(Vec::new());
    }
    let work = std::env::temp_dir().join(format!("j3_csslint_{}", uniq()));
    fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let result = (|| -> Result<String, String> {
        let mut head = String::new();
        for (k, s) in sources.iter().enumerate() {
            let p = work.join(format!("{k}.css"));
            fs::write(&p, &s.css).map_err(|e| e.to_string())?;
            head.push_str(&format!("S\t{}\t{}\t{}\t{}\n", s.scope, s.shown, s.line0, p.display()));
        }
        let sp = work.join("spec");
        fs::write(&sp, head + &spec).map_err(|e| e.to_string())?;
        let (exe, private) = tool()?;
        let out = Command::new(exe).arg(&sp).output().map_err(|e| format!("run css lint: {e}"));
        if let Some(d) = private {
            let _ = fs::remove_dir_all(d);
        }
        let out = out?;
        if !out.status.success() {
            return Err(format!("css lint program failed: {}", String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("")));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    })();
    let _ = fs::remove_dir_all(&work);
    let out = result?;
    for l in out.lines() {
        let f: Vec<&str> = l.splitn(4, '\t').collect();
        if let [file, line, col, msg] = f[..] {
            found.push((file.to_string(), line.parse().unwrap_or(0), col.parse().unwrap_or(0), msg.to_string()));
        }
    }
    // by file (as read: theme, then components), line, column
    let at = |f: &str| texts.iter().position(|t| t.0 == f).unwrap_or(usize::MAX);
    found.sort_by_key(|(f, l, c, _)| (at(f), *l, *c));
    let mut lines = Vec::new();
    for (file, line, col, msg) in found {
        let rule = RULES.iter().find(|r| msg.ends_with(&format!(" [{r}]"))).copied();
        if let Some(r) = rule {
            if u.lint_off.iter().any(|x| x == r) {
                continue;
            }
            if texts.iter().find(|t| t.0 == file).is_some_and(|t| ignored(&t.1, line, r)) {
                continue;
            }
        }
        lines.push(format!("warning: {file}:{line}:{col}: {msg}"));
    }
    Ok(lines)
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixture(name: &str, toml_extra: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("j3_csslint_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("ui")).unwrap();
        fs::write(root.join("j3.toml"), format!("[package]\nname = \"t\"\nentry = \"src/main.j3\"\n\n[ui]\ndir = \"ui\"\ntheme = \"ui/theme.css\"\n{toml_extra}")).unwrap();
        fs::write(root.join("src/main.j3"), "module t;\nuse t.ui.App;\nfn main() {}\n").unwrap();
        fs::write(root.join("ui/theme.css"), ":root { --bg: #111; }\nbody { color: var(--bg); }\n.card { background: var(--missing); }\n").unwrap();
        fs::write(
            root.join("ui/Kid.j3ui"),
            "<div class=\"kid\"></div>\n<style>\n.kid { color: red; }\n</style>\n",
        )
        .unwrap();
        fs::write(
            root.join("ui/App.j3ui"),
            "<div class=\"box\" id=\"main\">\n  <span class=\"hit r{n}\" style:width=\"revert\">x</span>\n  <Kid class=\"passed\" />\n  {#if true}<p>hi</p>{/if}\n</div>\n<style>\n.box { color: red; color: blue; }\n.hit.r2 { top: 1px; }\n.hit.q9 { color: red; }\n.passed { color: red; }\n.box span { text-overflow: ellipsis; }\n/* j3-lint-ignore never-matches */\n.gone { color: red; }\n.gone2 { color: red; }\ndiv.box.nope { color: red; }\n#main { color: red; }\n#other { color: red; }\nsection { color: red; }\n.box:has(.x) { color: red; }\n.j3-select-list { color: red; }\n</style>\n",
        )
        .unwrap();
        root
    }

    fn lint(root: &Path) -> Vec<String> {
        lint_project(&Manifest::load(&root.join("j3.toml")).unwrap()).unwrap()
    }

    #[test]
    fn reports_known_problems_with_exact_lines() {
        let root = fixture("a", "");
        let got = lint(&root);
        let want = [
            "warning: ui/theme.css:3:9: `var(--missing)` has no fallback and `--missing` is not declared in any stylesheet [undefined-var]",
            "warning: ui/App.j3ui:2:26: `style:width=\"revert\"` acts as `unset` (inline styles have no tag defaults to roll back to) [style-revert]",
            "warning: ui/App.j3ui:7:20: `color` is declared twice in this rule; the earlier one (line 7) is overridden [duplicate-decl]",
            "warning: ui/App.j3ui:8:11: `top` does nothing on an element that is not positioned: add `position: relative`, `absolute`, `fixed` or `sticky` [offset-without-position]",
            "warning: ui/App.j3ui:9:1: selector `.hit.q9` matches no element of this component: nothing has the class `.q9` [never-matches]",
            "warning: ui/App.j3ui:11:13: `text-overflow` does nothing without `overflow` (hidden, clip, auto or scroll) on the same element [ellipsis-without-overflow]",
            "warning: ui/App.j3ui:14:1: selector `.gone2` matches no element of this component: nothing has the class `.gone2` [never-matches]",
            "warning: ui/App.j3ui:15:1: selector `div.box.nope` matches no element of this component: nothing has the class `.nope` [never-matches]",
            "warning: ui/App.j3ui:17:1: selector `#other` matches no element of this component [never-matches]",
            "warning: ui/App.j3ui:18:1: selector `section` matches no element of this component: it has no `<section>` [never-matches]",
        ];
        assert_eq!(got, want, "{got:#?}");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn config_silences() {
        let root = fixture("b", "lint-off = [\"never-matches\", \"duplicate-decl\"]\n");
        let got = lint(&root);
        assert!(got.iter().all(|w| !w.contains("[never-matches]") && !w.contains("[duplicate-decl]")), "{got:#?}");
        assert!(!got.is_empty());
        let _ = fs::remove_dir_all(root);
        let root = fixture("c", "lint = false\n");
        assert!(lint(&root).is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn no_false_positives() {
        let root = std::env::temp_dir().join(format!("j3_csslint_fp_{}", uniq()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("ui/kit")).unwrap();
        fs::write(root.join("j3.toml"), "[package]\nname = \"t\"\nentry = \"src/main.j3\"\n\n[ui]\ndir = \"ui\"\n").unwrap();
        fs::write(root.join("src/main.j3"), "module t;\nfn main() {}\n").unwrap();
        fs::write(root.join("ui/kit/Card.j3ui"), "<div class=\"card\"><slot /></div>\n<style>\n.card { color: red; }\n</style>\n").unwrap();
        let app = r#"<div class="page" id="p">
  <kit.Card class="wide"><span class="inslot">x</span></kit.Card>
  {#each items as it}<li class="row">{it}</li>{/each}
  {#if ok}<b class="yes">y</b>{:else}<i class="no">n</i>{/if}
  {#match k}{:case 1}<u class="m1">a</u>{:else}<s class="m2">b</s>{/match}
  <component this={k} of={kit.Card} class="dyn" />
  <p class="r{n}" class:on={c}>t</p>
  <select><option>a</option></select>
  <div virtual row-height={20}><div class="vrow">r</div></div>
</div>
<style>
.inslot { color: red; }
.wide { color: red; }
.row { color: red; }
.yes, .no, .m1, .m2, .dyn { color: red; }
.r1 { color: red; }
p.on { color: red; }
.r1::before { color: red; content: "x"; }
.page:has(.yes) { color: red; }
.page:not(.zzz) { color: red; }
.page { .row { color: red; } &:hover { color: red; } }
.j3-option, .j3-select-list, .j3-tip { color: red; }
option, select, .vrow, #p, :root { color: red; }
.typo { color: red; }
</style>
"#;
        fs::write(root.join("ui/App.j3ui"), app).unwrap();
        let got = lint(&root);
        assert_eq!(got, ["warning: ui/App.j3ui:24:1: selector `.typo` matches no element of this component: nothing has the class `.typo` [never-matches]"], "{got:#?}");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn nothing_to_lint_runs_nothing() {
        let root = std::env::temp_dir().join(format!("j3_csslint_none_{}", uniq()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.j3"), "fn main() {}\n").unwrap();
        fs::write(root.join("j3.toml"), "[package]\nname = \"t\"\nentry = \"src/main.j3\"\n").unwrap();
        assert!(lint(&root).is_empty());
        fs::write(root.join("j3.toml"), "[package]\nname = \"t\"\nentry = \"src/main.j3\"\n\n[ui]\ndir = \"ui\"\n").unwrap();
        assert!(lint(&root).is_empty());
        let _ = fs::remove_dir_all(root);
    }
}
