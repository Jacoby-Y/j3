//! `.j3ui` / `ui/*.css` completion from the tables in the stdlib CSS files.

use super::*;


struct CssTables {
    props: Vec<String>,
    kws: Vec<(String, Vec<String>)>,
}

/// Property and keyword tables, read from the embedded `j3/ui/css/props.j3` (`PROP_NAMES`), `cssvalue.j3` (`kwList`) and
/// `cssprop.j3` (`groupFirst`) and `cssgrid.j3` (the `grid-*` names its `declare` takes) so they cannot drift.
fn css_tables() -> CssTables {
    let src: String = ["ui/css/props.j3", "ui/cssvalue.j3", "ui/cssprop.j3", "ui/cssprop/anim.j3"]
        .iter()
        .map(|n| crate::stdlib::files().find(|f| f.0 == *n).map(|f| f.1).unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    let src = src.as_str();
    let quoted = |s: &str| -> Vec<String> { s.split('"').skip(1).step_by(2).map(|x| x.to_string()).collect() };
    let mut props: Vec<String> = Vec::new();
    if let Some(i) = src.find("const PROP_NAMES: string = \"") {
        let rest = &src[i + "const PROP_NAMES: string = \"".len()..];
        props.extend(rest[..rest.find('"').unwrap_or(0)].split_whitespace().filter(|p| !p.starts_with('-')).map(|p| p.to_string()));
    }
    if let Some(i) = src.find("fn groupFirst") {
        let body = &src[i..i + src[i..].find("\n}").unwrap_or(0)];
        for q in quoted(body) {
            if !props.contains(&q) {
                props.push(q);
            }
        }
    }
    if let Some(g) = crate::stdlib::files().find(|f| f.0 == "ui/cssgrid.j3").map(|f| f.1) {
        for q in quoted(g) {
            if (q.starts_with("grid-") || q.starts_with("place-")) && !props.contains(&q) {
                props.push(q);
            }
        }
    }
    let mut kws = Vec::new();
    if let Some(i) = src.find("fn kwList") {
        let body = &src[i..i + src[i..].find("\n}").unwrap_or(0)];
        for line in body.lines() {
            let l = line.trim();
            let vals = quoted(l).first().map(|v| v.split_whitespace().map(|x| x.to_string()).collect::<Vec<_>>());
            let (Some(vals), true) = (vals, l.starts_with("if prop == P_") || l.starts_with("return")) else { continue };
            let name = match l.strip_prefix("if prop == P_") {
                Some(r) => r.split(' ').next().unwrap_or("").to_lowercase().replace('_', "-"),
                None => "text-align".to_string(),
            };
            kws.push((name, vals));
        }
    }
    CssTables { props, kws }
}

fn custom_props(text: &str, out: &mut Vec<String>) {
    let b = text.as_bytes();
    let mut i = 0;
    while let Some(k) = text[i..].find("--") {
        let s = i + k;
        let mut e = s + 2;
        while e < b.len() && (b[e].is_ascii_alphanumeric() || b[e] == b'-' || b[e] == b'_') {
            e += 1;
        }
        if text[e..].trim_start().starts_with(':') && e > s + 2 && !out.contains(&text[s..e].to_string()) {
            out.push(text[s..e].to_string());
        }
        i = e.max(s + 2);
    }
}

pub(super) fn css_items(text: &str, path: &Path, overlays: &[(PathBuf, String)], ui_file: Option<&Path>, off: usize) -> Vec<Completion> {
    // The CSS the cursor is in: a `<style>` block, or a whole `.css` file.
    let (lo, hi) = if ui_file.is_some() {
        let (_, _, style) = mask(text);
        style.unwrap_or((0, 0))
    } else {
        (0, text.len())
    };
    let head = &text[lo..off.min(text.len()).max(lo)];
    let mut depth = 0i32;
    let mut in_comment = false;
    let mut clean_b: Vec<u8> = Vec::new();
    let hb = head.as_bytes();
    let mut i = 0;
    while i < hb.len() {
        if in_comment {
            if hb[i..].starts_with(b"*/") {
                in_comment = false;
                i += 1;
            }
        } else if hb[i..].starts_with(b"/*") {
            in_comment = true;
            i += 1;
        } else {
            match hb[i] {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
            clean_b.push(hb[i]);
        }
        i += 1;
    }
    if depth <= 0 || in_comment {
        return Vec::new();
    }
    let clean = String::from_utf8_lossy(&clean_b).to_string();
    let seg_at = clean.rfind([';', '{', '}']).map(|i| i + 1).unwrap_or(0);
    let seg = &clean[seg_at..];
    let t = css_tables();
    let mut out = Vec::new();
    match seg.split_once(':') {
        None => {
            let prefix = seg.trim_start();
            for p in &t.props {
                if p.starts_with(prefix) {
                    out.push(it(p, 10, "property", &format!("{p}: "), false));
                }
            }
        }
        Some((prop, value)) => {
            let word = value.rsplit(|c: char| c.is_whitespace() || c == ',').next().unwrap_or("");
            if let Some(v) = word.strip_prefix("var(") {
                let mut names = Vec::new();
                custom_props(&text[lo..hi], &mut names);
                if let Some((root, _)) = ui_dir(path) {
                    if let Some(theme) = crate::project::resolve(path).ok().and_then(|i| match i {
                        crate::project::Input::Project(m) => m.ui.and_then(|u| u.theme),
                        _ => None,
                    }) {
                        if let Some(tt) = read_text(&root.join(theme), overlays) {
                            custom_props(&tt, &mut names);
                        }
                    }
                }
                names.push("--j3-tooltip-delay".to_string());
                for n in names {
                    if n.starts_with(v) {
                        out.push(it(&n, 6, "custom property", &n, false));
                    }
                }
            } else {
                if let Some((_, kws)) = t.kws.iter().find(|(n, _)| n == prop.trim()) {
                    for k in kws {
                        if k.starts_with(word) {
                            out.push(it(k, 12, "keyword", k, false));
                        }
                    }
                }
                for f in ["calc(", "min(", "max(", "clamp("] {
                    if !word.is_empty() && f.starts_with(word) {
                        out.push(it(f, 3, "css function", f, false));
                    }
                }
                let num = word.trim_end_matches(|c: char| c.is_ascii_alphabetic());
                if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit() || c == '.') {
                    for u in ["vw", "vh", "vmin", "vmax"] {
                        if u.starts_with(&word[num.len()..]) && word.len() > num.len() {
                            out.push(it(&format!("{num}{u}"), 12, "viewport unit", &format!("{num}{u}"), false));
                        }
                    }
                }
                if "var(--".starts_with(word) {
                    out.push(it("var(--", 15, "custom property", "var(--", false));
                }
            }
        }
    }
    out
}
