//! Wire format: framing, replies, URIs, and small JSON builders.

use super::*;

pub(super) fn completion_json(c: complete::Completion) -> Json {
    let mut pairs = vec![
        ("label".to_string(), Json::Str(c.label)),
        ("kind".to_string(), Json::Num(c.kind as f64)),
        ("detail".to_string(), Json::Str(c.detail)),
    ];
    if !c.insert.is_empty() {
        pairs.push(("insertText".to_string(), Json::Str(c.insert)));
    }
    if c.snippet {
        pairs.push(("insertTextFormat".to_string(), Json::Num(2.0)));
    }
    if !c.documentation.is_empty() {
        pairs.push((
            "documentation".to_string(),
            Json::obj(vec![
                ("kind", Json::Str("markdown".to_string())),
                ("value", Json::Str(c.documentation)),
            ]),
        ));
    }
    if !c.sort.is_empty() {
        pairs.push(("sortText".to_string(), Json::Str(c.sort)));
    }
    if c.commit_dot {
        pairs.push((
            "commitCharacters".to_string(),
            Json::Arr(vec![Json::Str(".".to_string())]),
        ));
    }
    Json::Obj(pairs)
}

pub(super) fn pos_json(line: u32, col: u32) -> Json {
    Json::obj(vec![
        ("line", Json::Num(line as f64)),
        ("character", Json::Num(col as f64)),
    ])
}

pub(super) fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    let decoded = url_decode(rest);
    Some(PathBuf::from(decoded))
}

pub(super) fn path_to_uri(path: &str) -> String {
    let abs = Path::new(path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(path));
    format!("file://{}", abs.display())
}

pub(super) fn url_decode(s: &str) -> String {
    let mut out = String::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hi = from_hex(b[i + 1]);
            let lo = from_hex(b[i + 2]);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as char);
                i += 3;
                continue;
            }
        }
        out.push(b[i] as char);
        i += 1;
    }
    out
}

pub(super) fn from_hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

pub(super) fn read_message(stdin: &mut impl BufRead) -> Result<Option<String>, String> {
    let mut headers = String::new();
    loop {
        let mut line = String::new();
        let n = stdin.read_line(&mut line).map_err(|e| e.to_string())?;
        if n == 0 {
            return Ok(None);
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        headers.push_str(&line);
    }
    let mut len = 0usize;
    for line in headers.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("Content-Length:") {
            len = v.trim().parse().map_err(|_| "bad Content-Length")?;
        }
    }
    if len == 0 {
        return Ok(None);
    }
    let mut buf = vec![0u8; len];
    stdin.read_exact(&mut buf).map_err(|e| e.to_string())?;
    String::from_utf8(buf).map(Some).map_err(|e| e.to_string())
}

pub(super) fn reply(out: &mut impl Write, id: Option<Json>, result: Json) {
    let mut obj = vec![("jsonrpc", Json::Str("2.0".to_string()))];
    if let Some(id) = id {
        obj.push(("id", id));
    }
    obj.push(("result", result));
    write_msg(out, &Json::obj_owned(obj));
}

pub(super) fn notify(out: &mut impl Write, method: &str, params: Json) {
    write_msg(
        out,
        &Json::obj(vec![
            ("jsonrpc", Json::Str("2.0".to_string())),
            ("method", Json::Str(method.to_string())),
            ("params", params),
        ]),
    );
}

pub(super) fn write_msg(out: &mut impl Write, v: &Json) {
    let body = v.stringify();
    let _ = write!(
        out,
        "Content-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let _ = out.flush();
}

impl Json {
    pub(super) fn obj_owned(pairs: Vec<(&str, Json)>) -> Json {
        Json::obj(pairs)
    }
}
