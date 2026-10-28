//! `j3.toml` + `src/` project layout. No crates.io TOML parser — this is the subset we take.

use std::path::{Path, PathBuf};

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
    /// `[ui]`: present when the project has `.j3ui` components.
    pub ui: Option<Ui>,
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
}

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
}

impl Output {
    pub const ALL: [(&'static str, Output); 3] = [("zip", Output::Zip), ("deb", Output::Deb), ("apk", Output::Apk)];
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
            opt: "0".to_string(),
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
        parse_manifest(&text, root)
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
    let mut android = Android { package: default_java_package(&name), orientation: "landscape".into(), native_libs: None };
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
        ui,
    })
}

#[derive(Clone, Debug)]
enum TomlVal {
    Str(String),
    List(Vec<String>),
}

impl TomlVal {
    fn as_str(&self) -> Option<&str> {
        match self {
            TomlVal::Str(s) => Some(s),
            TomlVal::List(_) => None,
        }
    }

    fn as_list(&self, key: &str) -> Result<Vec<String>, String> {
        match self {
            TomlVal::List(v) => Ok(v.clone()),
            TomlVal::Str(_) => Err(format!("j3.toml: `{key}` must be an array")),
        }
    }
}

struct TomlTable {
    vals: Vec<(String, String, TomlVal)>,
    /// `[section]` headers with their line numbers, in file order (a header with no keys still counts).
    heads: Vec<(String, usize)>,
    lines: Vec<usize>,
}

impl TomlTable {
    fn get(&self, section: &str, key: &str) -> Option<&TomlVal> {
        self.vals
            .iter()
            .rev()
            .find(|(s, k, _)| s == section && k == key)
            .map(|(_, _, v)| v)
    }
}

fn parse_toml(text: &str) -> Result<TomlTable, String> {
    let mut section = String::new();
    let (mut vals, mut heads, mut lines) = (Vec::new(), Vec::new(), Vec::new());
    for (i, raw) in text.lines().enumerate() {
        let line_no = i + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = parse_section(line) {
            heads.push((name.clone(), line_no));
            section = name;
            continue;
        }
        if section.is_empty() {
            return Err(format!("j3.toml:{line_no}: key outside a section"));
        }
        let (key, val) = parse_key_val(line, line_no)?;
        vals.push((section.clone(), key, val));
        lines.push(line_no);
    }
    Ok(TomlTable { vals, heads, lines })
}

fn strip_comment(line: &str) -> &str {
    let mut in_str = false;
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' if in_str => {
                in_str = false;
                i += 1;
            }
            b'"' => {
                in_str = true;
                i += 1;
            }
            b'\\' if in_str => i += 2,
            b'#' if !in_str => return &line[..i],
            _ => i += 1,
        }
    }
    line
}

fn parse_section(line: &str) -> Option<String> {
    let s = line.strip_prefix('[')?.strip_suffix(']')?;
    if s.is_empty() || s.contains('[') || s.contains(']') || s.contains(' ') {
        return None;
    }
    Some(s.to_string())
}

fn parse_key_val(line: &str, line_no: usize) -> Result<(String, TomlVal), String> {
    let eq = line
        .find('=')
        .ok_or_else(|| format!("j3.toml:{line_no}: expected `key = value`"))?;
    let key = line[..eq].trim();
    if key.is_empty() || !is_ident(key) {
        return Err(format!("j3.toml:{line_no}: bad key"));
    }
    let rhs = line[eq + 1..].trim();
    let val = if rhs.starts_with('[') {
        TomlVal::List(parse_string_array(rhs, line_no)?)
    } else if rhs.starts_with('"') {
        TomlVal::Str(parse_string(rhs, line_no)?)
    } else if is_bare(rhs) {
        TomlVal::Str(rhs.to_string())
    } else {
        TomlVal::Str(parse_string(rhs, line_no)?)
    };
    Ok((key.to_string(), val))
}

fn is_bare(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn parse_string(s: &str, line_no: usize) -> Result<String, String> {
    let inner = s
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .ok_or_else(|| format!("j3.toml:{line_no}: expected a \"string\""))?;
    Ok(inner.to_string())
}

fn parse_string_array(s: &str, line_no: usize) -> Result<Vec<String>, String> {
    let inner = s
        .strip_prefix('[')
        .and_then(|t| t.strip_suffix(']'))
        .ok_or_else(|| format!("j3.toml:{line_no}: expected an array"))?;
    let inner = inner.trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    for ch in inner.chars() {
        if ch == '"' {
            in_str = !in_str;
            cur.push(ch);
            continue;
        }
        if ch == ',' && !in_str {
            let part = cur.trim();
            if !part.is_empty() {
                out.push(parse_string(part, line_no)?);
            }
            cur.clear();
            continue;
        }
        cur.push(ch);
    }
    let part = cur.trim();
    if !part.is_empty() {
        out.push(parse_string(part, line_no)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bounce_shaped_toml() {
        let text = r#"
# comment
[package]
name = "bounce"
entry = "src/main.j3"

[cc]
std = "c11"
opt = "0"

[link]
libs = ["SDL2"]
search = []
files = []
flags = ["-lm"]
"#;
        let tmp = std::env::temp_dir().join("j3_toml_parse_test");
        let _ = std::fs::create_dir_all(tmp.join("src"));
        std::fs::write(tmp.join("src/main.j3"), "module t;\nfn main() {}\n").unwrap();
        let m = parse_manifest(text, tmp.clone()).expect("parse");
        assert_eq!(m.name, "bounce");
        assert_eq!(m.entry, tmp.join("src/main.j3"));
        assert_eq!(m.cc.std, "c11");
        assert_eq!(m.cc.opt, "0");
        assert_eq!(m.link.libs, vec!["SDL2".to_string()]);
        assert!(m.link.search.is_empty());
        assert_eq!(m.link.flags, vec!["-lm".to_string()]);
        assert_eq!(m.emit.backend, "llvm");
        assert_eq!(m.emit.opt, "0");
    }

    #[test]
    fn parse_emit_section_bare_tokens() {
        let text = r#"
[package]
name = "emitme"
entry = "src/main.j3"

[emit]
backend = llvm
opt = 2
debug = true
fast = false
cpu = native
flags = ["-mavx2"]
"#;
        let tmp = std::env::temp_dir().join("j3_toml_emit_test");
        let _ = std::fs::create_dir_all(tmp.join("src"));
        std::fs::write(tmp.join("src/main.j3"), "module t;\nfn main() {}\n").unwrap();
        let m = parse_manifest(text, tmp).expect("parse");
        assert_eq!(m.emit.backend, "llvm");
        assert_eq!(m.emit.opt, "2");
        assert!(m.emit.debug);
        assert!(!m.emit.fast);
        assert_eq!(m.emit.cpu, "native");
        assert_eq!(m.emit.flags, vec!["-mavx2".to_string()]);
        assert!(m.emit.targets.is_empty());
    }

    #[test]
    fn parse_target_array_and_string() {
        let tmp = std::env::temp_dir().join("j3_toml_targets_test");
        let _ = std::fs::create_dir_all(tmp.join("src"));
        std::fs::write(tmp.join("src/main.j3"), "module t;\nfn main() {}\n").unwrap();
        let many = r#"
[package]
name = "multi"
entry = "src/main.j3"
[emit]
target = ["x86_64-pc-linux-gnu", "x86_64-pc-windows-gnu", "arm64-apple-darwin"]
"#;
        let m = parse_manifest(many, tmp.clone()).expect("array");
        assert_eq!(
            m.emit.targets,
            vec![
                "x86_64-pc-linux-gnu".to_string(),
                "x86_64-pc-windows-gnu".to_string(),
                "arm64-apple-darwin".to_string(),
            ]
        );
        assert_eq!(m.emit.primary(), "x86_64-pc-linux-gnu");
        assert_eq!(
            m.out_path_for("x86_64-pc-windows-gnu"),
            tmp.join("target").join("x86_64-pc-windows-gnu").join("multi")
        );
        let one = r#"
[package]
name = "multi"
entry = "src/main.j3"
[emit]
target = "x86_64-pc-windows-gnu"
"#;
        let m = parse_manifest(one, tmp).expect("string");
        assert_eq!(m.emit.targets, vec!["x86_64-pc-windows-gnu".to_string()]);
        assert_eq!(m.emit.primary(), "x86_64-pc-windows-gnu");
        assert!(m.out_path().ends_with("multi"));
    }

    #[test]
    fn script_path_is_the_file_not_the_project() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/fps-shooter");
        match resolve(&root).expect("project") {
            Input::Project(m) => assert!(
                m.entry.ends_with("src/main.j3"),
                "{}",
                m.entry.display()
            ),
            Input::File(_) | Input::Hello => panic!("expected the game project"),
        }
        let script = root.join("tools/arena.j3s");
        match resolve(&script).expect("script") {
            Input::File(p) => assert_eq!(p, script),
            Input::Project(_) | Input::Hello => panic!("expected the script file"),
        }
    }

    #[test]
    fn dist_build_outputs() {
        let dir = std::env::temp_dir().join(format!("j3-dist-toml-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/main.j3"), "").unwrap();
        let m = |t: &str| parse_manifest(&format!("[package]\nname = \"g\"\n{t}"), dir.clone());
        let a = m("[dist]\nbuild_outputs = [\"deb\", \"zip\", \"deb\"]\nicon = \"i.png\"\n").unwrap();
        assert_eq!(a.dist.outputs, [Output::Deb, Output::Zip]);
        assert_eq!(a.dist.icon.as_deref(), Some("i.png"));
        assert_eq!(a.dist.categories, "Game;");
        assert_eq!((a.version.as_str(), a.description.as_str()), ("0.1.0", "g, made with j3"));
        assert_eq!(m("[dist]\nzip = true\n").unwrap().dist.outputs, [Output::Zip]);
        assert!(m("[dist]\nbuild_outputs = [\"msi\"]\n").unwrap_err().contains("zip, deb"));
        // `apk` without an Android triple only fails where it is built (dist/apk.rs)
        assert_eq!(m("[dist]\nbuild_outputs = [\"apk\"]\n").unwrap().dist.outputs, [Output::Apk]);
        let ok = m("[emit]\ntarget = [\"aarch64-linux-android\"]\n[dist]\nbuild_outputs = [\"apk\"]\n").unwrap();
        assert_eq!(ok.dist.outputs, [Output::Apk]);
        assert_eq!(ok.android, Android { package: "com.j3.g".into(), orientation: "landscape".into(), native_libs: None });
        let a = m("[android]\norientation = \"sensorPortrait\"\npackage = \"org.me.game\"\n").unwrap();
        assert_eq!((a.android.package.as_str(), a.android.orientation.as_str()), ("org.me.game", "sensorPortrait"));
        assert_eq!(m("[android]\nnative_libs = \"android/libs\"\n").unwrap().android.native_libs.as_deref(), Some("android/libs"));
        assert!(m("[android]\norientation = \"flip\"\n").unwrap_err().contains("android.orientation"));
        assert!(m("[android]\npackage = \"1bad.x\"\n").unwrap_err().contains("android.package"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn java_package_defaults_are_valid() {
        assert_eq!(default_java_package("My-Game"), "com.j3.my_game");
        assert_eq!(default_java_package("3d-demo"), "com.j3.app3d_demo");
        assert_eq!(default_java_package("_x"), "com.j3.app_x");
        assert_eq!(default_java_package("new"), "com.j3.new_");
        assert_eq!(default_java_package("é"), "com.j3.app");
        assert!(check_java_package(&default_java_package("desk-app")));
        assert!(!check_java_package("single"));
    }

    #[test]
    fn missing_name_is_error() {
        let err = parse_manifest("[link]\nlibs = [\"x\"]\n", PathBuf::from("/")).unwrap_err();
        assert!(err.contains("package"), "{err}");
    }
}

/// One `[section] key = "value"` from TOML text (used for the user config).
pub(crate) fn toml_str(text: &str, section: &str, key: &str) -> Option<String> {
    parse_toml(text).ok()?.get(section, key)?.as_str().map(str::to_string)
}

/// Section headers (with lines) and string/number entries `(section, key, value, line)` of TOML text, for
/// `j3c golden`. Errors read `j3.toml:LINE: ...`; the caller renames the file.
pub(crate) fn toml_doc(text: &str) -> Result<(Vec<(String, usize)>, Vec<(String, String, String, usize)>), String> {
    let t = parse_toml(text)?;
    let mut out = Vec::new();
    for ((s, k, v), line) in t.vals.into_iter().zip(t.lines) {
        match v {
            TomlVal::Str(v) => out.push((s, k, v, line)),
            TomlVal::List(_) => return Err(format!("j3.toml:{line}: `{k}` must be a string or number")),
        }
    }
    Ok((t.heads, out))
}
