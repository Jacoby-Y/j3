use super::*;

mod cycles;
mod paths;
mod scripts;

pub use scripts::script_paths;
pub(super) use cycles::check_import_cycles;
pub(super) use paths::{same_path, source_exists};
pub(crate) use paths::join_mod;
pub(super) use scripts::script_module_parts;

pub(super) struct Loader {
    pub(super) files: Vec<ParsedFile>,
    pub(super) loaded: HashSet<String>,
    pub(super) next_file: u32,
    pub(super) game: GameBackend,
    pub(super) os: OsKind,
    /// Index of `j3.sys.ARCH_*` for the target.
    pub(super) arch: usize,
    pub(super) src_dir: Option<PathBuf>,
    pub(super) package: Option<String>,
    /// `[package] version`, patched into `j3.build.VERSION`.
    pub(super) version: Option<String>,
    /// `[dependencies]` of the project, flat (`project::deps`): `use <name>.a;` is `<root>/a.j3`.
    pub(super) deps: Vec<crate::project::Dep>,
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

/// One file a wave will parse: a module's main file, or one of its parts (`part_of`).
struct Planned {
    key: String,
    path: String,
    text: String,
    id: FileId,
    part_of: Option<String>,
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
            if other_module(&text, &key) {
                continue;
            }
            self.parse_inner(part.display().to_string(), text, Some(&key))?;
        }
        Ok(())
    }

    /// The `j3.sys` files carry a placeholder `ARCH` and `ARCH_NAME`; the target's own go in (same line count).
    /// `j3.build` gets the package's `NAME` and `VERSION` the same way.
    fn with_arch(&self, path: &Path, src: &str) -> String {
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if matches!(name.as_str(), "build.j3" | "build_release.j3") && src.contains("module j3.build;") {
            let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
            let v = esc(self.version.as_deref().unwrap_or("0.0.0"));
            let n = esc(self.package.as_deref().unwrap_or(""));
            return src
                .replacen("pub const VERSION: string = \"0.0.0\";", &format!("pub const VERSION: string = \"{v}\";"), 1)
                .replacen("pub const NAME: string = \"\";", &format!("pub const NAME: string = \"{n}\";"), 1);
        }
        if !matches!(name.as_str(), "sys.j3" | "sys_windows.j3" | "sys_macos.j3" | "sys_android.j3") {
            return src.to_string();
        }
        const NAMES: [&str; 6] = ["x86_64", "aarch64", "x86", "arm", "riscv", "other"];
        src.replacen("pub const ARCH: int = 0;", &format!("pub const ARCH: int = {};", self.arch), 1)
            .replacen("pub const ARCH_NAME: string = \"x86_64\";", &format!("pub const ARCH_NAME: string = \"{}\";", NAMES[self.arch]), 1)
    }

    fn read_text(&self, path: &Path) -> Result<String, String> {
        for (p, src) in &self.overlays {
            if same_path(p, path) {
                return Ok(src.clone());
            }
        }
        if let Some(src) = crate::stdlib::embedded(path) {
            return Ok(self.with_arch(path, src));
        }
        if builtin_hello(path) {
            return Ok(HELLO_SRC.to_string());
        }
        let text = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        Ok(self.with_arch(path, &text))
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
        let parsed = if path.ends_with(".j3s") {
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
        };
        self.finish_parse(path, text, file_id, parsed, part_of)
    }

    /// Keep a parsed file: errors rendered against the source, a repeated module dropped.
    fn finish_parse(
        &mut self,
        path: String,
        text: String,
        file_id: FileId,
        parsed: Result<parse::File, parse::ParseError>,
        part_of: Option<&str>,
    ) -> Result<(), String> {
        let ast = match parsed {
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
        if key == crate::appconfig::MODULE {
            // `[splash]` and `[dist] icon`, baked in the same way
            let text = crate::appconfig::config_module(&self.script_root);
            return self.parse_text(crate::appconfig::LABEL.to_string(), text);
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
        self.load_path(&path)?;
        self.dep_module_line(&key, &path.display().to_string())
    }

    /// Breadth-first over `self.files`. Each wave plans the plain module loads of every file
    /// not yet scanned (paths, parts and `FileId`s in the order the serial walk would use),
    /// parses them in parallel, then keeps them in that order. A file whose imports need
    /// anything else (scripts, generated modules, an unresolved path) flushes the plan and
    /// goes through `imports_of`, which also produces the errors.
    pub(super) fn load_imports(&mut self) -> Result<(), String> {
        let mut i = 0;
        while i < self.files.len() {
            let end = self.files.len();
            let mut plan: Vec<Planned> = Vec::new();
            let mut planned: HashSet<String> = HashSet::new();
            while i < end {
                match self.plan_imports(i, &planned, self.next_file + plan.len() as u32) {
                    Some(more) => {
                        for p in &more {
                            if p.part_of.is_none() {
                                planned.insert(p.key.clone());
                            }
                        }
                        plan.extend(more);
                        i += 1;
                    }
                    None => break,
                }
            }
            self.commit_plan(plan)?;
            if i < end {
                self.imports_of(i)?;
                i += 1;
            }
        }
        Ok(())
    }

    /// The plain module loads of file `i`, or None when the file needs the serial path.
    fn plan_imports(&self, i: usize, planned: &HashSet<String>, first_id: u32) -> Option<Vec<Planned>> {
        if !script_deps::find(&self.files[i].ast).is_empty() {
            return None;
        }
        let mut out: Vec<Planned> = Vec::new();
        let mut keys: HashSet<String> = HashSet::new();
        for it in &self.files[i].ast.items {
            let parse::Item::Use(u) = it else { continue };
            let path: Vec<String> = u.path.iter().map(|p| p.name.clone()).collect();
            let joined = path.join(".");
            if joined == scene_scripts::MODULE || joined == crate::i18n::MODULE || joined == crate::appconfig::MODULE {
                return None;
            }
            let target: &[String] = if u.names.is_some() || source_exists(&self.module_path(&path)) {
                &path
            } else if path.len() >= 2 {
                &path[..path.len() - 1]
            } else {
                return None;
            };
            let key = target.join(".");
            if self.loaded.contains(&key) || planned.contains(&key) || keys.contains(&key) {
                continue;
            }
            if key == scene_scripts::MODULE
                || key == crate::i18n::MODULE
                || key == crate::appconfig::MODULE
                || self.ui_cfg.as_ref().is_some_and(|c| format!("{}.ui", c.package) == key)
            {
                return None;
            }
            let file = self.module_path(target);
            if !source_exists(&file) || file.to_string_lossy().ends_with(".j3s") {
                return None;
            }
            let text = self.read_text(&file).ok()?;
            out.push(Planned { key: key.clone(), path: file.display().to_string(), text, id: FileId(first_id + out.len() as u32), part_of: None });
            for part in crate::stdlib::parts(&file.with_extension("")) {
                let text = self.read_text(&part).ok()?;
                if other_module(&text, &key) {
                    continue;
                }
                out.push(Planned { key: key.clone(), path: part.display().to_string(), text, id: FileId(first_id + out.len() as u32), part_of: Some(key.clone()) });
            }
            keys.insert(key);
        }
        Some(out)
    }

    /// Parse the plan on all cores, then keep the files in plan order (first error wins).
    fn commit_plan(&mut self, plan: Vec<Planned>) -> Result<(), String> {
        if plan.is_empty() {
            return Ok(());
        }
        let next = std::sync::atomic::AtomicUsize::new(0);
        let results: Vec<std::sync::Mutex<Option<Result<parse::File, parse::ParseError>>>> =
            plan.iter().map(|_| std::sync::Mutex::new(None)).collect();
        let width = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(plan.len());
        std::thread::scope(|scope| {
            for _ in 0..width {
                scope.spawn(|| loop {
                    let n = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(p) = plan.get(n) else { break };
                    *results[n].lock().unwrap() = Some(parse::parse(&p.text, p.id));
                });
            }
        });
        self.next_file += plan.len() as u32;
        // A part follows its main file; it is kept only when the main file was.
        let mut main_kept: Option<String> = None;
        for (p, res) in plan.into_iter().zip(results) {
            let parsed = res.into_inner().unwrap().expect("every planned file is parsed");
            match &p.part_of {
                None => {
                    crate::phase::detail(&format!(
                        "file {}: {}",
                        self.files.len() + 1,
                        Path::new(&p.path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
                    ));
                    let before = self.files.len();
                    let (key, shown) = (p.key.clone(), p.path.clone());
                    self.finish_parse(p.path, p.text, p.id, parsed, None)?;
                    self.dep_module_line(&key, &shown)?;
                    main_kept = (self.files.len() > before).then(|| self.files[before].ast.module_key());
                }
                Some(_) => {
                    if let Some(key) = main_kept.clone() {
                        self.finish_parse(p.path, p.text, p.id, parsed, Some(&key))?;
                    }
                }
            }
        }
        Ok(())
    }

    /// The serial walk for one file: every `use`, then its scripts.
    fn imports_of(&mut self, i: usize) -> Result<(), String> {
        {
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
                } else if path.join(".") == crate::i18n::MODULE || path.join(".") == crate::appconfig::MODULE {
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
        // a project's own icons override Bootstrap names
        let icons = crate::ui::icons::load_dir(&cfg.root.join("icons"))?;
        let package = cfg.package.clone();
        let g = crate::ui::try_build(files, &package, theme, &icons).map_err(|e| {
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
}

/// A file in a parts folder whose `module` line names another module is its own module, so it
/// is not parsed as a part. No such line (or an odd one): the parser decides.
fn other_module(text: &str, key: &str) -> bool {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("module ") {
            return rest.strip_suffix(';').is_some_and(|name| name.trim() != key);
        }
    }
    false
}
