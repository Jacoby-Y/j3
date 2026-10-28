use super::*;

pub(super) fn script_path_prefix(src: &str, offset: usize) -> Option<String> {
    let head = src.get(..offset.min(src.len()))?;
    let at = head.rfind("@script")?;
    if lexical_mode(src, at) != Mode::Code {
        return None;
    }
    let mut rest = head.get(at + "@script".len()..)?.trim_start();
    rest = rest.strip_prefix('(')?.trim_start();
    rest = rest.strip_prefix('"')?;
    if rest.contains('"') || rest.contains('\n') {
        return None;
    }
    Some(rest.to_string())
}

pub(super) fn script_path_items(path: &std::path::Path, prefix: &str) -> Vec<Completion> {
    let root = match crate::project::resolve(path) {
        Ok(crate::project::Input::Project(m)) => m.root,
        _ => path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf(),
    };
    let mut out = Vec::new();
    for candidate in crate::compile::script_paths(&root) {
        if !candidate.starts_with(prefix) {
            continue;
        }
        let mut entry = item(&candidate, 17, "script path", "", "0", false);
        entry.insert = if prefix.contains('/') {
            candidate
                .rsplit('/')
                .next()
                .unwrap_or(&candidate)
                .to_string()
        } else {
            candidate.clone()
        };
        out.push(entry);
    }
    finish(prefix, out)
}

pub(super) fn script_prop_items(
    prog: &Program,
    path: &str,
    src: &str,
    prefix_at: usize,
    prefix: &str,
) -> Option<Vec<Completion>> {
    let brace = src.get(..prefix_at)?.rfind('{')?;
    if src[brace + 1..prefix_at].contains('}') {
        return None;
    }
    let head = src[..brace].trim_end();
    let file = query::file_by_path(&prog.cx, path)?;
    let sid = if head.ends_with(')') {
        let at = head.rfind("@script")?;
        let segment = &head[at..];
        if segment.find(')')? != segment.len() - 1 {
            return None;
        }
        let quoted = segment.split('"').nth(1)?;
        prog.cx.structs().iter().enumerate().find_map(|(i, s)| {
            s.script
                .as_ref()
                .filter(|m| m.path.ends_with(quoted))
                .map(|_| StructId(i as u32))
        })?
    } else {
        let bytes = head.as_bytes();
        let mut start = bytes.len();
        while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
            start -= 1;
        }
        let name = head.get(start..)?;
        prog.script_aliases
            .iter()
            .find(|a| a.file == file.id && a.name == name)
            .map(|a| a.struct_id)
            .or_else(|| {
                prog.cx.structs().iter().enumerate().find_map(|(i, s)| {
                    if s.script.is_some() && prog.cx.name_str(s.name) == name {
                        Some(StructId(i as u32))
                    } else {
                        None
                    }
                })
            })?
    };
    let s = prog.cx.struct_def(sid);
    let meta = s.script.as_ref()?;
    let typed = &src[brace + 1..prefix_at];
    let mut out = Vec::new();
    for field in s.fields.iter().take(meta.prop_spans.len()) {
        let name = prog.cx.name_str(field.name);
        if !name.starts_with(prefix) || field_written(typed, name) {
            continue;
        }
        let ty = dump::ty_string(&prog.cx, field.ty);
        let (detail, sort) = match &field.default {
            None => (format!("required prop: {ty}"), "0"),
            Some(default) => (format!("prop: {ty} = {}", default_text(default)), "1"),
        };
        out.push(item(name, 5, &detail, &field.doc, sort, false));
    }
    Some(finish(prefix, out))
}

pub(super) fn field_written(body: &str, name: &str) -> bool {
    body.match_indices(name).any(|(at, _)| {
        let before = body.as_bytes().get(at.wrapping_sub(1)).copied();
        let boundary = at == 0 || !before.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_');
        boundary && body[at + name.len()..].trim_start().starts_with(':')
    })
}

pub(super) fn default_text(op: &Operand) -> String {
    match op {
        Operand::Const(c) => match &c.kind {
            ConstKind::Bool(v) => v.to_string(),
            ConstKind::Int(v) => (*v as i64).to_string(),
            ConstKind::F32Bits(v) => f32::from_bits(*v).to_string(),
            ConstKind::F64Bits(v) => f64::from_bits(*v).to_string(),
            ConstKind::Bytes(v) => format!("\"{}\"", String::from_utf8_lossy(v)),
            ConstKind::Null => "0".to_string(),
            _ => "...".to_string(),
        },
        Operand::Copy(_) => "...".to_string(),
    }
}

pub(super) fn blank_line(src: &str, offset: u32) -> String {
    let offset = (offset as usize).min(src.len());
    let start = src[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let end = src[offset..]
        .find('\n')
        .map(|i| offset + i)
        .unwrap_or(src.len());
    let mut bytes = src.as_bytes().to_vec();
    for b in &mut bytes[start..end] {
        if *b != b'\n' && *b != b'\r' {
            *b = b' ';
        }
    }
    String::from_utf8(bytes).unwrap_or_else(|_| src.to_string())
}
