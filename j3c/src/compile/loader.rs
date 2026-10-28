use super::*;

pub(super) struct Loader {
    pub(super) files: Vec<ParsedFile>,
    pub(super) loaded: HashSet<String>,
    pub(super) next_file: u32,
    pub(super) game: GameBackend,
    pub(super) os: OsKind,
    pub(super) src_dir: Option<PathBuf>,
    pub(super) package: Option<String>,
    pub(super) script_root: PathBuf,
    pub(super) overlays: Vec<(PathBuf, String)>,
    /// Scene files scanned for attached scripts (once, on first need).
    pub(super) scene_scan: Option<scene_scripts::Scan>,
    /// `[ui]`: the `<package>.ui` module is generated from `*.j3ui` when something imports it.
    pub(super) ui_cfg: Option<UiCfg>,
    pub(super) ui_gen: Option<crate::ui::Generated>,
    /// The last generation error, with its position (the LSP reads it).
    pub(super) ui_err: Option<crate::ui::BuildErr>,
}

pub(super) struct UiCfg {
    pub(super) package: String,
    pub(super) root: PathBuf,
    pub(super) dir: String,
    pub(super) theme: Option<String>,
}

pub(super) fn ui_cfg(project: Option<&Manifest>) -> Option<UiCfg> {
    let m = project?;
    let u = m.ui.as_ref()?;
    Some(UiCfg { package: m.name.clone(), root: m.root.clone(), dir: u.dir.clone(), theme: u.theme.clone() })
}

impl Loader {
    pub(super) fn load_path(&mut self, path: &Path) -> Result<(), String> {
        let text = self.read_text(path)?;
        let before = self.files.len();
        self.parse_text(path.display().to_string(), text)?;
        if self.files.len() > before && !path.to_string_lossy().ends_with(".j3s") {
            self.load_parts(path)?;
        }
        Ok(())
    }

    /// Files in the folder beside `X.j3` that say `module <same name>;` are more of the same module
    /// (`ui/dom.j3` + `ui/dom/*.j3`). A file there with another module name is its own module and
    /// loads when something imports it.
    fn load_parts(&mut self, path: &Path) -> Result<(), String> {
        let key = self.files.last().map(|f| f.ast.module_key()).unwrap_or_default();
        for part in crate::stdlib::parts(&path.with_extension("")) {
            let text = self.read_text(&part)?;
            self.parse_inner(part.display().to_string(), text, Some(&key))?;
        }
        Ok(())
    }

    fn read_text(&self, path: &Path) -> Result<String, String> {
        for (p, src) in &self.overlays {
            if same_path(p, path) {
                return Ok(src.clone());
            }
        }
        if let Some(src) = crate::stdlib::embedded(path) {
            return Ok(src.to_string());
        }
        if builtin_hello(path) {
            return Ok(HELLO_SRC.to_string());
        }
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))
    }

    fn parse_text(&mut self, path: String, text: String) -> Result<(), String> {
        self.parse_inner(path, text, None)
    }

    /// `part_of`: the module key of the main file when this is one of its parts; a file that names
    /// another module is skipped, and a part is never dropped as already loaded.
    fn parse_inner(&mut self, path: String, text: String, part_of: Option<&str>) -> Result<(), String> {
        crate::phase::detail(&format!(
            "file {}: {}",
            self.files.len() + 1,
            Path::new(&path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
        ));
        let file_id = FileId(self.next_file);
        self.next_file += 1;
        let ast = match if path.ends_with(".j3s") {
            let rel = Path::new(&path)
                .strip_prefix(&self.script_root)
                .map_err(|_| {
                    format!(
                        "script {} is outside the project root {}",
                        path,
                        self.script_root.display()
                    )
                })?;
            let parts = script_module_parts(&rel.to_string_lossy())?;
            parse::parse_script(&text, file_id, &parts)
        } else {
            parse::parse(&text, file_id)
        } {
            Ok(f) => f,
            Err(e) => {
                if let Some(g) = self.ui_gen.as_ref().filter(|g| g.label == path) {
                    return Err(g.render_parse_error(e.span, &e.message));
                }
                return Err(parse::format_error(&path, &text, &e));
            }
        };
        let key = ast.module_key();
        match part_of {
            Some(main) if key != main => return Ok(()),
            Some(_) => {}
            None => {
                if !self.loaded.insert(key) {
                    return Ok(());
                }
            }
        }
        self.files.push(ParsedFile {
            path,
            src: text,
            ast,
            file_id,
        });
        Ok(())
    }

    pub(super) fn load_module(&mut self, parts: &[String]) -> Result<(), String> {
        let key = parts.join(".");
        if self.loaded.contains(&key) {
            return Ok(());
        }
        if key == scene_scripts::MODULE {
            return self.load_registry();
        }
        if key == crate::i18n::MODULE {
            // `[i18n]` of the project's j3.toml baked in as constants, so a built binary needs no j3.toml
            let text = crate::i18n::config_module(&self.script_root);
            return self.parse_text(crate::i18n::LABEL.to_string(), text);
        }
        if self.ui_cfg.as_ref().is_some_and(|u| format!("{}.ui", u.package) == key) {
            return self.load_ui();
        }
        let path = self.module_path(parts);
        if !source_exists(&path) {
            return Err(format!(
                "cannot find module `{}` (looked for {})",
                key,
                path.display()
            ));
        }
        self.load_path(&path)
    }

    fn module_path(&self, parts: &[String]) -> PathBuf {
        if parts.len() == 2 && parts[0] == "j3" && parts[1] == "game" {
            let name = match self.game {
                GameBackend::Sdl => "game.j3",
                GameBackend::Headless => "game_headless.j3",
            };
            return stdlib_dir().join("j3").join(name);
        }
        if parts.len() == 2 && parts[0] == "j3" && parts[1] == "build" {
            // `--release` swaps in the file with `RELEASE = true`, `DEV = false`
            let release = crate::cli::RELEASE.load(std::sync::atomic::Ordering::Relaxed);
            return stdlib_dir().join("j3").join(if release { "build_release.j3" } else { "build.j3" });
        }
        if parts.len() == 2 && parts[0] == "j3" && parts[1] == "sys" {
            let name = match self.os {
                OsKind::Linux => "sys.j3",
                OsKind::Windows => "sys_windows.j3",
                OsKind::Macos => "sys_macos.j3",
                OsKind::Android => "sys_android.j3",
            };
            return stdlib_dir().join("j3").join(name);
        }
        if parts.len() == 2 && parts[0] == "j3" && parts[1] == "threadsys" {
            let name = match self.os {
                OsKind::Windows => "threadsys_windows.j3",
                OsKind::Android => "threadsys_android.j3",
                OsKind::Linux | OsKind::Macos => "threadsys.j3",
            };
            return stdlib_dir().join("j3").join(name);
        }
        if parts.len() == 2 && parts[0] == "j3" && parts[1] == "netsys" {
            let name = match self.os {
                OsKind::Linux => "netsys.j3",
                OsKind::Windows => "netsys_windows.j3",
                OsKind::Macos => "netsys_macos.j3",
                OsKind::Android => "netsys_android.j3",
            };
            return stdlib_dir().join("j3").join(name);
        }
        if let Some(path) = self.project_module(parts) {
            return path;
        }
        module_file(parts)
    }

    fn project_module(&self, parts: &[String]) -> Option<PathBuf> {
        let src = self.src_dir.as_ref()?;
        if parts.first().map(|s| s.as_str()) == Some("j3") {
            return None;
        }
        let direct = join_mod(src, parts);
        if direct.is_file() {
            return Some(direct);
        }
        if let Some(pkg) = &self.package {
            if parts.first() == Some(pkg) && parts.len() > 1 {
                let stripped = join_mod(src, &parts[1..]);
                if stripped.is_file() {
                    return Some(stripped);
                }
            }
        }
        None
    }

    pub(super) fn load_imports(&mut self) -> Result<(), String> {
        let mut i = 0;
        while i < self.files.len() {
            let uses: Vec<(Vec<String>, Option<Vec<String>>)> = self.files[i]
                .ast
                .items
                .iter()
                .filter_map(|it| match it {
                    parse::Item::Use(u) => Some((
                        u.path.iter().map(|p| p.name.clone()).collect(),
                        u.names
                            .as_ref()
                            .map(|ns| ns.iter().map(|n| n.name.clone()).collect()),
                    )),
                    parse::Item::UseLib(_) => None,
                    _ => None,
                })
                .collect();
            for (path, names) in uses {
                if path.join(".") == scene_scripts::MODULE {
                    self.load_registry()?;
                } else if path.join(".") == crate::i18n::MODULE {
                    self.load_module(&path)?;
                } else if names.is_some() {
                    self.load_module(&path)?;
                } else if source_exists(&self.module_path(&path)) {
                    self.load_module(&path)?;
                } else if path.len() >= 2 {
                    self.load_module(&path[..path.len() - 1])?;
                } else {
                    return Err(format!("cannot resolve `use {}`", path.join(".")));
                }
            }
            let scripts = script_deps::find(&self.files[i].ast);
            for script in scripts {
                self.load_script(&script.path, i, script.span)?;
            }
            i += 1;
        }
        Ok(())
    }

    /// `j3.scene3d.scripts`, made from the project's scenes.
    fn load_ui(&mut self) -> Result<(), String> {
        let cfg = self.ui_cfg.as_ref().unwrap();
        let dir = cfg.root.join(&cfg.dir);
        let mut files: Vec<crate::ui::SrcFile> = Vec::new();
        let extra: Vec<PathBuf> = self.overlays.iter().map(|(p, _)| p.clone()).collect();
        for f in crate::ui::scan::scan(&dir, &cfg.dir, &extra)? {
            let text = self.read_text(&f.path)?;
            files.push(crate::ui::SrcFile { path: f.shown, name: f.name, text });
        }
        let theme = match &cfg.theme {
            Some(t) => Some(crate::ui::Theme { path: t.clone(), text: self.read_text(&cfg.root.join(t))? }),
            None => None,
        };
        if files.is_empty() {
            return Err(format!("cannot find module `{}.ui`: no .j3ui files in {}", cfg.package, dir.display()));
        }
        let package = cfg.package.clone();
        let g = crate::ui::try_build(files, &package, theme).map_err(|e| {
            let r = e.rendered.clone();
            self.ui_err = Some(e);
            r
        })?;
        let (label, text) = (g.label.clone(), g.text.clone());
        self.ui_gen = Some(g);
        self.parse_text(label, text)
    }

    fn load_registry(&mut self) -> Result<(), String> {
        if self.loaded.contains(scene_scripts::MODULE) {
            return Ok(());
        }
        let scan = self
            .scene_scan
            .get_or_insert_with(|| scene_scripts::scan(&self.script_root));
        let text = scene_scripts::registry(&self.script_root, scan)?;
        self.parse_text(scene_scripts::LABEL.to_string(), text)
    }

    /// When a scene attaches scripts, they and the registry join the program.
    pub(super) fn load_scene_scripts(&mut self) -> Result<(), String> {
        let root = self.script_root.clone();
        let scan = self.scene_scan.get_or_insert_with(|| scene_scripts::scan(&root));
        if scan.attachments.is_empty() {
            return Ok(());
        }
        self.load_registry()?;
        self.load_imports()
    }

    fn load_script(
        &mut self,
        rel: &str,
        source: usize,
        span: crate::span::Span,
    ) -> Result<(), String> {
        let parts = script_module_parts(rel)?;
        let key = parts.join(".");
        if self.loaded.contains(&key) {
            return Ok(());
        }
        let path = self.script_root.join(rel);
        if !path.is_file() && !self.overlays.iter().any(|(p, _)| same_path(p, &path)) {
            let source_file = &self.files[source];
            let mut error = crate::check::CheckError::new(
                &source_file.path,
                span,
                format!("no script at {rel}"),
            );
            if let Some(suggested) =
                crate::diag::did_you_mean(rel, &script_paths(&self.script_root))
            {
                error = error.help(format!("did you mean `{suggested}`?"));
            }
            return Err(error.render(&[(source_file.path.clone(), source_file.src.clone())]));
        }
        self.load_path(&path)
    }
}

/// Every `.j3s` under `root` (skipping `target/` and `.git/`), relative, `/`-separated.
pub fn script_paths(root: &Path) -> Vec<String> {
    fn visit(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && !path.is_symlink() {
                let name = entry.file_name();
                if name != ".git" && name != "target" {
                    visit(root, &path, out);
                }
            } else if path.extension().and_then(|s| s.to_str()) == Some("j3s") {
                if let Ok(rel) = path.strip_prefix(root) {
                    out.push(
                        rel.to_string_lossy()
                            .replace(std::path::MAIN_SEPARATOR, "/"),
                    );
                }
            }
        }
    }
    let mut out = Vec::new();
    visit(root, root, &mut out);
    out.sort();
    out
}

pub(super) fn script_module_parts(rel: &str) -> Result<Vec<String>, String> {
    let path = Path::new(rel);
    if path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(format!(
            "script path must be relative to the project root: {rel}"
        ));
    }
    let Some(stem) = rel.strip_suffix(".j3s") else {
        return Err(format!("script path must end in `.j3s`: {rel}"));
    };
    let mut parts = vec!["script".to_string()];
    for part in stem.split('/') {
        if part.is_empty()
            || !part
                .bytes()
                .enumerate()
                .all(|(i, b)| b.is_ascii_alphabetic() || b == b'_' || (i > 0 && b.is_ascii_digit()))
        {
            return Err(format!(
                "script path has an invalid module component: {rel}"
            ));
        }
        parts.push(part.to_string());
    }
    Ok(parts)
}

pub(super) fn check_import_cycles(files: &[ParsedFile]) -> Result<(), String> {
    let keys: Vec<String> = files.iter().map(|f| f.ast.module_key()).collect();
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();
    for f in files {
        let from = f.ast.module_key();
        for it in &f.ast.items {
            if let parse::Item::Use(u) = it {
                if let Some(to) = use_mod_key(u, &keys) {
                    if to != from {
                        graph.entry(from.clone()).or_default().push(to);
                    }
                }
            }
        }
        for script in script_deps::find(&f.ast) {
            let to = script_module_parts(&script.path)?.join(".");
            if to != from {
                graph.entry(from.clone()).or_default().push(to);
            }
        }
    }
    let mut done = HashSet::new();
    for start in &keys {
        if let Some(cycle) = cycle_from(&graph, start, &mut done) {
            return Err(format!("import cycle: {}", cycle.join(" -> ")));
        }
    }
    Ok(())
}

pub(super) fn use_mod_key(u: &parse::UseItem, loaded: &[String]) -> Option<String> {
    let parts: Vec<String> = u.path.iter().map(|p| p.name.clone()).collect();
    if u.names.is_some() {
        let k = parts.join(".");
        if loaded.iter().any(|x| x == &k) {
            return Some(k);
        }
        return None;
    }
    let as_mod = parts.join(".");
    if loaded.iter().any(|k| k == &as_mod) {
        return Some(as_mod);
    }
    if parts.len() >= 2 {
        let k = parts[..parts.len() - 1].join(".");
        if loaded.iter().any(|x| x == &k) {
            return Some(k);
        }
    }
    None
}

pub(super) fn cycle_from(
    graph: &HashMap<String, Vec<String>>,
    start: &str,
    done: &mut HashSet<String>,
) -> Option<Vec<String>> {
    fn dfs(
        graph: &HashMap<String, Vec<String>>,
        node: &str,
        stack: &mut Vec<String>,
        on_stack: &mut HashSet<String>,
        done: &mut HashSet<String>,
    ) -> Option<Vec<String>> {
        if on_stack.contains(node) {
            let i = stack.iter().position(|s| s == node)?;
            let mut c = stack[i..].to_vec();
            c.push(node.to_string());
            return Some(c);
        }
        if done.contains(node) {
            return None;
        }
        stack.push(node.to_string());
        on_stack.insert(node.to_string());
        if let Some(nexts) = graph.get(node) {
            for n in nexts {
                if let Some(c) = dfs(graph, n, stack, on_stack, done) {
                    return Some(c);
                }
            }
        }
        on_stack.remove(node);
        stack.pop();
        done.insert(node.to_string());
        None
    }
    dfs(graph, start, &mut Vec::new(), &mut HashSet::new(), done)
}

pub(super) fn source_exists(path: &Path) -> bool {
    crate::stdlib::embedded(path).is_some() || builtin_hello(path) || path.is_file()
}

pub(super) fn module_file(parts: &[String]) -> PathBuf {
    join_mod(&stdlib_dir(), parts)
}

pub(super) fn same_path(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a.as_os_str() == b.as_os_str(),
    }
}

pub(crate) fn join_mod(root: &Path, parts: &[String]) -> PathBuf {
    let mut p = root.to_path_buf();
    for (i, part) in parts.iter().enumerate() {
        if i + 1 == parts.len() {
            p.push(format!("{part}.j3"));
        } else {
            p.push(part);
        }
    }
    p
}
