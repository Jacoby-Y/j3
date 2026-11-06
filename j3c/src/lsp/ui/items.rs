//! `.j3ui` completion: context detection and the markup, block, script and J3 items.

use super::*;

// completion

#[derive(Debug, PartialEq)]
enum Ctx {
    Script,
    Style,
    /// Inside `{...}` of the markup.
    Hole,
    /// Inside a tag: the text from after `<` to the cursor.
    Tag(String),
    Other,
}

/// Blank `<script>`/`<style>` content (byte for byte, so offsets hold) so the markup scan does
/// not read J3 or CSS as tags.
pub(super) fn mask(text: &str) -> (Vec<u8>, Option<(usize, usize)>, Option<(usize, usize)>) {
    let mut out = text.as_bytes().to_vec();
    let mut found = [None, None];
    for (k, tag) in ["script", "style"].iter().enumerate() {
        if let Some(open) = text.find(&format!("<{tag}")) {
            if let Some(gt) = text[open..].find('>') {
                let cs = open + gt + 1;
                let ce = text[cs..].find(&format!("</{tag}")).map(|i| cs + i).unwrap_or(text.len());
                found[k] = Some((cs, ce));
                for b in &mut out[cs..ce] {
                    if *b != b'\n' {
                        *b = b' ';
                    }
                }
            }
        }
    }
    (out, found[0], found[1])
}

fn context(text: &str, off: usize) -> Ctx {
    let (m, script, style) = mask(text);
    if script.is_some_and(|(a, b)| off >= a && off <= b) {
        return Ctx::Script;
    }
    if style.is_some_and(|(a, b)| off >= a && off <= b) {
        return Ctx::Style;
    }
    #[derive(PartialEq, Clone, Copy)]
    enum M {
        Brace,
        Quote,
    }
    let b = &m[..];
    let end = off.min(b.len());
    let mut stack: Vec<M> = Vec::new();
    let mut tag: Option<usize> = None;
    let mut i = 0;
    while i < end {
        let c = b[i];
        match stack.last() {
            Some(M::Brace) => match c {
                b'{' => stack.push(M::Brace),
                b'}' => {
                    stack.pop();
                }
                b'"' | b'`' => {
                    i += 1;
                    while i < end && b[i] != c {
                        if b[i] == b'\\' {
                            i += 1;
                        }
                        i += 1;
                    }
                }
                _ => {}
            },
            Some(M::Quote) => match c {
                b'"' => {
                    stack.pop();
                }
                b'{' => stack.push(M::Brace),
                _ => {}
            },
            None => {
                if b[i..].starts_with(b"<!--") {
                    match b[i..].windows(3).position(|w| w == b"-->") {
                        Some(k) => i += k + 2,
                        None => return Ctx::Other,
                    }
                } else if tag.is_some() {
                    match c {
                        b'"' => stack.push(M::Quote),
                        b'{' => stack.push(M::Brace),
                        b'>' => tag = None,
                        _ => {}
                    }
                } else if c == b'<' && b.get(i + 1).is_some_and(|n| n.is_ascii_alphabetic() || *n == b'/') {
                    tag = Some(i + 1);
                } else if c == b'{' {
                    stack.push(M::Brace);
                }
            }
        }
        i += 1;
    }
    match (stack.last(), tag) {
        (Some(M::Brace), _) => Ctx::Hole,
        (Some(M::Quote), _) => Ctx::Other,
        (None, Some(t)) => Ctx::Tag(text[t..end].to_string()),
        _ => Ctx::Other,
    }
}

pub(super) fn it(label: &str, kind: i32, detail: &str, insert: &str, snippet: bool) -> Completion {
    Completion {
        label: label.to_string(),
        kind,
        detail: detail.to_string(),
        insert: insert.to_string(),
        snippet,
        documentation: String::new(),
        sort: String::new(),
        commit_dot: false,
    }
}

pub(super) fn complete_at(path: &Path, overlays: &[(PathBuf, String)], text: &str, off: usize) -> Vec<Completion> {
    let off = off.min(text.len());
    if is_css(path) {
        return css_items(text, path, overlays, None, off);
    }
    match context(text, off) {
        Ctx::Script => { let mut v = script_snippets(text, off); v.extend(j3_items(path, overlays, text, off)); v },
        Ctx::Hole => { let mut v = block_items(text, off); v.extend(j3_items(path, overlays, text, off)); v },
        Ctx::Style => css_items(text, path, overlays, Some(path), off),
        Ctx::Tag(t) => tag_items(path, overlays, &t),
        Ctx::Other => Vec::new(),
    }
}

/// `{#each` `{:case` `{/if`: block names after `{#`, `{:` or `{/`.
fn block_items(text: &str, off: usize) -> Vec<Completion> {
    let (p, _) = prefix_base(text, off);
    let prefix = &text[p..off];
    let Some(sig) = text[..p].strip_suffix(|c| c == '#' || c == ':' || c == '/').and_then(|h| h.strip_suffix('{')).map(|_| text.as_bytes()[p - 1]) else { return Vec::new() };
    let all: &[(&str, &str, &str)] = match sig {
        b'#' => &[("if", "if ${1:cond}}$0{/if", "block"), ("each", "each ${1:xs} as ${2:x} (${3:key})}$0{/each", "keyed loop"), ("match", "match ${1:x}}{:case ${2:a}}$0{/match", "block"), ("let", "let ${1:name} = ${2:expr}", "local")],
        b':' => &[("else", "else", "branch"), ("else if", "else if ${1:cond}", "branch"), ("case", "case ${1:a, b}", "match arm")],
        _ => &[("if", "if", "close"), ("each", "each", "close"), ("match", "match", "close")],
    };
    all.iter().filter(|(l, _, _)| l.starts_with(prefix)).map(|(l, ins, d)| it(l, 14, d, ins, true)).collect()
}

/// `.j3ui` `<script>` statements and lifecycle functions, at the start of a line.
fn script_snippets(text: &str, off: usize) -> Vec<Completion> {
    let (p, base) = prefix_base(text, off);
    let line = text[..p].rsplit('\n').next().unwrap_or("");
    if base.is_some() || !line.trim().is_empty() {
        return Vec::new();
    }
    let prefix = &text[p..off];
    const S: &[(&str, &str, &str)] = &[
        ("provide", "provide ${1:x};", "share a value with descendants"),
        ("inject", "inject ${1:x}: ${2:T};", "read a provided value"),
        ("emit", "emit ${1:pick}($0);", "fire a declared event"),
        ("events", "events {\n\t${1:pick}(${2:slot}: ${3:int});\n}", "declare events and payloads"),
        ("props", "props {\n\t$0\n}", "component props"),
        ("const", "const ${1:NAME}: ${2:int} = $0;", "script constant"),
        ("onMount", "fn onMount() {\n\t$0\n}", "lifecycle: after first build"),
        ("onFrame", "fn onFrame(dt: f32) {\n\t$0\n}", "lifecycle: every frame"),
        ("onDestroy", "fn onDestroy() {\n\t$0\n}", "lifecycle: before removal"),
    ];
    S.iter().filter(|(l, _, _)| l.starts_with(prefix)).map(|(l, ins, d)| it(l, 15, d, ins, true)).collect()
}

/// `(prefix, base)` before the cursor: `sim.go|` is `("go", Some("sim"))`.
fn prefix_base(text: &str, off: usize) -> (usize, Option<(usize, String)>) {
    let b = text.as_bytes();
    let id = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut p = off;
    while p > 0 && id(b[p - 1]) {
        p -= 1;
    }
    if p > 0 && b[p - 1] == b'.' {
        let dot = p - 1;
        let mut s = dot;
        while s > 0 && id(b[s - 1]) {
            s -= 1;
        }
        if s < dot {
            return (p, Some((dot, text[s..dot].to_string())));
        }
    }
    (p, None)
}

fn j3_items(path: &Path, overlays: &[(PathBuf, String)], text: &str, off: usize) -> Vec<Completion> {
    let (pstart, base) = prefix_base(text, off);
    let prefix = &text[pstart..off];
    if let Some((dot, base)) = base {
        // `sim.` does not parse. Swap `sim.pre` for a value that typechecks in that spot (the
        // spot decides which type fits, so try a few) and ask what `sim` is there.
        // ponytail: four analyses at worst; a typed hole could carry its expected type instead.
        let start = dot - base.len();
        if text[..start].trim_end().ends_with('.') {
            return Vec::new();
        }
        for stand_in in ["0", "\"\"", "false", "0;"] {
            let healed = format!("{}{stand_in}{}", &text[..start], &text[off..]);
            let Some(ui) = analyze(path, &with_text(overlays, path, healed)) else { return Vec::new() };
            if let Some((prog, g, go)) = to_gen(&ui, path, start) {
                return complete::member_items(prog, &g.label, go as u32, &base, prefix);
            }
        }
        return Vec::new();
    }
    let Some(ui) = analyze(path, overlays) else { return Vec::new() };
    // ponytail: no healing for a bare name; while the buffer does not typecheck there are no locals.
    let Some((prog, g, go)) = to_gen(&ui, path, off) else { return Vec::new() };
    complete::complete_at(prog, &g.label, &g.text, go as u32)
}

pub(super) fn ui_dir(path: &Path) -> Option<(PathBuf, String)> {
    match crate::project::resolve(path) {
        Ok(crate::project::Input::Project(m)) => m.ui.map(|u| (m.root.clone(), u.dir)),
        _ => None,
    }
}

/// Component names (`ui/Name.j3ui`, `ui/game/Bar.j3ui` is `game.Bar`), buffers included.
fn components(path: &Path, overlays: &[(PathBuf, String)]) -> Vec<String> {
    let Some((root, dir)) = ui_dir(path) else { return Vec::new() };
    let extra: Vec<PathBuf> = overlays.iter().map(|(p, _)| p.clone()).collect();
    let found = crate::ui::scan::scan(&root.join(&dir), &dir, &extra).unwrap_or_default();
    let mut names: Vec<String> = found.into_iter().map(|f| f.name).filter(|n| n.rsplit('.').next().is_some_and(|l| l.starts_with(|c: char| c.is_ascii_uppercase()))).collect();
    names.dedup();
    names
}

fn tag_items(path: &Path, overlays: &[(PathBuf, String)], tag_text: &str) -> Vec<Completion> {
    let body = tag_text.strip_prefix('/').unwrap_or(tag_text);
    let name_end = body.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.')).unwrap_or(body.len());
    let name = &body[..name_end];
    let mut out = Vec::new();
    if name_end == body.len() {
        for e in crate::ui::gen::ELEMENTS {
            if e.starts_with(name) {
                out.push(it(e, 14, "element", e, false));
            }
        }
        if "component".starts_with(name) {
            out.push(it("component", 14, "dynamic component from a closed candidate set", "component", false));
        }
        if "slot".starts_with(name) {
            out.push(it("slot", 14, "component content slot", "slot", false));
        }
        for c in components(path, overlays) {
            if c.starts_with(name) {
                // the client replaces only the word after the last `.`
                let insert = &c[name.rfind('.').map_or(0, |d| d + 1)..];
                out.push(it(&c, 22, "component", insert, false));
            }
        }
        return out;
    }
    if tag_text.starts_with('/') {
        return out;
    }
    let rest = &body[name_end..];
    let word_start = rest.rfind(|c: char| c.is_whitespace()).map(|i| i + 1).unwrap_or(0);
    let written: Vec<&str> = rest[..word_start].split_whitespace().map(|w| w.split('=').next().unwrap_or(w)).collect();
    let mut add = |label: &str, detail: &str, insert: &str| {
        if !written.contains(&label) {
            out.push(it(label, 5, detail, insert, true));
        }
    };
    add("slot", "put this child in a named slot", "slot=\"$0\"");
    add("style:name", "dynamic style property", "style:${1:width}={$0}");
    if name == "component" {
        add("this", "int index or candidate-set enum", "this={$0}");
        add("of", "closed component candidate set", "of={${1:TextMsg}, ${2:ImageMsg}}");
    }
    if name == "slot" {
        add("name", "slot name", "name=\"$0\"");
    }
    if name.rsplit('.').next().is_some_and(|l| l.starts_with(|c: char| c.is_ascii_uppercase())) {
        if let Some((props, events)) = component_api(path, overlays, name) {
            for (p, ty) in props {
                add(&p, &format!("prop: {ty}"), &format!("{p}={{$0}}"));
            }
            for e in events {
                add(&format!("on:{e}"), "event", &format!("on:{e}={{$0}}"));
            }
        }
    } else {
        for a in crate::ui::gen::element_attrs(name) {
            let insert = match a.split_once(':') {
                Some((d, n)) if n == "name" => format!("{d}:${{1:name}}={{$0}}"),
                Some((d, n)) => format!("{d}:{n}={{$0}}"),
                None if matches!(a, "disabled") => a.to_string(),
                None => format!("{a}=\"$0\""),
            };
            add(a, "attribute", &insert);
        }
    }
    out
}

/// A component's props (name, type) and events, from its `<script>`.
fn component_api(path: &Path, overlays: &[(PathBuf, String)], name: &str) -> Option<(Vec<(String, String)>, Vec<String>)> {
    let (root, dir) = ui_dir(path)?;
    let text = read_text(&root.join(dir).join(format!("{}.j3ui", name.replace('.', "/"))), overlays)?;
    let mk = crate::ui::markup::parse(&text).ok()?;
    let sc = crate::ui::markup::parse_script(&text, mk.script?).ok()?;
    let props = sc.props.iter().map(|p| (p.name.clone(), text[p.ty.0..p.ty.1].trim().to_string())).collect();
    Some((props, sc.events.iter().map(|e| e.0.clone()).collect()))
}

// ---------------------------------------------------------------------------------------------
// CSS
