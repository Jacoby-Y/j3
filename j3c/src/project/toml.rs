//! The TOML subset `j3.toml` and the user config use: tables, string and string-array values, and one-line
//! inline tables of strings (`{ name = "x", exec = "y" }`).

#[derive(Clone, Debug)]
pub(super) enum TomlVal {
    Str(String),
    List(Vec<String>),
    /// An inline table of string values, in file order.
    Map(Vec<(String, String)>),
}

impl TomlVal {
    pub(super) fn as_str(&self) -> Option<&str> {
        match self {
            TomlVal::Str(s) => Some(s),
            TomlVal::List(_) | TomlVal::Map(_) => None,
        }
    }

    pub(super) fn as_list(&self, key: &str) -> Result<Vec<String>, String> {
        match self {
            TomlVal::List(v) => Ok(v.clone()),
            TomlVal::Str(_) | TomlVal::Map(_) => Err(format!("j3.toml: `{key}` must be an array")),
        }
    }
}

pub(super) struct TomlTable {
    pub(super) vals: Vec<(String, String, TomlVal)>,
    /// `[section]` headers with their line numbers, in file order (a header with no keys still counts).
    heads: Vec<(String, usize)>,
    pub(super) lines: Vec<usize>,
}

impl TomlTable {
    pub(super) fn has_section(&self, section: &str) -> bool {
        self.heads.iter().any(|(n, _)| n == section)
    }

    pub(super) fn get(&self, section: &str, key: &str) -> Option<&TomlVal> {
        self.vals
            .iter()
            .rev()
            .find(|(s, k, _)| s == section && k == key)
            .map(|(_, _, v)| v)
    }
}

pub(super) fn parse_toml(text: &str) -> Result<TomlTable, String> {
    let mut section = String::new();
    let (mut vals, mut heads, mut lines) = (Vec::new(), Vec::new(), Vec::new());
    for (i, raw) in text.lines().enumerate() {
        let line_no = i + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = parse_section(line) {
            heads.push((name.clone(), line_no));
            section = name;
            continue;
        }
        if section.is_empty() {
            return Err(format!("j3.toml:{line_no}: key outside a section"));
        }
        let (key, val) = parse_key_val(line, line_no)?;
        vals.push((section.clone(), key, val));
        lines.push(line_no);
    }
    Ok(TomlTable { vals, heads, lines })
}

fn strip_comment(line: &str) -> &str {
    let mut in_str = false;
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' if in_str => {
                in_str = false;
                i += 1;
            }
            b'"' => {
                in_str = true;
                i += 1;
            }
            b'\\' if in_str => i += 2,
            b'#' if !in_str => return &line[..i],
            _ => i += 1,
        }
    }
    line
}

fn parse_section(line: &str) -> Option<String> {
    let s = line.strip_prefix('[')?.strip_suffix(']')?;
    if s.is_empty() || s.contains('[') || s.contains(']') || s.contains(' ') {
        return None;
    }
    Some(s.to_string())
}

fn parse_key_val(line: &str, line_no: usize) -> Result<(String, TomlVal), String> {
    let eq = line
        .find('=')
        .ok_or_else(|| format!("j3.toml:{line_no}: expected `key = value`"))?;
    let key = line[..eq].trim();
    if key.is_empty() || !is_ident(key) {
        return Err(format!("j3.toml:{line_no}: bad key"));
    }
    let rhs = line[eq + 1..].trim();
    let val = if rhs.starts_with('{') {
        TomlVal::Map(parse_inline_table(rhs, line_no)?)
    } else if rhs.starts_with('[') {
        TomlVal::List(parse_string_array(rhs, line_no)?)
    } else if rhs.starts_with('"') {
        TomlVal::Str(parse_string(rhs, line_no)?)
    } else if is_bare(rhs) {
        TomlVal::Str(rhs.to_string())
    } else {
        TomlVal::Str(parse_string(rhs, line_no)?)
    };
    Ok((key.to_string(), val))
}

fn is_bare(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

pub(super) fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn parse_string(s: &str, line_no: usize) -> Result<String, String> {
    let inner = s
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .ok_or_else(|| format!("j3.toml:{line_no}: expected a \"string\""))?;
    Ok(inner.to_string())
}

/// `{ a = "x", b = "y" }`: string values only, on one line.
fn parse_inline_table(s: &str, line_no: usize) -> Result<Vec<(String, String)>, String> {
    let inner = s
        .strip_prefix('{')
        .and_then(|t| t.strip_suffix('}'))
        .ok_or_else(|| format!("j3.toml:{line_no}: expected `{{ key = \"value\", ... }}` on one line"))?;
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    for ch in inner.chars().chain(std::iter::once(',')) {
        if ch == '"' {
            in_str = !in_str;
        }
        if ch == ',' && !in_str {
            let part = cur.trim();
            if !part.is_empty() {
                match parse_key_val(part, line_no)? {
                    (k, TomlVal::Str(v)) => out.push((k, v)),
                    (k, _) => return Err(format!("j3.toml:{line_no}: `{k}` in an inline table must be a string")),
                }
            }
            cur.clear();
            continue;
        }
        cur.push(ch);
    }
    Ok(out)
}

fn parse_string_array(s: &str, line_no: usize) -> Result<Vec<String>, String> {
    let inner = s
        .strip_prefix('[')
        .and_then(|t| t.strip_suffix(']'))
        .ok_or_else(|| format!("j3.toml:{line_no}: expected an array"))?;
    let inner = inner.trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    for ch in inner.chars() {
        if ch == '"' {
            in_str = !in_str;
            cur.push(ch);
            continue;
        }
        if ch == ',' && !in_str {
            let part = cur.trim();
            if !part.is_empty() {
                out.push(parse_string(part, line_no)?);
            }
            cur.clear();
            continue;
        }
        cur.push(ch);
    }
    let part = cur.trim();
    if !part.is_empty() {
        out.push(parse_string(part, line_no)?);
    }
    Ok(out)
}

/// One `[section] key = "value"` from TOML text (used for the user config).
pub(crate) fn toml_str(text: &str, section: &str, key: &str) -> Option<String> {
    parse_toml(text).ok()?.get(section, key)?.as_str().map(str::to_string)
}

/// A string array of TOML text; None when missing or not an array.
pub(crate) fn toml_list(text: &str, section: &str, key: &str) -> Option<Vec<String>> {
    parse_toml(text).ok()?.get(section, key)?.as_list(key).ok()
}

/// Section headers (with lines) and string/number entries `(section, key, value, line)` of TOML text, for
/// `j3c golden`. Errors read `j3.toml:LINE: ...`; the caller renames the file.
pub(crate) fn toml_doc(text: &str) -> Result<(Vec<(String, usize)>, Vec<(String, String, String, usize)>), String> {
    let t = parse_toml(text)?;
    let mut out = Vec::new();
    for ((s, k, v), line) in t.vals.into_iter().zip(t.lines) {
        match v {
            TomlVal::Str(v) => out.push((s, k, v, line)),
            TomlVal::List(_) | TomlVal::Map(_) => return Err(format!("j3.toml:{line}: `{k}` must be a string or number")),
        }
    }
    Ok((t.heads, out))
}
