//! Windows and macOS targets, LLVM debug info, C backend details.

use super::*;

#[test]
fn windows_hello_uses_kernel32() {
    let prog = compile_path_target(&default_hello_path(), "x86_64-pc-windows-gnu")
        .expect("compile windows hello");
    let dump = dump_program(&prog.cx, &prog.modules);
    assert!(!dump.contains("@syscall("), "{dump}");
    let mut opts = emit::EmitOpts::default();
    opts.target = "x86_64-pc-windows-gnu".to_string();
    let ll = emit::emit_llvm(&prog.cx, &prog.modules, &opts).expect("llvm");
    assert!(ll.contains("target triple = \"x86_64-pc-windows-gnu\""));
    assert!(ll.contains("CreateFileA"), "{ll}");
    assert!(ll.contains("VirtualAlloc"), "{ll}");
    assert!(!ll.contains("@syscall"), "{ll}");
    assert!(ll.contains("DILocation"), "{ll}");
    assert!(ll.contains("hello.j3"), "{ll}");
    let c = emit::emit_program(&prog.cx, &prog.modules).expect("c");
    assert!(!c.contains("unistd.h"), "{c}");
    assert!(!c.contains("j3_syscall"), "{c}");
    if mingw_clang() {
        emit::compile_text(&ll, &opts).expect("link windows hello");
    }
}

#[test]
fn macos_hello_uses_libc() {
    let prog = compile_path_target(&default_hello_path(), "arm64-apple-darwin")
        .expect("compile macos hello");
    let mut opts = emit::EmitOpts::default();
    opts.target = "arm64-apple-darwin".to_string();
    let ll = emit::emit_llvm(&prog.cx, &prog.modules, &opts).expect("llvm");
    assert!(ll.contains("target triple = \"arm64-apple-darwin\""));
    assert!(ll.contains("@mmap"), "{ll}");
    assert!(!ll.contains("@syscall"), "{ll}");
}

#[test]
fn llvm_line_info_follows_debug_flag() {
    let prog = compile_path(&default_hello_path()).expect("compile");
    let mut opts = emit::EmitOpts::default();
    let ll = emit::emit_llvm(&prog.cx, &prog.modules, &opts).expect("llvm");
    assert!(ll.contains("DILocation(line: 5"), "{ll}");
    assert!(ll.contains("hello.j3"), "{ll}");
    opts.cc.debug = false;
    let bare = emit::emit_llvm(&prog.cx, &prog.modules, &opts).expect("llvm");
    assert!(!bare.contains("DILocation"), "{bare}");
}

#[test]
fn llvm_names_locals_when_debug() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/programs/mesh.j3");
    let prog = compile_path(&path).expect("compile mesh");
    let mut opts = emit::EmitOpts::default();
    let ll = emit::emit_llvm(&prog.cx, &prog.modules, &opts).expect("llvm");
    assert!(ll.contains("DILocalVariable(name: \"q\""), "{ll}");
    assert!(ll.contains("llvm.dbg.declare"), "{ll}");
    assert!(ll.contains("= alloca "), "{ll}");
    opts.cc.debug = false;
    let bare = emit::emit_llvm(&prog.cx, &prog.modules, &opts).expect("llvm");
    assert!(!bare.contains("DILocalVariable"), "{bare}");
    assert!(!bare.contains("llvm.dbg.declare"), "{bare}");
}

#[test]
fn c_declares_exit_without_syscall() {
    let prog = compile_path_target(
        &speculative("01_basics.j3"),
        "x86_64-pc-windows-gnu",
    )
    .expect("windows 01");
    let c = emit::emit_program(&prog.cx, &prog.modules).expect("c");
    assert!(!c.contains("j3_syscall"), "{c}");
    assert!(c.contains("void _exit(int);"), "{c}");
}

#[test]
fn target_list_picks_a_sys_file_per_triple() {
    let toml = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("examples/3d-editor-scene/j3.toml");
    let mut m = crate::project::Manifest::load(&toml).expect("j3.toml");
    m.emit.targets = vec![
        "x86_64-pc-windows-gnu".to_string(),
        "x86_64-pc-linux-gnu".to_string(),
    ];
    assert_eq!(m.emit.primary(), "x86_64-pc-linux-gnu");
    let host = compile_manifest(&m).expect("host");
    let host_ir = dump_program(&host.cx, &host.modules);
    assert!(host_ir.contains("@syscall("), "{host_ir}");
    let win = compile_manifest_target(&m, "x86_64-pc-windows-gnu").expect("windows");
    let win_ir = dump_program(&win.cx, &win.modules);
    assert!(!win_ir.contains("@syscall("), "{win_ir}");
    assert!(win_ir.contains("CreateFileA") || win_ir.contains("kernel32"), "{win_ir}");
}

#[test]
fn windows_game_checks_and_missing_sdl_is_a_link_error() {
    let toml = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("examples/3d-editor-scene/j3.toml");
    let mut m = crate::project::Manifest::load(&toml).expect("j3.toml");
    m.emit.targets = vec!["x86_64-pc-windows-gnu".to_string()];
    let prog = compile_manifest(&m).expect("typecheck windows game");
    let opts = emit::EmitOpts::from_manifest(&m).expect("opts");
    let ll = emit::emit_llvm(&prog.cx, &prog.modules, &opts).expect("llvm");
    assert!(ll.contains("CreateFileA"), "{ll}");
    assert!(!ll.contains("@syscall"), "{ll}");
    if !mingw_clang() {
        return;
    }
    match emit::compile_text(&ll, &opts) {
        Ok(_) => {}
        Err(e) => {
            let err = e.to_lowercase();
            assert!(
                err.contains("cannot find")
                    || err.contains("undefined")
                    || err.contains("unresolved")
                    || err.contains("library not found")
                    || err.contains("-lsdl2"),
                "{e}"
            );
        }
    }
}

fn mingw_clang() -> bool {
    let dir = std::env::temp_dir();
    let src = dir.join(format!("j3c_mingw_probe_{}.c", std::process::id()));
    // Link, not just compile: clang can compile for Windows without mingw installed.
    let obj = dir.join(format!("j3c_mingw_probe_{}.exe", std::process::id()));
    if std::fs::write(&src, "int main(void){return 0;}\n").is_err() {
        return false;
    }
    let ok = std::process::Command::new("clang")
        .arg("--target=x86_64-pc-windows-gnu")
        .arg("-o")
        .arg(&obj)
        .arg(&src)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&obj);
    ok
}

/// `j3.io` on Windows paths (`\`, `C:`, open flags): build `win_io_test.j3` for Windows and run it
/// under wine. Skips without mingw clang or wine.
#[test]
fn windows_io_runs_under_wine() {
    let wine = std::process::Command::new("wine").arg("--version").output().map(|o| o.status.success()).unwrap_or(false);
    if !wine || !mingw_clang() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("j3-winio-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.j3");
    std::fs::write(&file, include_str!("../../../stdlib/tests/win_io_test.j3")).unwrap();
    let prog = compile_path_target(&file, "x86_64-pc-windows-gnu").expect("check");
    let mut opts = emit::EmitOpts::default();
    opts.target = "x86_64-pc-windows-gnu".to_string();
    let ll = emit::emit_llvm(&prog.cx, &prog.modules, &opts).expect("llvm");
    let exe = emit::compile_text(&ll, &opts).expect("link");
    // Output goes to a file, not a pipe: wine leaves wineserver and its services running, they inherit the pipe, and
    // `.output()` would wait forever for EOF. `.status()` waits for `timeout` alone.
    let log = dir.join("out.txt");
    let _ = std::process::Command::new("timeout")
        .args(["60", "wine"])
        .arg(&exe)
        .env("WINEDEBUG", "-all")
        .stdout(std::fs::File::create(&log).unwrap())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("wine");
    let out = std::fs::read_to_string(&log).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::process::Command::new("timeout")
        .args(["60", "wine", "cmd", "/c", "rmdir", "/s", "/q", "C:\\j3_win_io_test"])
        .env("WINEDEBUG", "-all")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    let _ = std::process::Command::new("wineserver").arg("-k").status();
    assert!(out.contains("win_io_test ok"), "{}", out);
}
