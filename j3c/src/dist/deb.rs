//! `[dist] build_outputs = ["deb"]`: `target/dist/<pkg>_<version>_<arch>.deb`, built without
//! dpkg (system `tar` plus a hand-written `ar`), so any Unix host can make one.
//!
//! Layout: the app folder at `/opt/<name>/`, a launcher `/usr/bin/<name>`, a menu entry
//! `/usr/share/applications/<name>.desktop`, and the `[dist] icon` under `hicolor` / `pixmaps`.
//! Games read data and write saves relative to the working directory, and `/opt` is read-only,
//! so the launcher runs the program from `~/.local/share/<name>`, which links to the installed files.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::project::Manifest;

/// Sizes the hicolor theme looks in; any other size goes to `pixmaps` by absolute path.
const HICOLOR: [u32; 13] = [16, 22, 24, 32, 36, 48, 64, 72, 96, 128, 192, 256, 512];

/// Build the `.deb` from `stage/root/opt/<name>/` (already holding the program `exe`).
pub(super) fn build(m: &Manifest, triple: &str, stage: &Path, exe: &Path, dist: &Path) -> Result<PathBuf, String> {
    let root = stage.join("root");
    let name = &m.name;
    let program = exe.file_name().ok_or("no program name")?.to_string_lossy().to_string();
    set_mode(exe, 0o755)?;

    write(&root.join("usr/bin").join(name), &launcher(name, &program), 0o755)?;

    let icon = match &m.dist.icon {
        Some(rel) => install_icon(&m.root.join(rel), name, &root)?,
        None => "applications-games".to_string(),
    };
    write(&root.join("usr/share/applications").join(format!("{name}.desktop")), &desktop(m, &icon), 0o644)?;

    let pkg = package_name(name);
    let arch = deb_arch(triple);
    let control = stage.join("control");
    let control_text = format!(
        "Package: {pkg}\nVersion: {}\nArchitecture: {arch}\nMaintainer: {}\nInstalled-Size: {}\n{}Section: games\nPriority: optional\nDescription: {}\n",
        m.version,
        maintainer(m),
        (tree_size(&root) + 1023) / 1024,
        match depends(m, exe).join(", ") {
            d if d.is_empty() => String::new(),
            d => format!("Depends: {d}\n"),
        },
        m.description.lines().next().unwrap_or(name),
    );
    write(&control.join("control"), &control_text, 0o644)?;

    normalize_modes(&root)?;
    let control_tar = stage.join("control.tar.gz");
    let data_tar = stage.join("data.tar.gz");
    tar(&control, &control_tar)?;
    tar(&root, &data_tar)?;
    let deb = dist.join(format!("{pkg}_{}_{arch}.deb", m.version));
    let mut bytes = Vec::new();
    ar(&mut bytes, &[
        ("debian-binary", b"2.0\n".to_vec()),
        ("control.tar.gz", read(&control_tar)?),
        ("data.tar.gz", read(&data_tar)?),
    ]);
    fs::write(&deb, bytes).map_err(|e| format!("write {}: {e}", deb.display()))?;
    Ok(deb)
}

/// `/usr/bin/<name>`: run from a per-user folder that links to `/opt/<name>`, so relative
/// reads find the data and writes (saves, settings) land in the user's folder.
fn launcher(name: &str, program: &str) -> String {
    format!(
        r#"#!/bin/sh
# {name} reads data and writes saves relative to its working directory, so run it from
# a per-user folder that links to the installed files.
set -e
app=/opt/{name}
dir="${{XDG_DATA_HOME:-$HOME/.local/share}}/{name}"
mkdir -p "$dir"
for f in "$app"/*; do ln -sfn "$f" "$dir/${{f##*/}}"; done
cd "$dir"
export SDL_VIDEO_X11_WMCLASS={name} SDL_VIDEO_WAYLAND_WMCLASS={name}
exec "$app/{program}" "$@"
"#
    )
}

fn desktop(m: &Manifest, icon: &str) -> String {
    let title = if m.dist.title.is_empty() { title_case(&m.name) } else { m.dist.title.clone() };
    format!(
        "[Desktop Entry]\nType=Application\nName={title}\nComment={}\nExec=/usr/bin/{name}\nIcon={icon}\nTerminal=false\nCategories={}\nStartupWMClass={name}\n",
        m.description.lines().next().unwrap_or(""),
        m.dist.categories,
        name = m.name,
    )
}

/// Copy the icon into the theme folders; returns the desktop file's `Icon=` value.
fn install_icon(src: &Path, name: &str, root: &Path) -> Result<String, String> {
    let bytes = read(src).map_err(|e| format!("[dist] icon: {e}"))?;
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let share = root.join("usr/share");
    match ext.as_str() {
        "svg" => {
            write_bytes(&share.join("icons/hicolor/scalable/apps").join(format!("{name}.svg")), &bytes)?;
            Ok(name.to_string())
        }
        "png" => {
            let (w, h) = png_size(&bytes).ok_or_else(|| format!("[dist] icon: {} is not a PNG", src.display()))?;
            let pixmap = share.join("pixmaps").join(format!("{name}.png"));
            write_bytes(&pixmap, &bytes)?;
            if w == h && HICOLOR.contains(&w) {
                write_bytes(&share.join(format!("icons/hicolor/{w}x{h}/apps/{name}.png")), &bytes)?;
                Ok(name.to_string())
            } else {
                eprintln!("j3c: dist: icon is {w}x{h}; a square 256x256 or 512x512 PNG looks best in menus");
                Ok(format!("/usr/share/pixmaps/{name}.png"))
            }
        }
        _ => Err(format!("[dist] icon: {} must be a .png or .svg", src.display())),
    }
}

fn png_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 24 || &b[..8] != b"\x89PNG\r\n\x1a\n" || &b[12..16] != b"IHDR" {
        return None;
    }
    let be = |i: usize| u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    Some((be(16), be(20)))
}

/// Debian package names: lowercase `[a-z0-9+.-]`, `_` becomes `-`.
fn package_name(name: &str) -> String {
    name.to_ascii_lowercase()
        .replace('_', "-")
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "+-.".contains(*c))
        .collect()
}

/// Debian architecture for a target triple (the host when empty).
fn deb_arch(triple: &str) -> String {
    let t = if triple.is_empty() { std::env::consts::ARCH } else { triple.split('-').next().unwrap_or("") };
    match t {
        "x86_64" => "amd64",
        "aarch64" | "arm64" => "arm64",
        "i386" | "i586" | "i686" | "x86" => "i386",
        "armv7" | "armv7l" | "arm" => "armhf",
        "riscv64" | "riscv64gc" => "riscv64",
        other => other,
    }
    .to_string()
}

fn title_case(name: &str) -> String {
    name.split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn maintainer(m: &Manifest) -> String {
    if !m.dist.maintainer.is_empty() {
        return m.dist.maintainer.clone();
    }
    if let (Ok(n), Ok(e)) = (std::env::var("DEBFULLNAME"), std::env::var("DEBEMAIL")) {
        return format!("{n} <{e}>");
    }
    let git = |key: &str| {
        let out = Command::new("git").args(["config", key]).current_dir(&m.root).output().ok()?;
        let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (out.status.success() && !v.is_empty()).then_some(v)
    };
    match (git("user.name"), git("user.email")) {
        (Some(n), Some(e)) => format!("{n} <{e}>"),
        _ => format!("{} <nobody@localhost>", m.name),
    }
}

/// Debian packages for the libraries the program loads (`objdump -p` NEEDED, else
/// `[link] libs`), then `[dist] depends`.
fn depends(m: &Manifest, exe: &Path) -> Vec<String> {
    let needed = super::objdump_p(exe, "NEEDED");
    let libs: Vec<String> = if needed.is_empty() { m.link.libs.clone() } else { needed.iter().map(|s| lib_stem(s)).collect() };
    let mut out: Vec<String> = Vec::new();
    for lib in &libs {
        match lib_package(lib) {
            Some(p) if !out.iter().any(|o| o == p) => out.push(p.to_string()),
            Some(_) => {}
            None => eprintln!("j3c: dist: no Debian package known for `{lib}`; add it to [dist] depends"),
        }
    }
    for d in &m.dist.depends {
        if !out.contains(d) {
            out.push(d.clone());
        }
    }
    out
}

/// `libSDL2_ttf-2.0.so.0` → `SDL2_ttf`, `libGL.so.1` → `GL`.
fn lib_stem(soname: &str) -> String {
    let s = soname.strip_prefix("lib").unwrap_or(soname);
    let s = s.split(".so").next().unwrap_or(s);
    s.strip_suffix("-2.0").unwrap_or(s).to_string()
}

fn lib_package(lib: &str) -> Option<&'static str> {
    Some(match lib {
        "SDL2" => "libsdl2-2.0-0",
        "SDL2_image" => "libsdl2-image-2.0-0",
        "SDL2_mixer" => "libsdl2-mixer-2.0-0",
        "SDL2_ttf" => "libsdl2-ttf-2.0-0",
        "GL" => "libgl1",
        "c" | "m" | "pthread" | "dl" | "rt" => "libc6",
        l if l.starts_with("ld-linux") => "libc6",
        _ => return None,
    })
}

/// `tar -czf out -C dir .` with every file owned by root (GNU tar, else bsdtar flags).
fn tar(dir: &Path, out: &Path) -> Result<(), String> {
    let mut err = String::new();
    for owner in [&["--owner=0", "--group=0", "--numeric-owner"][..], &["--uid", "0", "--gid", "0"][..]] {
        let r = Command::new("tar").args(owner).arg("-czf").arg(out).arg("-C").arg(dir).arg(".").output();
        match r {
            Ok(o) if o.status.success() => return Ok(()),
            Ok(o) => err = String::from_utf8_lossy(&o.stderr).to_string(),
            Err(e) => return Err(format!("run tar: {e} (install `tar`)")),
        }
    }
    Err(format!("tar failed:\n{err}"))
}

/// A `.deb` is an `ar` archive: `!<arch>\n`, then per member a 60-byte header and the data
/// padded to an even length.
fn ar(out: &mut Vec<u8>, members: &[(&str, Vec<u8>)]) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    out.extend_from_slice(b"!<arch>\n");
    for (name, data) in members {
        out.extend_from_slice(format!("{name:<16}{now:<12}{:<6}{:<6}{:<8}{:<10}`\n", 0, 0, "100644", data.len()).as_bytes());
        out.extend_from_slice(data);
        if data.len() % 2 == 1 {
            out.push(b'\n');
        }
    }
}

/// Installed files are root's: folders and programs 0755, everything else 0644 (not the
/// builder's umask, which may leave them group-writable).
fn normalize_modes(p: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = fs::metadata(p).map_err(|e| format!("stat {}: {e}", p.display()))?;
        if meta.is_dir() {
            set_mode(p, 0o755)?;
            for ent in fs::read_dir(p).map_err(|e| format!("read {}: {e}", p.display()))?.flatten() {
                normalize_modes(&ent.path())?;
            }
        } else {
            set_mode(p, if meta.permissions().mode() & 0o111 != 0 { 0o755 } else { 0o644 })?;
        }
    }
    let _ = p;
    Ok(())
}

fn tree_size(p: &Path) -> u64 {
    match fs::read_dir(p) {
        Ok(entries) => entries.flatten().map(|e| tree_size(&e.path())).sum(),
        Err(_) => fs::metadata(p).map(|m| m.len()).unwrap_or(0),
    }
}

fn read(p: &Path) -> Result<Vec<u8>, String> {
    fs::read(p).map_err(|e| format!("read {}: {e}", p.display()))
}

fn write_bytes(p: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    }
    fs::write(p, bytes).map_err(|e| format!("write {}: {e}", p.display()))
}

fn write(p: &Path, text: &str, mode: u32) -> Result<(), String> {
    write_bytes(p, text.as_bytes())?;
    set_mode(p, mode)
}

fn set_mode(p: &Path, mode: u32) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(p, fs::Permissions::from_mode(mode)).map_err(|e| format!("chmod {}: {e}", p.display()))?;
    }
    let _ = (p, mode);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_arches() {
        assert_eq!(package_name("Blocks_Adventure!"), "blocks-adventure");
        assert_eq!(title_case("blocks-adventure_2"), "Blocks Adventure 2");
        assert_eq!(deb_arch("x86_64-pc-linux-gnu"), "amd64");
        assert_eq!(deb_arch("aarch64-unknown-linux-gnu"), "arm64");
        assert_eq!(lib_stem("libSDL2_ttf-2.0.so.0"), "SDL2_ttf");
        assert_eq!(lib_stem("libGL.so.1"), "GL");
        assert_eq!(lib_package(&lib_stem("libSDL2-2.0.so.0")), Some("libsdl2-2.0-0"));
        assert_eq!(lib_package(&lib_stem("ld-linux-x86-64.so.2")), Some("libc6"));
    }

    #[test]
    fn ar_layout() {
        let mut b = Vec::new();
        ar(&mut b, &[("debian-binary", b"2.0\n".to_vec()), ("x", b"abc".to_vec())]);
        assert!(b.starts_with(b"!<arch>\ndebian-binary   "));
        assert_eq!(&b[8 + 58..8 + 60], b"`\n");
        assert_eq!(&b[8 + 48..8 + 58], b"4         ");
        // second member starts after header + 4 bytes; its 3 bytes get one pad byte
        assert_eq!(b.len(), 8 + 60 + 4 + 60 + 3 + 1);
    }

    #[test]
    fn png_header() {
        let png = crate::png::encode_rgba(2, 3, &[0; 24]);
        assert_eq!(png_size(&png), Some((2, 3)));
        assert_eq!(png_size(b"nope"), None);
    }
}
