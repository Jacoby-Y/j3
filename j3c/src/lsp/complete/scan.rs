pub(super) struct CallSite {
    pub(super) name: String,
    pub(super) receiver: Option<String>,
    pub(super) commas: u32,
}

pub(super) fn find_call(src: &str, offset: usize) -> Option<CallSite> {
    let b = src.as_bytes();
    let end = offset.min(b.len());
    let mut stack: Vec<Frame> = Vec::new();
    let mut i = 0;
    let mut mode = Mode::Code;
    while i < end {
        if mode == Mode::Comment {
            if b[i] == b'\n' {
                mode = Mode::Code;
            }
            i += 1;
            continue;
        }
        if mode == Mode::String || mode == Mode::Tick {
            if b[i] == b'\\' {
                i += 2;
                continue;
            }
            let quote = if mode == Mode::String { b'"' } else { b'`' };
            if b[i] == quote {
                mode = Mode::Code;
            }
            i += 1;
            continue;
        }
        if b[i] == b'/' && i + 1 < end && b[i + 1] == b'/' {
            mode = Mode::Comment;
            i += 2;
            continue;
        }
        match b[i] {
            b'"' => mode = Mode::String,
            b'`' => mode = Mode::Tick,
            b'(' => stack.push(Frame { kind: b'(', commas: 0, at: i }),
            b')' => {
                if stack.last().is_some_and(|f| f.kind == b'(') {
                    stack.pop();
                }
            }
            b'[' => stack.push(Frame { kind: b'[', commas: 0, at: i }),
            b']' => {
                if stack.last().is_some_and(|f| f.kind == b'[') {
                    stack.pop();
                }
            }
            b'{' => stack.push(Frame { kind: b'{', commas: 0, at: i }),
            b'}' => {
                if stack.last().is_some_and(|f| f.kind == b'{') {
                    stack.pop();
                }
            }
            b',' => {
                if let Some(f) = stack.last_mut() {
                    if f.kind == b'(' {
                        f.commas += 1;
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    let frame = stack.iter().rev().find(|f| f.kind == b'(')?;
    let (name, name_at) = ident_ending(src, skip_ws_before(src, frame.at))?;
    let receiver = receiver_before(src, name_at);
    Some(CallSite {
        name,
        receiver,
        commas: frame.commas,
    })
}

pub(super) struct Frame {
    pub(super) kind: u8,
    pub(super) commas: u32,
    pub(super) at: usize,
}

pub(super) fn receiver_before(src: &str, name_at: usize) -> Option<String> {
    let mut i = skip_ws_before(src, name_at);
    let (dot_at, ch) = prev_char(src, i)?;
    if ch != '.' {
        return None;
    }
    i = skip_ws_before(src, dot_at);
    ident_ending(src, i).map(|(n, _)| n)
}

pub(super) fn ident_ending(src: &str, end: usize) -> Option<(String, usize)> {
    let mut start = end;
    while let Some((j, c)) = prev_char(src, start) {
        if is_ident_continue(c) {
            start = j;
        } else {
            break;
        }
    }
    if start == end {
        return None;
    }
    let name = src.get(start..end)?;
    let first = name.chars().next()?;
    if !is_ident_start(first) {
        return None;
    }
    Some((name.to_string(), start))
}

pub(super) fn skip_ws_before(src: &str, mut i: usize) -> usize {
    while let Some((j, c)) = prev_char(src, i) {
        if c.is_whitespace() {
            i = j;
        } else {
            break;
        }
    }
    i
}

pub(super) fn prefix_of(src: &str, offset: usize) -> (String, usize, Option<String>) {
    let end = offset.min(src.len());
    let mut at = end;
    while let Some((j, c)) = prev_char(src, at) {
        if is_ident_continue(c) {
            at = j;
        } else {
            break;
        }
    }
    if at < end {
        if let Some(c) = src[at..].chars().next() {
            if !is_ident_start(c) {
                at = end;
            }
        }
    }
    let prefix = src.get(at..end).unwrap_or("").to_string();
    let mut j = at;
    loop {
        let Some((k, c)) = prev_char(src, j) else {
            return (prefix, at, None);
        };
        if c.is_whitespace() {
            j = k;
            continue;
        }
        if c != '.' {
            return (prefix, at, None);
        }
        j = skip_ws_before(src, k);
        return match ident_ending(src, j) {
            Some((base, _)) => (prefix, at, Some(base)),
            None => (prefix, at, None),
        };
    }
}

pub(super) fn attr_prefix(src: &str, offset: usize) -> Option<String> {
    let end = offset.min(src.len());
    let mut at = end;
    while let Some((j, c)) = prev_char(src, at) {
        if is_ident_continue(c) {
            at = j;
        } else {
            break;
        }
    }
    let mut j = at;
    loop {
        let Some((k, c)) = prev_char(src, j) else {
            return None;
        };
        if c.is_whitespace() {
            j = k;
            continue;
        }
        if c != '[' {
            return None;
        }
        let Some((_, h)) = prev_char(src, k) else {
            return None;
        };
        if h != '#' {
            return None;
        }
        return Some(src.get(at..end).unwrap_or("").to_string());
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Code,
    String,
    Tick,
    Comment,
}

pub(super) fn lexical_mode(src: &str, offset: usize) -> Mode {
    let b = src.as_bytes();
    let end = offset.min(b.len());
    let mut i = 0;
    let mut mode = Mode::Code;
    while i < end {
        if mode == Mode::Comment {
            if b[i] == b'\n' {
                mode = Mode::Code;
            }
            i += 1;
            continue;
        }
        if mode == Mode::String || mode == Mode::Tick {
            if b[i] == b'\\' {
                i += 2;
                continue;
            }
            let quote = if mode == Mode::String { b'"' } else { b'`' };
            if b[i] == quote {
                mode = Mode::Code;
            }
            i += 1;
            continue;
        }
        if b[i] == b'/' && i + 1 < end && b[i + 1] == b'/' {
            mode = Mode::Comment;
            i += 2;
            continue;
        }
        match b[i] {
            b'"' => mode = Mode::String,
            b'`' => mode = Mode::Tick,
            _ => {}
        }
        i += 1;
    }
    mode
}

pub(super) fn strip_comments(s: &str) -> String {
    let mut out = String::new();
    for line in s.split_inclusive('\n') {
        if let Some(i) = comment_at(line) {
            out.push_str(&line[..i]);
            if line.ends_with('\n') {
                out.push('\n');
            }
        } else {
            out.push_str(line);
        }
    }
    out
}

pub(super) fn comment_at(line: &str) -> Option<usize> {
    let b = line.as_bytes();
    let mut i = 0;
    let mut string = false;
    let mut tick = false;
    while i < b.len() {
        if string {
            if b[i] == b'\\' {
                i += 2;
                continue;
            }
            if b[i] == b'"' {
                string = false;
            }
            i += 1;
            continue;
        }
        if tick {
            if b[i] == b'`' {
                tick = false;
            }
            i += 1;
            continue;
        }
        if b[i] == b'"' {
            string = true;
        } else if b[i] == b'`' {
            tick = true;
        } else if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            return Some(i);
        }
        i += 1;
    }
    None
}

pub(super) fn prev_char(src: &str, offset: usize) -> Option<(usize, char)> {
    let mut i = offset;
    while i > 0 {
        i -= 1;
        if src.is_char_boundary(i) {
            return src[i..].chars().next().map(|c| (i, c));
        }
    }
    None
}

pub(super) fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

pub(super) fn is_ident_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

pub(super) fn is_ident_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if is_ident_start(c) => chars.all(is_ident_continue),
        _ => false,
    }
}

pub(super) const KEYWORDS: &[(&str, &str)] = &[
    ("module", "module ${1:name};"),
    ("use", "use ${1:j3.io}.${2:name};"),
    ("pub", "pub "),
    ("fn", "fn ${1:name}($2) {\n    $0\n}"),
    ("struct", "struct ${1:Name} {\n    $0\n}"),
    ("trait", "trait ${1:Name} {\n    fn ${2:name}(this: Self): ${3:int};\n}"),
    ("extend", "extend ${1:Type} {\n    fn ${2:name}(this: &${1:Type}) {\n        $0\n    }\n}"),
    ("enum", "enum ${1:Name} {\n    ${2:A},\n}"),
    ("const", "const ${1:NAME}: ${2:int} = ${3:0};"),
    ("let", "let ${1:name} = $0;"),
    ("if", "if ${1:cond} {\n    $0\n}"),
    ("else", "else {\n    $0\n}"),
    ("while", "while ${1:cond} {\n    $0\n}"),
    ("for", "for ${1:i} in ${2:0}..${3:n} {\n    $0\n}"),
    ("in", "in "),
    ("match", "match ${1:value} {\n    ${2:_} => { $0 }\n}"),
    ("return", "return $0;"),
    ("break", "break;"),
    ("continue", "continue;"),
    ("defer", "defer $0;"),
];
