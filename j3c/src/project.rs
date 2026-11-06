//! `j3.toml` + `src/` project layout. No crates.io TOML parser — this is the subset we take.

use std::path::{Path, PathBuf};

mod deps;
mod steam;
mod toml;
pub use deps::Dep;
pub use steam::Steam;
pub(crate) use steam::{parse_glibc, resolve_path};
pub(crate) use toml::{toml_doc, toml_list, toml_str};
use toml::{parse_toml, TomlVal};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub root: PathBuf,
    pub name: String,
    /// `[package] version`, default `0.1.0` (the `.deb` version).
    pub version: String,
    /// `[package] description`, default `<name>, made with j3`.
    pub description: String,
    pub entry: PathBuf,
    pub cc: Cc,
    pub emit: Emit,
    pub link: Link,
    pub dist: Dist,
    pub android: Android,
    /// `[steam]` (`build_outputs = ["steam"]`, dev env).
    pub steam: Steam,
    /// `[ui]`: present when the project has `.j3ui` components.
    pub ui: Option<Ui>,
    /// `[dependencies]`, transitive and flat (`j3c help manifest`).
    pub deps: Vec<Dep>,
}

/// `[ui]`: `dir` holds `*.j3ui`; `theme` is one global CSS file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ui {
    pub dir: String,
    pub theme: Option<String>,
    /// `lint = false` (`"*"`) or `lint-off = ["rule", ...]`: CSS lint rules `j3c check` skips.
    pub lint_off: Vec<String>,
}

/// `[dist]`: `j3c build --release` also writes each of `build_outputs` into `target/dist/`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dist {
    /// `build_outputs = ["zip", "deb"]`; `zip = true` is `["zip"]`.
    pub outputs: Vec<Output>,
    /// Files and folders shipped beside the program. Empty means whichever of
    /// `assets`, `scripts`, `scenes` exist.
    pub include: Vec<String>,
    /// `.png` or `.svg` for the app menu (`.deb`).
    pub icon: Option<String>,
    /// Menu name. Empty: the package name, title-cased.
    pub title: String,
    /// `.desktop` Categories, default `Game;`.
    pub categories: String,
    /// `Name <email>`. Empty: `$DEBFULLNAME`/`$DEBEMAIL`, else git config.
    pub maintainer: String,
    /// Extra `.deb` Depends, after the ones `[link] libs` imply.
    pub depends: Vec<String>,
    /// `.deb` Section, default `games`.
    pub section: String,
    /// `.deb` maintainer scripts (project-relative), copied into the control archive with mode 0755.
    pub postinst: Option<String>,
    pub prerm: Option<String>,
    /// `[dist.actions] id = { name = "New Island", exec = "--new-island" }`: `.desktop` actions (the dock or
    /// launcher menu), in file order. `exec` is the program's arguments.
    pub actions: Vec<DesktopAction>,
}

/// One `[dist.actions]` entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DesktopAction {
    pub id: String,
    pub name: String,
    pub exec: String,
}

/// `[android]`: used by `build_outputs = ["apk"]`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Android {
    /// Java package / application id. Default `com.j3.<name>`, sanitized.
    pub package: String,
    /// `screenOrientation` of the activity, one of [`ORIENTATIONS`]; default `landscape`.
    pub orientation: String,
    /// `native_libs = "dir"`: `dir/<abi>/lib*.so` ship beside `libmain.so` (helper programs, prebuilt libs).
    pub native_libs: Option<String>,
    /// `foreground_service = "connectedDevice"` (or `"dataSync"`): declares `J3Service`, which
    /// `j3.android.service.keepAlive` starts. None: no service, keepAlive does nothing.
    pub foreground_service: Option<String>,
}

/// Accepted `[android] foreground_service` values (Android's `foregroundServiceType` names).
pub const SERVICE_TYPES: [&str; 2] = ["connectedDevice", "dataSync"];

/// Accepted `[android] orientation` values (Android's `screenOrientation` names).
pub const ORIENTATIONS: [&str; 6] = ["landscape", "portrait", "sensorLandscape", "sensorPortrait", "fullSensor", "unspecified"];

const JAVA_KEYWORDS: [&str; 53] = [
    "abstract", "assert", "boolean", "break", "byte", "case", "catch", "char", "class", "const", "continue", "default", "do",
    "double", "else", "enum", "extends", "final", "finally", "float", "for", "goto", "if", "implements", "import",
    "instanceof", "int", "interface", "long", "native", "new", "package", "private", "protected", "public", "return",
    "short", "static", "strictfp", "super", "switch", "synchronized", "this", "throw", "throws", "transient", "try",
    "void", "volatile", "while", "true", "false", "null",
];

/// One Java package segment from free text: lowercase, `-` to `_`, other characters dropped,
/// a letter first (`app` is put in front otherwise), keywords get a `_` suffix.
fn java_segment(s: &str) -> String {
    let mut seg: String =
        s.to_ascii_lowercase().replace('-', "_").chars().filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_').collect();
    if !seg.starts_with(|c: char| c.is_ascii_lowercase()) {
        seg.insert_str(0, "app");
    }
    if JAVA_KEYWORDS.contains(&seg.as_str()) {
        seg.push('_');
    }
    seg
}

/// The default package for a project called `name`.
pub fn default_java_package(name: &str) -> String {
    format!("com.j3.{}", java_segment(name))
}

/// An explicit `[android] package` must already be a valid Java package of 2+ segments.
fn check_java_package(p: &str) -> bool {
    let segs: Vec<&str> = p.split('.').collect();
    segs.len() >= 2
        && segs.iter().all(|s| {
            s.starts_with(|c: char| c.is_ascii_alphabetic())
                && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !JAVA_KEYWORDS.contains(s)
        })
}

/// One `[dist] build_outputs` entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    Zip,
    Deb,
    /// Signed debug APK from the Android triples in `[emit] target` (`dist/apk.rs`).
    Apk,
    /// Steam depot content + `app_build_<id>.vdf` (`dist/steam.rs`).
    Steam,
}

impl Output {
    pub const ALL: [(&'static str, Output); 4] =
        [("zip", Output::Zip), ("deb", Output::Deb), ("apk", Output::Apk), ("steam", Output::Steam)];
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Emit {
    /// `llvm` (default) or `c`. Other printers plug in here later.
    pub backend: String,
    /// `0` / `1` / `2` / `3` / `s` / `z`. LLVM default `2`; C inherits `[cc] opt`.
    pub opt: String,
    pub debug: bool,
    pub fast: bool,
    /// One triple, or several. Empty means the host (Linux `j3.sys`).
    pub targets: Vec<String>,
    pub cpu: String,
    pub clang: String,
    pub flags: Vec<String>,
    /// Opt 0 reuses one object per module. Default true.
    pub incremental: bool,
    /// `incremental = "all"`: per-module objects at every opt, without cross-module inlining.
    pub incremental_all: bool,
    /// `chunks = N`: objects per big module (absent = automatic, 0 = never split).
    pub chunks: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cc {
    pub std: String,
    pub opt: String,
    pub fast: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub libs: Vec<String>,
    pub search: Vec<String>,
    pub files: Vec<String>,
    pub flags: Vec<String>,
}

impl Default for Cc {
    fn default() -> Cc {
        Cc {
            std: "c11".to_string(),
            opt: "1".to_string(),
            fast: false,
        }
    }
}

impl Default for Link {
    fn default() -> Link {
        Link {
            libs: Vec::new(),
            search: Vec::new(),
            files: Vec::new(),
            flags: Vec::new(),
        }
    }
}

impl Default for Emit {
    fn default() -> Emit {
        Emit {
            backend: "llvm".to_string(),
            opt: "2".to_string(),
            debug: true,
            fast: false,
            targets: Vec::new(),
            cpu: String::new(),
            clang: "clang".to_string(),
            flags: Vec::new(),
            incremental: true,
            incremental_all: false,
            chunks: None,
        }
    }
}

impl Manifest {
    pub fn load(toml_path: &Path) -> Result<Manifest, String> {
        let text = std::fs::read_to_string(toml_path)
            .map_err(|e| format!("read {}: {e}", toml_path.display()))?;
        let root = toml_path
            .parent()
            .ok_or_else(|| format!("{} has no parent", toml_path.display()))?
            .to_path_buf();
        let mut m = parse_manifest(&text, root)?;
        if HOST.load(std::sync::atomic::Ordering::Relaxed) {
            m.emit.targets.clear();
        }
        Ok(m)
    }

    pub fn src_dir(&self) -> PathBuf {
        self.root.join("src")
    }

    pub fn out_path(&self) -> PathBuf {
        self.out_path_for(self.emit.primary())
    }

    /// One target keeps `target/<name>`. Several get `target/<triple>/<name>`.
    /// An Android triple is always `target/<triple>/lib<name>.so` (the APK picks it up by ABI).
    pub fn out_path_for(&self, target: &str) -> PathBuf {
        let dir = self.root.join("target");
        if crate::emit::host::target_os(target) == crate::emit::host::TargetOs::Android {
            dir.join(target_slug(target)).join(format!("lib{}.so", self.name))
        } else if self.emit.targets.len() <= 1 {
            dir.join(&self.name)
        } else {
            dir.join(target_slug(target)).join(&self.name)
        }
    }
}

impl Emit {
    /// First Linux (or other host) triple. If every entry is Windows or macOS, the first one.
    pub fn primary(&self) -> &str {
        self.targets
            .iter()
            .find(|t| !is_foreign_target(t))
            .map(|s| s.as_str())
            .unwrap_or_else(|| self.targets.first().map(|s| s.as_str()).unwrap_or(""))
    }
}

fn is_foreign_target(target: &str) -> bool {
    let t = target.to_ascii_lowercase();
    t.contains("windows") || t.contains("apple") || t.contains("darwin")
}

pub fn target_slug(target: &str) -> String {
    target
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// File, directory, or `j3.toml`. Walks up from a `.j3` file to find a project.
/// `j3c run --file`: a `.j3` path is that one file, even inside a project.
/// A `.j3s` path is always that script (`pub fn main`), not the project above it.
pub static FILE_ONLY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// `--host` (run, check): `Manifest::load` drops `[emit] target`, so an Android package builds for
/// this machine in place. Everything else in j3.toml stays as written.
pub static HOST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn resolve(path: &Path) -> Result<Input, String> {
    if path.extension().and_then(|e| e.to_str()) == Some("j3s") {
        return Ok(Input::File(path.to_path_buf()));
    }
    if FILE_ONLY.load(std::sync::atomic::Ordering::Relaxed) && path.extension().and_then(|e| e.to_str()) == Some("j3") {
        return Ok(Input::File(path.to_path_buf()));
    }
    if path.is_dir() {
        let toml = path.join("j3.toml");
        if toml.is_file() {
            return Ok(Input::Project(Manifest::load(&toml)?));
        }
        return Err(format!(
            "no j3.toml in {}\n  = help: `j3c new {}`",
            path.display(),
            path.display()
        ));
    }
    if path.file_name().and_then(|n| n.to_str()) == Some("j3.toml") {
        return Ok(Input::Project(Manifest::load(path)?));
    }
    if let Some(toml) = find_toml_up(path) {
        return Ok(Input::Project(Manifest::load(&toml)?));
    }
    if path.extension().and_then(|e| e.to_str()) == Some("j3") {
        return Ok(Input::File(path.to_path_buf()));
    }
    Err(format!("not a J3 file or project: {}", path.display()))
}

pub fn resolve_cwd() -> Result<Input, String> {
    let here = std::env::current_dir().map_err(|e| format!("cwd: {e}"))?;
    let toml = here.join("j3.toml");
    if toml.is_file() {
        return Ok(Input::Project(Manifest::load(&toml)?));
    }
    Ok(Input::Hello)
}

pub enum Input {
    Hello,
    File(PathBuf),
    Project(Manifest),
}

pub(crate) fn find_toml_up(start: &Path) -> Option<PathBuf> {
    let mut dir = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        let toml = dir.join("j3.toml");
        if toml.is_file() {
            return Some(toml);
        }
        if !dir.pop() {
            return None;
        }
    }
}

pub(crate) fn parse_manifest(text: &str, root: PathBuf) -> Result<Manifest, String> {
    let tbl = parse_toml(text)?;
    let name = tbl
        .get("package", "name")
        .and_then(TomlVal::as_str)
        .ok_or_else(|| "j3.toml: missing [package] name".to_string())?
        .to_string();
    if name.is_empty() {
        return Err("j3.toml: package.name is empty".to_string());
    }
    let entry_rel = tbl
        .get("package", "entry")
        .and_then(TomlVal::as_str)
        .unwrap_or("src/main.j3");
    let entry = root.join(entry_rel);
    if !entry.is_file() {
        return Err(format!("j3.toml: entry {} not found", entry.display()));
    }
    let mut cc = Cc::default();
    if let Some(s) = tbl.get("cc", "std").and_then(TomlVal::as_str) {
        cc.std = s.to_string();
    }
    if let Some(s) = tbl.get("cc", "opt").and_then(TomlVal::as_str) {
        cc.opt = s.to_string();
    }
    if let Some(s) = tbl.get("cc", "fast").and_then(TomlVal::as_str) {
        cc.fast = s == "true";
    }
    let mut emit = Emit::default();
    if let Some(s) = tbl.get("emit", "backend").and_then(TomlVal::as_str) {
        emit.backend = s.to_string();
    }
    if let Some(s) = tbl.get("emit", "opt").and_then(TomlVal::as_str) {
        emit.opt = s.to_string();
    } else if tbl.get("emit", "backend").is_none() {
        emit.opt = cc.opt.clone();
    }
    if let Some(s) = tbl.get("emit", "debug").and_then(TomlVal::as_str) {
        emit.debug = s != "false" && s != "0";
    } else if cc.fast {
        emit.debug = false;
    }
    if let Some(s) = tbl.get("emit", "fast").and_then(TomlVal::as_str) {
        emit.fast = s == "true";
    }
    if let Some(v) = tbl.get("emit", "target") {
        emit.targets = match v {
            TomlVal::List(xs) => xs.clone(),
            TomlVal::Str(s) => vec![s.clone()],
            TomlVal::Map(_) => return Err("j3.toml: emit.target must be a string or an array".to_string()),
        };
        if emit.targets.iter().any(|s| s.is_empty()) {
            return Err("j3.toml: emit.target has an empty triple".to_string());
        }
    }
    if let Some(s) = tbl.get("emit", "cpu").and_then(TomlVal::as_str) {
        emit.cpu = s.to_string();
    }
    if let Some(s) = tbl.get("emit", "clang").and_then(TomlVal::as_str) {
        emit.clang = s.to_string();
    }
    if let Some(v) = tbl.get("emit", "flags") {
        emit.flags = v.as_list("emit.flags")?;
    }
    if let Some(s) = tbl.get("emit", "incremental").and_then(TomlVal::as_str) {
        emit.incremental = s != "false" && s != "0";
        emit.incremental_all = s == "all";
    }
    if let Some(s) = tbl.get("emit", "chunks").and_then(TomlVal::as_str) {
        emit.chunks = s.parse().ok();
    }
    let mut link = Link::default();
    if let Some(v) = tbl.get("link", "libs") {
        link.libs = v.as_list("link.libs")?;
    }
    if let Some(v) = tbl.get("link", "search") {
        link.search = v
            .as_list("link.search")?
            .into_iter()
            .map(|d| match (d.strip_prefix("~/"), std::env::var("HOME")) {
                (Some(rest), Ok(home)) => format!("{home}/{rest}"),
                _ => d,
            })
            .collect();
    }
    if let Some(v) = tbl.get("link", "files") {
        link.files = v.as_list("link.files")?;
    }
    if let Some(v) = tbl.get("link", "flags") {
        link.flags = v.as_list("link.flags")?;
    }
    let mut dist = Dist {
        categories: "Game;".to_string(),
        section: "games".to_string(),
        ..Dist::default()
    };
    if let Some(v) = tbl.get("dist", "build_outputs") {
        for o in v.as_list("dist.build_outputs")? {
            let Some(&(_, out)) = Output::ALL.iter().find(|(n, _)| *n == o) else {
                let names: Vec<&str> = Output::ALL.iter().map(|(n, _)| *n).collect();
                return Err(format!("j3.toml: dist.build_outputs: unknown `{o}` (have {})", names.join(", ")));
            };
            if !dist.outputs.contains(&out) {
                dist.outputs.push(out);
            }
        }
    }
    if tbl.get("dist", "zip").and_then(TomlVal::as_str) == Some("true") && !dist.outputs.contains(&Output::Zip) {
        dist.outputs.insert(0, Output::Zip);
    }
    if let Some(v) = tbl.get("dist", "include") {
        dist.include = v.as_list("dist.include")?;
    }
    if let Some(v) = tbl.get("dist", "depends") {
        dist.depends = v.as_list("dist.depends")?;
    }
    let dist_str = |key: &str| tbl.get("dist", key).and_then(TomlVal::as_str).map(str::to_string);
    dist.icon = dist_str("icon");
    dist.title = dist_str("title").unwrap_or_default();
    if let Some(c) = dist_str("categories") {
        dist.categories = c;
    }
    dist.maintainer = dist_str("maintainer").unwrap_or_default();
    if let Some(sec) = dist_str("section") {
        dist.section = sec;
    }
    dist.postinst = dist_str("postinst");
    dist.prerm = dist_str("prerm");
    for (s, id, v) in &tbl.vals {
        if s != "dist.actions" {
            continue;
        }
        let TomlVal::Map(kv) = v else {
            return Err(format!("j3.toml: dist.actions.{id} must be `{{ name = \"...\", exec = \"...\" }}`"));
        };
        if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(format!("j3.toml: dist.actions.{id}: an action id is letters, digits and `-`"));
        }
        let get = |k: &str| kv.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        let Some(name) = get("name") else {
            return Err(format!("j3.toml: dist.actions.{id} needs `name`"));
        };
        dist.actions.retain(|a| a.id != *id);
        dist.actions.push(DesktopAction { id: id.clone(), name, exec: get("exec").unwrap_or_default() });
    }
    let mut android = Android { package: default_java_package(&name), orientation: "landscape".into(), native_libs: None, foreground_service: None };
    if let Some(p) = tbl.get("android", "package").and_then(TomlVal::as_str) {
        if !check_java_package(p) {
            return Err(format!("j3.toml: android.package: `{p}` is not a Java package (like com.example.game: 2+ dot-separated segments, each starting with a letter)"));
        }
        android.package = p.to_string();
    }
    if let Some(o) = tbl.get("android", "orientation").and_then(TomlVal::as_str) {
        if !ORIENTATIONS.contains(&o) {
            return Err(format!("j3.toml: android.orientation: unknown `{o}` (have {})", ORIENTATIONS.join(", ")));
        }
        android.orientation = o.to_string();
    }
    android.native_libs = tbl.get("android", "native_libs").and_then(TomlVal::as_str).map(str::to_string);
    if let Some(t) = tbl.get("android", "foreground_service").and_then(TomlVal::as_str) {
        if !SERVICE_TYPES.contains(&t) {
            return Err(format!("j3.toml: android.foreground_service: unknown `{t}` (have {})", SERVICE_TYPES.join(", ")));
        }
        android.foreground_service = Some(t.to_string());
    }
    let version = tbl.get("package", "version").and_then(TomlVal::as_str).unwrap_or("0.1.0").to_string();
    let description = match tbl.get("package", "description").and_then(TomlVal::as_str) {
        Some(d) => d.to_string(),
        None => format!("{name}, made with j3"),
    };
    let ui = if tbl.get("ui", "dir").is_some() || tbl.get("ui", "theme").is_some() {
        Some(Ui {
            dir: tbl.get("ui", "dir").and_then(TomlVal::as_str).unwrap_or("ui").to_string(),
            theme: tbl.get("ui", "theme").and_then(TomlVal::as_str).map(str::to_string),
            lint_off: {
                let mut off = match tbl.get("ui", "lint-off") {
                    Some(v) => v.as_list("ui.lint-off")?,
                    None => Vec::new(),
                };
                if matches!(tbl.get("ui", "lint").and_then(TomlVal::as_str), Some("false")) {
                    off.push("*".into());
                }
                off
            },
        })
    } else {
        None
    };
    let steam = steam::parse(&tbl, &root, &name, &version)?;
    let deps = deps::resolve(&root, &tbl, &name)?;
    Ok(Manifest {
        root,
        name,
        version,
        description,
        entry,
        cc,
        emit,
        link,
        dist,
        android,
        steam,
        ui,
        deps,
    })
}

#[cfg(test)]
mod tests;
