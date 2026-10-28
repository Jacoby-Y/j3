//! `j3c doc` — public stdlib surface from the same parser. No second frontend.

use std::path::{Path, PathBuf};

use crate::compile::scene_scripts;
use crate::parse::syntax::{
    keyword_names, ATTRIBUTES, BUILTIN_TYPES, INTRINSICS, J3UI_CALL, J3UI_END, J3UI_IF, J3UI_POP,
};
use crate::parse::{self, EnumItem, Expr, FieldItem, File, FnItem, Ident, Item, StructItem, Type, TypeKind};
use crate::project::{self, Input, Manifest};
use crate::span::{FileId, Span};
use crate::stdlib::stdlib_dir;

mod load;
mod pages;
mod render;
mod search;
#[cfg(test)]
mod tests;

use load::*;
use pages::*;
use render::*;
use search::*;

const HIDDEN: &[&str] = &["game_headless.j3", "build_release.j3", "sys_windows.j3", "sys_macos.j3", "sys_android.j3", "apkassets.j3", "netsys.j3", "netsys_windows.j3", "netsys_macos.j3", "netsys_android.j3", "threadsys.j3", "threadsys_windows.j3", "threadsys_android.j3"];

/// Public so the runtime or editor can reach them, but not what a game calls.
const INTERNAL: &[(&str, &str)] = &[
    ("j3.scene3d.editor.", "editor internals"),
    ("j3.script", "VM behind `.j3s` scripts; games use `@script` (`j3c doc lang`)"),
    ("j3.gfx.urp.shaders", "GLSL for j3.gfx.urp.raytrace"),
];

const USAGE: &str = "\
usage: j3c doc [topic | module [item]] [--short]
    j3c doc                    tasks, then every stdlib module
    j3c doc j3.app             public items and /// comments
    j3c doc j3.app --short     names only, grouped by type
    j3c doc j3.app create      one item (and where it is defined)
    j3c doc j3.app App.create  one member of one type
    j3c doc app                same as j3.app
    j3c doc drawText           an item by name, when only one module has it
    j3c doc search <word>      modules, public names, then comments with the word
    j3c doc recipes [name]     short complete programs: app2d, scene3d, canvas, script, ...
    j3c doc scripts/door.j3s   a script: props, shared, state, public calls
    j3c doc j3.scene3d.scripts the module made from this project's scenes (run in a project)
    j3c doc mygame.world       a module of the project in this directory
    j3c doc css --short        a guide page's sections (any page below)
    j3c doc css layout         one section of a guide page (a prefix is enough)
    j3c doc lang               language
    j3c doc j3ui               `.j3ui` components (markup + CSS, compiled by j3c)
    j3c doc j3ui-immediate     `@j3ui` functions over the immediate-mode `j3.ui`
    j3c doc css                the CSS subset `.j3ui` supports, and how it differs from a browser
    j3c doc testing            run, capture, and diff headless (no window); goldens; HTML snapshot
    j3c doc editor             3D scene editor, scene files, scripts
    j3c doc urp                URP render passes (pass order, GL lifetime)
    j3c doc threads            threads and worker pools: rules, safe modules, mistakes
    j3c doc i18n               translated text: locale files, t/tf, plurals, numbers, RTL, j3c check
    j3c doc ship               release builds, zips, and .deb installers with a menu entry and icon
    j3c doc android            Android: APK builds, [android] keys, touch, running on a device";

pub fn cmd(rest: &[String]) -> i32 {
    let mut args: Vec<&str> = Vec::new();
    let mut short = false;
    for a in rest {
        if a == "--help" || a == "-h" {
            println!("{USAGE}");
            return 0;
        }
        if a == "--short" || a == "-s" {
            short = true;
            continue;
        }
        if a.starts_with('-') {
            eprintln!("j3c: unknown flag `{a}`");
            return 1;
        }
        args.push(a.as_str());
    }
    let text = match args.as_slice() {
        [] => index(),
        [page] if guide(page).is_some() => {
            let text = guide(page).unwrap_or_default();
            Ok(if short { toc(page, &text) } else { text })
        }
        [page, section] if guide(page).is_some() => guide_section(page, section),
        ["recipes"] | ["recipe"] => Ok(crate::helptext::recipe_index()),
        ["recipes", name] | ["recipe", name] => crate::helptext::recipe(name),
        ["search", word] => search(word),
        [query] if query.ends_with(".j3s") => script_doc(query),
        [query] => lookup(query, None, short),
        [query, item] => lookup(query, Some(item), short),
        _ => Err(USAGE.to_string()),
    };
    match text {
        Ok(s) => {
            print!("{s}");
            if !s.ends_with('\n') {
                println!();
            }
            0
        }
        Err(e) => {
            eprintln!("j3c: {e}");
            1
        }
    }
}

/// Guide pages by name (and alias), so `cmd` and `search` share one list.
const GUIDES: &[&[&str]] = &[
    &["lang"],
    &["j3ui"],
    &["j3ui-immediate", "@j3ui"],
    &["css"],
    &["testing", "test", "headless", "golden", "goldens", "imgdiff"],
    &["editor"],
    &["urp"],
    &["threads", "thread"],
    &["i18n", "locale", "locales", "translation", "l10n"],
    &["ship", "dist", "deb", "package", "packaging", "release"],
    &["android", "apk"],
];

fn guide(name: &str) -> Option<String> {
    let names = GUIDES.iter().find(|n| n.contains(&name))?;
    Some(match names[0] {
        "lang" => lang_page(),
        "j3ui" => J3UI_PAGE.to_string(),
        "j3ui-immediate" => j3ui_page(),
        "css" => CSS_PAGE.to_string(),
        "testing" => TESTING_PAGE.to_string(),
        "editor" => editor_page(),
        "urp" => urp_page(),
        "threads" => THREADS_PAGE.to_string(),
        "i18n" => I18N_PAGE.to_string(),
        "ship" => SHIP_PAGE.to_string(),
        _ => ANDROID_PAGE.to_string(),
    })
}

/// A guide page cut at its headings: `(name, byte range)`, covering the whole page in order.
/// A heading is a column-0 line that is a markdown `#` heading, a short title line (`Layout`,
/// `Pick a tool`, `Once:`), a line ending in `:`, a one-word column label (`Selectors   ...`),
/// or a short `Label.` / `- Label:` / `- Label (` lead (`Files.`, `- Lifecycle.`). A markdown page
/// (the editor README, URP) is cut at its `#` headings only.
fn sections(text: &str) -> Vec<(String, std::ops::Range<usize>)> {
    let mut out: Vec<(String, std::ops::Range<usize>)> = vec![("intro".to_string(), 0..0)];
    let (mut at, mut fence) = (0, false);
    let markdown = text.contains("\n## ") || text.contains("\n### ");
    for line in text.split_inclusive('\n') {
        let l = line.trim_end();
        if l.starts_with("```") {
            fence = !fence;
        }
        let name = if fence || (markdown && !l.starts_with('#')) { None } else { heading(l) };
        if let Some(name) = name {
            out.last_mut().unwrap().1.end = at;
            out.push((name, at..at));
        }
        at += line.len();
    }
    out.last_mut().unwrap().1.end = at;
    out.retain(|(_, r)| !r.is_empty());
    out
}

fn heading(l: &str) -> Option<String> {
    let label = |s: &str| s.chars().all(|c| c.is_alphanumeric() || " /-'".contains(c));
    let upper = |s: &str| s.chars().next().is_some_and(|c| c.is_ascii_uppercase());
    if let Some(h) = l.trim_start_matches('#').strip_prefix(' ').filter(|_| l.starts_with('#')) {
        return Some(h.trim().to_string());
    }
    if !upper(l) && !l.starts_with("- ") {
        return None;
    }
    if l.len() <= 45 && !l.ends_with(['.', ',', ';']) && upper(l) {
        return Some(l.trim_end_matches(':').to_string());
    }
    if l.len() <= 70 && l.ends_with(':') && upper(l) {
        return Some(l.trim_end_matches(':').to_string());
    }
    if let Some((word, _)) = l.split_once("  ") {
        if upper(word) && word.chars().all(char::is_alphabetic) {
            return Some(word.to_string());
        }
    }
    let lead = l.strip_prefix("- ").unwrap_or(l);
    let end = [". ", ": ", " ("].iter().filter_map(|p| lead.find(p)).min()?;
    let name = &lead[..end];
    (upper(name) && label(name) && name.split(' ').count() <= 5).then(|| name.to_string())
}

/// `j3c doc css --short`: one line per section.
fn toc(page: &str, text: &str) -> String {
    let mut out = format!("{page}: `j3c doc {page} <section>` prints one (a prefix is enough)\n");
    for (name, r) in sections(text) {
        out.push_str(&format!("    {name:<40} {} lines\n", text[r].lines().count()));
    }
    out
}

/// `j3c doc css layout`: the sections whose name starts with `want`, else those containing it.
fn guide_section(page: &str, want: &str) -> Result<String, String> {
    let text = guide(page).unwrap_or_default();
    let w = want.to_lowercase();
    let all = sections(&text);
    let mut hits: Vec<_> = all.iter().filter(|(n, _)| n.to_lowercase().starts_with(&w)).collect();
    if hits.is_empty() {
        hits = all.iter().filter(|(n, _)| n.to_lowercase().contains(&w)).collect();
    }
    if hits.is_empty() {
        return Err(format!("no section `{want}` in `{page}`\n{}", toc(page, &text)));
    }
    Ok(hits.iter().map(|(_, r)| &text[r.clone()]).collect())
}

fn script_doc(query: &str) -> Result<String, String> {
    let path = PathBuf::from(query);
    let src =
        std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let module = ["script".to_string(), "doc".to_string()];
    let file = parse::parse_script(&src, FileId(1), &module)
        .map_err(|e| parse::format_error(query, &src, &e))?;
    let mut out = format!("script {query}\n\n");
    let mut state = String::new();
    let mut calls = String::new();
    for item in &file.items {
        match item {
            Item::Props(b) => {
                out.push_str("props: set by whoever makes an instance; read-only inside the script\n");
                push_script_fields(&mut out, &b.fields, &src, true);
            }
            Item::Shared(b) => {
                out.push_str("shared: one value for every instance; only the script reads or writes it\n");
                push_script_fields(&mut out, &b.fields, &src, false);
            }
            Item::State(s) => {
                state.push_str(&format!("    {}", s.name.name));
                if let Some(ty) = &s.ty {
                    state.push_str(&format!(": {}", ty_str(ty)));
                }
                state.push_str(&format!(" = {}\n", expr_text(&src, &s.value)));
            }
            Item::Fn(f) if f.vis_pub => push_fn(&mut calls, f, None, "    "),
            _ => {}
        }
    }
    if !state.is_empty() {
        out.push_str("state: each instance's own; private to the script\n");
        out.push_str(&state);
        out.push('\n');
    }
    out.push_str("public calls: `instance.name(...)` from J3 (outside code can't read fields)\n");
    if calls.is_empty() {
        out.push_str("    (none)\n");
    }
    out.push_str(&calls);
    Ok(out)
}

fn push_script_fields(out: &mut String, fields: &[FieldItem], src: &str, props: bool) {
    for field in fields {
        out.push_str(&format!("    {}: {}", field.name.name, ty_str(&field.ty)));
        match &field.default {
            Some(e) => out.push_str(&format!(" = {}", expr_text(src, e))),
            None if props => out.push_str(" (required)"),
            None => {}
        }
        out.push('\n');
        push_doc(out, &field.doc, "        ");
    }
    out.push('\n');
}

pub fn index() -> Result<String, String> {
    let mut out = format!("j3c doc — from this j3c build ({})\n\n", crate::stdlib::provenance());
    out.push_str(TASKS);
    out.push_str("stdlib modules (`j3c doc <module>`):\n");
    let mut internal = Vec::new();
    for path in stdlib_files()? {
        let m = load_stdlib(&path)?;
        if INTERNAL.iter().any(|(k, _)| m.key.starts_with(k)) {
            internal.push(m.key);
            continue;
        }
        // First sentence only; `j3c doc <module>` prints the rest, thread rules included.
        let head: Vec<&str> = m.file.doc.lines().map(str::trim).take_while(|l| !l.is_empty()).collect();
        let head = head.join(" ");
        let first = head.split_inclusive(". ").next().unwrap_or("").trim();
        out.push_str(&format!("    {:<26} {}\n", m.key, snippet(first, 90)));
    }
    out.push_str(&format!("internal (not for games): {}\n\n", internal.join(" ")));
    out.push_str("every page and kind of query: `j3c doc --help`\n");
    Ok(out)
}

fn lookup(query: &str, item: Option<&str>, short: bool) -> Result<String, String> {
    let (m, rest) = match resolve(query) {
        Ok(found) => found,
        Err(e) if item.is_none() => return by_name(query.trim()).ok_or(e)?,
        Err(e) => return Err(e),
    };
    let item = item.map(str::to_string).or(rest);
    if short && item.is_none() {
        return Ok(short_view(&m));
    }
    render_module(&m, item.as_deref())
}

/// `j3c doc drawText`: the module that has it, or the choices when several do.
fn by_name(name: &str) -> Option<Result<String, String>> {
    let mut hits: Vec<(String, String)> = Vec::new();
    for path in stdlib_files().ok()? {
        let Ok(m) = load_stdlib(&path) else { continue };
        for s in symbols(&m.file) {
            if s.qual == name || s.qual.ends_with(&format!(".{name}")) {
                hits.push((m.key.clone(), s.qual));
            }
        }
    }
    match hits.as_slice() {
        [] => None,
        [(module, qual)] => Some(lookup(module, Some(qual), false)),
        _ => {
            let mut out = format!("`{name}` is in more than one place:\n");
            for (module, qual) in &hits {
                out.push_str(&format!("    j3c doc {module} {qual}\n"));
            }
            Some(Ok(out))
        }
    }
}
