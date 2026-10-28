use super::*;

pub fn import_edits(prog: &Program, src: &str, messages: &[String]) -> Vec<ImportEdit> {
    let mut out = Vec::new();
    let uses = uses_of(prog, src);
    for msg in messages {
        let Some(name) = unknown_name(msg) else {
            continue;
        };
        if !is_ident_name(name) || BUILTIN_TYPES.contains(&name) {
            continue;
        }
        let mut hits: Vec<&Export> = prog.exports.iter().filter(|e| e.name == name).collect();
        hits.sort_by(|a, b| a.module.cmp(&b.module));
        hits.dedup_by(|a, b| a.module == b.module && a.name == b.name);
        for exp in hits {
            if uses.imports.iter().any(|(m, n)| m == &exp.module && n == &exp.name) {
                continue;
            }
            let title = format!("Import {name} from {}", exp.module);
            if out.iter().any(|e: &ImportEdit| e.title == title) {
                continue;
            }
            let anchor = import_anchor(src);
            let mut new_text = format!("use {}.{name};\n", exp.module);
            if anchor > 0 && !src[..anchor].ends_with('\n') {
                new_text.insert(0, '\n');
            }
            out.push(ImportEdit {
                title,
                new_text,
                offset: anchor as u32,
            });
        }
    }
    out
}

pub(super) fn complete_use(prog: &Program, use_at: &UseAt) -> Vec<Completion> {
    let modules = module_keys(prog);
    let mut out = Vec::new();
    if use_at.brace {
        push_exports(prog, &use_at.path, &use_at.prefix, false, &mut out);
        return finish(&use_at.prefix, out);
    }
    for seg in child_segments(&modules, &use_at.path) {
        if !seg.starts_with(&use_at.prefix) {
            continue;
        }
        out.push(item(&seg, 9, "module", &parent_doc(&use_at.path, &seg), "5", true));
    }
    push_exports(prog, &use_at.path, &use_at.prefix, false, &mut out);
    finish(&use_at.prefix, out)
}

pub(super) fn import_anchor(src: &str) -> usize {
    let mut last_use = None;
    let mut module_end = None;
    let mut i = 0;
    for line in src.split_inclusive('\n') {
        let t = strip_comments(line).trim().to_string();
        if t.starts_with("module ") || t.starts_with("module\t") {
            module_end = Some(i + line.len());
        }
        if t.starts_with("use ") || t == "use" {
            last_use = Some(i + line.len());
        }
        i += line.len();
    }
    last_use.or(module_end).unwrap_or(0)
}

pub(super) struct Uses {
    pub(super) imports: Vec<(String, String)>,
    pub(super) aliases: Vec<(String, String)>,
}

pub(super) fn next_semi(src: &str, from: usize) -> usize {
    let b = src.as_bytes();
    let mut i = from;
    let mut braces = 0i32;
    let mut mode = Mode::Code;
    while i < b.len() {
        let c = b[i];
        if mode == Mode::Comment {
            if c == b'\n' {
                mode = Mode::Code;
            }
            i += 1;
            continue;
        }
        if mode == Mode::String {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == b'"' {
                mode = Mode::Code;
            }
            i += 1;
            continue;
        }
        if mode == Mode::Tick {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == b'`' {
                mode = Mode::Code;
            }
            i += 1;
            continue;
        }
        if c == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            mode = Mode::Comment;
            i += 2;
            continue;
        }
        match c {
            b'"' => mode = Mode::String,
            b'`' => mode = Mode::Tick,
            b'{' => braces += 1,
            b'}' => braces -= 1,
            b';' if braces == 0 => return i,
            _ => {}
        }
        i += 1;
    }
    b.len()
}

pub(super) struct UseAt {
    pub(super) lib: bool,
    pub(super) brace: bool,
    pub(super) path: Vec<String>,
    pub(super) prefix: String,
}

pub(super) struct ParsedUse {
    pub(super) lib: bool,
    pub(super) brace: bool,
    pub(super) segs: Vec<String>,
    pub(super) braced: Vec<String>,
    pub(super) partial: String,
    pub(super) ended_on_dot: bool,
}

pub(super) fn use_at(src: &str, offset: usize) -> Option<UseAt> {
    let start = stmt_start(src, offset);
    let stmt = strip_comments(&src[start..offset.min(src.len())]);
    let parsed = parse_use(stmt.trim())?;
    if parsed.lib {
        return Some(UseAt {
            lib: true,
            brace: false,
            path: Vec::new(),
            prefix: String::new(),
        });
    }
    Some(UseAt {
        lib: false,
        brace: parsed.brace,
        path: parsed.segs,
        prefix: parsed.partial,
    })
}

pub(super) fn finish_use(mut parsed: ParsedUse) -> ParsedUse {
    if parsed.partial.is_empty() {
        return parsed;
    }
    if parsed.brace {
        parsed.braced.push(std::mem::take(&mut parsed.partial));
    } else if !parsed.ended_on_dot {
        parsed.segs.push(std::mem::take(&mut parsed.partial));
    }
    parsed
}

pub(super) fn classify(modules: &[String], parsed: ParsedUse, uses: &mut Uses) {
    if parsed.lib {
        return;
    }
    if parsed.brace {
        let module = parsed.segs.join(".");
        if modules.iter().any(|m| m == &module) {
            for name in parsed.braced {
                uses.imports.push((module.clone(), name));
            }
        }
        return;
    }
    if parsed.segs.is_empty() {
        return;
    }
    let full = parsed.segs.join(".");
    if modules.iter().any(|m| m == &full) {
        if let Some(alias) = parsed.segs.last() {
            uses.aliases.push((alias.clone(), full));
        }
        return;
    }
    if parsed.segs.len() >= 2 {
        let module = parsed.segs[..parsed.segs.len() - 1].join(".");
        if modules.iter().any(|m| m == &module) {
            if let Some(name) = parsed.segs.last() {
                uses.imports.push((module, name.clone()));
            }
        }
    }
}

pub(super) fn parse_use(stmt: &str) -> Option<ParsedUse> {
    let rest = stmt.strip_prefix("use")?;
    if !rest.is_empty() {
        let c = rest.chars().next()?;
        if c.is_ascii_alphanumeric() || c == '_' {
            return None;
        }
    }
    let rest = rest.trim_start();
    if rest == "lib" || rest.starts_with("lib ") || rest.starts_with("lib\t") || rest.starts_with("lib\"") {
        return Some(ParsedUse {
            lib: true,
            brace: false,
            segs: Vec::new(),
            braced: Vec::new(),
            partial: String::new(),
            ended_on_dot: false,
        });
    }
    let b = rest.as_bytes();
    let mut i = 0;
    let mut segs = Vec::new();
    let mut braced = Vec::new();
    let mut cur = String::new();
    let mut brace = false;
    let mut ended_on_dot = false;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_ascii_alphanumeric() || c == '_' {
            cur.push(c);
            ended_on_dot = false;
            i += 1;
            continue;
        }
        if c == '.' {
            if !cur.is_empty() {
                if brace {
                    braced.push(std::mem::take(&mut cur));
                } else {
                    segs.push(std::mem::take(&mut cur));
                }
            }
            ended_on_dot = true;
            i += 1;
            let mut j = i;
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < b.len() && b[j] == b'{' {
                brace = true;
                ended_on_dot = false;
                i = j + 1;
            }
            continue;
        }
        if c == '{' {
            if !cur.is_empty() {
                segs.push(std::mem::take(&mut cur));
            }
            brace = true;
            ended_on_dot = false;
            i += 1;
            continue;
        }
        if c == ',' || c == '}' || c.is_ascii_whitespace() {
            if !cur.is_empty() && brace && (c == ',' || c == '}') {
                braced.push(std::mem::take(&mut cur));
            } else if !cur.is_empty() && !brace && c == ',' {
                segs.push(std::mem::take(&mut cur));
            }
            if c != ' ' && c != '\t' && c != '\n' && c != '\r' {
                ended_on_dot = false;
            }
            i += 1;
            continue;
        }
        i += 1;
    }
    Some(ParsedUse {
        lib: false,
        brace,
        segs,
        braced,
        partial: cur,
        ended_on_dot,
    })
}

pub(super) fn stmt_start(src: &str, offset: usize) -> usize {
    let b = src.as_bytes();
    let mut i = offset.min(b.len());
    let mut braces = 0i32;
    let mut parens = 0i32;
    let mut brackets = 0i32;
    while i > 0 {
        i -= 1;
        if !src.is_char_boundary(i) {
            continue;
        }
        match b[i] {
            b'}' => braces += 1,
            b')' => parens += 1,
            b']' => brackets += 1,
            b'{' if braces == 0 && parens == 0 && brackets == 0 => {
                if !head_is_use(&src[..i]) {
                    return i + 1;
                }
            }
            b'{' if braces > 0 => braces -= 1,
            b'(' if parens > 0 => parens -= 1,
            b'[' if brackets > 0 => brackets -= 1,
            b';' if braces == 0 && parens == 0 && brackets == 0 => return i + 1,
            _ => {}
        }
    }
    0
}

pub(super) fn head_is_use(before: &str) -> bool {
    let tail = before.rsplit(';').next().unwrap_or(before);
    let t = strip_comments(tail);
    let t = t.trim();
    let Some(rest) = t.strip_prefix("use") else {
        return false;
    };
    if rest.is_empty() {
        return true;
    }
    let c = rest.chars().next().unwrap();
    if c.is_ascii_alphanumeric() || c == '_' {
        return false;
    }
    let rest = rest.trim_start();
    !(rest == "lib" || rest.starts_with("lib ") || rest.starts_with("lib\t") || rest.starts_with("lib\""))
}

pub(super) fn uses_of(prog: &Program, src: &str) -> Uses {
    let modules = module_keys(prog);
    let mut uses = Uses {
        imports: Vec::new(),
        aliases: Vec::new(),
    };
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let start = i;
        let end = next_semi(src, i);
        let stmt = strip_comments(&src[start..end.min(b.len())]);
        if let Some(parsed) = parse_use(stmt.trim()) {
            classify(&modules, finish_use(parsed), &mut uses);
        }
        if end >= b.len() {
            break;
        }
        i = end + 1;
    }
    uses
}
