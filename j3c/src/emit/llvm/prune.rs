//! Dropping declarations no emitted line mentions.

use super::*;

/// `@name` / `%name` tokens of an LLVM text line (`%` also matches locals, which only adds misses).
fn ll_names<'a>(line: &'a str, sigil: char, out: &mut HashSet<&'a str>) {
    let mut rest = line;
    while let Some(at) = rest.find(sigil) {
        rest = &rest[at + 1..];
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '$'))
            .unwrap_or(rest.len());
        if end > 0 {
            out.insert(&rest[..end]);
        }
        rest = &rest[end..];
    }
}

/// The name a declaration line introduces: `declare ... @f(`, `@g = external ...`, `%T = type ...`.
fn declared_name(line: &str) -> Option<(char, &str)> {
    let name = |s: &str| s.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '$')).unwrap_or(s.len());
    if line.starts_with("declare ") {
        let rest = &line[line.find('@')? + 1..];
        return Some(('@', &rest[..name(rest)]));
    }
    if let Some(rest) = line.strip_prefix('@') {
        let n = name(rest);
        if rest[n..].starts_with(" = external ") {
            return Some(('@', &rest[..n]));
        }
    }
    if let Some(rest) = line.strip_prefix('%') {
        let n = name(rest);
        if rest[n..].starts_with(" = type ") {
            return Some(('%', &rest[..n]));
        }
    }
    None
}

/// Drop the declarations and types a per-module text never uses. Every module declares the whole
/// program (types, `use lib` bindings, foreign globals and functions), which clang would parse for
/// every object. A `; use lib` comment goes with the declaration under it (`incr::lib_notes`).
pub(super) fn prune_unused(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let decl: Vec<Option<(char, &str)>> = lines.iter().map(|l| declared_name(l)).collect();
    let mut used = HashSet::new();
    let mut types = HashSet::new();
    for (line, d) in lines.iter().zip(&decl) {
        if d.is_none() && !line.starts_with("; use lib ") {
            ll_names(line, '@', &mut used);
            ll_names(line, '%', &mut types);
        }
    }
    let keep_decl = |d: &Option<(char, &str)>, types: &HashSet<&str>| match d {
        Some(('@', n)) => used.contains(*n),
        Some((_, n)) => types.contains(*n),
        None => true,
    };
    // Kept `@` declarations name types too; then close over the type lines themselves.
    for (line, d) in lines.iter().zip(&decl) {
        if matches!(d, Some(('@', _))) && keep_decl(d, &types) {
            ll_names(line, '%', &mut types);
        }
    }
    let mut stack: Vec<&str> = types.iter().copied().collect();
    let by_type: std::collections::HashMap<&str, &str> = lines
        .iter()
        .zip(&decl)
        .filter_map(|(l, d)| match d {
            Some(('%', n)) => Some((*n, *l)),
            _ => None,
        })
        .collect();
    while let Some(t) = stack.pop() {
        if let Some(line) = by_type.get(t) {
            let mut inner = HashSet::new();
            ll_names(line.split_once(" = type ").map(|p| p.1).unwrap_or(""), '%', &mut inner);
            for n in inner {
                if types.insert(n) {
                    stack.push(n);
                }
            }
        }
    }
    let mut out = String::with_capacity(text.len());
    for (i, line) in lines.iter().enumerate() {
        if line.starts_with("; use lib ") {
            if decl.get(i + 1).is_some_and(|d| d.is_some() && !keep_decl(d, &types)) {
                continue;
            }
        } else if !keep_decl(&decl[i], &types) {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}
