//! `<icon name="...">` data: the vendored Bootstrap Icons set (`icons/bootstrap.txt`, MIT, `icons/LICENSE`)
//! and a project's own `icons/*.svg`, both turned into one text form that `j3.ui.media` reads:
//! `x y w h|E<d>|N<d>...` (the viewBox, then each shape's path data, `E` even-odd, `N` non-zero).
//! Only the icons a project names are copied into its generated module.

const BOOTSTRAP: &str = include_str!("icons/bootstrap.txt");

/// A project icon: its name (the file stem) and the data form.
pub struct Icon {
    pub name: String,
    pub data: String,
}

/// Every vendored line as `(name, data)`.
pub fn bootstrap() -> impl Iterator<Item = (&'static str, &'static str)> {
    BOOTSTRAP.lines().filter(|l| !l.starts_with('#')).filter_map(|l| l.split_once('\t'))
}

/// The data of `name`: a project icon first, then Bootstrap.
// ponytail: linear scan of ~2000 lines per lookup, sort + binary search if build time shows it
pub fn find<'a>(name: &str, project: &'a [Icon]) -> Option<&'a str> {
    if let Some(i) = project.iter().find(|i| i.name == name) {
        return Some(&i.data);
    }
    bootstrap().find(|(n, _)| *n == name).map(|(_, d)| d)
}

/// Up to three known names close to `name` (same words first, then edit distance).
pub fn suggest(name: &str, project: &[Icon]) -> Vec<String> {
    let words: Vec<&str> = name.split('-').filter(|w| !w.is_empty()).collect();
    let mut all: Vec<(usize, String)> = project
        .iter()
        .map(|i| i.name.as_str())
        .chain(bootstrap().map(|(n, _)| n))
        .map(|n| {
            let shared = words.iter().filter(|w| n.split('-').any(|x| x == **w)).count();
            (edit(name, n) + 4 * (words.len() - shared), n.to_string())
        })
        .collect();
    all.sort();
    all.into_iter().take(3).map(|(_, n)| n).collect()
}

fn edit(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for j in 0..b.len() {
            let cur = row[j + 1];
            row[j + 1] = (prev + if ca == b[j] { 0 } else { 1 }).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

/// Read `dir/*.svg` (a project's `icons/`); a missing folder is no icons.
pub fn load_dir(dir: &std::path::Path) -> Result<Vec<Icon>, String> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return Ok(out) };
    let mut paths: Vec<_> = rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "svg")).collect();
    paths.sort();
    for p in paths {
        let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        let data = from_svg(&text).map_err(|e| format!("{}: {e}", p.display()))?;
        let name = p.file_stem().unwrap().to_string_lossy().to_string();
        out.push(Icon { name, data });
    }
    Ok(out)
}

/// One SVG file in the data form. Shapes: `path`, `circle`, `ellipse`, `rect` (with `rx`/`ry`),
/// `polygon`, `polyline`; `fill-rule` on the shape or the `<svg>`; `fill="none"` shapes are skipped
/// (strokes are not drawn). A `transform` is only taken on a circle, ellipse or rect, and only when it
/// keeps axes (`matrix(a 0 0 d e f)`, `translate`, `scale`).
pub fn from_svg(text: &str) -> Result<String, String> {
    let mut vb: Option<String> = None;
    let mut root_eo = false;
    let mut parts: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find('<') {
        rest = &rest[i + 1..];
        let end = rest.find('>').ok_or("unclosed tag")?;
        let tag = &rest[..end];
        rest = &rest[end + 1..];
        if tag.starts_with('/') || tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        let name: String = tag.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
        let at = |k: &str| attr(tag, k);
        let num = |k: &str| at(k).and_then(|v| v.trim().trim_end_matches("px").parse::<f64>().ok()).unwrap_or(0.0);
        match name.as_str() {
            "svg" => {
                vb = at("viewBox");
                if vb.is_none() {
                    vb = Some(format!("0 0 {} {}", num("width"), num("height")));
                }
                root_eo = at("fill-rule").as_deref() == Some("evenodd");
                continue;
            }
            "title" | "desc" | "defs" | "metadata" => continue,
            "g" => {
                if at("transform").is_some() {
                    return Err("a `<g transform>` is not supported; flatten it in the editor first".into());
                }
                continue;
            }
            _ => {}
        }
        if at("fill").as_deref() == Some("none") {
            continue;
        }
        let m = match at("transform") {
            None => [1.0, 1.0, 0.0, 0.0],
            Some(t) if matches!(name.as_str(), "circle" | "ellipse" | "rect") => axis_transform(&t).ok_or_else(|| format!("`transform=\"{t}\"` on `<{name}>`: only matrix(a 0 0 d e f), translate and scale are supported"))?,
            Some(_) => return Err(format!("`transform` on `<{name}>` is not supported; flatten it in the editor first")),
        };
        let d = match name.as_str() {
            "path" => at("d").unwrap_or_default(),
            "circle" => ellipse(num("cx"), num("cy"), num("r"), num("r"), m),
            "ellipse" => ellipse(num("cx"), num("cy"), num("rx"), num("ry"), m),
            "rect" => {
                let (rx, ry) = match (at("rx"), at("ry")) {
                    (None, None) => (0.0, 0.0),
                    (Some(_), None) => (num("rx"), num("rx")),
                    (None, Some(_)) => (num("ry"), num("ry")),
                    _ => (num("rx"), num("ry")),
                };
                rect(num("x"), num("y"), num("width"), num("height"), rx, ry, m)
            }
            "polygon" | "polyline" => format!("M{}{}", at("points").unwrap_or_default(), if name == "polygon" { "Z" } else { "" }),
            other => return Err(format!("`<{other}>` is not supported (path, circle, ellipse, rect, polygon, polyline)")),
        };
        let d = d.split_whitespace().collect::<Vec<_>>().join(" ");
        if d.contains('|') {
            return Err("path data has a `|`".into());
        }
        let eo = match at("fill-rule").as_deref() {
            Some("evenodd") => true,
            Some(_) => false,
            None => root_eo,
        };
        parts.push(format!("{}{d}", if eo { 'E' } else { 'N' }));
    }
    let vb = vb.ok_or("no `<svg>` element")?;
    if parts.is_empty() {
        return Err("no filled shapes".into());
    }
    if parts.len() > 8 {
        return Err(format!("{} shapes; an icon holds at most 8 (merge paths in the editor)", parts.len()));
    }
    Ok(format!("{}|{}", vb.split_whitespace().collect::<Vec<_>>().join(" "), parts.join("|")))
}

/// `key="value"` in a tag's text (whole attribute name).
fn attr(tag: &str, key: &str) -> Option<String> {
    let mut s = tag;
    while let Some(i) = s.find(key) {
        let before = if i == 0 { ' ' } else { s[..i].chars().last().unwrap() };
        let after = &s[i + key.len()..];
        let after = after.trim_start();
        if before.is_whitespace() && after.starts_with('=') {
            let v = after[1..].trim_start();
            let q = v.chars().next()?;
            if q == '"' || q == '\'' {
                let v = &v[1..];
                return v.find(q).map(|e| v[..e].to_string());
            }
        }
        s = &s[i + key.len()..];
    }
    None
}

/// `[sx, sy, tx, ty]` for a transform that keeps axes.
fn axis_transform(t: &str) -> Option<[f64; 4]> {
    let (f, args) = t.trim().split_once('(')?;
    let v: Vec<f64> = args.trim_end_matches(')').split(|c: char| c == ',' || c.is_whitespace()).filter(|x| !x.is_empty()).map(|x| x.parse().ok()).collect::<Option<_>>()?;
    match (f.trim(), v.as_slice()) {
        ("matrix", [a, 0.0, 0.0, d, e, f]) => Some([*a, *d, *e, *f]),
        ("translate", [x]) => Some([1.0, 1.0, *x, 0.0]),
        ("translate", [x, y]) => Some([1.0, 1.0, *x, *y]),
        ("scale", [s]) => Some([*s, *s, 0.0, 0.0]),
        ("scale", [x, y]) => Some([*x, *y, 0.0, 0.0]),
        _ => None,
    }
}

fn ellipse(cx: f64, cy: f64, rx: f64, ry: f64, m: [f64; 4]) -> String {
    let (cx, cy, rx, ry) = (cx * m[0] + m[2], cy * m[1] + m[3], (rx * m[0]).abs(), (ry * m[1]).abs());
    format!("M{} {} A{rx} {ry} 0 1 0 {} {} A{rx} {ry} 0 1 0 {} {} Z", cx - rx, cy, cx + rx, cy, cx - rx, cy)
}

fn rect(x: f64, y: f64, w: f64, h: f64, rx: f64, ry: f64, m: [f64; 4]) -> String {
    // corners mapped, then sorted, so a flip (`matrix(1 0 0 -1 ..)`) stays a plain rect
    let (ax, bx) = (x * m[0] + m[2], (x + w) * m[0] + m[2]);
    let (ay, by) = (y * m[1] + m[3], (y + h) * m[1] + m[3]);
    let (x0, x1, y0, y1) = (ax.min(bx), ax.max(bx), ay.min(by), ay.max(by));
    let rx = (rx * m[0]).abs().min((x1 - x0) / 2.0);
    let ry = (ry * m[1]).abs().min((y1 - y0) / 2.0);
    if rx <= 0.0 || ry <= 0.0 {
        return format!("M{x0} {y0} H{x1} V{y1} H{x0} Z");
    }
    format!(
        "M{} {y0} H{} A{rx} {ry} 0 0 1 {x1} {} V{} A{rx} {ry} 0 0 1 {} {y1} H{} A{rx} {ry} 0 0 1 {x0} {} V{} A{rx} {ry} 0 0 1 {} {y0} Z",
        x0 + rx, x1 - rx, y0 + ry, y1 - ry, x1 - rx, x0 + rx, y1 - ry, y0 + ry, x0 + rx
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_become_path_data() {
        let c = from_svg(r#"<svg viewBox="0 0 16 16"><circle cx="8" cy="8" r="8"/></svg>"#).unwrap();
        assert_eq!(c, "0 0 16 16|NM0 8 A8 8 0 1 0 16 8 A8 8 0 1 0 0 8 Z");
        let r = from_svg(r#"<svg viewBox="0 0 16 16"><rect width="4" height="12" rx="1" transform="matrix(1 0 0 -1 6 15)"/></svg>"#).unwrap();
        assert!(r.starts_with("0 0 16 16|NM7 3 H9 A1 1 0 0 1 10 4 V14"), "{r}");
        let e = from_svg(r#"<svg viewBox="0 0 16 16"><path fill-rule="evenodd" d="M0 0h16v16H0z M4 4h8v8H4z"/><path d="M1 1h1v1z"/></svg>"#).unwrap();
        assert_eq!(e, "0 0 16 16|EM0 0h16v16H0z M4 4h8v8H4z|NM1 1h1v1z");
        assert!(from_svg(r#"<svg viewBox="0 0 1 1"><path transform="rotate(4)" d="M0 0"/></svg>"#).is_err());
        assert!(from_svg(r#"<svg viewBox="0 0 1 1"><line x1="0"/></svg>"#).is_err());
    }

    #[test]
    fn bootstrap_lookup_and_suggestions() {
        assert!(find("trash", &[]).unwrap().starts_with("0 0 16 16|"));
        assert!(find("trash-can", &[]).is_none());
        assert!(suggest("trash-can", &[]).iter().any(|n| n.starts_with("trash")));
        let own = [Icon { name: "trash".into(), data: "0 0 1 1|NM0 0".into() }];
        assert_eq!(find("trash", &own), Some("0 0 1 1|NM0 0"));
    }

    /// Rewrites `icons/bootstrap.txt` from the `icons/` folder of the bootstrap-icons npm package:
    /// `BOOTSTRAP_ICONS_DIR=.../package/icons cargo test regen_bootstrap_icons -- --ignored`.
    #[test]
    #[ignore]
    fn regen_bootstrap_icons() {
        let dir = std::path::PathBuf::from(std::env::var("BOOTSTRAP_ICONS_DIR").expect("BOOTSTRAP_ICONS_DIR"));
        let icons = load_dir(&dir).unwrap();
        let mut out = String::from("# bootstrap-icons 1.11.3 (MIT, see LICENSE): name<TAB>viewBox|E or N + path data|...\n");
        for i in &icons {
            out.push_str(&format!("{}\t{}\n", i.name, i.data));
        }
        std::fs::write(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui/icons/bootstrap.txt"), out).unwrap();
    }
}
