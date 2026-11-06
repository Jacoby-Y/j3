use super::*;

/// One public name: `drawText`, `Texture`, `Texture.destroy`, `Key.Space`.
pub(super) struct Symbol {
    pub(super) qual: String,
    pub(super) doc: String,
}

pub(super) fn symbols(file: &File) -> Vec<Symbol> {
    let mut out = Vec::new();
    let mut add = |qual: String, doc: &str| out.push(Symbol { qual, doc: doc.to_string() });
    for item in &file.items {
        match item {
            Item::Fn(f) if f.vis_pub && shown(&f.name.name) => add(f.name.name.clone(), &f.doc),
            Item::Const(c) if c.vis_pub && shown(&c.name.name) => add(c.name.name.clone(), &c.doc),
            Item::Trait(t) if t.vis_pub => add(t.name.name.clone(), &t.doc),
            Item::Struct(s) if s.vis_pub => {
                add(s.name.name.clone(), &s.doc);
                for f in &s.fields {
                    add(format!("{}.{}", s.name.name, f.name.name), &f.doc);
                }
                for m in &s.methods {
                    add(format!("{}.{}", s.name.name, m.name.name), &m.doc);
                }
            }
            Item::Enum(e) if e.vis_pub => {
                add(e.name.name.clone(), &e.doc);
                for v in &e.variants {
                    add(format!("{}.{}", e.name.name, v.name.name), &v.doc);
                }
            }
            _ => {}
        }
    }
    out
}

/// `j3c doc search <word>`: modules about it, public names containing it, then names whose
/// comment mentions it.
pub(super) fn search(word: &str) -> Result<String, String> {
    let w = word.to_lowercase();
    let (mut modules, mut names, mut docs) = (Vec::new(), Vec::new(), Vec::new());
    for path in stdlib_files()? {
        let m = load_stdlib(&path)?;
        let internal = INTERNAL.iter().any(|(k, _)| m.key.starts_with(k));
        if m.key.to_lowercase().contains(&w) || m.file.doc.to_lowercase().contains(&w) {
            let first = m.file.doc.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
            modules.push((internal, format!("    j3c doc {}  — {}\n", m.key, snippet(first, 70))));
        }
        for s in symbols(&m.file) {
            let first = s.doc.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
            let mut line = format!("    j3c doc {} {}", m.key, s.qual);
            if !first.is_empty() {
                line.push_str(&format!("  — {}", snippet(first, 70)));
            }
            line.push('\n');
            let name = s.qual.rsplit('.').next().unwrap_or("").to_lowercase();
            if name.contains(&w) {
                names.push((internal, line));
            } else if s.doc.to_lowercase().contains(&w) {
                docs.push((internal, line));
            }
        }
    }
    // Game APIs first; internals last.
    modules.sort_by_key(|(internal, _)| *internal);
    names.sort_by_key(|(internal, _)| *internal);
    docs.sort_by_key(|(internal, _)| *internal);
    const MAX: usize = 30;
    let mut out = String::new();
    for (title, list) in [("modules about", &modules), ("names containing", &names), ("comments mentioning", &docs)] {
        if list.is_empty() {
            continue;
        }
        out.push_str(&format!("{title} `{word}`:\n"));
        for (_, line) in list.iter().take(MAX) {
            out.push_str(line);
        }
        if list.len() > MAX {
            out.push_str(&format!("    ... {} more; use a longer word\n", list.len() - MAX));
        }
        out.push('\n');
    }
    // Guide pages and recipes, so a task word finds the section that teaches it.
    let mut pages = Vec::new();
    for names in GUIDES {
        let text = guide(names[0]).unwrap_or_default();
        let hits: Vec<_> = sections(&text).into_iter().filter(|(_, r)| text[r.clone()].to_lowercase().contains(&w)).collect();
        for (name, _) in hits.iter().take(3) {
            let q = if name.contains(' ') { format!("\"{name}\"") } else { name.clone() };
            pages.push(format!("    j3c doc {} {q}\n", names[0]));
        }
        if hits.len() > 3 {
            pages.push(format!("    j3c doc {} --short  ({} more sections)\n", names[0], hits.len() - 3));
        }
    }
    for r in crate::helptext::recipes() {
        if r.name.contains(&w) || r.what.to_lowercase().contains(&w) || r.text.to_lowercase().contains(&w) {
            pages.push(format!("    j3c doc recipes {}  — {}\n", r.name, r.what));
        }
    }
    if !pages.is_empty() {
        out.push_str(&format!("guide pages and recipes mentioning `{word}`:\n"));
        pages.iter().for_each(|l| out.push_str(l));
        out.push('\n');
    }
    if out.is_empty() {
        return Err(format!("no public name or comment mentions `{word}`\n  = help: `j3c doc` lists modules by task"));
    }
    Ok(out)
}
