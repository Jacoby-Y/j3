use super::*;
use crate::emit;
use crate::ir::dump::dump_program;

fn llvm_opts() -> emit::EmitOpts {
    let mut o = emit::EmitOpts::default();
    o.backend = emit::Backend::Llvm;
    if !emit::clang_available(&o.clang) {
        o.backend = emit::Backend::C;
    }
    o
}

fn exec(prog: &Program) -> emit::RunResult {
    emit::run_program(&prog.cx, &prog.modules, &llvm_opts()).expect("emit/run")
}

#[test]
fn sys_start_symbol_is_stable_when_entry_grows() {
    fn symbol(src: &str) -> String {
        let dir = std::env::temp_dir().join(format!(
            "j3-sym-{}-{}",
            std::process::id(),
            src.len()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("main.j3");
        std::fs::write(&file, src).unwrap();
        let prog = compile_path(&file).expect("check");
        let ll = emit::emit_llvm(&prog.cx, &prog.modules, &llvm_opts()).expect("llvm");
        let needle = "j3_j3_sys_start_";
        let i = ll.find(needle).expect("j3.sys.start symbol");
        let rest = &ll[i..];
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        let sym = rest[..end].to_string();
        let _ = std::fs::remove_dir_all(&dir);
        sym
    }
    let plain = symbol("module a;\n\nfn main() {\n}\n");
    let extra = symbol("module a;\n\nfn extra() {\n}\n\nfn main() {\n}\n");
    assert_eq!(plain, extra, "a new entry function renamed j3.sys.start");
}

/// A failed runtime check exits 102 and a `panic` exits 101; both say `file:line`.
#[test]
fn runtime_failures_name_the_place_and_use_distinct_exit_codes() {
    fn run(name: &str, src: &str) -> emit::RunResult {
        let dir = std::env::temp_dir().join(format!("j3-fail-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("main.j3");
        std::fs::write(&file, src).unwrap();
        let prog = compile_path(&file).expect("check");
        let out = exec(&prog);
        let _ = std::fs::remove_dir_all(&dir);
        out
    }
    let oob = run(
        "oob",
        "module a;\n\nuse j3.sys.argCount;\n\nfn main() {\n    let xs: [2]int = [1, 2];\n    let i = 5 + argCount();\n    xs[i] = 3;\n}\n",
    );
    let err = String::from_utf8_lossy(&oob.stderr);
    assert_eq!(oob.status, 102, "{err}");
    assert!(err.contains("main.j3:8: index out of bounds: index 6, len 2"), "{err}");
    if cfg!(all(target_arch = "x86_64", target_os = "linux")) && std::process::Command::new("addr2line").arg("--version").output().is_ok() {
        // debug builds walk the frames on a failed check; `symbolize` names them
        assert!(err.contains("called from:") && err.contains("a.main at"), "{err}");
    }
    let boom = run("panic", "module a;\n\nuse j3.sys.panic;\n\nfn main() {\n    panic(\"boom\");\n}\n");
    let err = String::from_utf8_lossy(&boom.stderr);
    assert_eq!(boom.status, 101, "{err}");
    assert!(err.contains("main.j3:6: boom"), "{err}");
}

/// `f().len` and `f().x` work on a call that returns a string, a slice or a struct by value
/// (the call is spilled to a temp, as for a call that returns a reference).
#[test]
fn field_of_a_call_result() {
    let dir = std::env::temp_dir().join(format!("j3-callfield-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.j3");
    std::fs::write(
        &file,
        "module a;\n\nuse j3.io.printLn;\n\nstruct P {\n    x: int;\n}\n\nfn name(): string {\n    return \"abc\";\n}\n\nfn pt(): P {\n    return P { x: 7 };\n}\n\nfn main() {\n    printLn(`{name().len} {pt().x}`);\n}\n",
    )
    .unwrap();
    let prog = compile_path(&file).expect("check");
    let out = exec(&prog);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "3 7");
}

/// `main/*.j3` that say `module a;` are more of module `a` (private names shared); one that
/// names another module is not merged.
#[test]
fn module_parts_share_one_namespace() {
    let dir = std::env::temp_dir().join(format!("j3-parts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("main")).unwrap();
    let file = dir.join("main.j3");
    std::fs::write(
        &file,
        "module a;\n\nuse j3.io.printLn;\n\nfn main() {\n    printLn(`{helper(2)}`);\n}\n",
    )
    .unwrap();
    std::fs::write(dir.join("main/more.j3"), "module a;\n\nfn helper(x: int): int {\n    return x * 21;\n}\n").unwrap();
    std::fs::write(dir.join("main/other.j3"), "module a.other;\n\nfn helper(x: int): int {\n    return 0;\n}\n").unwrap();
    let prog = compile_path(&file).expect("check");
    let out = exec(&prog);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "42");
}

#[test]
fn thread_attr_and_atomics_are_checked() {
    let err = compile_src("thread_fn", "module t;\n#[thread]\nfn main() {}\n").err().expect("must fail");
    assert!(err.contains("#[thread]"), "{err}");
    let err = compile_src("thread_plain_let", "module t;\n#[thread]\nlet x: int = 0;\nfn main() {}\n")
        .err().expect("must fail");
    assert!(err.contains("singleton"), "{err}");
    let err = compile_src("atomic_arity", "module t;\nfn main() { let x = 0; @atomicAdd(&x); }\n")
        .err().expect("must fail");
    assert!(err.contains("atomicAdd"), "{err}");
    let ok = compile_src(
        "atomic_ok",
        "module t;\nfn main() { let x = 0; @atomicAdd(&x, 1); let b = @atomicCas(&x, 1, 2); }\n",
    );
    assert!(ok.is_ok(), "{:?}", ok.err());
}

#[test]
fn signed_div_min_is_a_branch() {
    let path = std::env::temp_dir().join(format!(
        "j3c_signed_div_min_{}.j3",
        std::process::id()
    ));
    std::fs::write(
        &path,
        // Plan wrote `-9223372036854775808`; parser rejects that literal
        // (unary-minus of an out-of-range positive). Equivalent MIN:
        r#"module t;
fn main() {
    let a = -9223372036854775807 - 1;
    let b = -1;
    let q = a / b;
    let _ = q;
}
"#,
    )
    .expect("write temp");
    let prog = compile_path(&path).expect("compile");
    let _ = std::fs::remove_file(&path);
    let ll = emit::emit_llvm(&prog.cx, &prog.modules, &emit::EmitOpts::default())
        .expect("emit llvm");
    assert!(ll.contains("sdiv "), "{ll}");
    assert!(ll.contains("phi "), "{ll}");
    for line in ll.lines() {
        assert!(
            !(line.contains("select i1") && line.contains("-9223372036854775808")),
            "select+min on one line:\n{line}\n{ll}"
        );
    }
}

#[test]
fn defer_is_block_scoped() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/programs/defer_scope.j3");
    let prog = compile_path(&path).expect("compile defer_scope");
    let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}\nc:\n{c}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "a\ninner\nb\ni=0\nd\nd\ndone\n"
    );
}

#[test]
fn import_cycle_is_error() {
    let a = parse_file(1, "a.j3", "module a;\nuse b;\nfn main() {}\n");
    let b = parse_file(2, "b.j3", "module b;\nuse a;\n");
    let err = check_import_cycles(&[a, b]).unwrap_err();
    assert!(err.contains("import cycle"), "{err}");
}

fn parse_file(id: u32, path: &str, src: &str) -> ParsedFile {
    let ast = parse::parse(src, FileId(id)).expect(path);
    ParsedFile {
        path: path.to_string(),
        src: src.to_string(),
        ast,
        file_id: FileId(id),
    }
}

#[test]
fn occur_mutual_is_error() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/programs/honesty/occur_mutual.j3");
    let err = match compile_path(&path) {
        Ok(_) => panic!("expected occur error"),
        Err(e) => e,
    };
    assert!(err.contains("recursive field"), "{err}");
}

#[test]
fn occur_array_is_error() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/programs/honesty/occur_array.j3");
    let err = match compile_path(&path) {
        Ok(_) => panic!("expected occur error"),
        Err(e) => e,
    };
    assert!(err.contains("recursive field"), "{err}");
}


#[test]
fn private_use_is_error() {
    let hidden = include_str!("../../tests/programs/honesty/hidden.j3");
    let user = include_str!("../../tests/programs/honesty/private_use.j3");
    let files = vec![
        parse_file(1, "hidden.j3", hidden),
        parse_file(2, "private_use.j3", user),
    ];
    let err = match check::check_files(files) {
        Ok(_) => panic!("expected private error"),
        Err(e) => e,
    };
    assert!(err.message.contains("private"), "{}", err.message);
}

#[test]
fn type_mismatch_is_rustc_shaped() {
    let files = vec![parse_file(
        1,
        "t.j3",
        "module t;\nfn main() {\n    let x: int = \"hi\";\n}\n",
    )];
    let err = match check::check_files(files) {
        Ok(_) => panic!("expected type error"),
        Err(e) => e.render(&[("t.j3".into(), "module t;\nfn main() {\n    let x: int = \"hi\";\n}\n".into())]),
    };
    assert!(err.contains("error:"), "{err}");
    assert!(err.contains("expected `int`"), "{err}");
    assert!(err.contains("found `string`"), "{err}");
    assert!(err.contains("--> t.j3:"), "{err}");
    assert!(err.contains("let x: int"), "{err}");
}

#[test]
fn unknown_name_suggests_typo() {
    let src = "module t;\nfn main() {\n    let start = 1;\n    let x = statr;\n}\n";
    let files = vec![parse_file(1, "t.j3", src)];
    let err = match check::check_files(files) {
        Ok(_) => panic!("expected unknown name"),
        Err(e) => e,
    };
    assert!(err.message.contains("unknown name `statr`"), "{}", err.message);
    assert!(
        err.helps.iter().any(|h| h.contains("`start`")),
        "{:?}",
        err.helps
    );
}

#[test]
fn generic_method_error_is_in_defining_file() {
    let boxy = include_str!("../../tests/programs/honesty/gen_box.j3");
    let user = include_str!("../../tests/programs/honesty/gen_use.j3");
    let files = vec![
        parse_file(1, "boxy.j3", boxy),
        parse_file(2, "user.j3", user),
    ];
    let err = match check::check_files(files) {
        Ok(_) => panic!("expected generic body error"),
        Err(e) => e,
    };
    assert_eq!(err.path, "boxy.j3", "{}", err.message);
    assert!(
        err.notes.iter().any(|n| n.contains("instantiating")),
        "{:?}",
        err.notes
    );
    assert!(
        err.message.contains("expected `int`") && err.message.contains("found `string`"),
        "{}",
        err.message
    );
}

#[test]
fn singleton_init_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/programs/singleton_init.j3");
    let prog = compile_path(&path).expect("compile singleton");
    let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
    assert!(c.contains("7ll") || c.contains("= 7"), "{c}");
    let run = exec(&prog);
    assert_eq!(run.status, 0, "{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "n=7\n");
}

#[test]
fn fast_strips_abort() {
    let prog = compile_path(&speculative("05_arrays.j3")).expect("compile 05");
    let slow_ir = dump_program(&prog.cx, &prog.modules);
    assert!(slow_ir.contains("abort;"), "{slow_ir}");
    let mut fast_prog = compile_path(&speculative("05_arrays.j3")).expect("compile 05");
    apply_fast(&mut fast_prog);
    let fast_ir = dump_program(&fast_prog.cx, &fast_prog.modules);
    assert!(!fast_ir.contains("abort;"), "{fast_ir}");
}

#[test]
fn fast_keeps_div0() {
    let src = r#"
module t;
fn main() {
    let n = 1;
    let d = 0;
    let q = n / d;
    let _ = q;
}
"#;
    let mut prog = check::check_files(vec![parse_file(1, "t.j3", src)]).expect("check");
    let slow_ir = dump_program(&prog.cx, &prog.modules);
    assert!(slow_ir.contains("abort div0;"), "{slow_ir}");
    apply_fast(&mut prog);
    let fast_ir = dump_program(&prog.cx, &prog.modules);
    assert!(fast_ir.contains("abort div0;"), "{fast_ir}");
}

#[test]
fn read_file_j3_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/programs/read_file.j3");
    let prog = compile_path(&path).expect("compile read_file");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "len=11\n");
}

#[test]
fn gallery_project_typechecks() {
    let toml = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("examples/gallery/j3.toml");
    let m = crate::project::Manifest::load(&toml).expect("j3.toml");
    assert_eq!(m.name, "gallery");
    let prog = compile_manifest(&m).expect("compile gallery");
    let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
    assert!(c.contains("use lib \"SDL2\""), "{c}");
    assert!(
        prog.modules.iter().any(|mod_| {
            prog.cx.name_str(mod_.name[0]) == "gallery"
                && (mod_.name.len() == 1 || prog.cx.name_str(mod_.name[1]) == "world")
        }),
        "expected gallery + gallery.world"
    );
}

fn honesty(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/programs/honesty")
        .join(name)
}

fn run_ok(path: &Path, stdout: &str) {
    let prog = compile_path(path).expect("compile");
    let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
    assert!(!c.contains("({"), "GNU statement-expr in C:\n{c}");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}\nc:\n{c}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), stdout);
}

fn run_both_ok(path: &Path, stdout: &str) {
    let prog = compile_path(path).expect("compile");
    for backend in [emit::Backend::Llvm, emit::Backend::C] {
        let mut o = emit::EmitOpts::default();
        o.backend = backend;
        if backend == emit::Backend::Llvm && !emit::clang_available(&o.clang) {
            continue;
        }
        let run = emit::run_program(&prog.cx, &prog.modules, &o).expect("run");
        assert_eq!(run.status, 0, "{backend:?} stderr {}", String::from_utf8_lossy(&run.stderr));
        assert_eq!(String::from_utf8_lossy(&run.stdout), stdout, "{backend:?}");
    }
}

/// Compile `src` from a temp file (so the stdlib loads).
fn compile_src(tag: &str, src: &str) -> Result<Program, String> {
    let path = std::env::temp_dir().join(format!("j3c_{tag}_{}.j3", std::process::id()));
    std::fs::write(&path, src).expect("write temp");
    let prog = compile_path(&path).map_err(|e| format!("{e:?}"));
    let _ = std::fs::remove_file(&path);
    prog
}

fn user_dump(prog: &Program) -> String {
    let mut out = String::new();
    for module in &prog.modules {
        if prog.cx.name_str(module.name[0]) == "j3" {
            continue;
        }
        crate::ir::dump::dump_module_into(&prog.cx, module, &mut out);
        out.push('\n');
    }
    out
}

fn assert_golden(name: &str, got: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/ir/goldens")
        .join(name);
    if std::env::var("J3C_WRITE_GOLDENS").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, got).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("missing golden {}: {e} (J3C_WRITE_GOLDENS=1 to create)", path.display())
    });
    assert_eq!(want, got, "golden {name} drifted");
}

#[test]
fn ir_goldens_01_through_08_and_samples() {
    let samples = crate::ir::samples::place_samples();
    assert_golden(
        "samples.ir",
        &crate::ir::dump::dump_module(&samples.cx, &samples.module),
    );
    for name in [
        "01_basics.j3",
        "02_data.j3",
        "03_embedding.j3",
        "04_control.j3",
        "05_arrays.j3",
        "06_enums_errors.j3",
        "07_memory.j3",
        "08_game_loop.j3",
    ] {
        let prog = if name == "08_game_loop.j3" {
            compile_path_headless(&speculative(name)).expect(name)
        } else {
            compile_path(&speculative(name)).expect(name)
        };
        let stem = name.trim_end_matches(".j3");
        assert_golden(&format!("{stem}.ir"), &user_dump(&prog));
    }
}



mod atomics;
mod bitops;
mod closures;
mod deps;
mod extend;
mod f16;
mod generic_fns;
mod simd;
mod stdlib_cmp;
mod strcmp;
mod traits;
mod shaders;
mod ufcs;
fn speculative(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("speculative")
        .join(name)
}

/// A throwaway project under the temp dir.
fn temp_project(name: &str, files: &[(&str, &str)]) -> Manifest {
    static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("j3c_{name}_{}_{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    Manifest::load(&root.join("j3.toml")).expect("temp manifest")
}

/// Runs `script` (a `.j3s` body with `pub fn run()`) compiled in on LLVM and C, then on the VM,
/// and checks each prints `want`.
fn three_ways(name: &str, script: &str, want: &str) {
    three_ways_with(name, &[], script, want);
}

/// `three_ways` with extra project files (`("lib.j3", src)`) the script can `use`.
fn three_ways_with(name: &str, extra: &[(&str, &str)], script: &str, want: &str) {
    let main = "module lang;\nconst S = @script(\"scripts/s.j3s\");\nfn main() {\n    let s = S {};\n    s.run();\n}\n";
    let toml = "[package]\nname = \"lang\"\nentry = \"main.j3\"\n\n[emit]\nopt = 0\n";
    let mut files = vec![("j3.toml", toml), ("main.j3", main), ("scripts/s.j3s", script)];
    files.extend_from_slice(extra);
    let m = temp_project(name, &files);
    let aot = compile_manifest(&m).expect("aot check");
    for backend in [emit::Backend::Llvm, emit::Backend::C] {
        let mut opts = llvm_opts();
        opts.backend = backend;
        let r = emit::run_program(&aot.cx, &aot.modules, &opts).expect("aot run");
        assert_eq!(r.status, 0, "{backend:?}: {}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), want, "{backend:?}");
    }
    let mut prog = compile_manifest_dev(&m).expect("dev check");
    let bytecode = dev::bytecode(&prog).expect("bytecode");
    for module in &bytecode {
        module.write_to(&m.root).expect("write bytecode");
    }
    dev::lower(&mut prog, &bytecode).expect("dev lower");
    let mut opts = llvm_opts();
    opts.cwd = Some(m.root.clone());
    let r = emit::run_program(&prog.cx, &prog.modules, &opts).expect("vm run");
    assert_eq!(r.status, 0, "vm: {}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(String::from_utf8_lossy(&r.stdout), want, "vm");
    let _ = std::fs::remove_dir_all(&m.root);
}

fn check_err(src: &str) -> String {
    match check::check_files(vec![parse_file(1, "t.j3", src)]) {
        Ok(_) => panic!("expected a check error"),
        Err(e) => e.message,
    }
}

/// Runs a native program on LLVM and C and returns both results.
fn both_backends(name: &str, src: &str) -> Vec<emit::RunResult> {
    both_backends_opt(name, src, "0")
}

/// `both_backends` at C compiler opt level `opt` ("0", "2", "s", ...).
fn both_backends_opt(name: &str, src: &str, opt: &str) -> Vec<emit::RunResult> {
    let dir = std::env::temp_dir().join(format!("j3-both-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.j3");
    std::fs::write(&file, src).unwrap();
    let prog = compile_path(&file).expect("check");
    let mut out = Vec::new();
    for backend in [emit::Backend::Llvm, emit::Backend::C] {
        let mut opts = llvm_opts();
        opts.backend = backend;
        opts.cc.opt = opt.to_string();
        out.push(emit::run_program(&prog.cx, &prog.modules, &opts).expect("run"));
    }
    let _ = std::fs::remove_dir_all(&dir);
    out
}

mod defaults;
mod diag;
mod gaps;
mod samples;
mod script_dev;
mod script_scene;
mod statements;
mod stdlib_programs;
mod targets;
