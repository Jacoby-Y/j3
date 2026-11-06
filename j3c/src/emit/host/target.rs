//! Target OS, release/linker flags and cross-target helpers.

use super::*;

/// What a target needs to know about its platform. One place to add the next one: a new
/// `TargetOs` arm and a row in [`release_flags`]. `Other` (wasm, ...) gets only
/// the portable size wins and no linker flags, so an unknown linker never sees a flag it rejects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TargetOs {
    Linux,
    Windows,
    Macos,
    Android,
    Other,
}

/// `""` is the host.
pub(crate) fn target_os(triple: &str) -> TargetOs {
    let t = triple.to_ascii_lowercase();
    if t.is_empty() {
        return if cfg!(windows) {
            TargetOs::Windows
        } else if cfg!(target_os = "macos") {
            TargetOs::Macos
        } else {
            TargetOs::Linux
        };
    }
    if t.contains("windows") {
        TargetOs::Windows
    } else if t.contains("apple") || t.contains("darwin") {
        TargetOs::Macos
    } else if t.contains("android") {
        // before Linux: `x86_64-linux-android` contains "linux"
        TargetOs::Android
    } else if ["linux", "freebsd", "netbsd", "openbsd"].iter().any(|n| t.contains(n)) {
        TargetOs::Linux
    } else {
        TargetOs::Other
    }
}

pub(crate) struct ReleaseFlags {
    /// Clang compile flags.
    pub compile: Vec<&'static str>,
    /// Appended after the inputs and libraries.
    pub link: Vec<&'static str>,
}

/// Flags that shrink a shipped binary (`j3c build --release`).
/// - Linux/ELF: strip, no build-id, no RELRO, GNU hash only, no unwind tables (J3 has no
///   exceptions and the crash handler walks frame pointers); identical-code folding with lld.
/// - Windows (mingw): strip, no timestamp. Unwind tables stay: x64 needs `.pdata` for SEH.
/// - macOS (ld64): strip locals and debug (`-s` is ignored there). Compact unwind stays.
pub(crate) fn release_flags(os: TargetOs, lld: bool) -> ReleaseFlags {
    match os {
        TargetOs::Linux => {
            let mut link = vec!["-Wl,-s", "-Wl,--build-id=none", "-Wl,-z,norelro", "-Wl,--hash-style=gnu"];
            if lld {
                link.extend(["-fuse-ld=lld", "-Wl,--icf=safe"]);
            }
            ReleaseFlags {
                compile: vec!["-fno-asynchronous-unwind-tables", "-fno-unwind-tables"],
                link,
            }
        }
        TargetOs::Windows => ReleaseFlags { compile: vec![], link: vec!["-Wl,-s", "-Wl,--no-insert-timestamp"] },
        TargetOs::Macos => ReleaseFlags { compile: vec![], link: vec!["-Wl,-x", "-Wl,-S"] },
        // Unwind tables stay: bionic's unwinder (and the NDK's crash reports) read them.
        TargetOs::Android => ReleaseFlags { compile: vec!["-fno-asynchronous-unwind-tables"], link: vec!["-Wl,--gc-sections", "-Wl,-s"] },
        TargetOs::Other => ReleaseFlags { compile: vec![], link: vec![] },
    }
}

/// `ld.lld` on `PATH`, looked up once.
pub(crate) fn lld_present() -> bool {
    static LLD: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *LLD.get_or_init(|| {
        Command::new("ld.lld")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

/// `mold` on `PATH`, looked up once.
pub(crate) fn mold_on_path() -> bool {
    static FOUND: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FOUND.get_or_init(|| {
        std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).any(|d| d.join("mold").is_file()))
            .unwrap_or(false)
    })
}

/// Dev links on the host use mold when it is installed. Release keeps its own linker flags.
pub(crate) fn use_mold(cmd: &mut Command, release: bool, target: &str) {
    if !release && cfg!(target_os = "linux") && target.is_empty() && mold_on_path() {
        cmd.arg("-fuse-ld=mold"); // a manifest `-fuse-ld=` flag comes later and wins
    }
}

/// Unwind tables are dropped in release builds only where [`release_flags`] drops them.
pub(crate) fn release_nounwind(release: bool, triple: &str) -> bool {
    release && target_os(triple) == TargetOs::Linux
}

/// `--target=` for `target`; an Android triple also gets the API level, NDK sysroot and `-fPIC`.
pub(crate) fn target_args(cmd: &mut Command, target: &str) {
    if target_os(target) == TargetOs::Android {
        cmd.args(crate::emit::android::target_flags(target));
    } else if !target.is_empty() {
        cmd.arg(format!("--target={target}"));
    }
}

/// The clang for `target`: `[emit] clang`, else the NDK's on Android, else `clang`.
pub(crate) fn clang_for(clang: &str, target: &str) -> String {
    if target_os(target) == TargetOs::Android {
        crate::emit::android::compiler(clang)
    } else if clang.is_empty() {
        "clang".to_string()
    } else {
        clang.to_string()
    }
}

pub(crate) fn link_gc_flag(target: &str) -> &'static str {
    let t = target.to_ascii_lowercase();
    if t.contains("apple") || t.contains("darwin") {
        "-Wl,-dead_strip"
    } else {
        "-Wl,--gc-sections"
    }
}

pub(super) fn foreign_target(target: &str) -> bool {
    let t = target.to_ascii_lowercase();
    t.contains("windows") || t.contains("apple") || t.contains("darwin")
}

pub(super) fn windows_target(target: &str) -> bool {
    target.to_ascii_lowercase().contains("windows")
}

pub(super) fn keep_flag(flag: &str, target: &str) -> bool {
    if !windows_target(target) {
        return true;
    }
    let f = flag.to_ascii_lowercase();
    !f.contains("rpath") && !f.contains("--disable-new-dtags")
}

/// `~/.cache/j3/sdl2-mingw/<pkg>/<arch>-w64-mingw32/lib` for every SDL package there, so a
/// Windows target links `-lSDL2_ttf` without `[link] search` entries.
pub(super) fn mingw_sdl_libs(target: &str) -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME") else { return Vec::new() };
    let arch = target.split('-').next().unwrap_or("x86_64");
    let Ok(rd) = std::fs::read_dir(PathBuf::from(home).join(".cache/j3/sdl2-mingw")) else { return Vec::new() };
    let mut dirs: Vec<PathBuf> =
        rd.flatten().map(|e| e.path().join(format!("{arch}-w64-mingw32/lib"))).filter(|p| p.is_dir()).collect();
    dirs.sort();
    dirs
}

/// `-lNAME` as the target names it: Windows has no libc/libdl to link and calls GL `opengl32`.
pub(crate) fn windows_lib(name: &str) -> Option<&str> {
    match name {
        "c" | "dl" => None,
        "GL" => Some("opengl32"),
        other => Some(other),
    }
}
