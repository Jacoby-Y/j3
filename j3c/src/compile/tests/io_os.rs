//! `j3.io` OS pieces (plan B6): the `io_os_test.j3` program (paths, atomic replace killed
//! halfway, flock between two processes, copy, walk, temp, `proc.waitAny`) and a stdio echo
//! server driven through a pipe with `Content-Length` messages split across writes and packed
//! two to a write. Both backends.
use super::*;
use std::io::{Read, Write};

#[test]
fn io_os_test_runs_on_both_backends() {
    for r in both_backends("ioos", include_str!("../../../stdlib/tests/io_os_test.j3")) {
        assert_eq!(r.status, 0, "{}{}", String::from_utf8_lossy(&r.stdout), String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), "io_os_test ok\n");
    }
}

fn frame(body: &str) -> String {
    format!("Content-Length: {}\r\n\r\n{}", body.len(), body)
}

#[test]
fn stdio_echo_server_handles_split_and_packed_messages() {
    let dir = std::env::temp_dir().join(format!("j3-stdio-echo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.j3");
    std::fs::write(&file, include_str!("../../../stdlib/tests/stdio_echo.j3")).unwrap();
    let prog = compile_path(&file).expect("check");
    let big = "x".repeat(150_000);
    for backend in [emit::Backend::Llvm, emit::Backend::C] {
        let mut opts = llvm_opts();
        opts.backend = backend;
        opts.cc.out = Some(dir.join(format!("echo-{backend:?}")));
        let exe = emit::compile_program(&prog.cx, &prog.modules, &opts).expect("build");
        let mut child = std::process::Command::new(&exe)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("spawn");
        let mut stdin = child.stdin.take().unwrap();
        let pause = || std::thread::sleep(std::time::Duration::from_millis(30));
        // one message split mid-header and mid-body, across separate writes
        let m1 = frame("hello world");
        for part in [&m1[..9], &m1[9..20], &m1[20..m1.len() - 3], &m1[m1.len() - 3..]] {
            stdin.write_all(part.as_bytes()).unwrap();
            stdin.flush().unwrap();
            pause();
        }
        // two messages in one write, then one larger than the 64 KiB read buffer
        let two = format!("{}{}", frame("a"), frame("{\"id\":2}"));
        stdin.write_all(two.as_bytes()).unwrap();
        pause();
        stdin.write_all(frame(&big).as_bytes()).unwrap();
        drop(stdin);
        let mut out = String::new();
        child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
        assert!(child.wait().unwrap().success(), "{out}");
        let want = format!(
            "{}{}{}{}done 4\n",
            frame("echo:hello world\n"),
            frame("echo:a\n"),
            frame("echo:{\"id\":2}\n"),
            frame(&format!("echo:{big}\n"))
        );
        assert!(out == want, "{backend:?}: got {} bytes: {:?}", out.len(), &out[..out.len().min(200)]);
    }
    let _ = std::fs::remove_dir_all(&dir);
}
