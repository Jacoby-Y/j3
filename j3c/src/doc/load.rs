use super::*;

/// The documented stdlib files, `j3/game` before `j3/game/anim`.
pub(super) fn stdlib_files() -> Result<Vec<PathBuf>, String> {
    let mut files = read_dir_j3(&stdlib_dir().join("j3"))?;
    files.sort_by_key(|p| p.with_extension(""));
    files.retain(|p| {
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        !HIDDEN.contains(&name) && !is_part(p)
    });
    Ok(files)
}

/// True for `ui/dom/tree.j3` when it says `module j3.ui.dom;`: a part of `ui/dom.j3`, documented there.
pub(super) fn is_part(path: &Path) -> bool {
    let Some(main) = path.parent().map(|d| d.with_extension("j3")) else { return false };
    if main == *path || !(crate::stdlib::embedded(&main).is_some() || main.is_file()) {
        return false;
    }
    let (Ok(a), Ok(b)) = (read_source(path), read_source(&main)) else { return false };
    module_line(&a).is_some() && module_line(&a) == module_line(&b)
}

/// A parsed module and where its source came from.
pub(super) struct Module {
    pub(super) key: String,
    pub(super) file: File,
    pub(super) src: String,
    /// Where `at` lines point; empty for generated source.
    pub(super) origin: String,
    /// Part files appended to `src`: (offset where each starts, its origin).
    pub(super) parts: Vec<(usize, String)>,
    pub(super) embedded: bool,
    /// `j3.scene3d.scripts`: the scene attachments it was made from.
    pub(super) scan: Option<scene_scripts::Scan>,
}

pub(super) fn load_stdlib(path: &Path) -> Result<Module, String> {
    let embedded = crate::stdlib::embedded(path).is_some();
    let origin_of = |p: &Path| p.strip_prefix(stdlib_dir()).unwrap_or(p).display().to_string();
    let (src, parts) = with_parts(path, read_source(path)?, &origin_of)?;
    let file = parse_file(path, &src)?;
    Ok(Module { key: file.module_key(), file, src, origin: origin_of(path), parts, embedded, scan: None })
}

/// The `module` line of a source file, if it has one.
pub(super) fn module_line(src: &str) -> Option<&str> {
    src.lines().find(|l| l.starts_with("module "))
}

/// `src` with the files of the folder beside `path` that name the same module appended (the module is
/// spread over `X.j3` + `X/*.j3`). Each part's `module` line is blanked so the text parses as one
/// file, and its start offset and origin are returned for `at` lines.
pub(super) fn with_parts(path: &Path, mut src: String, origin_of: &dyn Fn(&Path) -> String) -> Result<(String, Vec<(usize, String)>), String> {
    let main = module_line(&src).map(str::to_string);
    let mut parts = Vec::new();
    for part in crate::stdlib::parts(&path.with_extension("")) {
        let text = read_source(&part)?;
        if module_line(&text).map(str::to_string) != main {
            continue;
        }
        src.push('\n');
        parts.push((src.len(), origin_of(&part)));
        for line in text.split_inclusive('\n') {
            if line.starts_with("module ") {
                src.push_str(&" ".repeat(line.trim_end_matches('\n').len()));
                src.push_str(&line[line.trim_end_matches('\n').len()..]);
            } else {
                src.push_str(line);
            }
        }
    }
    Ok((src, parts))
}

pub(super) fn here() -> Option<Manifest> {
    match project::resolve_cwd() {
        Ok(Input::Project(m)) => Some(m),
        _ => None,
    }
}

/// `j3.scene3d.scripts`, made the way the compiler makes it: from this project's scenes.
pub(super) fn scripts_module() -> Result<Module, String> {
    let m = here().ok_or_else(|| {
        format!(
            "`{}` is made from a project's scenes, so run this in a project directory (one with j3.toml)\n  = help: `j3c doc lang` (Scripts) explains it",
            scene_scripts::MODULE
        )
    })?;
    scripts_at(&m.root)
}

pub(super) fn scripts_at(root: &Path) -> Result<Module, String> {
    let scan = scene_scripts::scan(root);
    let src = scene_scripts::registry(root, &scan)?;
    let file = parse::parse(&src, FileId(1))
        .map_err(|e| parse::format_error(scene_scripts::LABEL, &src, &e))?;
    Ok(Module {
        key: scene_scripts::MODULE.to_string(),
        file,
        src,
        origin: String::new(),
        parts: Vec::new(),
        embedded: false,
        scan: Some(scan),
    })
}

/// A module by its dotted name: the generated scripts module, the stdlib (`j3.` optional),
/// then the project in this directory.
pub(super) fn open_module(parts: &[&str]) -> Result<Option<Module>, String> {
    let key = parts.join(".");
    if key == scene_scripts::MODULE || key == "scene3d.scripts" {
        return scripts_module().map(Some);
    }
    let std_parts = if parts[0] == "j3" { &parts[1..] } else { parts };
    if !std_parts.is_empty() {
        let path = stdlib_dir().join("j3").join(format!("{}.j3", std_parts.join("/")));
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !HIDDEN.contains(&name) && (crate::stdlib::embedded(&path).is_some() || path.is_file()) {
            return load_stdlib(&path).map(Some);
        }
    }
    if parts[0] == "j3" {
        return Ok(None);
    }
    let Some(m) = here() else { return Ok(None) };
    let owned: Vec<String> = parts.iter().map(|s| s.to_string()).collect();
    let mut tries = vec![crate::compile::join_mod(&m.src_dir(), &owned)];
    if owned.len() > 1 && owned[0] == m.name {
        tries.push(crate::compile::join_mod(&m.src_dir(), &owned[1..]));
    }
    for path in tries {
        if path.is_file() {
            let src = std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
            let origin_of = |p: &Path| p.strip_prefix(&m.root).unwrap_or(p).display().to_string();
            let (src, parts) = with_parts(&path, src, &origin_of)?;
            let file = parse_file(&path, &src)?;
            return Ok(Some(Module { key: file.module_key(), file, src, origin: origin_of(&path), parts, embedded: false, scan: None }));
        }
    }
    Ok(None)
}

/// The longest module prefix of `query`, and the rest as an item.
pub(super) fn resolve(query: &str) -> Result<(Module, Option<String>), String> {
    let parts: Vec<&str> = query.trim().split('.').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return Err("missing module\n  = help: `j3c doc`".to_string());
    }
    for end in (1..=parts.len()).rev() {
        if let Some(m) = open_module(&parts[..end])? {
            let item = (end < parts.len()).then(|| parts[end..].join("."));
            return Ok((m, item));
        }
    }
    Err(format!("no module `{}`\n  = help: `j3c doc` lists them; `j3c doc search <word>` finds a name", query.trim()))
}

pub(super) fn read_source(path: &Path) -> Result<String, String> {
    if let Some(src) = crate::stdlib::embedded(path) {
        return Ok(src.to_string());
    }
    std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))
}

pub(super) fn read_dir_j3(dir: &Path) -> Result<Vec<PathBuf>, String> {
    if !crate::stdlib::using_override() {
        let mut out: Vec<PathBuf> = crate::stdlib::files().map(|(name, _)| dir.join(name)).collect();
        out.sort();
        return Ok(out);
    }
    let mut out = Vec::new();
    collect_j3(dir, &mut out)?;
    Ok(out)
}

pub(super) fn collect_j3(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let ents = std::fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
    for ent in ents.flatten() {
        let path = ent.path();
        if path.is_dir() {
            collect_j3(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("j3") {
            out.push(path);
        }
    }
    Ok(())
}

pub(super) fn parse_file(path: &Path, src: &str) -> Result<File, String> {
    parse::parse(src, FileId(1)).map_err(|e| parse::format_error(&path.display().to_string(), src, &e))
}
