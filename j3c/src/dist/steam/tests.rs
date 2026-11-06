use super::*;
use crate::project::Output;

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("j3-steam-test-{}-{tag} sp", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/main.j3"), "").unwrap();
    d
}

fn manifest(dir: &Path, extra: &str) -> Manifest {
    let t = format!("[package]\nname = \"mygame\"\nversion = \"1.2.0\"\n{extra}");
    crate::project::parse_manifest(&t, dir.to_path_buf()).unwrap()
}

fn fake_sdk(dir: &Path) -> PathBuf {
    let sdk = dir.join("sdk");
    for (d, f) in [("linux64", "libsteam_api.so"), ("win64", "steam_api64.dll")] {
        fs::create_dir_all(sdk.join("redistributable_bin").join(d)).unwrap();
        fs::write(sdk.join("redistributable_bin").join(d).join(f), f).unwrap();
    }
    sdk
}

const FULL: &str = "[steam]\napp_id = 480\nsdk = \"sdk\"\ndepot_linux = 481\ndepot_windows = 482\nbranch = \"beta\"\ndesc = \"say hi\"\n[dist]\nbuild_outputs = [\"steam\"]\ninclude = [\"assets\", \"steam_appid.txt\"]\n";

#[test]
fn vdf_golden_and_escaping() {
    let v = app_build_vdf(480, "a \"b\" c:\\d\ne", "", &[("linux", 481)]);
    assert_eq!(
        v,
        "\"AppBuild\"\n{\n\t\"AppID\" \"480\"\n\t\"Desc\" \"a \\\"b\\\" c:\\\\d e\"\n\t\"ContentRoot\" \"../content/\"\n\t\"BuildOutput\" \"../output/\"\n\t\"SetLive\" \"\"\n\t\"Depots\"\n\t{\n\t\t\"481\"\n\t\t{\n\t\t\t\"FileMapping\" { \"LocalPath\" \"linux/*\" \"DepotPath\" \".\" \"recursive\" \"1\" }\n\t\t\t\"FileExclusion\" \"*.pdb\"\n\t\t}\n\t}\n}\n"
    );
    let both = app_build_vdf(1, "d", "beta", &[("linux", 2), ("windows", 3)]);
    assert!(both.contains("\"SetLive\" \"beta\"") && both.contains("\"3\"") && both.contains("windows/*"));
}

#[test]
fn stage_lays_out_content() {
    let dir = scratch("stage");
    fs::create_dir_all(dir.join("assets")).unwrap();
    fs::write(dir.join("assets/a.txt"), "a").unwrap();
    fake_sdk(&dir);
    let m = manifest(&dir, FULL);
    assert_eq!(m.dist.outputs, [Output::Steam]);
    let app = dir.join("app");
    fs::create_dir_all(app.join("assets")).unwrap();
    fs::write(app.join("mygame"), "exe").unwrap();
    fs::write(app.join("assets/a.txt"), "a").unwrap();
    fs::write(app.join("steam_appid.txt"), "480").unwrap();
    let built = vec![("x86_64-pc-linux-gnu".to_string(), app.join("mygame"))];
    let plan = plan(&m, &built).unwrap();
    assert_eq!(plan, [("linux", 481)]);
    stage(&m, "x86_64-pc-linux-gnu", &app).unwrap();
    finish(&m, &plan).unwrap();
    let c = dir.join("target/dist/steam/content/linux");
    assert!(c.join("mygame").is_file() && c.join("assets/a.txt").is_file());
    assert_eq!(fs::read_to_string(c.join("libsteam_api.so")).unwrap(), "libsteam_api.so");
    assert!(!c.join("steam_appid.txt").exists());
    let vdf = fs::read_to_string(dir.join("target/dist/steam/scripts/app_build_480.vdf")).unwrap();
    assert!(vdf.contains("\"Desc\" \"say hi\"") && vdf.contains("\"481\"") && !vdf.contains("\"482\""));
    let readme = fs::read_to_string(dir.join("target/dist/steam/scripts/README.txt")).unwrap();
    assert!(readme.contains("+run_app_build \"") && readme.contains("sp/target/dist/steam/scripts/app_build_480.vdf\" +quit"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn windows_gets_its_dll() {
    let dir = scratch("win");
    fake_sdk(&dir);
    let m = manifest(&dir, FULL);
    let app = dir.join("app");
    fs::create_dir_all(&app).unwrap();
    fs::write(app.join("mygame.exe"), "exe").unwrap();
    stage(&m, "x86_64-pc-windows-gnu", &app).unwrap();
    assert!(dir.join("target/dist/steam/content/windows/steam_api64.dll").is_file());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn errors_name_the_key() {
    let dir = scratch("err");
    let built = vec![("x86_64-pc-linux-gnu".to_string(), dir.join("x"))];
    let err = |extra: &str| plan(&manifest(&dir, extra), &built).unwrap_err();
    assert!(err("[steam]\nsdk = \"sdk\"\n").contains("[steam] app_id"));
    assert!(err("[steam]\napp_id = 1\n").contains("[steam] sdk"));
    assert!(err("[steam]\napp_id = 1\nsdk = \"sdk\"\n").contains("[steam] depot_linux"));
    assert!(err("[steam]\napp_id = 1\nsdk = \"sdk\"\ndepot_linux = 2\n").contains("[steam] sdk: "));
    // macOS is skipped, not an error
    let mac = vec![("arm64-apple-darwin".to_string(), dir.join("x"))];
    assert!(plan(&manifest(&dir, ""), &mac).unwrap().is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn dev_env_sets_and_respects_existing() {
    let dir = scratch("env");
    let sdk = fake_sdk(&dir);
    fs::write(dir.join("j3.toml"), format!("[steam]\napp_id = 480\nsdk = \"{}\"\n", sdk.display())).unwrap();
    let env = dev_env(&dir);
    let get = |k: &str| env.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone());
    // the test process may have these set; only check the unset case
    if std::env::var_os("SteamAppId").is_none() {
        assert_eq!(get("SteamAppId").as_deref(), Some("480"));
    }
    if std::env::var_os("J3_STEAM_LIB").is_none() && cfg!(unix) {
        assert!(get("J3_STEAM_LIB").unwrap().ends_with("linux64/libsteam_api.so"));
    }
    fs::write(dir.join("j3.toml"), "[package]\nname = \"x\"\n").unwrap();
    assert!(dev_env(&dir).is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn glibc_scan_and_runtime_libs() {
    let t = "0000 DF *UND*\t0000 (GLIBC_2.2.5) printf\n0000 DF *UND*\t0000 (GLIBC_2.34) __libc_start_main\n0000 DF *UND*\t0000 (GLIBC_2.9) pipe2\n0000 g DF .text 0000 (GLIBC_2.99) mine\n";
    assert_eq!(max_glibc(t), Some(((2, 34), "__libc_start_main".to_string())));
    assert_eq!(max_glibc(""), None);
    let dir = scratch("libs");
    fake_sdk(&dir);
    fs::create_dir_all(dir.join("rt")).unwrap();
    fs::write(dir.join("rt/libssl.so.3"), "s").unwrap();
    fs::write(dir.join("rt/notes.txt"), "n").unwrap();
    let m = manifest(&dir, &format!("{FULL}[steam]\n"));
    assert!(m.steam.runtime_libs.is_none());
    let m = manifest(&dir, "[steam]\napp_id = 1\nsdk = \"sdk\"\ndepot_linux = 2\nruntime_libs = \"rt\"\nglibc_max = \"2.17\"\n");
    assert_eq!(m.steam.glibc_max, "2.17");
    let app = dir.join("app");
    fs::create_dir_all(&app).unwrap();
    fs::write(app.join("mygame"), "exe").unwrap();
    stage(&m, "x86_64-pc-linux-gnu", &app).unwrap();
    let c = dir.join("target/dist/steam/content/linux");
    assert!(c.join("libssl.so.3").is_file() && !c.join("notes.txt").exists());
    let bad = manifest(&dir, "[steam]\napp_id = 1\nsdk = \"sdk\"\ndepot_linux = 2\nruntime_libs = \"nope\"\n");
    let built = vec![("x86_64-pc-linux-gnu".to_string(), dir.join("x"))];
    assert!(plan(&bad, &built).unwrap_err().contains("[steam] runtime_libs"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sniper_script_and_elf_magic() {
    let dir = scratch("sh");
    fs::write(dir.join("a"), b"\x7fELFxx").unwrap();
    fs::write(dir.join("b"), b"#!/bin/sh").unwrap();
    assert!(is_elf(&dir.join("a")) && !is_elf(&dir.join("b")) && !is_elf(&dir.join("none")));
    fs::write(dir.join("s.sh"), SNIPER_SCRIPT).unwrap();
    let ok = std::process::Command::new("bash").arg("-n").arg(dir.join("s.sh")).status().unwrap();
    assert!(ok.success());
    let _ = fs::remove_dir_all(&dir);
}
