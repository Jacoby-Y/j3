//! Scripts: AOT golden, dev-mode VM, live edits and the scripted example.

use super::*;

const DOOR_OUTPUT: &str = "door 1: open false t 0.8 opened 2\ndoor 2: open true t 0.5 opened 2\n\
door 3: open true t 0.5 opened 3\nangles 80.64 60 45\n";

fn script_demo() -> Manifest {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../speculative/11_script");
    Manifest::load(&root.join("j3.toml")).expect("script manifest")
}

#[test]
fn script_aot_ir_golden_and_both_backends() {
    let prog = compile_manifest(&script_demo()).expect("script checks");
    assert_golden("11_script.ir", &user_dump(&prog));
    let bytecode = dev::bytecode(&prog).expect("script bytecode");
    assert_eq!(bytecode.len(), 1);
    assert_golden("11_script.bc", &bytecode[0].dump());
    assert_eq!(&bytecode[0].encode()[..4], b"J3SB");
    for backend in [emit::Backend::Llvm, emit::Backend::C] {
        let mut opts = llvm_opts();
        opts.backend = backend;
        let result = emit::run_program(&prog.cx, &prog.modules, &opts).expect("script run");
        assert_eq!(result.status, 0, "{}", String::from_utf8_lossy(&result.stderr));
        assert_eq!(String::from_utf8_lossy(&result.stdout), DOOR_OUTPUT);
    }
}

/// Dev mode: every script call goes through the J3 VM, and the output matches AOT.
#[test]
fn script_dev_mode_matches_aot_on_both_backends() {
    let manifest = script_demo();
    let mut prog = compile_manifest_dev(&manifest).expect("dev check");
    let bytecode = dev::bytecode(&prog).expect("bytecode");
    for module in &bytecode {
        module.write_to(&manifest.root).expect("write bytecode");
    }
    dev::lower(&mut prog, &bytecode).expect("dev lower");
    for backend in [emit::Backend::Llvm, emit::Backend::C] {
        let mut opts = llvm_opts();
        opts.backend = backend;
        opts.cwd = Some(manifest.root.clone());
        let result = emit::run_program(&prog.cx, &prog.modules, &opts).expect("dev run");
        assert_eq!(result.status, 0, "{}", String::from_utf8_lossy(&result.stderr));
        assert_eq!(String::from_utf8_lossy(&result.stdout), DOOR_OUTPUT, "{backend:?}");
    }
}

pub(super) const TICK_TOML: &str = "[package]\nname = \"ticker\"\nentry = \"main.j3\"\n\n[emit]\nopt = 0\n";
const TICK_MAIN: &str = "module ticker;\n\
use j3.memory.frameHook;\n\
const Tick = @script(\"scripts/tick.j3s\");\n\
fn sleepMs(ms: int) { let ts: [2]int = [0, ms * 1000000]; @syscall(35, &ts[0] as ptr, 0); }\n\
fn main() {\n    let tick = Tick {};\n    for i in 0..400 {\n        frameHook();\n        tick.say(i);\n        sleepMs(10);\n    }\n}\n";
const TICK_SCRIPT: &str = "use j3.io.printLn;\n\nprops {\n    gain: int = 1;\n}\n\n\
pub fn say(i: int) {\n    if i % 10 == 0 { printLn(`value {gain}`); }\n}\n";

fn first_line(child: &mut std::process::Child) -> String {
    use std::io::BufRead;
    let out = child.stdout.as_mut().expect("piped stdout");
    let mut line = String::new();
    std::io::BufReader::new(out).read_line(&mut line).expect("read line");
    line
}

fn launch(exe: &Path, root: &Path) -> std::process::Child {
    std::process::Command::new(exe)
        .current_dir(root)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("launch dev binary")
}

pub(super) fn dev_built(m: &Manifest) -> crate::devrun::Built {
    match crate::devrun::prepare(m, &llvm_opts(), false).expect("prepare") {
        crate::devrun::Prepared::Dev(built) => built,
        crate::devrun::Prepared::NoScripts(_) => panic!("expected scripts"),
    }
}

/// Editing a script (a constant, a new prop with a default, new state) never runs clang.
#[test]
fn script_edits_rerun_without_clang() {
    let m = temp_project(
        "script_edits",
        &[("j3.toml", TICK_TOML), ("main.j3", TICK_MAIN), ("scripts/tick.j3s", TICK_SCRIPT)],
    );
    let first = dev_built(&m);
    assert!(first.rebuilt, "first dev build compiles the program");
    let edits = [
        ("value {gain}", "value {gain} b"),
        ("    gain: int = 1;\n", "    gain: int = 1;\n    extra: int = 7;\n"),
        ("pub fn say", "let seen = 3;\n\npub fn say"),
        ("value {gain} b", "value {gain} {extra} {seen}"),
    ];
    let path = m.root.join("scripts/tick.j3s");
    for (from, to) in edits {
        let text = std::fs::read_to_string(&path).unwrap().replace(from, to);
        std::fs::write(&path, text).unwrap();
        let started = std::time::Instant::now();
        let built = dev_built(&m);
        assert!(!built.rebuilt, "script edit `{to}` ran clang");
        let mut child = launch(&built.exe, &m.root);
        let line = first_line(&mut child);
        let elapsed = started.elapsed();
        let _ = child.kill();
        let _ = child.wait();
        assert!(elapsed.as_millis() < 2000, "edit to first line took {elapsed:?}");
        if to.contains("{extra} {seen}") {
            assert_eq!(line, "value 1 7 3\n");
        }
    }
    let _ = std::fs::remove_dir_all(&m.root);
}

/// A script that needs a module the program never loaded rebuilds the program.
#[test]
fn script_new_native_module_rebuilds() {
    let m = temp_project(
        "script_rebuild",
        &[("j3.toml", TICK_TOML), ("main.j3", TICK_MAIN), ("scripts/tick.j3s", TICK_SCRIPT)],
    );
    dev_built(&m);
    let path = m.root.join("scripts/tick.j3s");
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace("use j3.io.printLn;", "use j3.io.printLn;\nuse j3.rand.Rng;")
        .replace("value {gain}", "value {gain} {roll()}");
    let text = format!("{text}\nfn roll(): int {{\n    let r = Rng.new(5);\n    return r.next() % 1;\n}}\n");
    std::fs::write(&path, text).unwrap();
    let built = dev_built(&m);
    assert!(built.rebuilt, "a new native module needs a program rebuild");
    let mut child = launch(&built.exe, &m.root);
    assert_eq!(first_line(&mut child), "value 1 0\n");
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&m.root);
}

/// While the program runs, a code edit swaps in at the next `frameHook`.
#[test]
fn script_code_edit_swaps_live() {
    use std::io::BufRead;
    let m = temp_project(
        "script_swap",
        &[("j3.toml", TICK_TOML), ("main.j3", TICK_MAIN), ("scripts/tick.j3s", TICK_SCRIPT)],
    );
    let built = dev_built(&m);
    let mut child = launch(&built.exe, &m.root);
    let out = child.stdout.take().expect("stdout");
    let mut lines = std::io::BufReader::new(out).lines();
    assert_eq!(lines.next().unwrap().unwrap(), "value 1");
    let path = m.root.join("scripts/tick.j3s");
    let text = std::fs::read_to_string(&path).unwrap().replace("value {gain}", "swapped {gain}");
    std::fs::write(&path, text).unwrap();
    let started = std::time::Instant::now();
    let next = dev_built(&m);
    assert!(!next.rebuilt);
    assert_eq!(next.layouts, built.layouts, "a code edit keeps the layout");
    crate::devrun::bump_reload(&m.root);
    let mut swapped = false;
    for line in lines.by_ref() {
        if line.unwrap() == "swapped 1" {
            swapped = true;
            break;
        }
    }
    let elapsed = started.elapsed();
    let _ = child.kill();
    let _ = child.wait();
    assert!(swapped, "the running program never picked up the edit");
    assert!(elapsed.as_millis() < 1000, "swap took {elapsed:?}");
    let _ = std::fs::remove_dir_all(&m.root);
}

/// `examples/scripted`: the same output from the VM (dev) and compiled in (release).
#[test]
fn scripted_example_same_in_dev_and_release() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/scripted");
    let manifest = Manifest::load(&root.join("j3.toml")).expect("example manifest");
    let release = compile_manifest(&manifest).expect("release check");
    let aot = emit::run_program(&release.cx, &release.modules, &llvm_opts()).expect("release run");
    assert_eq!(aot.status, 0, "{}", String::from_utf8_lossy(&aot.stderr));
    let mut prog = compile_manifest_dev(&manifest).expect("dev check");
    let bytecode = dev::bytecode(&prog).expect("bytecode");
    assert_eq!(bytecode.len(), 2);
    for module in &bytecode {
        module.write_to(&manifest.root).expect("write bytecode");
    }
    dev::lower(&mut prog, &bytecode).expect("dev lower");
    let mut opts = llvm_opts();
    opts.cwd = Some(manifest.root.clone());
    let vm = emit::run_program(&prog.cx, &prog.modules, &opts).expect("dev run");
    assert_eq!(vm.status, 0, "{}", String::from_utf8_lossy(&vm.stderr));
    let out = String::from_utf8_lossy(&aot.stdout);
    assert!(out.contains("Bo is down (2 so far)"), "{out}");
    assert_eq!(out, String::from_utf8_lossy(&vm.stdout));
}
