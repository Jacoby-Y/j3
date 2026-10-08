//! `j3.toml` + `src/` project layout. No crates.io TOML parser — this is the subset we take.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub root: PathBuf,
    pub name: String,
    pub entry: PathBuf,
    pub cc: Cc,
    pub emit: Emit,
    pub link: Link,
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

    /// One target keeps `target/<name>`. Several get `target/<name>-<triple>`.
    pub fn out_path_for(&self, target: &str) -> PathBuf {
        let dir = self.root.join("target");
        if self.emit.targets.len() <= 1 {
            dir.join(&self.name)
        } else {
            dir.join(format!("{}-{}", self.name, target_slug(target)))
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

fn target_slug(target: &str) -> String {
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

fn find_toml_up(start: &Path) -> Option<PathBuf> {
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

fn parse_manifest(text: &str, root: PathBuf) -> Result<Manifest, String> {
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
    Ok(Manifest {
        root,
        name,
        entry,
        cc,
        emit,
        link,
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
    let mut vals = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line_no = i + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = parse_section(line) {
            section = name;
            continue;
        }
        if section.is_empty() {
            return Err(format!("j3.toml:{line_no}: key outside a section"));
        }
        let (key, val) = parse_key_val(line, line_no)?;
        vals.push((section.clone(), key, val));
    }
    Ok(TomlTable { vals })
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
            tmp.join("target").join("multi-x86_64-pc-windows-gnu")
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
    fn missing_name_is_error() {
        let err = parse_manifest("[link]\nlibs = [\"x\"]\n", PathBuf::from("/")).unwrap_err();
        assert!(err.contains("package"), "{err}");
    }
}
