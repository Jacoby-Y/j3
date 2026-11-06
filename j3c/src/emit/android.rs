//! Android targets: NDK lookup, the triple -> ABI map and the SDL lib dir, and `use lib` names.
//! A build links a shared library (`lib<name>.so`) that SDL's SDLActivity loads; its entry is
//! `SDL_main` (see `emit/llvm.rs`). Everything here is inert for non-Android triples.

use std::path::{Path, PathBuf};

/// The `--target=<triple><API_LEVEL>` level and the minimum the packaged app declares.
pub(crate) const API_LEVEL: u32 = 26;

/// NDK pieces a compile or link needs.
#[derive(Clone, Debug)]
pub(crate) struct Ndk {
    pub clang: PathBuf,
    pub sysroot: PathBuf,
}

/// Android ABI name (APK `lib/<abi>/`, SDL cache folder) for a triple.
pub(crate) fn abi(triple: &str) -> Option<&'static str> {
    let t = triple.to_ascii_lowercase();
    if t.starts_with("aarch64") {
        Some("arm64-v8a")
    } else if t.starts_with("x86_64") {
        Some("x86_64")
    } else {
        None
    }
}

/// Root of the SDL build cache: `J3_SDL_CACHE`, else `<home>/.cache/j3/android/sdl`.
pub(crate) fn sdl_root(env: Option<&str>, home: Option<&Path>) -> Option<PathBuf> {
    match env.filter(|s| !s.is_empty()) {
        Some(p) => Some(PathBuf::from(p)),
        None => Some(home?.join(".cache/j3/android/sdl")),
    }
}

/// `<root>/<abi>/lib`, where `tools/android/build-sdl.sh` puts the SDL libraries.
pub(crate) fn sdl_lib_dir(triple: &str, root: &Path) -> Option<PathBuf> {
    Some(root.join(abi(triple)?).join("lib"))
}

/// `<root>/<abi>/lib` of the OpenSSL cache (`J3_OPENSSL_CACHE`, else `~/.cache/j3/android/openssl`),
/// where `tools/android/build-openssl.sh` puts `libssl.so` and `libcrypto.so`.
pub(crate) fn openssl_lib_dir(triple: &str) -> Option<PathBuf> {
    let root = match std::env::var("J3_OPENSSL_CACHE").ok().filter(|s| !s.is_empty()) {
        Some(p) => PathBuf::from(p),
        None => PathBuf::from(std::env::var_os("HOME")?).join(".cache/j3/android/openssl"),
    };
    Some(root.join(abi(triple)?).join("lib"))
}

/// The Android SDK root: `ANDROID_HOME` when set and non-empty, else `<HOME>/Android/Sdk`.
/// None when neither applies. Every SDK lookup in j3c goes through this.
pub(crate) fn sdk_root() -> Option<PathBuf> {
    sdk_root_in(std::env::var("ANDROID_HOME").ok().as_deref(), std::env::var_os("HOME").map(PathBuf::from).as_deref())
}

/// [`sdk_root`] with the environment passed in, so the rule is testable.
fn sdk_root_in(android_home: Option<&str>, home: Option<&Path>) -> Option<PathBuf> {
    match android_home.filter(|s| !s.is_empty()) {
        Some(s) => Some(PathBuf::from(s)),
        None => Some(home?.join("Android/Sdk")),
    }
}

/// The NDK root: `ANDROID_NDK_HOME`, then `ANDROID_NDK_ROOT`, then the highest `<sdk>/ndk/*`
/// (`sdk` is [`sdk_root`]). An empty or invalid entry falls through to the next.
pub(crate) fn find_ndk(ndk_home: Option<&str>, ndk_root: Option<&str>, sdk: Option<&Path>) -> Option<PathBuf> {
    let valid = |p: &Path| prebuilt(p).is_some();
    for var in [ndk_home, ndk_root].into_iter().flatten().filter(|s| !s.is_empty()) {
        if valid(Path::new(var)) {
            return Some(PathBuf::from(var));
        }
    }
    // Highest first; an invalid newer folder is passed over, which `highest_version_dir` cannot do.
    version_dirs(&sdk?.join("ndk")).into_iter().rev().map(|(_, p)| p).find(|p| valid(p))
}

/// Subfolders of `parent` named as dotted numbers, lowest version first. Other names are skipped.
fn version_dirs(parent: &Path) -> Vec<(Vec<u64>, PathBuf)> {
    let mut v: Vec<(Vec<u64>, PathBuf)> = std::fs::read_dir(parent)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter_map(|p| {
            let key = p.file_name()?.to_str()?.split('.').map(|n| n.parse().ok()).collect::<Option<Vec<u64>>>()?;
            Some((key, p))
        })
        .collect();
    v.sort();
    v
}

/// The subfolder of `parent` with the highest dotted version, compared numerically
/// (`27.2.12479018` beats `27.1.1` and `9.0.0`). Non-version names are ignored.
pub(crate) fn highest_version_dir(parent: &Path) -> Option<PathBuf> {
    version_dirs(parent).pop().map(|(_, p)| p)
}

/// `<ndk>/toolchains/llvm/prebuilt/<host>`: the one host folder, whatever it is called.
fn prebuilt(ndk: &Path) -> Option<PathBuf> {
    let dir = ndk.join("toolchains/llvm/prebuilt");
    let mut hosts: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).collect();
    hosts.sort();
    hosts.into_iter().find(|h| h.join("bin").is_dir())
}

/// The NDK on this machine, looked up once from the environment.
pub(crate) fn ndk() -> Option<&'static Ndk> {
    static NDK: std::sync::OnceLock<Option<Ndk>> = std::sync::OnceLock::new();
    NDK.get_or_init(|| {
        let var = |k: &str| std::env::var(k).ok();
        let root = find_ndk(var("ANDROID_NDK_HOME").as_deref(), var("ANDROID_NDK_ROOT").as_deref(), sdk_root().as_deref())?;
        let host = prebuilt(&root)?;
        let exe = if cfg!(windows) { "clang.exe" } else { "clang" };
        Some(Ndk { clang: host.join("bin").join(exe), sysroot: host.join("sysroot") })
    })
    .as_ref()
}

/// SDL lib dir for `triple` on this machine (`J3_SDL_CACHE` overrides the root).
pub(crate) fn sdl_dir(triple: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let root = sdl_root(std::env::var("J3_SDL_CACHE").ok().as_deref(), home.as_deref())?;
    sdl_lib_dir(triple, &root)
}

/// Compile and link flags for an Android triple, in place of plain `--target=`.
/// With no NDK found only the target is named; [`ready`] reports the missing NDK first.
pub(crate) fn target_flags(triple: &str) -> Vec<String> {
    let mut v = vec![format!("--target={triple}{API_LEVEL}")];
    if let Some(n) = ndk() {
        v.push(format!("--sysroot={}", n.sysroot.display()));
    }
    v.push("-fPIC".to_string());
    v
}

/// The compiler for an Android triple: an explicit `[emit] clang`, else the NDK's.
/// `EmitOpts` fills in plain `clang` when the manifest names none, so that counts as unset.
pub(crate) fn compiler(explicit: &str) -> String {
    if !explicit.is_empty() && explicit != "clang" {
        return explicit.to_string();
    }
    ndk().map(|n| n.clang.display().to_string()).unwrap_or_else(|| "clang".to_string())
}

/// Link-only flags: a shared library with every symbol resolved at build time.
pub(crate) fn link_flags(triple: &str) -> Vec<String> {
    let mut v = vec!["-shared".to_string(), "-Wl,--no-undefined".to_string()];
    if let Some(d) = sdl_dir(triple) {
        v.push(format!("-L{}", d.display()));
    }
    v.extend(["-ldl", "-lm", "-llog"].map(String::from));
    v
}

/// `use lib` name on Android: desktop GL is GLES 3 there.
pub(crate) fn lib_name(name: &str) -> &str {
    match name {
        "GL" => "GLESv3",
        other => other,
    }
}

/// Fails with one line, before anything is built, when the NDK or the SDL cache is missing.
pub(crate) fn ready(triple: &str) -> Result<(), String> {
    if ndk().is_none() {
        return Err("no Android NDK found: set ANDROID_NDK_HOME or ANDROID_NDK_ROOT, or install one under $ANDROID_HOME/ndk (see `j3c doc android`)".into());
    }
    let abi = abi(triple).ok_or_else(|| format!("unsupported Android target `{triple}` (use aarch64-linux-android or x86_64-linux-android)"))?;
    let ok = sdl_dir(triple).map(|d| d.join("libSDL2.so").is_file()).unwrap_or(false);
    if !ok {
        return Err(format!("SDL for Android ({abi}) is not built: run tools/android/build-sdl.sh"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_ndk(root: &Path, ver: &str) -> PathBuf {
        let p = root.join("ndk").join(ver);
        std::fs::create_dir_all(p.join("toolchains/llvm/prebuilt/linux-x86_64/bin")).unwrap();
        p
    }

    #[test]
    fn triple_maps_to_abi_and_sdl_dir() {
        assert_eq!(abi("aarch64-linux-android"), Some("arm64-v8a"));
        assert_eq!(abi("x86_64-linux-android"), Some("x86_64"));
        assert_eq!(abi("armv7-linux-androideabi"), None);
        assert_eq!(
            sdl_lib_dir("aarch64-linux-android", Path::new("/c")),
            Some(PathBuf::from("/c/arm64-v8a/lib"))
        );
        assert_eq!(sdl_root(Some("/x"), Some(Path::new("/h"))), Some(PathBuf::from("/x")));
        assert_eq!(sdl_root(Some(""), Some(Path::new("/h"))), Some(PathBuf::from("/h/.cache/j3/android/sdl")));
    }

    #[test]
    fn ndk_lookup_order() {
        let tmp = std::env::temp_dir().join(format!("j3c_ndk_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let sdk = tmp.join("sdk");
        fake_ndk(&sdk, "9.0.1");
        let high = fake_ndk(&sdk, "27.2.12479018");
        fake_ndk(&sdk, "26.1.1");
        let home = tmp.join("h");
        let (a, b) = (fake_ndk(&tmp.join("a"), "1"), fake_ndk(&tmp.join("b"), "1"));
        let s = |p: &Path| p.display().to_string();
        // NDK_HOME wins, then NDK_ROOT, then the highest under ANDROID_HOME (numeric, so 27 > 9)
        assert_eq!(find_ndk(Some(&s(&a)), Some(&s(&b)), Some(sdk.as_path())), Some(a.clone()));
        assert_eq!(find_ndk(Some(""), Some(&s(&b)), Some(sdk.as_path())), Some(b.clone()));
        assert_eq!(find_ndk(Some("/nonexist"), None, Some(sdk.as_path())), Some(high));
        // ANDROID_HOME defaults to ~/Android/Sdk
        let def = fake_ndk(&home.join("Android/Sdk"), "5.0");
        assert_eq!(find_ndk(None, None, sdk_root_in(None, Some(&home)).as_deref()), Some(def));
        assert_eq!(find_ndk(None, None, sdk_root_in(Some("/nonexist"), Some(&home)).as_deref()), None);
        assert_eq!(find_ndk(None, None, sdk_root_in(Some(""), None).as_deref()), None);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn highest_version_dir_is_numeric_and_skips_names() {
        let tmp = std::env::temp_dir().join(format!("j3c_hvd_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        for v in ["9.0.0", "27.1.1", "27.2.12479018", "not-a-version"] {
            std::fs::create_dir_all(tmp.join(v)).unwrap();
        }
        std::fs::create_dir_all(tmp.join("only/abc")).unwrap();
        assert_eq!(highest_version_dir(&tmp), Some(tmp.join("27.2.12479018")));
        assert_eq!(highest_version_dir(&tmp.join("only")), None);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn gl_maps_to_gles() {
        assert_eq!(lib_name("GL"), "GLESv3");
        assert_eq!(lib_name("SDL2"), "SDL2");
        assert_eq!(lib_name("SDL2_ttf"), "SDL2_ttf");
    }
}
