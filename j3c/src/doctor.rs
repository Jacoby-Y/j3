//! `j3c doctor`: is this machine ready to build and run J3 programs? One table of checks, one
//! line each, exit 1 when a required one is missing. Linux names; other OSes say what is unchecked.

use std::path::Path;
use std::process::Command;

enum Probe {
    /// A program on PATH; the first line of `<it> --version` is the detail.
    Tool(&'static str),
    /// A shared library the linker can find (`[link] libs` name).
    Lib(&'static str),
    File(&'static str),
    CacheWritable,
}

struct Check {
    what: &'static str,
    probe: Probe,
    required: bool,
    /// Debian/Ubuntu package.
    apt: &'static str,
}

const CHECKS: &[Check] = &[
    Check { what: "clang (compiles and links every program)", probe: Probe::Tool("clang"), required: true, apt: "clang" },
    Check { what: "ld (linker clang calls)", probe: Probe::Tool("ld"), required: true, apt: "binutils" },
    Check { what: "SDL2 (windows, input, audio)", probe: Probe::Lib("SDL2"), required: true, apt: "libsdl2-2.0-0" },
    Check { what: "libGL (3D, .j3ui, paint)", probe: Probe::Lib("GL"), required: true, apt: "libgl1" },
    Check { what: "SDL2_ttf (text)", probe: Probe::Lib("SDL2_ttf"), required: false, apt: "libsdl2-ttf-2.0-0" },
    Check { what: "SDL2_image (loadTexture)", probe: Probe::Lib("SDL2_image"), required: false, apt: "libsdl2-image-2.0-0" },
    Check { what: "SDL2_mixer (sounds, music)", probe: Probe::Lib("SDL2_mixer"), required: false, apt: "libsdl2-mixer-2.0-0" },
    Check {
        what: "DejaVu Sans (editor UI font; J3_EDITOR_FONT overrides)",
        probe: Probe::File("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
        required: false,
        apt: "fonts-dejavu-core",
    },
    Check {
        what: "Noto Color Emoji (emoji fallback in text)",
        probe: Probe::File("/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf"),
        required: false,
        apt: "fonts-noto-color-emoji",
    },
    Check { what: "~/.cache/j3 writable (editor, incremental builds)", probe: Probe::CacheWritable, required: true, apt: "" },
];

pub fn cmd(_args: &[String]) -> i32 {
    println!("j3c {} doctor", env!("CARGO_PKG_VERSION"));
    let linux = cfg!(target_os = "linux");
    if !linux {
        println!("note: only Linux is checked; on this OS the library, font, and package lines are skipped");
    }
    let mut missing_required = false;
    let mut apt: Vec<&str> = Vec::new();
    for c in CHECKS {
        let os_specific = matches!(c.probe, Probe::Lib(_) | Probe::File(_));
        if os_specific && !linux {
            println!("  skip     {}", c.what);
            continue;
        }
        match probe(&c.probe) {
            Ok(detail) if detail.is_empty() => println!("  ok       {}", c.what),
            Ok(detail) => println!("  ok       {}: {detail}", c.what),
            Err(why) => {
                let tag = if c.required { "MISSING " } else { "optional" };
                let fix = if c.apt.is_empty() || !linux { String::new() } else { format!(" (sudo apt install {})", c.apt) };
                println!("  {tag} {}: {why}{fix}", c.what);
                missing_required |= c.required;
                if !c.apt.is_empty() {
                    apt.push(c.apt);
                }
            }
        }
    }
    if !apt.is_empty() && linux {
        println!("all at once (Debian/Ubuntu): sudo apt install {}", apt.join(" "));
    }
    if missing_required {
        println!("not ready: a required item is missing");
        1
    } else {
        println!("ready");
        0
    }
}

fn probe(p: &Probe) -> Result<String, String> {
    match p {
        Probe::Tool(t) => match Command::new(t).arg("--version").output() {
            Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("").trim().to_string()),
            Ok(o) => Err(format!("`{t} --version` exited {}", o.status)),
            Err(_) => Err("not on PATH".to_string()),
        },
        Probe::Lib(name) => {
            if crate::emit::host_lib_present(name, &[]) {
                Ok(String::new())
            } else {
                Err("not found".to_string())
            }
        }
        Probe::File(f) => if Path::new(f).is_file() { Ok(String::new()) } else { Err(format!("no {f}")) },
        Probe::CacheWritable => {
            let dir = crate::editor::cache_root().ok_or("no XDG_CACHE_HOME or HOME")?;
            std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            let probe = dir.join(format!(".doctor-{}", std::process::id()));
            std::fs::write(&probe, b"").map_err(|e| format!("{}: {e}", dir.display()))?;
            let _ = std::fs::remove_file(&probe);
            Ok(dir.display().to_string())
        }
    }
}
