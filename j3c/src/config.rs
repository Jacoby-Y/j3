//! User config: `$XDG_CONFIG_HOME/j3/config.toml` (else `~/.config/j3/config.toml`),
//! `%APPDATA%\j3\config.toml` on Windows, `~/Library/Application Support/j3/config.toml`
//! on macOS. `J3_CONFIG` overrides the path.
//!
//! ```toml
//! [compiler]
//! compilation_tracking = true   # append each build to <project>/target/compilation.log
//! ```
//! `J3C_COMPILATION_TRACKING=0|1` overrides the file.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub fn path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("J3_CONFIG") {
        return Some(p.into());
    }
    let dir = if cfg!(windows) {
        PathBuf::from(std::env::var_os("APPDATA")?)
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support")
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?
    };
    Some(dir.join("j3").join("config.toml"))
}

/// A `[section] key` string from the user config, if the file and key exist.
pub fn get(section: &str, key: &str) -> Option<String> {
    crate::project::toml_str(&std::fs::read_to_string(path()?).ok()?, section, key)
}

pub fn compilation_tracking() -> bool {
    let v = std::env::var("J3C_COMPILATION_TRACKING").ok().or_else(|| get("compiler", "compilation_tracking"));
    matches!(v.as_deref(), Some("1" | "true" | "on"))
}

/// Last error `print_fail` showed, for the log.
pub static LAST_ERR: Mutex<String> = Mutex::new(String::new());

pub struct Compile<'a> {
    pub root: &'a Path,
    pub opt: &'a str,
    pub backend: &'a str,
    pub target: &'a str,
    pub modules: usize,
    pub secs: f64,
    /// Per-phase times from `phase::report`, empty if none were recorded.
    pub phases: String,
    pub ok: bool,
}

/// Append one entry to `<root>/target/compilation.log`. Never fails the build.
pub fn log_compile(c: &Compile) {
    let mut s = format!("COMPILE: {}\n", now_utc());
    s += &format!("    j3c version: {}\n    backend={} opt={}\n", env!("CARGO_PKG_VERSION"), c.backend, c.opt);
    if !c.target.is_empty() {
        s += &format!("    target: {}\n", c.target);
    }
    s += &format!("    {} modules\n    time: {:.1}s\n", c.modules, c.secs);
    if !c.phases.is_empty() {
        s += &format!("    phases:\n{}", c.phases);
    }
    if c.ok {
        s += "    result: ok\n";
    } else {
        let err = LAST_ERR.lock().map(|e| e.clone()).unwrap_or_default();
        let line = err.lines().find(|l| l.contains("error")).or_else(|| err.lines().next()).unwrap_or("build failed");
        s += &format!("    result: FAILED\n    {}\n", line.trim());
    }
    let dir = c.root.join("target");
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("compilation.log")) {
        let _ = writeln!(f, "{s}");
    }
}

fn now_utc() -> String {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (days, rem) = ((t / 86400) as i64, t % 86400);
    // civil-from-days (Howard Hinnant)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + (m <= 2) as i64;
    format!("{y}-{m:02}-{d:02} {:02}:{:02} UTC", rem / 3600, rem % 3600 / 60)
}

#[cfg(test)]
mod tests {
    #[test]
    fn utc_format() {
        assert!(super::now_utc().starts_with("20"));
    }
}
