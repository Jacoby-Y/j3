//! The SDK side: declarations from `steam_api_flat.h` and a small type table (typedefs, enums,
//! structs) read from the headers, so a C type can be reduced to the kind J3 passes it as.

use std::collections::{HashMap, HashSet};

/// How a value travels in a call, which is all the ABI check needs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum K {
    Void,
    Bool,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
    Ptr,
    Struct,
    Unknown,
}

impl K {
    pub fn size(self) -> usize {
        match self {
            K::Bool | K::I8 | K::U8 => 1,
            K::I16 | K::U16 => 2,
            K::I32 | K::U32 | K::F32 => 4,
            _ => 8,
        }
    }
    pub fn is_float(self) -> bool {
        matches!(self, K::F32 | K::F64)
    }
}

/// A `SteamAPI_` declaration: return type and parameters as written (comments stripped).
pub struct Decl {
    pub ret: String,
    pub params: Vec<String>,
}

impl Decl {
    pub fn show(&self, name: &str) -> String {
        format!("{} {}({})", self.ret, name, self.params.join(", "))
    }
}

pub fn strip_comments(s: &str) -> String {
    let b = s.as_bytes();
    let (mut out, mut i) = (String::with_capacity(s.len()), 0);
    while i < b.len() {
        if b[i] == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                if b[i] == b'\n' {
                    out.push('\n');
                }
                i += 1;
            }
            i += 2;
        } else {
            let ch = s[i..].chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn is_id(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Index of `word` in `s` at or after `from`, bounded by non-identifier characters.
fn find_word(s: &str, word: &str, from: usize) -> Option<usize> {
    let mut at = from;
    while let Some(i) = s.get(at..)?.find(word) {
        let (a, b) = (at + i, at + i + word.len());
        let before = s[..a].chars().next_back().is_none_or(|c| !is_id(c));
        let after = s[b..].chars().next().is_none_or(|c| !is_id(c));
        if before && after {
            return Some(a);
        }
        at = b;
    }
    None
}

fn split_commas(s: &str) -> Vec<String> {
    let (mut depth, mut cur, mut out) = (0i32, String::new(), Vec::new());
    for c in s.chars() {
        match c {
            '(' | '[' | '<' => depth += 1,
            ')' | ']' | '>' => depth -= 1,
            ',' if depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// Every `S_API <ret> [S_CALLTYPE] SteamAPI_Name( params );` in the flat header.
pub fn parse_flat(text: &str) -> HashMap<String, Decl> {
    let text = strip_comments(text);
    let mut out = HashMap::new();
    let mut at = 0;
    while let Some(i) = find_word(&text, "S_API", at) {
        at = i + 5;
        let Some(semi) = text[at..].find(';') else { break };
        let stmt = &text[at..at + semi];
        let Some(open) = stmt.find('(') else { continue };
        let Some(close) = stmt.rfind(')') else { continue };
        let head = stmt[..open].trim_end();
        let name = head.rsplit(|c: char| !is_id(c)).next().unwrap_or("");
        if !name.starts_with("SteamAPI_") {
            continue;
        }
        let ret = head[..head.len() - name.len()].replace("S_CALLTYPE", "").trim().to_string();
        let mut params = split_commas(&stmt[open + 1..close]);
        if params.len() == 1 && params[0] == "void" {
            params.clear();
        }
        out.insert(name.to_string(), Decl { ret, params });
    }
    out
}

const QUAL: [&str; 6] = ["const", "struct", "class", "enum", "volatile", "union"];
const PRIM: [&str; 6] = ["int", "char", "long", "short", "unsigned", "signed"];

/// The type part of a parameter or return text: `"*"` for anything pointer-like, else the type words.
pub fn type_of(text: &str, has_name: bool) -> String {
    if text.contains(['*', '&', '[']) {
        return "*".into();
    }
    let mut toks: Vec<&str> = text.split_whitespace().filter(|t| !QUAL.contains(t)).collect();
    if has_name && toks.len() >= 2 && !PRIM.contains(&toks[toks.len() - 1]) {
        toks.pop();
    }
    toks.join(" ")
}

#[derive(Default)]
pub struct Types {
    td: HashMap<String, String>,
    enums: HashSet<String>,
    structs: HashSet<String>,
}

impl Types {
    /// Learn the typedefs, enums and structs of one header.
    pub fn add_text(&mut self, text: &str) {
        let t = strip_comments(text);
        for kw in ["enum", "struct", "class"] {
            let mut at = 0;
            while let Some(i) = find_word(&t, kw, at) {
                at = i + kw.len();
                let rest = t[at..].trim_start();
                let rest = rest.strip_prefix("class").map_or(rest, str::trim_start);
                let name: String = rest.chars().take_while(|&c| is_id(c)).collect();
                let after = rest[name.len()..].trim_start();
                let body = after.starts_with('{') || (after.starts_with(':') && !after.starts_with("::"));
                if !name.is_empty() && body {
                    if kw == "enum" { &mut self.enums } else { &mut self.structs }.insert(name);
                }
            }
        }
        let mut at = 0;
        while let Some(i) = find_word(&t, "typedef", at) {
            let (stmt, end) = typedef_stmt(&t, i + 7);
            at = end;
            self.add_typedef(&stmt);
        }
    }

    fn add_typedef(&mut self, stmt: &str) {
        let (head, tail) = match (stmt.find('{'), stmt.rfind('}')) {
            (Some(a), Some(b)) if b > a => (&stmt[..a], &stmt[b + 1..]),
            _ => (stmt, ""),
        };
        let braced = head.len() != stmt.len();
        let words = |s: &str| s.split_whitespace().map(String::from).collect::<Vec<_>>();
        if braced {
            let name: String = tail.trim().chars().rev().take_while(|&c| is_id(c)).collect::<Vec<_>>().into_iter().rev().collect();
            if !name.is_empty() {
                let set = if head.contains("enum") { &mut self.enums } else { &mut self.structs };
                set.insert(name);
            }
            return;
        }
        if let Some(p) = stmt.find("(*") {
            let name: String = stmt[p + 2..].chars().take_while(|&c| is_id(c)).collect();
            self.td.insert(name, "*".into());
            return;
        }
        let toks = words(stmt);
        let Some(name) = toks.last() else { return };
        let base: Vec<&str> = toks[..toks.len() - 1].iter().map(String::as_str).collect();
        if base.contains(&"struct") || base.contains(&"class") {
            self.structs.insert(name.clone());
        } else if base.contains(&"enum") {
            self.enums.insert(name.clone());
        } else {
            self.td.insert(name.clone(), type_of(&base.join(" "), false));
        }
    }

    pub fn kind(&self, ty: &str) -> K {
        self.kind_at(ty, 0)
    }

    fn kind_at(&self, ty: &str, depth: usize) -> K {
        if ty == "*" {
            return K::Ptr;
        }
        if let Some(k) = builtin(ty) {
            return k;
        }
        if self.enums.contains(ty) {
            return K::I32;
        }
        if self.structs.contains(ty) {
            return K::Struct;
        }
        match self.td.get(ty) {
            Some(base) if depth < 8 && base != ty => self.kind_at(base, depth + 1),
            _ => K::Unknown,
        }
    }
}

/// `typedef` statement text from `from` to its `;`, skipping braces; and where it ended.
fn typedef_stmt(t: &str, from: usize) -> (String, usize) {
    let mut depth = 0;
    for (i, c) in t[from..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ';' if depth == 0 => return (t[from..from + i].to_string(), from + i),
            _ => {}
        }
    }
    (String::new(), t.len())
}

fn builtin(ty: &str) -> Option<K> {
    Some(match ty {
        "void" => K::Void,
        "bool" => K::Bool,
        "char" | "int8" | "int8_t" | "signed char" => K::I8,
        "uint8" | "uint8_t" | "unsigned char" => K::U8,
        "int16" | "int16_t" | "short" => K::I16,
        "uint16" | "uint16_t" | "unsigned short" => K::U16,
        "int" | "int32" | "int32_t" | "signed int" => K::I32,
        "uint32" | "uint32_t" | "unsigned" | "unsigned int" => K::U32,
        "int64" | "int64_t" | "long long" | "intp" | "intptr_t" => K::I64,
        "uint64" | "uint64_t" | "unsigned long long" | "uint64_steamid" | "uint64_gameid" | "CSteamID" | "CGameID" | "uintp" | "size_t" => K::U64,
        "float" => K::F32,
        "double" => K::F64,
        _ => return None,
    })
}

/// The kind of a J3 type name as written in an `as fn(...)` cast.
pub fn j3_kind(t: &str) -> K {
    match t {
        "ptr" => K::Ptr,
        "bool" => K::Bool,
        "i8" => K::I8,
        "u8" => K::U8,
        "i16" => K::I16,
        "u16" => K::U16,
        "i32" => K::I32,
        "u32" => K::U32,
        "i64" | "int" => K::I64,
        "u64" | "uint" => K::U64,
        "f32" => K::F32,
        "f64" => K::F64,
        _ if t.starts_with('&') => K::Ptr,
        _ => K::Struct,
    }
}
