//! `[dist] build_outputs = ["zip", "deb"]`: after `j3c build --release`, ready-to-ship files
//! per target in `target/dist/`. A zip `<name>-<os>-<arch>.zip` holds a `<name>/` folder with
//! the program, the `[dist] include` files, and (Windows) the DLLs the `.exe` imports or `[link] libs`
//! names (SDL add-ons are opened at runtime), with their own imports, found in
//! the `[link] search` dirs, their `../bin`, or `~/.cache/j3/sdl2-mingw/dll`. A `.deb` (Linux
//! targets) installs that folder to `/opt/<name>` with a launcher, menu entry, and icon (`deb.rs`).
//! An `.apk` (`apk.rs`) packs every Android target's `lib<name>.so` with SDL and the assets.

pub mod apk;
mod deb;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::project::{Manifest, Output};

/// Package every built target. `built` is `(triple, exe path)`; the triple may be empty (host).
pub fn package(m: &Manifest, built: &[(String, PathBuf)]) -> Result<Vec<PathBuf>, String> {
    let dist = m.root.join("target/dist");
    fs::create_dir_all(&dist).map_err(|e| format!("mkdir {}: {e}", dist.display()))?;
    let include = include(m);
    let mut out = Vec::new();
    // One APK holds every Android triple; those triples have no zip/deb of their own.
    let android = |t: &str| crate::emit::host::target_os(t) == crate::emit::host::TargetOs::Android;
    if m.dist.outputs.contains(&Output::Apk) {
        let libs: Vec<(String, PathBuf)> = built.iter().filter(|(t, _)| android(t)).cloned().collect();
        out.push(apk::build(m, &libs, &include)?);
    }
    for (triple, exe) in built.iter().filter(|(t, _)| !android(t)) {
        let windows = triple.contains("windows");
        let exe = if windows && !exe.is_file() { exe.with_extension("exe") } else { exe.clone() };
        let stage = m.root.join("target/.j3c/dist").join(crate::project::target_slug(triple));
        let _ = fs::remove_dir_all(&stage);
        // The app folder sits where the `.deb` installs it, so both outputs share one copy.
        let opt = stage.join("root/opt");
        let dir = opt.join(&m.name);
        fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
        let file = exe.file_name().ok_or("no program name")?;
        copy(&exe, &dir.join(file))?;
        for item in &include {
            let src = m.root.join(item);
            if !src.exists() {
                return Err(format!("[dist] include: {} does not exist", src.display()));
            }
            copy_all(&src, &dir.join(item))?;
        }
        if windows {
            for dll in dlls(m, &exe) {
                let to = dir.join(dll.file_name().expect("dll name"));
                copy(&dll, &to)?;
                // SDL2_ttf.dll ships ~68 MB of debug info.
                for strip in ["x86_64-w64-mingw32-strip", "llvm-strip"] {
                    if Command::new(strip).arg("--strip-debug").arg(&to).status().map(|s| s.success()).unwrap_or(false) {
                        break;
                    }
                }
            }
        }
        for o in &m.dist.outputs {
            match o {
                Output::Zip => out.push(zip(m, &opt, &dist.join(format!("{}-{}.zip", m.name, platform(triple))))?),
                Output::Deb if platform(triple).starts_with("linux-") => {
                    out.push(deb::build(m, triple, &stage, &dir.join(file), &dist)?)
                }
                Output::Apk => {}
                Output::Deb => eprintln!("j3c: dist: no .deb for {} (Linux targets only)", platform(triple)),
            }
        }
        let _ = fs::remove_dir_all(&stage);
    }
    let _ = fs::remove_dir_all(m.root.join("target/.j3c/dist"));
    Ok(out)
}

/// Files and folders shipped beside the program (`[dist] include`, else whichever default dirs exist).
fn include(m: &Manifest) -> Vec<String> {
    if m.dist.include.is_empty() {
        ["assets", "scripts", "scenes", "locales"].iter().filter(|d| m.root.join(d).exists()).map(|d| d.to_string()).collect()
    } else {
        m.dist.include.clone()
    }
}

/// Zip the `<name>/` folder inside `parent`.
fn zip(m: &Manifest, parent: &Path, zip: &Path) -> Result<PathBuf, String> {
    let _ = fs::remove_file(zip);
    let out = Command::new("zip")
        .arg("-qr")
        .arg(zip)
        .arg(&m.name)
        .current_dir(parent)
        .output()
        .map_err(|e| format!("run zip: {e} (install `zip`)"))?;
    if !out.status.success() {
        return Err(format!("zip failed:\n{}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(zip.to_path_buf())
}

/// `linux-x86_64`, `windows-x86_64`, `macos-arm64`; the host when the triple is empty.
fn platform(triple: &str) -> String {
    let (os, arch) = if triple.is_empty() {
        (std::env::consts::OS.to_string(), std::env::consts::ARCH.to_string())
    } else {
        let t = triple.to_ascii_lowercase();
        let os = if t.contains("windows") {
            "windows"
        } else if t.contains("apple") || t.contains("darwin") {
            "macos"
        } else if t.contains("android") {
            "android"
        } else if t.contains("linux") {
            "linux"
        } else {
            t.split('-').nth(2).unwrap_or("unknown")
        };
        (os.to_string(), t.split('-').next().unwrap_or("").to_string())
    };
    format!("{os}-{arch}")
}

/// DLLs `exe` imports that live outside Windows itself, following their own imports too.
fn dlls(m: &Manifest, exe: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for s in &m.link.search {
        let p = if Path::new(s).is_absolute() { PathBuf::from(s) } else { m.root.join(s) };
        dirs.push(p.join("../bin"));
        dirs.push(p);
    }
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".cache/j3/sdl2-mingw/dll"));
    }
    let mut found: Vec<PathBuf> = Vec::new();
    // `[link] libs` too: SDL_image/mixer/ttf are opened at runtime (`game.j3` `soTry`), not imported.
    let mut todo: Vec<String> = m.link.libs.iter().filter_map(|l| crate::emit::host::windows_lib(l)).map(|l| format!("{l}.dll")).collect();
    todo.extend(imports(exe));
    while let Some(name) = todo.pop() {
        if found.iter().any(|f| f.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(&name))) {
            continue;
        }
        match dirs.iter().map(|d| d.join(&name)).find(|p| p.is_file()) {
            Some(p) => {
                todo.extend(imports(&p));
                found.push(p);
            }
            None if !system_dll(&name) => eprintln!("j3c: dist: {name} not found; ship it beside the .exe yourself"),
            None => {}
        }
    }
    found
}

fn imports(bin: &Path) -> Vec<String> {
    objdump_p(bin, "DLL Name:")
}

/// Values of `objdump -p` lines starting with `key` (`DLL Name:` on PE, `NEEDED` on ELF).
/// Empty when no objdump runs.
fn objdump_p(bin: &Path, key: &str) -> Vec<String> {
    for tool in ["x86_64-w64-mingw32-objdump", "llvm-objdump", "objdump"] {
        if let Ok(out) = Command::new(tool).arg("-p").arg(bin).output() {
            if out.status.success() {
                return String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .filter_map(|l| l.trim().strip_prefix(key).map(|n| n.trim().to_string()))
                    .collect();
            }
        }
    }
    Vec::new()
}

fn system_dll(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with("api-ms-win-")
        || [
            "kernel32.dll", "user32.dll", "gdi32.dll", "opengl32.dll", "msvcrt.dll", "ucrtbase.dll",
            "ws2_32.dll", "advapi32.dll", "shell32.dll", "ole32.dll", "oleaut32.dll", "winmm.dll",
            "imm32.dll", "version.dll", "setupapi.dll", "bcrypt.dll", "cfgmgr32.dll", "dinput8.dll",
            "dsound.dll", "dxgi.dll", "d3d11.dll", "hid.dll", "comdlg32.dll", "shlwapi.dll", "usp10.dll",
            "rpcrt4.dll",
        ]
        .contains(&n.as_str())
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    fs::copy(from, to).map(|_| ()).map_err(|e| format!("copy {} to {}: {e}", from.display(), to.display()))
}

/// Copy a file or a whole folder (following symlinks).
fn copy_all(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    if !from.is_dir() {
        return copy(from, to);
    }
    fs::create_dir_all(to).map_err(|e| format!("mkdir {}: {e}", to.display()))?;
    let entries = fs::read_dir(from).map_err(|e| format!("read {}: {e}", from.display()))?;
    for ent in entries {
        let ent = ent.map_err(|e| format!("read {}: {e}", from.display()))?;
        copy_all(&ent.path(), &to.join(ent.file_name()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_names() {
        assert_eq!(platform("x86_64-pc-linux-gnu"), "linux-x86_64");
        assert_eq!(platform("x86_64-pc-windows-gnu"), "windows-x86_64");
        assert_eq!(platform("arm64-apple-darwin"), "macos-arm64");
    }
}
