//! `[dist] build_outputs = ["apk"]`: `target/dist/<name>-<version>.apk`, a debug-signed APK of
//! every Android triple in `[emit] target`, built with the Android SDK's command line tools
//! (no Gradle): `javac` + `d8` for SDL's Java and a 3-line `J3Activity`, `aapt2` for the manifest
//! and icon, `zip` to add the dex, native libraries and assets, then `zipalign` and `apksigner`.
//!
//! Layout: `lib/<abi>/libmain.so` (the project's `lib<name>.so`) beside the four SDL libraries,
//! and `assets/` holding the `[dist] include` folders plus `j3assets.txt`, the list `j3.apkassets`
//! copies to the app's storage on first start (format: `stdlib/j3/apkassets.j3`). Its stamp is a
//! hash of every listed path and its bytes, so any asset change recopies on the device.
//! Tools come from `$ANDROID_HOME` (default `~/Android/Sdk`): the highest `build-tools/*` and
//! `platforms/android-35/android.jar`. Temp files live in `target/.j3c/apk` and are removed.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::emit::android;
use crate::project::Manifest;

const TARGET_SDK: u32 = 35;
/// The libraries `J3Activity.getLibraries()` loads, in order; `main` is the project's.
const SDL_LIBS: [&str; 4] = ["SDL2", "SDL2_image", "SDL2_mixer", "SDL2_ttf"];

/// Where the APK of `m` is written: `target/dist/<name>-<version>.apk`.
pub fn path(m: &Manifest) -> PathBuf {
    m.root.join("target/dist").join(format!("{}-{}.apk", m.name, m.version))
}

/// Build the APK from `libs` (`(triple, lib<name>.so)` for each Android triple); `include` are the
/// asset folders (see `dist::include`).
pub(super) fn build(m: &Manifest, libs: &[(String, PathBuf)], include: &[String]) -> Result<PathBuf, String> {
    if libs.is_empty() {
        return Err("apk: [dist] build_outputs has `apk` but [emit] target has no Android triple: add \"aarch64-linux-android\" to [emit] target".into());
    }
    let tools = Tools::find()?;
    let code = version_code(&m.version)?;
    // Absolute: `zip` runs inside the staged root, so a relative path would resolve there.
    let stage = std::path::absolute(m.root.join("target/.j3c/apk")).map_err(|e| format!("apk: stage dir: {e}"))?;
    let _ = fs::remove_dir_all(&stage);
    let res = build_in(m, libs, include, &tools, code, &stage);
    let _ = fs::remove_dir_all(&stage);
    res
}

fn build_in(m: &Manifest, libs: &[(String, PathBuf)], include: &[String], t: &Tools, code: u32, stage: &Path) -> Result<PathBuf, String> {
    let pkg = &m.android.package;
    let root = stage.join("root");
    let (gen, classes, dex, res) = (stage.join("gen"), stage.join("classes"), stage.join("dex"), stage.join("res"));
    for d in [&root, &classes, &dex, &res] {
        mkdir(d)?;
    }

    // Java: SDL's sources plus the activity -> classes.dex.
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let sdl_root = android::sdl_root(std::env::var("J3_SDL_CACHE").ok().as_deref(), home.as_deref()).ok_or("apk: HOME is not set")?;
    let sdl_java = sdl_root.join("java/org/libsdl/app");
    if !sdl_java.is_dir() {
        return Err(format!("apk: SDL's Java sources are missing ({}): run tools/android/build-sdl.sh", sdl_java.display()));
    }
    let act_dir = gen.join(pkg.replace('.', "/"));
    let service = m.android.foreground_service.as_deref();
    write(&act_dir.join("J3Activity.java"), &activity_java(pkg, service.is_some()))?;
    write(&act_dir.join("J3Service.java"), &service_java(pkg, service))?;
    let mut sources = vec![act_dir.join("J3Activity.java"), act_dir.join("J3Service.java")];
    let mut sdl: Vec<PathBuf> = list(&sdl_java)?.into_iter().filter(|p| p.extension().is_some_and(|e| e == "java")).collect();
    sdl.sort();
    sources.extend(sdl);
    let mut javac = Command::new(java_tool("javac"));
    javac.args(["--release", "8", "-nowarn", "-Xlint:-options", "-cp"]).arg(&t.jar).arg("-d").arg(&classes).args(&sources);
    run(javac, "javac", "install a JDK 17 or newer, or set JAVA_HOME")?;
    let mut class_files = Vec::new();
    walk(&classes, &mut class_files)?;
    class_files.sort();
    let mut d8 = Command::new(t.bin.join("d8"));
    d8.args(["--release", "--min-api"]).arg(android::API_LEVEL.to_string()).arg("--lib").arg(&t.jar).arg("--output").arg(&dex).args(&class_files);
    run(d8, "d8", "needs a JDK on PATH or JAVA_HOME")?;
    fs::copy(dex.join("classes.dex"), root.join("classes.dex")).map_err(|e| format!("copy classes.dex: {e}"))?;

    // aapt2: manifest (+ icon) -> base.apk
    let icon = match &m.dist.icon {
        Some(rel) if rel.to_ascii_lowercase().ends_with(".png") => {
            let src = m.root.join(rel);
            let to = res.join("mipmap/ic_launcher.png");
            mkdir(to.parent().expect("res dir"))?;
            fs::copy(&src, &to).map_err(|e| format!("[dist] icon: copy {}: {e}", src.display()))?;
            true
        }
        Some(rel) => {
            eprintln!("j3c: dist: the APK icon must be a .png; {rel} is left out");
            false
        }
        None => {
            // No `[dist] icon`: the J3 logo.
            let to = res.join("mipmap/ic_launcher.png");
            mkdir(to.parent().expect("res dir"))?;
            fs::write(&to, super::LOGO_PNG).map_err(|e| format!("write {}: {e}", to.display()))?;
            true
        }
    };
    let manifest = stage.join("AndroidManifest.xml");
    write(&manifest, &manifest_xml(m, code, icon))?;
    let base = stage.join("base.apk");
    let mut link = Command::new(t.bin.join("aapt2"));
    link.arg("link").arg("-I").arg(&t.jar).arg("--manifest").arg(&manifest).arg("-o").arg(&base);
    if icon {
        let compiled = stage.join("res.zip");
        let mut c = Command::new(t.bin.join("aapt2"));
        c.arg("compile").arg("--dir").arg(&res).arg("-o").arg(&compiled);
        run(c, "aapt2 compile", "")?;
        link.arg(&compiled);
    }
    run(link, "aapt2 link", "")?;

    // Native libraries and assets, then added to base.apk (aapt2's own entries stay as they are).
    for (triple, so) in libs {
        let abi = android::abi(triple).ok_or_else(|| format!("apk: unsupported Android target `{triple}` (use aarch64-linux-android or x86_64-linux-android)"))?;
        let dir = root.join("lib").join(abi);
        mkdir(&dir)?;
        copy(so, &dir.join("libmain.so"), "build it first: the .so is missing")?;
        let from = android::sdl_lib_dir(triple, &sdl_root).expect("abi checked");
        for lib in SDL_LIBS {
            let name = format!("lib{lib}.so");
            copy(&from.join(&name), &dir.join(&name), "run tools/android/build-sdl.sh")?;
        }
        if uses_tls(so)? {
            let ossl = android::openssl_lib_dir(triple).ok_or("apk: HOME is not set")?;
            for name in ["libssl.so", "libcrypto.so"] {
                copy(&ossl.join(name), &dir.join(name), "the app uses j3.net.tls: run tools/android/build-openssl.sh")?;
            }
        }
        if let Some(extra) = &m.android.native_libs {
            for so in native_libs(&m.root.join(extra).join(abi))? {
                let name = so.file_name().expect("file").to_owned();
                copy(&so, &dir.join(name), "")?;
            }
        }
    }
    let assets = root.join("assets");
    mkdir(&assets)?;
    let files = collect_assets(&m.root, include)?;
    let mut stamp = Fnv::new();
    for (rel, path) in &files {
        let bytes = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        stamp.add(rel.as_bytes());
        stamp.add(&(bytes.len() as u64).to_le_bytes());
        stamp.add(&bytes);
        let to = assets.join(rel);
        mkdir(to.parent().expect("asset dir"))?;
        fs::write(&to, &bytes).map_err(|e| format!("write {}: {e}", to.display()))?;
    }
    let rels: Vec<&str> = files.iter().map(|(r, _)| r.as_str()).collect();
    write(&assets.join("j3assets.txt"), &j3assets(&rels, &stamp.hex()))?;
    let mut zip = Command::new("zip");
    zip.args(["-q", "-X", "-r"]).arg(&base).args(["classes.dex", "lib", "assets"]).current_dir(&root);
    run(zip, "zip", "install `zip`")?;

    // zipalign must come after every entry is added, signing last.
    let aligned = stage.join("aligned.apk");
    let mut za = Command::new(t.bin.join("zipalign"));
    za.args(["-p", "-f", "4"]).arg(&base).arg(&aligned);
    run(za, "zipalign", "")?;
    let ks = keystore()?;
    let out = path(m);
    let _ = fs::remove_file(&out);
    let mut sign = Command::new(t.bin.join("apksigner"));
    sign.arg("sign").arg("--ks").arg(&ks).args(["--ks-key-alias", "androiddebugkey", "--ks-pass", "pass:android", "--key-pass", "pass:android"])
        .arg("--out").arg(&out).arg(&aligned);
    run(sign, "apksigner", "needs a JDK on PATH or JAVA_HOME")?;
    let _ = fs::remove_file(out.with_extension("apk.idsig"));
    Ok(out)
}

/// `[android] native_libs`: the `lib*.so` files in `<dir>/<abi>` (none when that folder is missing).
/// Android only extracts `lib*.so` names, and `libmain.so` or an SDL name would replace ours.
fn native_libs(dir: &Path) -> Result<Vec<PathBuf>, String> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for p in list(dir)? {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let taken = name == "libmain.so" || SDL_LIBS.iter().any(|l| name == format!("lib{l}.so"));
        if !p.is_file() || !name.starts_with("lib") || !name.ends_with(".so") || taken {
            return Err(format!("apk: [android] native_libs: {} must be a lib<name>.so file, not libmain.so or an SDL library", p.display()));
        }
        out.push(p);
    }
    out.sort();
    Ok(out)
}

/// SDK tools: `bin` is the highest `build-tools/<version>`, `jar` is `android-35/android.jar`.
struct Tools {
    bin: PathBuf,
    jar: PathBuf,
}

impl Tools {
    fn find() -> Result<Tools, String> {
        let sdk = android::sdk_root().ok_or("apk: HOME is not set")?;
        let bin = android::highest_version_dir(&sdk.join("build-tools")).filter(|b| b.join("aapt2").is_file()).ok_or_else(|| {
            format!("apk: no Android build-tools under {}: install them (sdkmanager \"build-tools;35.0.0\") or set ANDROID_HOME", sdk.display())
        })?;
        let jar = sdk.join(format!("platforms/android-{TARGET_SDK}/android.jar"));
        if !jar.is_file() {
            return Err(format!("apk: {} is missing: sdkmanager \"platforms;android-{TARGET_SDK}\"", jar.display()));
        }
        Ok(Tools { bin, jar })
    }
}

/// `$JAVA_HOME/bin/<name>` when it exists, else `<name>` from PATH.
fn java_tool(name: &str) -> PathBuf {
    match std::env::var_os("JAVA_HOME").map(|h| PathBuf::from(h).join("bin").join(name)) {
        Some(p) if p.is_file() => p,
        _ => PathBuf::from(name),
    }
}

/// `~/.android/debug.keystore`, created with `keytool` when missing.
fn keystore() -> Result<PathBuf, String> {
    let ks = PathBuf::from(std::env::var_os("HOME").ok_or("apk: HOME is not set")?).join(".android/debug.keystore");
    if !ks.is_file() {
        mkdir(ks.parent().expect("keystore dir"))?;
        let mut k = Command::new(java_tool("keytool"));
        k.args(["-genkeypair", "-noprompt", "-keystore"]).arg(&ks).args([
            "-alias", "androiddebugkey", "-storepass", "android", "-keypass", "android", "-keyalg", "RSA", "-keysize", "2048",
            "-validity", "10000", "-dname", "CN=Android Debug,O=Android,C=US",
        ]);
        run(k, "keytool", "install a JDK, or set JAVA_HOME")?;
    }
    Ok(ks)
}

/// Run `cmd` without stdin, capturing output; failure shows both streams. `fix` is added to a spawn error.
fn run(mut cmd: Command, what: &str, fix: &str) -> Result<(), String> {
    cmd.stdin(Stdio::null());
    let out = cmd.output().map_err(|e| format!("apk: run {what}: {e}{}", if fix.is_empty() { String::new() } else { format!(" ({fix})") }))?;
    if out.status.success() {
        return Ok(());
    }
    Err(format!("apk: {what} failed ({}):\n{}{}", out.status, String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

fn mkdir(d: &Path) -> Result<(), String> {
    fs::create_dir_all(d).map_err(|e| format!("mkdir {}: {e}", d.display()))
}

fn write(p: &Path, text: &str) -> Result<(), String> {
    mkdir(p.parent().expect("parent"))?;
    fs::write(p, text).map_err(|e| format!("write {}: {e}", p.display()))
}

fn copy(from: &Path, to: &Path, fix: &str) -> Result<(), String> {
    fs::copy(from, to).map(|_| ()).map_err(|e| format!("apk: copy {}: {e} ({fix})", from.display()))
}

fn list(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let rd = fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
    Ok(rd.flatten().map(|e| e.path()).collect())
}

/// Every file under `dir`, following symlinks.
fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for p in list(dir)? {
        if p.is_dir() {
            walk(&p, out)?;
        } else {
            out.push(p);
        }
    }
    Ok(())
}

/// `versionCode`: `major * 1_000_000 + minor * 1_000 + patch` from the leading digits of the first
/// three dot parts (missing parts are 0; `1.2.3-beta` is 1002003). Minor and patch must be below
/// 1000 and major below 2000, so the code stays under Android's 2_100_000_000 limit and grows with the version.
pub(super) fn version_code(version: &str) -> Result<u32, String> {
    let mut part = version.split('.');
    let mut n = |max: u32| -> Result<u32, String> {
        let s = part.next().unwrap_or("0");
        let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
        match digits.parse::<u32>() {
            Ok(v) if v < max => Ok(v),
            _ => Err(format!("apk: [package] version `{version}` has no versionCode: use MAJOR.MINOR.PATCH numbers (major < {max}, minor and patch < 1000)")),
        }
    };
    let (a, b, c) = (n(2000)?, n(1000)?, n(1000)?);
    Ok(a * 1_000_000 + b * 1_000 + c)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// `AndroidManifest.xml`. Of SDL's template the touchscreen and gamepad features (optional) and
/// the vibrate permission are kept; Bluetooth, USB host, `type.pc` and the USB-attached filter are left out.
fn manifest_xml(m: &Manifest, code: u32, icon: bool) -> String {
    let label = if m.dist.title.is_empty() { m.name.clone() } else { m.dist.title.clone() };
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="{pkg}"
    android:versionCode="{code}"
    android:versionName="{ver}">
    <uses-sdk android:minSdkVersion="{min}" android:targetSdkVersion="{TARGET_SDK}" />
    <uses-feature android:glEsVersion="0x00030000" android:required="true" />
    <uses-feature android:name="android.hardware.touchscreen" android:required="false" />
    <uses-feature android:name="android.hardware.gamepad" android:required="false" />
    <uses-permission android:name="android.permission.VIBRATE" />
    <uses-permission android:name="android.permission.INTERNET" />
    <uses-permission android:name="android.permission.POST_NOTIFICATIONS" />{service_perms}
    <application android:label="{label}"{icon}
        android:allowBackup="true"
        android:extractNativeLibs="true"
        android:hasCode="true"
        android:hardwareAccelerated="true"
        android:theme="@android:style/Theme.NoTitleBar.Fullscreen">
        <activity android:name=".J3Activity"
            android:label="{label}"
            android:alwaysRetainTaskState="true"
            android:launchMode="singleInstance"
            android:configChanges="layoutDirection|locale|orientation|uiMode|screenLayout|screenSize|smallestScreenSize|keyboard|keyboardHidden|navigation"
            android:preferMinimalPostProcessing="true"
            android:screenOrientation="{orient}"
            android:exported="true">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>{service}
    </application>
</manifest>
"#,
        pkg = m.android.package,
        ver = xml_escape(&m.version),
        min = android::API_LEVEL,
        label = xml_escape(&label),
        icon = if icon { "\n        android:icon=\"@mipmap/ic_launcher\"" } else { "" },
        orient = m.android.orientation,
        service_perms = match m.android.foreground_service.as_deref() {
            None => String::new(),
            Some(t) => {
                // connectedDevice needs one of its listed permissions on Android 14+; CHANGE_NETWORK_STATE is a normal one.
                let (perm, extra) = if t == "dataSync" { ("DATA_SYNC", "") } else { ("CONNECTED_DEVICE", "\n    <uses-permission android:name=\"android.permission.CHANGE_NETWORK_STATE\" />") };
                format!("\n    <uses-permission android:name=\"android.permission.FOREGROUND_SERVICE\" />\n    <uses-permission android:name=\"android.permission.FOREGROUND_SERVICE_{perm}\" />\n    <uses-permission android:name=\"android.permission.WAKE_LOCK\" />{extra}")
            }
        },
        service = match m.android.foreground_service.as_deref() {
            None => String::new(),
            Some(t) => format!("\n        <service android:name=\".J3Service\" android:exported=\"false\" android:foregroundServiceType=\"{t}\" />"),
        },
    )
}

/// The activity (`apk/J3Activity.java`): SDL's, with our library list, `J3_NATIVE_LIB_DIR`, and the
/// methods `j3.android` calls (vibrate, notifications, keepAlive).
fn activity_java(pkg: &str, service: bool) -> String {
    let libs = SDL_LIBS.iter().chain(&["main"]).map(|l| format!("\"{l}\"")).collect::<Vec<_>>().join(", ");
    include_str!("apk/J3Activity.java").replace("@PKG@", pkg).replace("@LIBS@", &libs).replace("@HAS_SERVICE@", if service { "true" } else { "false" })
}

/// `J3Service` (`apk/J3Service.java`), the foreground service of `[android] foreground_service`.
/// Always compiled; declared in the manifest only when that key is set.
fn service_java(pkg: &str, kind: Option<&str>) -> String {
    // ServiceInfo.FOREGROUND_SERVICE_TYPE_*: dataSync 1, connectedDevice 16.
    let ty = if kind == Some("dataSync") { "1" } else { "16" };
    include_str!("apk/J3Service.java").replace("@PKG@", pkg).replace("@FGS_TYPE@", ty)
}

/// Files under the `include` entries as `(path relative to the project, absolute path)`, sorted.
/// A path `j3.apkassets` would reject is skipped with a warning; a missing entry is an error.
fn collect_assets(root: &Path, include: &[String]) -> Result<Vec<(String, PathBuf)>, String> {
    let mut found = Vec::new();
    for item in include {
        let src = root.join(item);
        if !src.exists() {
            return Err(format!("[dist] include: {} does not exist", src.display()));
        }
        let mut files = Vec::new();
        walk_or_file(&src, &mut files)?;
        for f in files {
            let rel = f.strip_prefix(root).unwrap_or(&f).components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
            if valid_rel(&rel) {
                found.push((rel, f));
            } else {
                eprintln!("j3c: dist: {rel} is not a valid APK asset name (no `..`, `\\`, `*.tmp`, j3assets.*); skipped");
            }
        }
    }
    found.sort();
    found.dedup_by(|a, b| a.0 == b.0);
    Ok(found)
}

fn walk_or_file(p: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if p.is_dir() {
        walk(p, out)
    } else {
        out.push(p.to_path_buf());
        Ok(())
    }
}

/// The path rules of `j3.apkassets.safePath`, plus no newline (one path per line).
fn valid_rel(rel: &str) -> bool {
    !rel.is_empty()
        && rel.len() <= 700
        && !rel.contains('\\')
        && !rel.chars().any(char::is_control)
        && rel.split('/').all(|s| !s.is_empty() && s != "." && s != "..")
        && !rel.ends_with(".tmp")
        && !rel.starts_with("j3assets.")
}

/// 64-bit FNV-1a, fed in pieces.
struct Fnv(u64);

impl Fnv {
    fn new() -> Fnv {
        Fnv(0xcbf29ce484222325)
    }
    fn add(&mut self, b: &[u8]) {
        for &x in b {
            self.0 = (self.0 ^ x as u64).wrapping_mul(0x100000001b3);
        }
    }
    fn hex(&self) -> String {
        format!("{:016x}", self.0)
    }
}

/// `assets/j3assets.txt`: `j3assets 1 <stamp>`, then one path per line.
/// True when the built library looks up `OPENSSL_init_ssl` (only `j3.net.tls` does), so the APK
/// must carry OpenSSL: Android does not let apps load the system's private libssl. Not `libssl.so`:
/// the linker packs string literals and that one does not survive whole.
fn uses_tls(so: &Path) -> Result<bool, String> {
    let b = fs::read(so).map_err(|e| format!("read {}: {e}", so.display()))?;
    Ok(b.windows(16).any(|w| w == b"OPENSSL_init_ssl"))
}

fn j3assets(rels: &[&str], stamp: &str) -> String {
    let mut s = format!("j3assets 1 {stamp}\n");
    for r in rels {
        s.push_str(r);
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(extra: &str) -> Manifest {
        let dir = std::env::temp_dir().join(format!("j3-apk-test-{}", std::process::id()));
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/main.j3"), "").unwrap();
        crate::project::parse_manifest(&format!("[package]\nname = \"my-game\"\nversion = \"1.2.3\"\n{extra}"), dir).unwrap()
    }

    #[test]
    fn version_code_rule() {
        assert_eq!(version_code("0.1.0"), Ok(1_000));
        assert_eq!(version_code("1.2.3"), Ok(1_002_003));
        assert_eq!(version_code("1.2.3-beta"), Ok(1_002_003));
        assert_eq!(version_code("2"), Ok(2_000_000));
        assert!(version_code("1.2.3").unwrap() < version_code("1.10.0").unwrap());
        assert!(version_code("1.1000.0").is_err());
        assert!(version_code("x.1.0").unwrap_err().contains("versionCode"));
        assert!(version_code("2000.0.0").is_err());
    }

    #[test]
    fn manifest_contents() {
        let m = manifest("[dist]\ntitle = \"Me & <You>\"\n[android]\norientation = \"portrait\"\n");
        let x = manifest_xml(&m, 1_002_003, true);
        for want in [
            "package=\"com.j3.my_game\"",
            "android:versionCode=\"1002003\"",
            "android:versionName=\"1.2.3\"",
            "android:minSdkVersion=\"26\" android:targetSdkVersion=\"35\"",
            "android:glEsVersion=\"0x00030000\" android:required=\"true\"",
            "android:label=\"Me &amp; &lt;You&gt;\"",
            "android:icon=\"@mipmap/ic_launcher\"",
            "android:extractNativeLibs=\"true\"",
            "android:hasCode=\"true\"",
            "@android:style/Theme.NoTitleBar.Fullscreen",
            "android:name=\".J3Activity\"",
            "android:screenOrientation=\"portrait\"",
            "android:exported=\"true\"",
            "android.intent.category.LAUNCHER",
        ] {
            assert!(x.contains(want), "missing {want}:\n{x}");
        }
        assert!(!manifest_xml(&m, 1, false).contains("android:icon"));
        assert!(x.contains("android.permission.POST_NOTIFICATIONS") && !x.contains("J3Service"));
        let fg = manifest("[android]\nforeground_service = \"connectedDevice\"\n");
        let x = manifest_xml(&fg, 1, true);
        for want in ["FOREGROUND_SERVICE_CONNECTED_DEVICE", "CHANGE_NETWORK_STATE", "WAKE_LOCK", "android:name=\".J3Service\"", "foregroundServiceType=\"connectedDevice\""] {
            assert!(x.contains(want), "missing {want}:\n{x}");
        }
        assert!(service_java("com.j3.my_game", Some("dataSync")).contains("startForeground(ID, n, 1)"));
        let j = activity_java("com.j3.my_game", false);
        assert!(j.contains("HAS_SERVICE = false") && !j.contains("@PKG@") && !j.contains("@LIBS@"));
        assert!(j.starts_with("package com.j3.my_game;"));
        assert!(j.contains("extends org.libsdl.app.SDLActivity"));
        assert!(j.contains("\"SDL2\", \"SDL2_image\", \"SDL2_mixer\", \"SDL2_ttf\", \"main\""));
        assert!(j.contains("setenv(\"J3_NATIVE_LIB_DIR\", getApplicationInfo().nativeLibraryDir, true)"));
    }

    #[test]
    fn native_libs_listing() {
        let d = std::env::temp_dir().join(format!("j3-apk-native-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        assert!(native_libs(&d).unwrap().is_empty());
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("libb.so"), "").unwrap();
        fs::write(d.join("liba.so"), "").unwrap();
        assert_eq!(native_libs(&d).unwrap(), [d.join("liba.so"), d.join("libb.so")]);
        for bad in ["stockfish", "libmain.so", "libSDL2.so"] {
            fs::write(d.join(bad), "").unwrap();
            assert!(native_libs(&d).unwrap_err().contains("native_libs"), "{bad}");
            fs::remove_file(d.join(bad)).unwrap();
        }
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn assets_listing_and_stamp() {
        assert_eq!(j3assets(&["assets/a.png", "scenes/x/y.j3scene"], "abc"), "j3assets 1 abc\nassets/a.png\nscenes/x/y.j3scene\n");
        for bad in ["a/../b", "a\\b", "x.tmp", "j3assets.stamp", "j3assets.txt", "a//b", "", "a\nb", "./a"] {
            assert!(!valid_rel(bad), "{bad:?}");
        }
        assert!(valid_rel("assets/j3assets.txt.png") && valid_rel("a/b c/d.txt"));

        let root = std::env::temp_dir().join(format!("j3-apk-assets-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("assets/sub")).unwrap();
        fs::write(root.join("assets/b.txt"), "b").unwrap();
        fs::write(root.join("assets/sub/a.txt"), "a").unwrap();
        fs::write(root.join("assets/skip.tmp"), "t").unwrap();
        let stamp = |files: &[(String, PathBuf)]| {
            let mut h = Fnv::new();
            for (rel, p) in files {
                let b = fs::read(p).unwrap();
                h.add(rel.as_bytes());
                h.add(&(b.len() as u64).to_le_bytes());
                h.add(&b);
            }
            h.hex()
        };
        let files = collect_assets(&root, &["assets".into()]).unwrap();
        let rels: Vec<&str> = files.iter().map(|(r, _)| r.as_str()).collect();
        assert_eq!(rels, ["assets/b.txt", "assets/sub/a.txt"]);
        let s1 = stamp(&files);
        assert_eq!(s1, stamp(&collect_assets(&root, &["assets".into()]).unwrap()));
        fs::write(root.join("assets/sub/a.txt"), "A").unwrap();
        assert_ne!(s1, stamp(&collect_assets(&root, &["assets".into()]).unwrap()));
        assert!(collect_assets(&root, &["nope".into()]).unwrap_err().contains("does not exist"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn build_tools_highest_is_numeric() {
        let d = std::env::temp_dir().join(format!("j3-apk-bt-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        for v in ["9.0.0", "35.0.0", "34.0.0"] {
            fs::create_dir_all(d.join(v)).unwrap();
        }
        assert_eq!(android::highest_version_dir(&d), Some(d.join("35.0.0")));
        let _ = fs::remove_dir_all(&d);
    }
}
