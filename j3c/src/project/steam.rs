//! `[steam]`: Steamworks app id, SDK folder, depots (`dist/steam.rs`, `j3c doc steam`).

use std::path::{Path, PathBuf};

use super::toml::{TomlTable, TomlVal};

/// `[steam]`. Every key is optional; the `"steam"` dist output and the dev env need `app_id`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Steam {
    pub app_id: Option<u32>,
    /// Unzipped Steamworks SDK root, made absolute (`~/` and project-relative accepted).
    pub sdk: Option<PathBuf>,
    pub depot_linux: Option<u32>,
    pub depot_windows: Option<u32>,
    /// Branch the build is set live on. None: nothing goes live.
    pub branch: Option<String>,
    /// Build description. Default `<name> <version>`.
    pub desc: String,
    /// Folder of `.so` files copied beside the Linux exe (libs built inside the sniper SDK).
    pub runtime_libs: Option<PathBuf>,
    /// Highest `GLIBC_x.y` the Linux build may need, `"2.31"` (the sniper runtime) by default.
    pub glibc_max: String,
}

const KEYS: [&str; 8] = ["app_id", "sdk", "depot_linux", "depot_windows", "branch", "desc", "runtime_libs", "glibc_max"];

/// `~/x` and `x` (relative to `root`) to an absolute path.
pub(crate) fn resolve_path(root: &Path, s: &str) -> PathBuf {
    match (s.strip_prefix("~/"), std::env::var("HOME")) {
        (Some(rest), Ok(home)) => PathBuf::from(home).join(rest),
        _ => root.join(s),
    }
}

fn id(tbl: &TomlTable, key: &str) -> Result<Option<u32>, String> {
    let Some(v) = tbl.get("steam", key) else { return Ok(None) };
    match v.as_str().and_then(|s| s.parse::<u32>().ok()) {
        Some(n) if n > 0 => Ok(Some(n)),
        _ => Err(format!("j3.toml: steam.{key}: must be a positive whole number")),
    }
}

pub(super) fn parse(tbl: &TomlTable, root: &Path, name: &str, version: &str) -> Result<Steam, String> {
    for (s, k, _) in &tbl.vals {
        if s == "steam" && !KEYS.contains(&k.as_str()) {
            eprintln!("j3c: j3.toml: steam.{k}: unknown key (have {})", KEYS.join(", "));
        }
    }
    let str_of = |key: &str| tbl.get("steam", key).and_then(TomlVal::as_str).map(str::to_string);
    let branch = str_of("branch").filter(|b| !b.is_empty());
    if branch.as_deref() == Some("default") {
        eprintln!("j3c: j3.toml: steam.branch = \"default\": steamcmd usually cannot set the default branch live; use a named branch and promote it on the partner site");
    }
    let glibc_max = str_of("glibc_max").unwrap_or_else(|| "2.31".into());
    if parse_glibc(&glibc_max).is_none() {
        return Err("j3.toml: steam.glibc_max: must look like \"2.31\"".into());
    }
    Ok(Steam {
        app_id: id(tbl, "app_id")?,
        sdk: str_of("sdk").map(|s| resolve_path(root, &s)),
        depot_linux: id(tbl, "depot_linux")?,
        depot_windows: id(tbl, "depot_windows")?,
        branch,
        runtime_libs: str_of("runtime_libs").map(|s| resolve_path(root, &s)),
        glibc_max,
        desc: str_of("desc").map(|d| unescape(&d)).unwrap_or_else(|| format!("{name} {version}")),
    })
}

/// `"2.31"` to `(2, 31)`.
pub(crate) fn parse_glibc(v: &str) -> Option<(u32, u32)> {
    let (a, b) = v.split_once('.')?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

/// `\"` and `\\` in a TOML string value (the reader keeps escapes as written).
fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match (c, it.peek()) {
            ('\\', Some(&n)) if n == '"' || n == '\\' => {
                out.push(n);
                it.next();
            }
            _ => out.push(c),
        }
    }
    out
}
