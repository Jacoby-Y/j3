//! A parsed `j3.toml` as JSON: what package tools read (`j3c manifest --json`, plan 09), so no second TOML
//! reader is needed. Every table is included, unknown ones too.

use super::{parse_toml, unescape, TomlTable, TomlVal};
use crate::cli::json_str;

/// `j3c manifest --json`: TOML text as one line of JSON, every table included (unknown ones too). Errors
/// read `j3.toml:LINE: ...`; the caller renames the file.
pub(crate) fn toml_json(text: &str) -> Result<String, String> {
    to_json(&parse_toml(text)?)
}

/// One JSON object while it is built: keys in the order the file first mentions them.
#[derive(Default)]
struct Obj<'a> {
    keys: Vec<(&'a str, Node<'a>)>,
}

enum Node<'a> {
    Val(&'a TomlVal),
    Obj(Obj<'a>),
}

impl<'a> Obj<'a> {
    /// The table at `path` (`[a.b]` is `["a", "b"]`), made when missing.
    fn table(&mut self, path: &[&'a str], head: &str, line: usize) -> Result<&mut Obj<'a>, String> {
        let Some((first, rest)) = path.split_first() else { return Ok(self) };
        let i = match self.keys.iter().position(|(k, _)| k == first) {
            Some(i) => i,
            None => {
                self.keys.push((first, Node::Obj(Obj::default())));
                self.keys.len() - 1
            }
        };
        match &mut self.keys[i].1 {
            Node::Obj(o) => o.table(rest, head, line),
            Node::Val(_) => Err(format!("j3.toml:{line}: [{head}]: `{first}` is already a value, not a table")),
        }
    }
}

/// The parsed document as JSON. Key order is the file's: tables and keys appear where the file first
/// names them, and a key set twice keeps its last value (as `get` does). `[a.b]` nests `b` inside `a`.
/// Quoted values are strings, string arrays are arrays, inline tables are objects of strings; a bare
/// `true`/`false` is a bool, a bare token that is a JSON number (`2`, `-1.5`, `1e3`) is a number, and any
/// other bare token (`llvm`, `1.2.3`, `007`) is a string.
pub(crate) fn to_json(t: &TomlTable) -> Result<String, String> {
    let mut root = Obj::default();
    let mut vals = t.vals.iter().zip(&t.lines).peekable();
    let mut heads = t.heads.iter().peekable();
    // walk headers and keys together in line order, so sub-tables and keys keep the file's order
    let mut path: Vec<&str> = Vec::new();
    let mut head = "";
    loop {
        match (heads.peek(), vals.peek()) {
            (None, None) => break,
            (Some((_, h)), v) if v.map_or(true, |(_, l)| h < *l) => {
                let (name, line) = heads.next().unwrap();
                path = name.split('.').collect();
                head = name;
                root.table(&path, head, *line)?;
            }
            _ => {
                let ((_, key, val), line) = vals.next().unwrap();
                let obj = root.table(&path, head, *line)?;
                match obj.keys.iter_mut().find(|(k, _)| k == key) {
                    Some((_, Node::Val(old))) => *old = val,
                    Some((_, Node::Obj(_))) => return Err(format!("j3.toml:{line}: [{head}] {key}: already a table")),
                    None => obj.keys.push((key, Node::Val(val))),
                }
            }
        }
    }
    let mut out = String::new();
    write_obj(&mut out, &root);
    Ok(out)
}

fn write_obj(out: &mut String, o: &Obj) {
    out.push('{');
    for (i, (k, n)) in o.keys.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&json_str(k));
        out.push(':');
        match n {
            Node::Obj(o) => write_obj(out, o),
            Node::Val(v) => write_val(out, v),
        }
    }
    out.push('}');
}

fn write_val(out: &mut String, v: &TomlVal) {
    let q = |s: &str| json_str(&unescape(s));
    match v {
        TomlVal::Str(s) => out.push_str(&q(s)),
        TomlVal::Bare(s) if s == "true" || s == "false" || is_json_number(s) => out.push_str(s),
        TomlVal::Bare(s) => out.push_str(&json_str(s)),
        TomlVal::List(xs) => {
            out.push('[');
            out.push_str(&xs.iter().map(|x| q(x)).collect::<Vec<_>>().join(","));
            out.push(']');
        }
        TomlVal::Map(kv) => {
            out.push('{');
            // a key written twice keeps its last value
            let last: Vec<&(String, String)> = kv.iter().enumerate().filter(|(i, (k, _))| !kv[i + 1..].iter().any(|(k2, _)| k2 == k)).map(|(_, e)| e).collect();
            out.push_str(&last.iter().map(|(k, v)| format!("{}:{}", json_str(k), q(v))).collect::<Vec<_>>().join(","));
            out.push('}');
        }
    }
}

/// JSON's number grammar: `-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?`.
fn is_json_number(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = usize::from(b.first() == Some(&b'-'));
    let digits = |i: &mut usize| {
        let start = *i;
        while b.get(*i).is_some_and(u8::is_ascii_digit) {
            *i += 1;
        }
        *i > start
    };
    let int_start = i;
    if !digits(&mut i) || (b[int_start] == b'0' && i - int_start > 1) {
        return false;
    }
    if b.get(i) == Some(&b'.') {
        i += 1;
        if !digits(&mut i) {
            return false;
        }
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        if !digits(&mut i) {
            return false;
        }
    }
    i == b.len()
}
