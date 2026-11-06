//! `j3.cmp` and `j3.data.map` (chunk 15 of plan 029): the stdlib test programs on both backends.
use super::*;

fn run(name: &str, src: &str, want: &str) {
    for r in both_backends(name, src) {
        assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), want);
    }
}

#[test]
fn cmp_test_runs_on_both_backends() {
    run("cmp15", include_str!("../../../stdlib/tests/cmp_test.j3"), "cmp_test ok\n");
}

#[test]
fn map_test_runs_on_both_backends() {
    run("map15", include_str!("../../../stdlib/tests/map_test.j3"), "map_test ok\n");
}

#[test]
fn j2_heap_test_runs_on_both_backends() {
    run("j2heap", include_str!("../../../stdlib/tests/j2_heap_test.j3"), "j2_heap_test ok\n");
}

/// Reads `../examples/j2_script/j2-scripts/*/main.j2` (cargo runs tests in `j3c/`).
#[test]
fn j2_lex_test_runs_on_both_backends() {
    run("j2lex", include_str!("../../../stdlib/tests/j2_lex_test.j3"), "j2_lex_test ok\n");
}

/// Reads the example scripts like `j2_lex_test`, round-trips them through `parse.print`.
#[test]
fn j2_parse_test_runs_on_both_backends() {
    run("j2parse", include_str!("../../../stdlib/tests/j2_parse_test.j3"), "j2_parse_test ok\n");
}

/// The j2 compiler and VM case table; also compiles `../examples/j2_script` scripts like `j2_lex_test`.
#[test]
fn j2_vm_test_runs_on_both_backends() {
    run("j2vm", include_str!("../../../stdlib/tests/j2_vm_test.j3"), "j2_vm_test ok\n");
}

/// The j2 host API: natives, `Args`, re-entrant calls and their unwind, keep/release, table views.
#[test]
fn j2_host_test_runs_on_both_backends() {
    run("j2host", include_str!("../../../stdlib/tests/j2_host_test.j3"), "j2_host_test ok\n");
}

/// j2 modules: `pub` in the exports table, `use` through the resolver, cycles, load-time name errors.
#[test]
fn j2_module_test_runs_on_both_backends() {
    run("j2module", include_str!("../../../stdlib/tests/j2_module_test.j3"), "j2_module_test ok\n");
}

/// j2 errors and limits: trace, `error()`, step budget, depth, heap cap, recovery.
#[test]
fn j2_limits_test_runs_on_both_backends() {
    run("j2limits", include_str!("../../../stdlib/tests/j2_limits_test.j3"), "j2_limits_test ok\n");
}

/// j2 GC: churn stays small, keep/release, weak intern table, stress-mode collections, the heap cap.
#[test]
fn j2_gc_test_runs_on_both_backends() {
    run("j2gc", include_str!("../../../stdlib/tests/j2_gc_test.j3"), "j2_gc_test ok\n");
}

/// j2 built-in libs: one case per function, argument errors, callbacks under stress mode, the seed.
#[test]
fn j2_lib_test_runs_on_both_backends() {
    run("j2lib", include_str!("../../../stdlib/tests/j2_lib_test.j3"), "j2_lib_test ok\n");
}

/// `j3.steam` without Steam (fake mode, layouts). LLVM only: `j3.steam.game` names `j3.app`, which the C backend cannot emit.
#[test]
fn steam_test_runs() {
    let dir = std::env::temp_dir().join(format!("j3-steam1-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.j3");
    std::fs::write(&file, include_str!("../../../stdlib/tests/steam_test.j3")).unwrap();
    let prog = compile_path(&file).expect("check");
    let r = emit::run_program(&prog.cx, &prog.modules, &llvm_opts()).expect("run");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(String::from_utf8_lossy(&r.stdout), "steam_test ok\n");
}

/// `j3.steam` achievements, stats and leaderboards without Steam (absent, fake mode, both struct layouts). LLVM only, like `steam_test_runs`.
#[test]
fn steam_stats_test_runs() {
    let dir = std::env::temp_dir().join(format!("j3-steam2-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.j3");
    std::fs::write(&file, include_str!("../../../stdlib/tests/steam_stats_test.j3")).unwrap();
    let prog = compile_path(&file).expect("check");
    let r = emit::run_program(&prog.cx, &prog.modules, &llvm_opts()).expect("run");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(String::from_utf8_lossy(&r.stdout), "steam_stats_test ok\n");
}

/// `j3.steam` friends, overlay, utils, screenshots and timeline without Steam (absent, fake mode, both struct layouts). LLVM only, like `steam_test_runs`.
#[test]
fn steam_social_test_runs() {
    let dir = std::env::temp_dir().join(format!("j3-steam3-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.j3");
    std::fs::write(&file, include_str!("../../../stdlib/tests/steam_social_test.j3")).unwrap();
    let prog = compile_path(&file).expect("check");
    let r = emit::run_program(&prog.cx, &prog.modules, &llvm_opts()).expect("run");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(String::from_utf8_lossy(&r.stdout), "steam_social_test ok\n");
}

/// `j3.steam` lobbies and matchmaking without Steam (absent, fake mode, both struct layouts). LLVM only, like `steam_test_runs`.
#[test]
fn steam_lobby_test_runs() {
    let dir = std::env::temp_dir().join(format!("j3-steam7-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.j3");
    std::fs::write(&file, include_str!("../../../stdlib/tests/steam_lobby_test.j3")).unwrap();
    let prog = compile_path(&file).expect("check");
    let r = emit::run_program(&prog.cx, &prog.modules, &llvm_opts()).expect("run");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(String::from_utf8_lossy(&r.stdout), "steam_lobby_test ok\n");
}

/// `j3.steam` networking messages, relay status and shared networking types without Steam (absent, fake mode, both struct layouts). LLVM only, like `steam_test_runs`.
#[test]
fn steam_net_test_runs() {
    let dir = std::env::temp_dir().join(format!("j3-steam8-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.j3");
    std::fs::write(&file, include_str!("../../../stdlib/tests/steam_net_test.j3")).unwrap();
    let prog = compile_path(&file).expect("check");
    let r = emit::run_program(&prog.cx, &prog.modules, &llvm_opts()).expect("run");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(String::from_utf8_lossy(&r.stdout), "steam_net_test ok\n");
}

/// `j3.steam` P2P sockets (ISteamNetworkingSockets: fake loopback, poll groups, callbacks 1221/1222) without Steam (absent, fake mode, both struct layouts). LLVM only, like `steam_test_runs`.
#[test]
fn steam_sock_test_runs() {
    let dir = std::env::temp_dir().join(format!("j3-steam8b-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.j3");
    std::fs::write(&file, include_str!("../../../stdlib/tests/steam_sock_test.j3")).unwrap();
    let prog = compile_path(&file).expect("check");
    let r = emit::run_program(&prog.cx, &prog.modules, &llvm_opts()).expect("run");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(String::from_utf8_lossy(&r.stdout), "steam_sock_test ok\n");
}

/// `j3.steam` apps, DLC and Steam Cloud (plus `userDir`) without Steam (absent, fake mode, both struct layouts). LLVM only, like `steam_test_runs`.
#[test]
fn steam_cloud_test_runs() {
    let dir = std::env::temp_dir().join(format!("j3-steam4-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.j3");
    std::fs::write(&file, include_str!("../../../stdlib/tests/steam_cloud_test.j3")).unwrap();
    let prog = compile_path(&file).expect("check");
    let r = emit::run_program(&prog.cx, &prog.modules, &llvm_opts()).expect("run");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(String::from_utf8_lossy(&r.stdout), "steam_cloud_test ok\n");
}

/// `j3.steam` Steam Input without Steam (absent, fake pads, caches, both struct layouts). LLVM only, like `steam_test_runs`.
#[test]
fn steam_input_test_runs() {
    let dir = std::env::temp_dir().join(format!("j3-steam5-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.j3");
    std::fs::write(&file, include_str!("../../../stdlib/tests/steam_input_test.j3")).unwrap();
    let prog = compile_path(&file).expect("check");
    let r = emit::run_program(&prog.cx, &prog.modules, &llvm_opts()).expect("run");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(String::from_utf8_lossy(&r.stdout), "steam_input_test ok\n");
}

/// `j3.sys.ARCH` and `ARCH_NAME` follow the host CPU (the loader rewrites them from the target triple).
#[test]
fn sys_arch_matches_host() {
    let dir = std::env::temp_dir().join(format!("j3-arch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.j3");
    std::fs::write(
        &file,
        "module main;\nuse j3.io.printLn;\nuse j3.sys;\npub fn main() {\n    printLn(`{sys.ARCH_NAME} {sys.ARCH}`);\n}\n",
    )
    .unwrap();
    let prog = compile_path(&file).expect("check");
    let r = emit::run_program(&prog.cx, &prog.modules, &llvm_opts()).expect("run");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
    let want = match std::env::consts::ARCH {
        "x86_64" => "x86_64 0",
        "aarch64" => "aarch64 1",
        other => panic!("unlisted host {other}"),
    };
    assert_eq!(String::from_utf8_lossy(&r.stdout).trim(), want);
}

/// `Map.get` names `Option` in its signature; a user module that never imports it must still compile.
#[test]
fn generic_signature_names_resolve_in_the_declaring_module() {
    run(
        "map_noopt",
        "module t;\nuse j3.data.map.Map;\nuse j3.io.printLn;\nuse j3.memory.{ Arena, List };\n\
fn main() {\n    let mem = Arena.create(4096);\n    let m = Map<int, int>.new(&mem, 4);\n    m.put(1, 2);\n\
    let l = List<int>.new(&mem);\n    l.push(3);\n    printLn(`{m.getOr(1, 0)} {l.len}`);\n}\n",
        "2 1\n",
    );
}

/// Idle frame skipping (chunk 26): the loop's decision and `Dom.idleMs`, no window. LLVM only: the C backend cannot emit a `Dom` yet.
#[test]
fn idle_test_runs() {
    let dir = std::env::temp_dir().join(format!("j3-idle26-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.j3");
    std::fs::write(&file, include_str!("../../../stdlib/tests/idle_test.j3")).unwrap();
    let prog = compile_path(&file).expect("check");
    let r = emit::run_program(&prog.cx, &prog.modules, &llvm_opts()).expect("run");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(String::from_utf8_lossy(&r.stdout), "idle_test ok\n");
}

/// `on:mousedown` / `on:mouseup` / `on:contextmenu` bubble to ancestors and `|stop` ends the walk. LLVM only (a `Dom`).
#[test]
fn bubble_test_runs() {
    let dir = std::env::temp_dir().join(format!("j3-bubble-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.j3");
    std::fs::write(&file, include_str!("../../../stdlib/tests/bubble_test.j3")).unwrap();
    let prog = compile_path(&file).expect("check");
    let r = emit::run_program(&prog.cx, &prog.modules, &llvm_opts()).expect("run");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(r.status, 0, "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(String::from_utf8_lossy(&r.stdout), "bubble_test ok\n");
}

/// `j3.term` and `j3.term.ansi` (chunk 1 of plan 031): parser events, every split point, SGR, strip, runs, fuzz.
#[test]
fn term_ansi_test_runs_on_both_backends() {
    run("term_ansi", include_str!("../../../stdlib/tests/term_ansi_test.j3"), "term_ansi_test ok\n");
}

/// `j3.ui.ansi` (chunk 5 of plan 031): AnsiLog segs, colors, split feeds, dropped escapes, CR overwrite.
#[test]
fn ui_ansi_test_runs_on_both_backends() {
    run("ui_ansi", include_str!("../../../stdlib/tests/ui_ansi_test.j3"), "ui_ansi_test ok\n");
}

/// `j3.term.width` (chunk 2 of plan 031): cpWidth over controls, ASCII, combining, CJK, Hangul, emoji and range edges.
#[test]
fn term_width_test_runs_on_both_backends() {
    run("term_width", include_str!("../../../stdlib/tests/term_width_test.j3"), "term_width_test ok\n");
}

/// `j3.term.grid` (chunk 3 of plan 031): cursor, deferred wrap, regions, erase, wide chars, alt screen, resize, scrollback,
/// replies, 1 MB of text, and captured program output. Reads `stdlib/tests/term_fixtures/` relative to the crate root.
#[test]
fn term_grid_test_runs_on_both_backends() {
    run("term_grid", include_str!("../../../stdlib/tests/term_grid_test.j3"), "term_grid_test ok\n");
}

/// `j3.term.pty` (chunk 6 of plan 031): a program on a pty, size, colors, controlling tty, resize, missing program, env.
#[test]
fn term_pty_test_runs_on_both_backends() {
    run("term_pty", include_str!("../../../stdlib/tests/term_pty_test.j3"), "term_pty_test ok\n");
}

/// `j3.term.tty` and `j3.term.screen` (chunk 7 of plan 031): frames drawn to a pipe replay into a Grid equal to the back
/// buffer (styles, wide chars, clip, resize, bottom-right cell), small diffs, raw mode failing on a pipe, and the exit hook
/// restoring the terminal when a forked child panics.
#[test]
fn term_screen_test_runs_on_both_backends() {
    run("term_screen", include_str!("../../../stdlib/tests/term_screen_test.j3"), "term_screen_test ok\n");
}

/// `j3.term.input` (chunk 4 of plan 031): key, paste, mouse and focus decode (split feeds, ESC timeout, garbage), encode in
/// both cursor-key modes, paste injection, and an encode-then-decode round trip.
/// Kitty graphics in `j3.term.grid`: transmit (RGBA, RGB, zlib, chunks), put, query, delete, scrolling, ED, RIS.
#[test]
fn term_kitty_test_runs_on_both_backends() {
    run("term_kitty", include_str!("../../../stdlib/tests/term_kitty_test.j3"), "term_kitty_test ok\n");
}

#[test]
fn term_input_test_runs_on_both_backends() {
    run("term_input", include_str!("../../../stdlib/tests/term_input_test.j3"), "term_input_test ok\n");
}

/// `j3.instance` request/reply: ping, deferred reply, 1 MB reply, timeout, a killed client, claim messages beside requests.
#[test]
fn instance_test_runs_on_both_backends() {
    run("instance", include_str!("../../../stdlib/tests/instance_test.j3"), "instance_test ok\n");
}
