//! Link arguments for a build and how `-lNAME` libraries are found.

use super::hints::find_soname;
use super::target::{foreign_target, keep_flag, mingw_sdl_libs, windows_target};
use super::*;

pub(crate) fn append_link_args(cmd: &mut Command, opts: &CcOpts, source: &str, target: &str) -> Result<(), String> {
    let android = target_os(target) == TargetOs::Android;
    if android {
        cmd.args(crate::emit::android::link_flags(target));
    }
    for file in &opts.files {
        cmd.arg(file);
    }
    for dir in &opts.search {
        // ponytail: path sniff; a per-target [link] table is the real fix
        if !windows_target(target) && dir.contains("mingw") {
            continue;
        }
        cmd.arg(format!("-L{dir}"));
        if !foreign_target(target) {
            cmd.arg(format!("-Wl,-rpath-link,{dir}"));
        }
    }
    if windows_target(target) {
        for dir in mingw_sdl_libs(target) {
            cmd.arg(format!("-L{}", dir.display()));
        }
    }
    let mut libs = libs_from_c(source);
    for lib in &opts.libs {
        if !libs.iter().any(|l| l == lib) {
            libs.push(lib.clone());
        }
    }
    for lib in &libs {
        if windows_target(target) {
            if let Some(name) = windows_lib(lib) {
                cmd.arg(format!("-l{name}"));
            }
            continue;
        }
        cmd.arg(link_lib(lib, &opts.search, target)?);
    }
    for flag in &opts.flags {
        if keep_flag(flag, target) {
            cmd.arg(flag);
        }
    }
    if windows_target(target) {
        // The mingw link leaves the TLS directory (`#[thread]` globals) without base relocations, so
        // an exe that Windows ASLR moves off 0x140000000 crashes at its first thread-local access.
        cmd.arg("-Wl,--disable-dynamicbase");
        cmd.arg("-Wl,--disable-high-entropy-va");
    }
    Ok(())
}

/// True when the host linker can see `lib{name}`.
pub fn host_lib_present(name: &str, search: &[String]) -> bool {
    link_lib(name, search, "").is_ok()
}

/// `-lNAME` when the linker can see `libNAME.so`; otherwise a found `libNAME*.so*`.
/// Debian often ships `libSDL2-2.0.so.0` without the `-dev` symlink.
/// Windows and macOS pass `-lNAME` through so a missing library is the linker's error.
pub(super) fn link_lib(name: &str, search: &[String], target: &str) -> Result<String, String> {
    if target_os(target) == TargetOs::Android {
        return Ok(format!("-l{}", crate::emit::android::lib_name(name)));
    }
    if foreign_target(target) {
        return Ok(format!("-l{name}"));
    }
    if name == "m" {
        return Ok("-lm".to_string());
    }
    let mut dirs: Vec<PathBuf> = search.iter().map(PathBuf::from).collect();
    for extra in [
        "/usr/lib/x86_64-linux-gnu",
        "/lib/x86_64-linux-gnu",
        "/usr/lib",
        "/usr/local/lib",
    ] {
        let p = PathBuf::from(extra);
        if p.is_dir() && !dirs.iter().any(|d| d == &p) {
            dirs.push(p);
        }
    }
    for dir in &dirs {
        if dir.join(format!("lib{name}.so")).is_file() {
            return Ok(format!("-l{name}"));
        }
    }
    for dir in &dirs {
        if let Some(found) = find_soname(dir, name) {
            return Ok(found);
        }
    }
    let msg = format!("cannot find library `{name}`");
    Err(match failure_hint(&msg) {
        Some(hint) => format!("{msg}\n  = help: {hint}"),
        None => format!("{msg}\n  = help: install it, or take it out of [link] libs"),
    })
}
