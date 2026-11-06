//! Reads the `j3.steam` sources (the embedded ones, or `J3_STDLIB`) and lists what the checker
//! compares with the SDK: symbol literals with their casts, accessor rows, layout rows, callback
//! ids and constants. Plain line patterns; the stdlib's own style keeps one fact per line.

use std::collections::HashMap;
use std::path::Path;

/// One source file of `j3.steam`: `file` is shown in messages, `text` is scanned.
pub struct Src {
    pub file: String,
    pub text: String,
}

/// Where a fact was written: `j3/steam/pack.j3:57`.
#[derive(Clone)]
pub struct Loc {
    pub file: String,
    pub line: usize,
}

impl std::fmt::Display for Loc {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}:{}", self.file, self.line)
    }
}

/// `as fn(P1, P2): R` as written in J3 (type names only).
pub struct Cast {
    pub params: Vec<String>,
    pub ret: Option<String>,
}

/// One use of a `"SteamAPI_..."` literal; `cast` is set when the line casts the symbol to a `fn` type.
pub struct Site {
    pub loc: Loc,
    pub sym: String,
    pub cast: Option<Cast>,
}

/// One `addLayout(struct, field, pack4, pack8)` row; `field` is `SIZE` for the struct size.
pub struct Row {
    pub loc: Loc,
    pub name: String,
    pub field: String,
    pub p4: i64,
    pub p8: i64,
}

#[derive(Default)]
pub struct Scan {
    pub sites: Vec<Site>,
    pub accessors: Vec<(String, Loc)>,
    pub rows: Vec<Row>,
    pub callbacks: Vec<(String, i64, Loc)>,
    pub cb_consts: Vec<(String, i64, Loc)>,
    pub consts: HashMap<String, (i64, Loc)>,
    /// Names of the `pub const`s, with where.
    pub pubs: Vec<(String, Loc)>,
    /// Lines with a symbol literal in a form the checker does not know.
    pub odd: Vec<(Loc, String)>,
}

/// The `j3.steam` sources: `steam.j3` and `steam/*.j3`.
pub fn sources() -> Vec<Src> {
    let mut out = Vec::new();
    if crate::stdlib::using_override() {
        let root = crate::stdlib::stdlib_dir().join("j3");
        push_disk(&mut out, &root, "steam.j3");
        let mut names: Vec<String> = std::fs::read_dir(root.join("steam"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(String::from))
            .filter(|n| n.ends_with(".j3"))
            .collect();
        names.sort();
        for n in names {
            push_disk(&mut out, &root, &format!("steam/{n}"));
        }
    } else {
        for (name, text) in crate::stdlib::files() {
            if name == "steam.j3" || name.starts_with("steam/") {
                out.push(Src { file: format!("j3/{name}"), text: text.to_string() });
            }
        }
    }
    out
}

fn push_disk(out: &mut Vec<Src>, root: &Path, name: &str) {
    if let Ok(text) = std::fs::read_to_string(root.join(name)) {
        out.push(Src { file: format!("j3/{name}"), text });
    }
}

/// Byte ranges of the `"SteamAPI_name"` literals on a line (the bare prefix `"SteamAPI_"` is skipped).
fn literals(line: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = line[at..].find("\"SteamAPI_") {
        let start = at + i + 1;
        let Some(len) = line[start..].find('"') else { break };
        let end = start + len;
        let name = &line[start..end];
        if name.len() > 9 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            out.push((start, end));
        }
        at = end + 1;
    }
    out
}

/// Parse what follows `as fn(`: the parameter types, then an optional `: Ret`.
fn parse_cast(s: &str) -> Option<Cast> {
    let (mut depth, mut cur, mut params) = (1, String::new(), Vec::new());
    let mut end = None;
    for (i, c) in s.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
            ',' if depth == 1 => {
                params.push(cur.trim().to_string());
                cur.clear();
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    let end = end?;
    if !cur.trim().is_empty() {
        params.push(cur.trim().to_string());
    }
    let rest = s[end + 1..].trim_start();
    let ret = rest.strip_prefix(':').map(|r| {
        r.trim().split(|c: char| matches!(c, ';' | '{' | ',' | ')') || c.is_whitespace()).next().unwrap_or("").to_string()
    });
    Some(Cast { params, ret: ret.filter(|r| !r.is_empty()) })
}

fn int(s: &str) -> Option<i64> {
    let s = s.trim().trim_end_matches(';').trim();
    if let Some(r) = s.strip_prefix("0 - ") {
        return r.trim().parse::<i64>().ok().map(|v| -v);
    }
    s.parse().ok()
}

fn args_of<'a>(line: &'a str, call: &str) -> Option<Vec<&'a str>> {
    let i = line.find(call)? + call.len();
    let j = line.rfind(')')?;
    (j >= i).then(|| line[i..j].split(',').map(str::trim).collect())
}

fn unquote(s: &str) -> String {
    s.trim_matches('"').to_string()
}

pub fn scan(srcs: &[Src]) -> Scan {
    let mut sc = Scan::default();
    for src in srcs {
        // `X = fnp("SteamAPI_Y")` then `X as fn(..)` elsewhere in the file
        let mut vars: HashMap<String, String> = HashMap::new();
        for raw in src.text.lines() {
            let line = raw.split("//").next().unwrap_or("");
            if let (Some(lit), Some(eq)) = (literals(line).first(), line.find(" = fnp(")) {
                vars.insert(line[..eq].trim().trim_start_matches("let ").to_string(), line[lit.0..lit.1].to_string());
            }
        }
        for (n, raw) in src.text.lines().enumerate() {
            let loc = Loc { file: src.file.clone(), line: n + 1 };
            let line = raw.split("//").next().unwrap_or("").trim();
            scan_line(&mut sc, &loc, line, &vars);
        }
    }
    sc
}

fn scan_line(sc: &mut Scan, loc: &Loc, line: &str, vars: &HashMap<String, String>) {
    let lits = literals(line);
    if line.contains("addAccessor(\"") {
        for (a, b) in lits {
            sc.accessors.push((line[a..b].to_string(), loc.clone()));
        }
    } else if !lits.is_empty() {
        let cast_at = line.find(" as fn(").map(|i| i + 7);
        for (a, b) in &lits {
            let cast = cast_at.filter(|c| c > b).and_then(|c| parse_cast(&line[c..]));
            let known = cast.is_some() || line.contains("hasFn(") || line.contains(" = fnp(");
            if !known {
                sc.odd.push((loc.clone(), line.to_string()));
            }
            sc.sites.push(Site { loc: loc.clone(), sym: line[*a..*b].to_string(), cast });
        }
    } else if let Some(c) = line.find(" as fn(") {
        let head = line[..c].rsplit(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_')).next().unwrap_or("");
        if let (Some(sym), Some(cast)) = (vars.get(head), parse_cast(&line[c + 7..])) {
            sc.sites.push(Site { loc: loc.clone(), sym: sym.clone(), cast: Some(cast) });
        }
    }
    if line.contains("addLayout(\"") {
        if let Some(a) = args_of(line, "addLayout(") {
            if let (4, Some(p4), Some(p8)) = (a.len(), int(a[2]), int(a[3])) {
                sc.rows.push(Row { loc: loc.clone(), name: unquote(a[0]), field: unquote(a[1]), p4, p8 });
            }
        }
    }
    if line.contains("addCallback(\"") {
        if let Some(a) = args_of(line, "addCallback(") {
            if let (2, Some(id)) = (a.len(), int(a[1])) {
                sc.callbacks.push((unquote(a[0]), id, loc.clone()));
            }
        }
    }
    let decl = line.strip_prefix("pub ").unwrap_or(line);
    if let Some(rest) = decl.strip_prefix("const ") {
        if let Some((name, tail)) = rest.split_once(':') {
            if decl.len() != line.len() {
                sc.pubs.push((name.trim().to_string(), loc.clone()));
            }
            if let Some(v) = tail.split_once('=').and_then(|(_, v)| int(v)) {
                let name = name.trim().to_string();
                if name.starts_with("CB_") {
                    sc.cb_consts.push((name.clone(), v, loc.clone()));
                }
                sc.consts.insert(name, (v, loc.clone()));
            }
        }
    }
}
