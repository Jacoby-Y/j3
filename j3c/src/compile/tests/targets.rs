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
fn web_hello_uses_libc() {
    let triple = "wasm64-unknown-emscripten";
    let prog = compile_path_target(&default_hello_path(), triple).expect("compile web hello");
    let mut opts = emit::EmitOpts::default();
    opts.target = triple.to_string();
    let ll = emit::emit_llvm(&prog.cx, &prog.modules, &opts).expect("llvm");
    assert!(ll.contains("target triple = \"wasm64-unknown-emscripten\""));
    assert!(!ll.contains("target datalayout"), "{ll}");
    assert!(!ll.contains("@syscall"), "{ll}");
    assert!(!ll.contains("@dlopen"), "{ll}");
    assert!(ll.contains("@mmap"), "{ll}");
    // system clang can lower it to a wasm object (no emsdk needed for this step)
    let wasm = std::process::Command::new("clang")
        .arg("-print-targets")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("wasm64"))
        .unwrap_or(false);
    if wasm {
        let dir = std::env::temp_dir().join(format!("j3-web-hello-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("hello.ll");
        std::fs::write(&src, &ll).unwrap();
        let out = std::process::Command::new("clang")
            .args([&format!("--target={triple}"), "-c", "-O1", "-Wno-override-module"])
            .arg(&src)
            .arg("-o")
            .arg(dir.join("hello.o"))
            .output()
            .expect("clang");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    }
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
/// Wine tests run one at a time: each ends with `wineserver -k`, which kills the other's programs.
static WINE: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn windows_io_runs_under_wine() {
    let _wine = WINE.lock().unwrap_or_else(|e| e.into_inner());
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

/// The Win32 twins (`proc_windows`, `term/pty_windows`, `io/watch_windows`, `instance_windows`,
/// `game/hotkey_windows`) check for Windows and call no raw `syscall`; `j3.io.watch` has none on web either (macOS links
/// libSystem's `syscall` for `threadsys.j3` anyway).
#[test]
fn windows_os_modules_check_without_syscall() {
    let dir = std::env::temp_dir().join(format!("j3-winos-check-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.j3");
    std::fs::write(
        &file,
        "module m;\nuse j3.proc;\nuse j3.term.pty;\nuse j3.io.watch.Watcher;\nuse j3.instance;\nuse j3.game.hotkey;\nuse j3.game.Key;\nuse j3.memory.Arena;\n\
         fn main() {\n    let a = Arena.create(65536);\n    let w = Watcher.create(&a);\n    w.close();\n    instance.claim(\"x\");\n    hotkey.register(Key.F8, hotkey.CTRL);\n    let v: [1]string = [\"cmd\"];\n    proc.run(v[:], \"\", &a);\n    pty.spawn(v[:], \"\", v[0..0], 80, 24, &a);\n}\n",
    )
    .unwrap();
    let prog = compile_path_target(&file, "x86_64-pc-windows-gnu").expect("windows check");
    let mut opts = emit::EmitOpts::default();
    opts.target = "x86_64-pc-windows-gnu".to_string();
    let ll = emit::emit_llvm(&prog.cx, &prog.modules, &opts).expect("llvm");
    assert!(!ll.contains("@syscall("), "raw syscall on Windows");
    for f in ["CreatePseudoConsole", "CreateProcessW", "ReadDirectoryChangesW", "CreateNamedPipeA", "RegisterHotKey"] {
        assert!(ll.contains(f), "{f} missing");
    }
    std::fs::write(&file, "module m;\nuse j3.io.watch.Watcher;\nuse j3.memory.Arena;\nfn main() {\n    let a = Arena.create(65536);\n    let w = Watcher.create(&a);\n    w.add(\"/tmp\");\n    w.close();\n}\n").unwrap();
    for target in ["wasm64-unknown-emscripten"] {
        let prog = compile_path_target(&file, target).expect("check");
        let mut opts = emit::EmitOpts::default();
        opts.target = target.to_string();
        let ll = emit::emit_llvm(&prog.cx, &prog.modules, &opts).expect("llvm");
        assert!(!ll.contains("@syscall("), "raw syscall in j3.io.watch on {target}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// `win_os_test.j3` under wine: proc, pty (ConPTY), watch, instance request/reply. Wine attaches a pty child to the
/// console only when the parent has one, so it runs inside `script`. Skips without mingw clang, wine or `script`.
#[test]
fn windows_os_runs_under_wine() {
    let _wine = WINE.lock().unwrap_or_else(|e| e.into_inner());
    let have = |c: &str, a: &str| std::process::Command::new(c).arg(a).output().map(|o| o.status.success()).unwrap_or(false);
    if !have("wine", "--version") || !have("script", "--version") || !mingw_clang() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("j3-winos-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.j3");
    std::fs::write(&file, include_str!("../../../stdlib/tests/win_os_test.j3")).unwrap();
    let prog = compile_path_target(&file, "x86_64-pc-windows-gnu").expect("check");
    let mut opts = emit::EmitOpts::default();
    opts.target = "x86_64-pc-windows-gnu".to_string();
    let ll = emit::emit_llvm(&prog.cx, &prog.modules, &opts).expect("llvm");
    let exe = emit::compile_text(&ll, &opts).expect("link");
    // output to a file, not a pipe: wineserver inherits pipes and `.output()` would wait for it
    let log = dir.join("out.txt");
    let _ = std::process::Command::new("timeout")
        .args(["120", "script", "-qec"])
        .arg(format!("wine '{}'", exe.display()))
        .arg("/dev/null")
        .env("WINEDEBUG", "-all")
        .stdin(std::process::Stdio::null())
        .stdout(std::fs::File::create(&log).unwrap())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("script");
    let out = String::from_utf8_lossy(&std::fs::read(&log).unwrap_or_default()).to_string();
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::process::Command::new("timeout")
        .args(["60", "wine", "cmd", "/c", "rmdir", "/s", "/q", "C:\\j3_win_os_test"])
        .env("WINEDEBUG", "-all")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    let _ = std::process::Command::new("wineserver").arg("-k").status();
    assert!(out.contains("win_os_test ok"), "{}", out);
}
