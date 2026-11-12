//! Tests for link flags, target helpers and failure hints.

#[cfg(test)]
#[test]
fn link_gc_flag_matches_target() {
    assert_eq!(super::link_gc_flag(""), "-Wl,--gc-sections");
    assert_eq!(super::link_gc_flag("x86_64-pc-linux-gnu"), "-Wl,--gc-sections");
    assert_eq!(super::link_gc_flag("x86_64-pc-windows-gnu"), "-Wl,--gc-sections");
    assert_eq!(super::link_gc_flag("arm64-apple-darwin"), "-Wl,-dead_strip");
    assert_eq!(super::link_gc_flag("x86_64-apple-darwin"), "-Wl,-dead_strip");
}

#[test]
fn release_flags_per_platform() {
    use super::{release_flags, target_os, TargetOs};
    assert_eq!(target_os("x86_64-pc-linux-gnu"), TargetOs::Linux);
    assert_eq!(target_os("x86_64-pc-windows-gnu"), TargetOs::Windows);
    assert_eq!(target_os("arm64-apple-darwin"), TargetOs::Macos);
    assert_eq!(target_os("aarch64-linux-android"), TargetOs::Android);
    assert_eq!(target_os("x86_64-linux-android26"), TargetOs::Android);
    assert_eq!(target_os("wasm32-unknown-unknown"), TargetOs::Other);
    assert_eq!(target_os("wasm64-unknown-emscripten"), TargetOs::Web);
    let linux = release_flags(TargetOs::Linux, false);
    assert!(linux.link.contains(&"-Wl,-s") && !linux.link.contains(&"-fuse-ld=lld"));
    assert!(linux.compile.contains(&"-fno-unwind-tables"));
    assert!(release_flags(TargetOs::Linux, true).link.contains(&"-Wl,--icf=safe"));
    let win = release_flags(TargetOs::Windows, true);
    assert!(win.compile.is_empty() && !win.link.contains(&"-Wl,--icf=safe"));
    assert!(release_flags(TargetOs::Macos, true).link.contains(&"-Wl,-x"));
    let other = release_flags(TargetOs::Other, true);
    assert!(other.compile.is_empty() && other.link.is_empty());
    assert!(!super::release_nounwind(true, "x86_64-pc-windows-gnu"));
    assert!(super::release_nounwind(true, "x86_64-pc-linux-gnu"));
}

#[test]
fn android_flags_and_libs() {
    use super::{clang_for, link_lib, release_flags, TargetOs};
    let r = release_flags(TargetOs::Android, true);
    assert!(!r.compile.contains(&"-fno-unwind-tables"));
    assert!(r.link.contains(&"-Wl,--gc-sections") && r.link.contains(&"-Wl,-s"));
    assert_eq!(link_lib("GL", &[], "aarch64-linux-android").unwrap(), "-lGLESv3");
    assert_eq!(link_lib("SDL2_ttf", &[], "x86_64-linux-android").unwrap(), "-lSDL2_ttf");
    assert_eq!(clang_for("", "x86_64-pc-linux-gnu"), "clang");
    assert_eq!(clang_for("zig", "x86_64-pc-linux-gnu"), "zig");
    assert_eq!(clang_for("/opt/my-clang", "aarch64-linux-android"), "/opt/my-clang");
}

#[test]
fn missing_library_names_itself() {
    let err = super::link_lib("not-a-real-lib", &[], "").unwrap_err();
    assert!(err.contains("not-a-real-lib"), "{err}");
    assert!(err.contains("take it out of [link] libs"), "{err}");
}

#[test]
fn missing_sdl_names_the_package() {
    let err = super::failure_hint("cannot find library `SDL2_mixer`").unwrap();
    assert!(err.contains("libsdl2-mixer-2.0-0"), "{err}");
    assert!(!err.contains("libsdl2-2.0-0"), "{err}");
    let image = super::failure_hint("libSDL2_image-2.0.so.0\n").unwrap();
    assert!(image.contains("libsdl2-image-2.0-0"), "{image}");
    let both = super::failure_hint(
        "undefined reference to `IMG_Quit'\nundefined reference to `TTF_Quit'",
    )
    .unwrap();
    assert!(both.contains("libsdl2-image-2.0-0"), "{both}");
    assert!(both.contains("libsdl2-ttf-2.0-0"), "{both}");
    assert!(super::failure_hint("error: expected `;`\n --> src/main.j3:1:1\n").is_none());
    let cc = super::failure_hint("spawn clang: No such file or directory (os error 2)").unwrap();
    assert!(cc.contains("sudo apt install clang"), "{cc}");
    let ld = super::failure_hint("/usr/bin/ld: cannot find -lSDL2: No such file or directory\n/usr/bin/ld: cannot find -lGL").unwrap();
    assert!(ld.contains("libsdl2-2.0-0") && ld.contains("libgl1"), "{ld}");
    let gone = super::spawn_err("cc", std::io::Error::from(std::io::ErrorKind::NotFound));
    assert_eq!(gone, "error: clang (or cc) not found on PATH; install clang (Debian/Ubuntu: sudo apt install clang)");
    assert!(super::failure_hint(&gone).is_none(), "one line, no second hint");
}
