//! `build_outputs = ["steam"]`: `target/dist/steam/content/<os>/` (the zip's app folder plus the
//! Steam API library) and `scripts/app_build_<appid>.vdf` for `steamcmd +run_app_build`.
//! j3c never uploads. `dev_env` is the `[steam]` env a dev run gets.

use std::fs;
use std::path::{Path, PathBuf};

use super::{copy, copy_all, platform};
use crate::project::Manifest;

/// `(os, depot id)` for a supported triple, after the checks below.
type Target = (&'static str, u32);

/// The OS a built triple ships as (`linux`, `windows`), else None with a warning.
fn os_of(triple: &str) -> Option<&'static str> {
    match platform(triple).as_str() {
        "linux-x86_64" => Some("linux"),
        "windows-x86_64" => Some("windows"),
        p => {
            eprintln!("j3c: dist: no steam content for {p} (linux-x86_64 and windows-x86_64 only)");
            None
        }
    }
}

/// Redistributable library path inside the SDK, per OS.
fn lib_path(sdk: &Path, os: &str) -> PathBuf {
    match os {
        "windows" => sdk.join("redistributable_bin/win64/steam_api64.dll"),
        _ => sdk.join("redistributable_bin/linux64/libsteam_api.so"),
    }
}

/// Check `[steam]` for the built triples before anything is copied, and clear the old output.
/// Returns the targets in `linux`, `windows` order.
pub(super) fn plan(m: &Manifest, built: &[(String, PathBuf)]) -> Result<Vec<Target>, String> {
    let s = &m.steam;
    let mut oses: Vec<&'static str> = built.iter().filter_map(|(t, _)| os_of(t)).collect();
    oses.sort_unstable();
    oses.dedup();
    if oses.is_empty() {
        return Ok(Vec::new());
    }
    if s.app_id.is_none() {
        return Err("[steam] app_id is missing (the numeric app id from the Steamworks partner site)".into());
    }
    let sdk = s.sdk.as_ref().ok_or("[steam] sdk is missing (path to the unzipped Steamworks SDK)")?;
    if let Some(d) = s.runtime_libs.as_ref().filter(|d| !d.is_dir()) {
        return Err(format!("[steam] runtime_libs: {} is not a folder", d.display()));
    }
    let mut out = Vec::new();
    for os in oses {
        let (key, depot) = if os == "linux" { ("depot_linux", s.depot_linux) } else { ("depot_windows", s.depot_windows) };
        let depot = depot.ok_or_else(|| format!("[steam] {key} is missing (a {os} target was built)"))?;
        let lib = lib_path(sdk, os);
        if !lib.is_file() {
            return Err(format!("[steam] sdk: {} is not a file (is `sdk` the SDK root with redistributable_bin/?)", lib.display()));
        }
        out.push((os, depot));
    }
    let _ = fs::remove_dir_all(steam_dir(m));
    Ok(out)
}

fn steam_dir(m: &Manifest) -> PathBuf {
    m.root.join("target/dist/steam")
}

/// Copy the finished app folder `app` to `content/<os>/` and add the Steam API library.
pub(super) fn stage(m: &Manifest, triple: &str, app: &Path) -> Result<(), String> {
    let Some(os) = os_of(triple) else { return Ok(()) };
    let to = steam_dir(m).join("content").join(os);
    copy_all(app, &to)?;
    if to.join("steam_appid.txt").exists() {
        eprintln!("j3c: dist: steam_appid.txt is in [dist] include; left out of the {os} depot (the store build must not carry it)");
        fs::remove_file(to.join("steam_appid.txt")).map_err(|e| format!("remove steam_appid.txt: {e}"))?;
    }
    let lib = lib_path(m.steam.sdk.as_deref().ok_or("[steam] sdk is missing")?, os);
    copy(&lib, &to.join(lib.file_name().expect("lib name")))?;
    if os == "linux" {
        linux_libs(m, &to)?;
    }
    Ok(())
}

/// Copy `[steam] runtime_libs` (`*.so*`) beside the Linux exe, then warn about any file in the
/// depot that needs a newer glibc than `glibc_max`.
fn linux_libs(m: &Manifest, to: &Path) -> Result<(), String> {
    if let Some(dir) = &m.steam.runtime_libs {
        let ents = fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
        for ent in ents.flatten() {
            if ent.file_name().to_string_lossy().contains(".so") && ent.path().is_file() {
                copy(&ent.path(), &to.join(ent.file_name()))?;
            }
        }
    }
    let max = crate::project::parse_glibc(&m.steam.glibc_max).expect("checked at parse");
    for ent in fs::read_dir(to).map_err(|e| format!("read {}: {e}", to.display()))?.flatten() {
        let p = ent.path();
        let n = ent.file_name().to_string_lossy().into_owned();
        // the exe has no extension; libsteam_api.so is Valve's own
        if !p.is_file() || n == "libsteam_api.so" || !is_elf(&p) {
            continue;
        }
        let Some(syms) = objdump_t(&p) else {
            eprintln!("j3c: dist: glibc check skipped (no objdump/llvm-objdump)");
            return Ok(());
        };
        if let Some(((a, b), sym)) = max_glibc(&syms).filter(|(v, _)| *v > max) {
            eprintln!(
                "j3c: dist: {n} needs GLIBC_{a}.{b} (symbol {sym}), above [steam] glibc_max {}; the sniper runtime may not run it. Run scripts/build-sniper.sh (podman, sniper SDK container)",
                m.steam.glibc_max
            );
        }
    }
    Ok(())
}

const SNIPER_SCRIPT: &str = r##"#!/usr/bin/env bash
# Build the game inside the Steam Linux Runtime 3.0 (sniper) SDK container (glibc 2.31), so the
# executable runs on sniper. The host j3c (newer glibc) cannot run there, so j3c is built in the
# container too, into j3c/target-sniper (your host build is untouched). Needs podman, network on
# first run (rustup, if the image has no cargo), and clang in the image (the sniper SDK ships it;
# verify). The game build overwrites ./target and this steam folder with the sniper-built ones.
# Run from the project root:   J3_SRC=/path/to/j3 target/dist/steam/scripts/build-sniper.sh
set -euo pipefail
J3_SRC="${J3_SRC:?set J3_SRC to the j3 repository (the folder that holds j3c/)}"
PROJECT="${PROJECT:-$PWD}"
IMAGE=registry.gitlab.steamos.cloud/steamrt/sniper/sdk
HOME_DIR="${SNIPER_HOME:-$HOME/.cache/j3-sniper-home}"
mkdir -p "$HOME_DIR"
exec podman run --rm --userns=keep-id -e HOME=/home/build   -v "$HOME_DIR":/home/build:Z -v "$J3_SRC":/j3:Z -v "$PROJECT":/work:Z -w /work   "$IMAGE" bash -euc '
    export PATH="$HOME/.cargo/bin:$PATH"
    if ! command -v cargo >/dev/null; then
      curl --proto =https --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
      export PATH="$HOME/.cargo/bin:$PATH"
    fi
    command -v clang >/dev/null || { echo "the image has no clang; j3c needs it" >&2; exit 1; }
    (cd /j3/j3c && J3C_NO_INSTALL=1 CARGO_TARGET_DIR=/j3/j3c/target-sniper cargo build --release)
    /j3/j3c/target-sniper/release/j3c build --release
  '
"##;

/// Whether `p` starts with the ELF magic.
fn is_elf(p: &Path) -> bool {
    use std::io::Read;
    let mut m = [0u8; 4];
    fs::File::open(p).and_then(|mut f| f.read_exact(&mut m)).is_ok() && m == *b"\x7fELF"
}

/// Undefined dynamic symbols of `bin` (`objdump -T`); None when no objdump runs.
fn objdump_t(bin: &Path) -> Option<String> {
    for tool in ["objdump", "llvm-objdump"] {
        if let Ok(out) = std::process::Command::new(tool).arg("-T").arg(bin).output() {
            if out.status.success() {
                return Some(String::from_utf8_lossy(&out.stdout).into_owned());
            }
        }
    }
    None
}

/// Highest `GLIBC_x.y` among the undefined symbols of `objdump -T` text, with the symbol.
fn max_glibc(text: &str) -> Option<((u32, u32), String)> {
    let mut best: Option<((u32, u32), String)> = None;
    for line in text.lines().filter(|l| l.contains("*UND*")) {
        let mut w = line.split_whitespace().rev();
        let (Some(sym), Some(ver)) = (w.next(), w.next()) else { continue };
        let Some(v) = ver.trim_matches(|c| c == '(' || c == ')').strip_prefix("GLIBC_") else { continue };
        let v = v.split('.').take(2).map(|x| x.parse::<u32>().ok()).collect::<Vec<_>>();
        if let [Some(a), Some(b)] = v[..] {
            if best.as_ref().is_none_or(|(old, _)| (a, b) > *old) {
                best = Some(((a, b), sym.to_string()));
            }
        }
    }
    best
}

/// Write `scripts/app_build_<appid>.vdf` and `scripts/README.txt`; returns the steam folder.
pub(super) fn finish(m: &Manifest, targets: &[Target]) -> Result<PathBuf, String> {
    let dir = steam_dir(m);
    let scripts = dir.join("scripts");
    fs::create_dir_all(&scripts).map_err(|e| format!("mkdir {}: {e}", scripts.display()))?;
    let id = m.steam.app_id.expect("checked by plan");
    let vdf = scripts.join(format!("app_build_{id}.vdf"));
    fs::write(&vdf, app_build_vdf(id, &m.steam.desc, m.steam.branch.as_deref().unwrap_or(""), targets))
        .map_err(|e| format!("write {}: {e}", vdf.display()))?;
    let abs = vdf.canonicalize().unwrap_or(vdf);
    let sh = scripts.join("build-sniper.sh");
    fs::write(&sh, SNIPER_SCRIPT).map_err(|e| format!("write {}: {e}", sh.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&sh, fs::Permissions::from_mode(0o755));
    }
    let readme = scripts.join("README.txt");
    fs::write(&readme, readme_text(&abs, &m.steam.glibc_max)).map_err(|e| format!("write {}: {e}", readme.display()))?;
    Ok(dir)
}

/// A VDF quoted string body: `\` and `"` escaped, line breaks flattened.
fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace(['\n', '\r'], " ")
}

fn app_build_vdf(app_id: u32, desc: &str, set_live: &str, targets: &[Target]) -> String {
    let mut s = format!(
        "\"AppBuild\"\n{{\n\t\"AppID\" \"{app_id}\"\n\t\"Desc\" \"{}\"\n\t\"ContentRoot\" \"../content/\"\n\t\"BuildOutput\" \"../output/\"\n\t\"SetLive\" \"{}\"\n\t\"Depots\"\n\t{{\n",
        esc(desc),
        esc(set_live)
    );
    for (os, depot) in targets {
        s.push_str(&format!(
            "\t\t\"{depot}\"\n\t\t{{\n\t\t\t\"FileMapping\" {{ \"LocalPath\" \"{os}/*\" \"DepotPath\" \".\" \"recursive\" \"1\" }}\n\t\t\t\"FileExclusion\" \"*.pdb\"\n\t\t}}\n"
        ));
    }
    s.push_str("\t}\n}\n");
    s
}

fn readme_text(vdf: &Path, glibc: &str) -> String {
    format!(
        "Steam upload for this build. j3c never uploads; you run steamcmd yourself:

    steamcmd +login <your-steam-user> +run_app_build \"{}\" +quit

Paths in the script are relative to it, so this whole folder can be moved. steamcmd needs
the Steamworks SDK's `steamcmd` and a build account with the \"Edit App Metadata\" and
\"Publish App Changes\" permissions. A build goes live only when [steam] branch is set.

Linux runtime: on the partner site (Installation > General Installation > Launch Options)
add a Linux entry with the executable and set its compatibility tool to \"Steam Linux Runtime
3.0 (sniper)\". Sniper is Debian 11 based (glibc 2.31), so the game and every .so beside it
must not need a newer glibc than [steam] glibc_max ({glibc}); j3c warned during the build if one
does. To build against sniper, run scripts/build-sniper.sh (next to this file; podman; it
builds j3c and then the game inside the sniper SDK container, which needs cargo or network for
rustup, and clang; verify). From the project root:
  J3_SRC=/path/to/j3 target/dist/steam/scripts/build-sniper.sh

Libraries a J3 game opens at run time (dlopen), what sniper is believed to provide, and what to do:
  libSDL2-2.0.so.0                         sniper provides it (linked)
  libSDL2_image / _mixer / _ttf-2.0.so.0   believed in sniper; VERIFY with a test install
  libX11.so.6 (clipboard, hotkey)          believed in sniper; VERIFY
  libssl.so.3 (j3.net.http, only if used)  sniper has OpenSSL 1.1 as far as known; VERIFY
Anything missing: build it inside the sniper SDK container, put the .so files (libssl.so.3,
libcrypto.so.3, the SDL2 add-ons, and any dependency not in sniper) in one folder, and set
[steam] runtime_libs = \"<folder>\". Every .so there is copied beside the executable, and J3 opens
libssl.so.3 and the SDL add-ons from the executable's folder before the system ones. Without a
library, the feature that needs it returns its empty/Err value; the game still starts.
j3c bundles nothing from the build host.
",
        vdf.display()
    )
}

/// Env a dev run of the project at `root` gets from `[steam]`: `SteamAppId`/`SteamGameId` (so no
/// `steam_appid.txt` is needed) and `J3_STEAM_LIB` from `sdk`. Anything already set is left alone.
pub(crate) fn dev_env(root: &Path) -> Vec<(&'static str, String)> {
    let Ok(text) = fs::read_to_string(root.join("j3.toml")) else { return Vec::new() };
    let get = |k: &str| crate::project::toml_str(&text, "steam", k);
    let mut out = Vec::new();
    let unset = |k: &str| std::env::var_os(k).is_none();
    if let Some(id) = get("app_id").filter(|i| i.parse::<u32>().is_ok_and(|n| n > 0)) {
        for k in ["SteamAppId", "SteamGameId"] {
            if unset(k) {
                out.push((k, id.clone()));
            }
        }
    }
    if let (true, Some(sdk)) = (unset("J3_STEAM_LIB"), get("sdk")) {
        let os = if cfg!(windows) { "windows" } else { "linux" };
        let lib = lib_path(&crate::project::resolve_path(root, &sdk), os);
        if lib.is_file() {
            out.push(("J3_STEAM_LIB", lib.to_string_lossy().into_owned()));
        }
    }
    out
}

#[cfg(test)]
mod tests;
