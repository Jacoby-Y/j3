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
    assert!(err.contains("main.j3:8: index out of bounds"), "{err}");
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
fn hello_j3_prints_hello_world() {
    let prog = compile_path(&default_hello_path()).expect("compile hello.j3");
    let dump = dump_program(&prog.cx, &prog.modules);
    assert!(dump.contains("printLn"), "{dump}");
    assert!(dump.contains("@syscall("), "{dump}");
    assert!(dump.contains("Hello, world!"), "{dump}");
    assert!(dump.contains("fn start()"), "{dump}");

    let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
    assert!(c.contains("#line "), "missing #line in C:\n{c}");
    let ll = emit::emit_llvm(&prog.cx, &prog.modules, &emit::EmitOpts::default()).expect("emit llvm");
    assert!(ll.contains("target triple"), "{ll}");
    assert!(ll.contains("define i32 @main(i32 %argc, ptr %argv)"), "{ll}");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "Hello, world!\n");
}

#[test]
fn c_backend_still_runs_hello() {
    let prog = compile_path(&default_hello_path()).expect("compile hello.j3");
    let mut opts = emit::EmitOpts::default();
    opts.backend = emit::Backend::C;
    let run = emit::run_program(&prog.cx, &prog.modules, &opts).expect("c run");
    assert_eq!(
        run.status,
        0,
        "stderr {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "Hello, world!\n");
}

#[test]
fn c_backend_runs_02_data() {
    let prog = compile_path(&speculative("02_data.j3")).expect("compile 02");
    let mut opts = emit::EmitOpts::default();
    opts.backend = emit::Backend::C;
    let run = emit::run_program(&prog.cx, &prog.modules, &opts).expect("c run 02");
    assert_eq!(
        run.status,
        0,
        "stderr {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "len=4.47214 player.pos=Vec2 { x: 12, y: 9 }\n"
    );
}

#[test]
fn c_backend_runs_08_headless() {
    let prog = compile_path_headless(&speculative("08_game_loop.j3")).expect("compile 08");
    let mut opts = emit::EmitOpts::default();
    opts.backend = emit::Backend::C;
    let run = emit::run_program(&prog.cx, &prog.modules, &opts).expect("c run 08");
    assert_eq!(
        run.status,
        0,
        "stderr {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "bye\n");
}

#[test]
fn hud_quad_static_init_has_no_union() {
    let prog = compile_path(&speculative("08_game_loop.j3")).expect("compile 08 sdl");
    let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
    let line = c
        .lines()
        .find(|l| l.contains("HUD_QUAD"))
        .expect("HUD_QUAD line missing");
    assert!(
        !line.contains("union"),
        "HUD_QUAD static init used union:\n{line}"
    );
}

#[test]
fn analyze_hello_and_overlay() {
    let path = default_hello_path();
    let prog = analyze_path(&path, &[]).expect("analyze");
    assert!(prog.cx.files.iter().any(|f| f.path.contains("hello.j3")));
    let src = std::fs::read_to_string(&path).unwrap().replace("Hello, world!", "Hi");
    let overlay = vec![(path.clone(), src)];
    let prog2 = analyze_path(&path, &overlay).expect("overlay");
    let dump = dump_program(&prog2.cx, &prog2.modules);
    assert!(dump.contains("Hi"), "{dump}");
}

#[test]
fn project_emit_cwd_is_package_root() {
    let toml = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("examples/scripted/j3.toml");
    let m = crate::project::Manifest::load(&toml).expect("toml");
    let opts = emit::EmitOpts::from_manifest(&m).expect("opts");
    assert_eq!(opts.cwd.as_ref().unwrap(), &m.root);
}

fn speculative(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("speculative")
        .join(name)
}

#[test]
fn basics_j3_runs() {
    let prog = compile_path(&speculative("01_basics.j3")).expect("compile 01");
    let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
    assert!(!c.contains("({"), "GNU statement-expr in C:\n{c}");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "Hello, world!\nHello, Coby, how are you?\n"
    );
}

#[test]
fn control_j3_runs() {
    let prog = compile_path(&speculative("04_control.j3")).expect("compile 04");
    let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
    assert!(!c.contains("({"), "GNU statement-expr in C:\n{c}");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "hp=3\nn=3\nn=2\nn=1\nabs=4\ni=0\ni=1\ni=3\nhello player\nleaving main\n"
    );
}

#[test]
fn data_j3_runs() {
    let prog = compile_path(&speculative("02_data.j3")).expect("compile 02");
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
        "len=4.47214 player.pos=Vec2 { x: 12, y: 9 }\n"
    );
}

#[test]
fn embedding_j3_runs() {
    let prog = compile_path(&speculative("03_embedding.j3")).expect("compile 03");
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
        "x=10, width=1\nx=10, width=1\n"
    );
}

#[test]
fn arrays_j3_runs() {
    let prog = compile_path(&speculative("05_arrays.j3")).expect("compile 05");
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
        "sum=102 mid0=21\nxs[0]=11\nxs[1]=21\nxs[2]=30\nxs[3]=40\nhi Coby bytes=4\n"
    );
}

#[test]
fn enums_j3_runs() {
    let prog = compile_path(&speculative("06_enums_errors.j3")).expect("compile 06");
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
        "north\nhit 2\nmissing\n"
    );
}

#[test]
fn memory_j3_runs() {
    let prog = compile_path(&speculative("07_memory.j3")).expect("compile 07");
    let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}\nc:\n{c}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "entities=1 hits=1\n");
}

#[test]
fn game_loop_j3_runs() {
    let prog = compile_path_headless(&speculative("08_game_loop.j3")).expect("compile 08");
    let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}\nc:\n{c}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "bye\n");
}

/// The `threads` recipe is a real program: it must give one answer on LLVM and on C.
#[test]
fn threads_recipe_runs_on_both_backends() {
    let r = crate::helptext::recipes().into_iter().find(|r| r.name == "threads").unwrap();
    let src = &r.files.iter().find(|(p, _)| p.ends_with(".j3")).unwrap().1;
    let path = std::env::temp_dir().join(format!("j3-recipe-threads-{}.j3", std::process::id()));
    std::fs::write(&path, src).unwrap();
    let prog = compile_path(&path).expect("recipe compiles");
    let _ = std::fs::remove_file(&path);
    for backend in [emit::Backend::Llvm, emit::Backend::C] {
        let mut opts = emit::EmitOpts::default();
        opts.backend = backend;
        let run = emit::run_program(&prog.cx, &prog.modules, &opts).expect("run");
        assert_eq!(run.status, 0, "stderr {}", String::from_utf8_lossy(&run.stderr));
        assert_eq!(String::from_utf8_lossy(&run.stdout), "accepted=8 stale=1 total=262131\n");
    }
}

#[test]
fn both_backends_run_01_through_08() {
    let cases: &[(&str, bool, &str)] = &[
        (
            "01_basics.j3",
            false,
            "Hello, world!\nHello, Coby, how are you?\n",
        ),
        (
            "02_data.j3",
            false,
            "len=4.47214 player.pos=Vec2 { x: 12, y: 9 }\n",
        ),
        ("03_embedding.j3", false, "x=10, width=1\nx=10, width=1\n"),
        (
            "04_control.j3",
            false,
            "hp=3\nn=3\nn=2\nn=1\nabs=4\ni=0\ni=1\ni=3\nhello player\nleaving main\n",
        ),
        (
            "05_arrays.j3",
            false,
            "sum=102 mid0=21\nxs[0]=11\nxs[1]=21\nxs[2]=30\nxs[3]=40\nhi Coby bytes=4\n",
        ),
        ("06_enums_errors.j3", false, "north\nhit 2\nmissing\n"),
        ("07_memory.j3", false, "entities=1 hits=1\n"),
        ("08_game_loop.j3", true, "bye\n"),
        (
            "13_threads.j3",
            false,
            "locked=40000 atomic=40000 main seen=0\nqueue sum=5050\nsquares total=332833500\ntrySubmit first=true second=false pending=2 ran=2 after=0\n",
        ),
    ];

    for &(name, headless, expected) in cases {
        let prog = if headless {
            compile_path_headless(&speculative(name)).expect(name)
        } else {
            compile_path(&speculative(name)).expect(name)
        };

        let mut llvm_opts = emit::EmitOpts::default();
        llvm_opts.backend = emit::Backend::Llvm;
        if !emit::clang_available(&llvm_opts.clang) {
            panic!("clang required for dual-backend test");
        }
        let llvm_run =
            emit::run_program(&prog.cx, &prog.modules, &llvm_opts).expect("llvm run");

        let mut c_opts = emit::EmitOpts::default();
        c_opts.backend = emit::Backend::C;
        let c_run = emit::run_program(&prog.cx, &prog.modules, &c_opts).expect("c run");

        assert_eq!(
            llvm_run.status,
            0,
            "{name} llvm stderr {}",
            String::from_utf8_lossy(&llvm_run.stderr)
        );
        assert_eq!(
            c_run.status,
            0,
            "{name} c stderr {}",
            String::from_utf8_lossy(&c_run.stderr)
        );
        let llvm_out = String::from_utf8_lossy(&llvm_run.stdout);
        let c_out = String::from_utf8_lossy(&c_run.stdout);
        assert_eq!(llvm_out.as_ref(), expected, "{name} llvm stdout");
        assert_eq!(c_out.as_ref(), expected, "{name} c stdout");
        assert_eq!(
            llvm_out, c_out,
            "{name} llvm vs c\nllvm stderr {}\nc stderr {}",
            String::from_utf8_lossy(&llvm_run.stderr),
            String::from_utf8_lossy(&c_run.stderr)
        );
    }
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
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/defer_scope.j3");
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
fn stdlib_game_j3_typechecks() {
    let game = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stdlib/j3/game.j3");
    let prog = check_path_lib(&game).expect("check shipped j3.game");
    assert!(
        prog.modules.iter().any(|m| {
            m.name.len() == 2
                && prog.cx.name_str(m.name[0]) == "j3"
                && prog.cx.name_str(m.name[1]) == "game"
        }),
        "expected j3.game module"
    );
    let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
    assert!(c.contains("use lib \"SDL2\""), "{c}");

    check_path_lib(&speculative("09_stdlib_game.j3")).expect("check 09");
}

#[test]
fn stdlib_option_result_typecheck() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stdlib/j3");
    check_path_lib(&dir.join("option.j3")).expect("check j3.option");
    check_path_lib(&dir.join("result.j3")).expect("check j3.result");
    check_path_lib(&dir.join("rand.j3")).expect("check j3.rand");
    check_path_lib(&dir.join("math.j3")).expect("check j3.math");
    check_path_lib(&dir.join("game/ecs.j3")).expect("check j3.game.ecs");
    check_path_lib(&dir.join("app.j3")).expect("check j3.app");
    check_path_lib(&dir.join("game/collide.j3")).expect("check j3.game.collide");
    check_path_lib(&dir.join("data/res.j3")).expect("check j3.data.res");
    check_path_lib(&dir.join("data/scene.j3")).expect("check j3.data.scene");
    check_path_lib(&dir.join("data/save.j3")).expect("check j3.data.save");
    check_path_lib(&dir.join("game/tile.j3")).expect("check j3.game.tile");
    check_path_lib(&dir.join("game/fx.j3")).expect("check j3.game.fx");
    check_path_lib(&dir.join("data/debug.j3")).expect("check j3.data.debug");
    check_path_lib(&dir.join("ui.j3")).expect("check j3.ui");
    check_path_lib(&speculative("10_j3ui.j3")).expect("check 10 j3ui");
    check_path_lib(&dir.join("data/assets.j3")).expect("check j3.data.assets");
    check_path_lib(&dir.join("gfx.j3")).expect("check j3.gfx");
    check_path_lib(&dir.join("gfx/mesh.j3")).expect("check j3.gfx.mesh");
    check_path_lib(&dir.join("gfx/voxel.j3")).expect("check j3.gfx.voxel");
}

#[test]
fn math_rand_j3_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/math_rand.j3");
    let prog = compile_path(&path).expect("compile math_rand");
    let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}\nc:\n{c}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "n=12\n");
}

#[test]
fn mesh_fmt_j3_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/mesh.j3");
    let prog = compile_path(&path).expect("compile mesh");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
}

#[test]
fn scene3d_check_j3_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/scene3d_check.j3");
    let prog = compile_path(&path).expect("compile scene3d_check");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
}

#[test]
fn fpwalk_check_j3_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/fpwalk_check.j3");
    let prog = compile_path(&path).expect("compile fpwalk_check");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
}

#[test]
fn cull_check_j3_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/cull_check.j3");
    let prog = compile_path(&path).expect("compile cull_check");
    let run = exec(&prog);
    assert_eq!(run.status, 0, "stderr {}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
}

#[test]
fn bvh_check_j3_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/bvh_check.j3");
    let prog = compile_path(&path).expect("compile bvh_check");
    let run = exec(&prog);
    assert_eq!(run.status, 0, "stderr {}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
}

#[test]
fn png_check_j3_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/png_check.j3");
    let prog = compile_path(&path).expect("compile png_check");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
}

#[test]
fn ray_aabb_j3_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/ray_aabb.j3");
    let prog = compile_path(&path).expect("compile ray_aabb");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
}

#[test]
fn voxel_grid_j3_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/voxel_grid.j3");
    let prog = compile_path(&path).expect("compile voxel_grid");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
}

#[test]
fn voxel_ao_j3_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/voxel_ao.j3");
    let prog = compile_path(&path).expect("compile voxel_ao");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
}

#[test]
fn ecs_basic_j3_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/ecs_basic.j3");
    let prog = compile_path(&path).expect("compile ecs_basic");
    let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}\nc:\n{c}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
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
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/honesty/occur_mutual.j3");
    let err = match compile_path(&path) {
        Ok(_) => panic!("expected occur error"),
        Err(e) => e,
    };
    assert!(err.contains("recursive field"), "{err}");
}

#[test]
fn occur_array_is_error() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/honesty/occur_array.j3");
    let err = match compile_path(&path) {
        Ok(_) => panic!("expected occur error"),
        Err(e) => e,
    };
    assert!(err.contains("recursive field"), "{err}");
}

#[test]
fn left_literal_takes_right_numeric_type() {
    let src = "\
module t;
fn scale(far: f32): f32 {
    return 2 * far;
}
fn below(far: f32): bool {
    return 0 < far;
}
fn keep(): float {
    return 2.0 + 2;
}
fn main() {}
";
    check::check_files(vec![parse_file(1, "t.j3", src)]).expect("left literal");
}

#[test]
fn inclusive_for_runs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/inclusive_for.j3");
    let prog = compile_path(&path).expect("compile inclusive_for");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "0123\n");
}

#[test]
fn for_index_ends_with_the_loop() {
    let src = "\
module t;
fn main() {
    let n = 0;
    for i in 0..2 {
        n = n + i;
    }
    for i in 0..2 {
        n = n + i;
    }
}
";
    check::check_files(vec![parse_file(1, "t.j3", src)]).expect("two fors");
}

#[test]
fn let_then_for_still_shadows() {
    let src = "\
module t;
fn main() {
    let i = 0;
    for i in 0..2 {
    }
}
";
    let err = match check::check_files(vec![parse_file(1, "t.j3", src)]) {
        Ok(_) => panic!("expected shadowing"),
        Err(e) => e,
    };
    assert!(err.message.contains("shadowing"), "{}", err.message);
}

#[test]
fn default_fields_fill_omitted() {
    let src = "\
module t;
struct Hit {
    ok: bool = false;
    x: int = 0;
}
fn main() {
    let a = Hit {};
    let b = Hit { ok: true };
    if a.ok || a.x != 0 || !b.ok || b.x != 0 {
        return;
    }
}
";
    check::check_files(vec![parse_file(1, "t.j3", src)]).expect("defaults");
}

#[test]
fn omitted_hit_texture_mesh_use_zeros() {
    let path = std::env::temp_dir().join(format!(
        "j3c_omitted_hit_texture_mesh_{}.j3",
        std::process::id()
    ));
    let src = "\
module t;
use j3.gfx.voxel.Hit;
use j3.game.Texture;
use j3.gfx.Mesh;
use j3.io.printLn;
fn main() {
    let h = Hit {};
    let tex = Texture {};
    let mesh = Mesh {};
    if h.ok || h.x != 0 || h.face != 0 {
        printLn(\"hit\");
    }
    if tex.w != 0 || tex.h != 0 {
        printLn(\"tex\");
    }
    if mesh.count != 0 || mesh.vao != 0 {
        printLn(\"mesh\");
    }
}
";
    std::fs::write(&path, src).expect("write temp");
    let result = compile_path(&path);
    let _ = std::fs::remove_file(&path);
    let prog = result.expect("compile");
    let run = exec(&prog);
    assert_eq!(
        run.status,
        0,
        "stderr {}\nstdout {}",
        String::from_utf8_lossy(&run.stderr),
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "");
}

#[test]
fn missing_field_without_default_is_error() {
    let src = "\
module t;
struct Pair {
    a: int = 0;
    b: int;
}
fn main() {
    let p = Pair {};
}
";
    let err = match check::check_files(vec![parse_file(1, "t.j3", src)]) {
        Ok(_) => panic!("expected missing field"),
        Err(e) => e,
    };
    assert!(err.message.contains("missing field `b`"), "{}", err.message);
}

#[test]
fn generic_field_default_is_error() {
    let src = "\
module t;
struct Box<T> {
    v: T = 0;
}
fn main() {}
";
    let err = match check::check_files(vec![parse_file(1, "t.j3", src)]) {
        Ok(_) => panic!("expected generic default error"),
        Err(e) => e,
    };
    assert!(
        err.message.contains("default on a generic field"),
        "{}",
        err.message
    );
}

#[test]
fn non_literal_field_default_is_error() {
    let src = "\
module t;
struct Hit {
    x: int = 1 + 2;
}
fn main() {}
";
    let err = match check::check_files(vec![parse_file(1, "t.j3", src)]) {
        Ok(_) => panic!("expected literal default error"),
        Err(e) => e,
    };
    assert!(
        err.message.contains("field default must be a literal"),
        "{}",
        err.message
    );
}

#[test]
fn two_literals_still_default_from_the_left() {
    let src = "\
module t;
fn main() {
    let x = 2 + 2.0;
}
";
    let err = match check::check_files(vec![parse_file(1, "t.j3", src)]) {
        Ok(_) => panic!("expected arithmetic mismatch"),
        Err(e) => e,
    };
    assert!(
        err.message.contains("arithmetic type mismatch"),
        "{}",
        err.message
    );
}

#[test]
fn private_use_is_error() {
    let hidden = include_str!("../../examples/honesty/hidden.j3");
    let user = include_str!("../../examples/honesty/private_use.j3");
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
    let boxy = include_str!("../../examples/honesty/gen_box.j3");
    let user = include_str!("../../examples/honesty/gen_use.j3");
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
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/singleton_init.j3");
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
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/read_file.j3");
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
        .join("examples/honesty")
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

#[test]
fn gaps_rem_runs_on_both_backends() {
    run_both_ok(&honesty("rem.j3"), "ok\n");
}

#[test]
fn gaps_rem_by_zero_aborts() {
    let prog = compile_src(
        "rem0",
        "module t;\nfn main() {\n    let a = 1;\n    let b = 0;\n    let _ = a % b;\n}\n",
    )
    .expect("compile");
    let run = exec(&prog);
    assert_ne!(run.status, 0, "% 0 should abort");
}

#[test]
fn const_literal_types_run() {
    let prog = compile_src(
        "constlit",
        "module t;\nuse j3.io.printLn;\nconst FLAG: bool = true;\nconst OFF: bool = !FLAG;\nconst NUDGE: f32 = -0.5;\nfn main() {\n    if FLAG && !OFF && NUDGE < 0 {\n        printLn(`ok`);\n    }\n}\n",
    )
    .expect("compile");
    let run = exec(&prog);
    assert_eq!(run.status, 0, "stderr {}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
}

#[test]
fn const_bool_rejects_an_int() {
    let err = match compile_src("constbad", "module t;\nconst FLAG: bool = 1;\nfn main() {}\n") {
        Ok(_) => panic!("expected a type error"),
        Err(e) => e,
    };
    assert!(err.contains("const type does not match"), "{err}");
}

#[test]
fn gaps_const_rem_by_zero_is_error() {
    let files = vec![parse_file(1, "t.j3", "module t;\nconst X = 1 % 0;\nfn main() {}\n")];
    let err = match check::check_files(files) {
        Ok(_) => panic!("expected %0 error"),
        Err(e) => e,
    };
    assert!(err.message.contains("division by zero"), "{}", err.message);
}

#[test]
fn gaps_text_find_runs() {
    run_both_ok(&honesty("text_find.j3"), "ok\n");
}

#[test]
fn gaps_module_const_runs() {
    run_both_ok(&honesty("module_const.j3"), "ok\n");
}

#[test]
fn array_const_runs_on_both_backends() {
    run_both_ok(&honesty("array_const.j3"), "ok\n");
}

#[test]
fn array_const_errors_have_spans() {
    let head = "module t;\nstruct P {\n    x: int;\n}\nconst XS: [3]int = [1, 2, 3];\nconst PS: [1]P = [P { x: 1 }];\n";
    let bad = |tag: &str, body: &str| match compile_src(tag, &format!("{head}fn main() {{\n{body}\n}}\n")) {
        Ok(_) => panic!("expected error for {body}"),
        Err(e) => e,
    };
    let e = compile_src("acl", "module t;\nconst Z: [2]int = [1, 2, 3];\nfn main() {}\n").err().expect("length mismatch");
    assert!(e.contains("array length mismatch") && e.contains(":2:"), "{e}");
    for (tag, body) in [("aca", "    XS[0] = 5;"), ("acb", "    XS[0] += 5;"), ("acc", "    PS[0].x = 2;")] {
        let e = bad(tag, body);
        assert!(e.contains("cannot assign into const array"), "{body}: {e}");
    }
    let e = bad("acd", "    let r = &XS[0];");
    assert!(e.contains("cannot take `&`"), "{e}");
}

#[test]
fn array_const_bytes_and_negative_floats_match_on_both_backends() {
    run_both_ok(&honesty("array_const_bytes.j3"), "ok\n");
}

/// Same exit code (102) and message on both backends.
#[test]
fn array_const_index_out_of_bounds_aborts() {
    let src = "module t;\nuse j3.sys.argCount;\nconst XS: [2]int = [1, 2];\nfn main() {\n    let i = 5 + argCount();\n    let v = XS[i];\n    if v == 3 {\n        return;\n    }\n}\n";
    let prog = compile_src("acoob", src).expect("check");
    for backend in [emit::Backend::Llvm, emit::Backend::C] {
        let mut o = emit::EmitOpts::default();
        o.backend = backend;
        if backend == emit::Backend::Llvm && !emit::clang_available(&o.clang) {
            continue;
        }
        let run = emit::run_program(&prog.cx, &prog.modules, &o).expect("run");
        let err = String::from_utf8_lossy(&run.stderr);
        assert_eq!(run.status, 102, "{backend:?} {err}");
        assert!(err.contains("index out of bounds"), "{backend:?} {err}");
    }
}

/// A callee that writes through a `[]T` made from a const array writes ordinary module
/// data (the array is not in read-only memory): no crash, on both backends.
#[test]
fn array_const_slice_write_does_not_crash() {
    let src = "module t;\nuse j3.io.printLn;\nconst XS: [3]int = [1, 2, 3];\nfn bump(a: []int) {\n    a[0] = 9;\n}\nfn main() {\n    bump(XS);\n    printLn(`{XS[0]}`);\n}\n";
    let path = std::env::temp_dir().join(format!("j3c_acsw_{}.j3", std::process::id()));
    std::fs::write(&path, src).unwrap();
    run_both_ok(&path, "9\n");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn gaps_module_const_missing_is_named() {
    let err = match compile_src("mcnope", "module t;\nuse j3.gfx.mesh;\nfn main() {\n    let k = mesh.NOPE;\n}\n") {
        Ok(_) => panic!("expected missing const error"),
        Err(e) => e,
    };
    assert!(err.contains("has no pub const"), "{err}");
}

#[test]
fn short_circuit_skips_oob() {
    run_ok(&honesty("short_circuit.j3"), "ok\n");
}

#[test]
fn if_expr_skips_untaken_arm() {
    run_ok(&honesty("if_expr_skip.j3"), "ok\n");
}

#[test]
fn else_if_expr_and_field_of_rvalue_on_both_backends() {
    run_both_ok(&honesty("else_if_expr.j3"), "ok\nok2\n");
}

#[test]
fn else_if_expr_skips_untaken_arms_and_needs_final_else() {
    let src = "module t;\nuse j3.io.printLn;\nfn main() {\n    let xs: [1]int = [7];\n    let i = 3;\n    let x = if i < 1 { xs[i] } else if i < 2 { xs[i + 1] } else if i < 4 { 5 } else { xs[i + 2] };\n    if x == 5 {\n        printLn(\"ok\");\n    }\n}\n";
    let path = std::env::temp_dir().join(format!("j3c_eie_{}.j3", std::process::id()));
    std::fs::write(&path, src).unwrap();
    run_both_ok(&path, "ok\n");
    let _ = std::fs::remove_file(&path);
    let err = match compile_src("eie2", "module t;\nfn main() {\n    let i = 1;\n    let x = if i < 1 { 1 } else if i < 2 { 2 };\n}\n") {
        Ok(_) => panic!("expected error"),
        Err(e) => e,
    };
    assert!(err.contains("needs `else`"), "{err}");
}

#[test]
fn rvalue_base_evaluates_once_and_slice_results_stay_writable() {
    run_both_ok(&honesty("rvalue_eval_once.j3"), "once\norder\nslice-store\n");
}

#[test]
fn assigning_into_a_temporary_is_an_error() {
    for body in ["pt().x = 1;", "(Vec2 { x: 1.0, y: 1.0 } + Vec2 { x: 1.0, y: 1.0 }).x = 1.0;", "arr()[0] = 1;", "pt().x += 1;", "arr()[sl(arr())[0]] = 1;", "id(sl(arr())[1]).x = 3;"] {
        let src = format!("module t;\nuse j3.math.Vec2;\nstruct P {{\n    x: int;\n}}\nfn pt(): P {{\n    return P {{ x: 1 }};\n}}\nfn arr(): [3]int {{\n    return [1, 2, 3];\n}}\nfn sl(a: [3]int): []int {{\n    return a;\n}}\nfn id(i: int): P {{\n    return P {{ x: i }};\n}}\nfn main() {{\n    {body}\n}}\n");
        let err = match compile_src("tmpassign", &src) {
            Ok(_) => panic!("expected error for {body}"),
            Err(e) => e,
        };
        assert!(err.contains("temporary"), "{body}: {err}");
        // span: the temporary at the root, line 13 (body line), col 5 for `pt()` / `arr()`, col 6 for the parenthesized sum
        let want = if body.starts_with('(') { ":19:6" } else { ":19:5" };
        assert!(err.contains(want), "{body}: wrong span {err}");
    }
}

#[test]
fn defer_does_not_clobber_return() {
    run_ok(&honesty("defer_return.j3"), "ok\n");
}

#[test]
fn vec2_assign_op_calls_operator() {
    run_ok(&honesty("vec2_add_eq.j3"), "ok\n");
}

#[test]
fn div_by_zero_aborts() {
    let prog = compile_path(&honesty("div0.j3")).expect("compile");
    let run = exec(&prog);
    assert_ne!(run.status, 0, "div0 should abort");
}

#[test]
fn const_div_by_zero_is_error() {
    let files = vec![parse_file(
        1,
        "t.j3",
        "module t;\nconst X = 1 / 0;\nfn main() {}\n",
    )];
    let err = match check::check_files(files) {
        Ok(_) => panic!("expected /0 error"),
        Err(e) => e,
    };
    assert!(err.message.contains("division by zero"), "{}", err.message);
}

#[test]
fn string_index_is_not_an_lvalue() {
    let src = "module t;\nfn main() {\n    let s = \"hi\";\n    let p = &s[0];\n}\n";
    let files = vec![parse_file(1, "t.j3", src)];
    let err = match check::check_files(files) {
        Ok(_) => panic!("expected &s[0] error"),
        Err(e) => e,
    };
    assert!(
        err.message.contains("cannot take `&`") || err.message.contains("not a place"),
        "{}",
        err.message
    );
}

#[test]
fn string_slice_store_is_error() {
    let src = "module t;\nfn main() {\n    let s = \"hi\";\n    let mid = s[0..1];\n    mid[0] = 65;\n}\n";
    let files = vec![parse_file(1, "t.j3", src)];
    let err = match check::check_files(files) {
        Ok(_) => panic!("expected string slice store error"),
        Err(e) => e,
    };
    assert!(
        err.message.contains("cannot assign") || err.message.contains("not a place"),
        "{}",
        err.message
    );
}

#[test]
fn arena_field_is_error() {
    let err = match compile_path(&honesty("arena_field.j3")) {
        Ok(_) => panic!("expected Arena field error"),
        Err(e) => e,
    };
    assert!(err.contains("Arena by value") || err.contains("&Arena"), "{err}");
}

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
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/mesh.j3");
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
    let obj = dir.join(format!("j3c_mingw_probe_{}.o", std::process::id()));
    if std::fs::write(&src, "int main(void){return 0;}\n").is_err() {
        return false;
    }
    let ok = std::process::Command::new("clang")
        .args(["--target=x86_64-pc-windows-gnu", "-c"])
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

const TICK_TOML: &str = "[package]\nname = \"ticker\"\nentry = \"main.j3\"\n\n[emit]\nopt = 0\n";
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

fn dev_built(m: &Manifest) -> crate::devrun::Built {
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

const SCENE_TOML: &str = "[package]\nname = \"sc\"\nentry = \"src/main.j3\"\n";
const SCENE_MAIN: &str = "module sc;\nuse j3.memory.Arena;\nuse j3.option.Option;\nuse j3.scene3d;\nuse j3.scene3d.scripts;\n\
const Spin = @script(\"scripts/spin.j3s\");\n\
fn main() {\n    let mem = Arena.create(65536);\n    let scene = scene3d.load(scripts.scene(), &mem);\n    scripts.start(&scene);\n    scripts.update(&scene, 0.1);\n\
    match scene.script::<Spin>(scene.find(\"Cube\")) { Option.Some(s) { s.report(); } Option.None {} }\n}\n";
const SPIN: &str = "use j3.scene3d.{ Scene, NodeId };\nuse j3.io.printLn;\n\
props {\n    node: NodeId;\n    target: NodeId;\n    speed: f32 = 1.0;\n    label: string = \"x\";\n}\n\
pub fn update(scene: &Scene, dt: f32) {\n    let n = scene.node(node);\n    n.rotation.y += speed * dt;\n}\n\
pub fn report() { printLn(label); }\n";

fn scene_file(extra: &str) -> String {
    format!(
        "scene.version=1\nscene.count=2\nscene.nextId=3\n\
node.0.id=1\nnode.0.parent=0\nnode.0.kind=1\nnode.0.name s=Cube\nnode.0.script s=scripts/spin.j3s\n\
node.1.id=2\nnode.1.parent=0\nnode.1.kind=1\nnode.1.name s=Other\n{extra}"
    )
}

fn scene_err(scene: &str, spin: &str) -> String {
    match scene_check(scene, spin) {
        Ok(_) => panic!("expected a scene error"),
        Err(e) => e,
    }
}

fn scene_check(scene: &str, spin: &str) -> Result<Program, String> {
    let m = temp_project(
        "scene_scripts",
        &[
            ("j3.toml", SCENE_TOML),
            ("j3.editor.toml", "[editor]\nscene = \"scenes/main.j3scene\"\n"),
            ("src/main.j3", SCENE_MAIN),
            ("scripts/spin.j3s", spin),
            ("scenes/main.j3scene", scene),
        ],
    );
    let out = compile_manifest(&m);
    let _ = std::fs::remove_dir_all(&m.root);
    out
}

#[test]
fn scene_attached_scripts_check_against_the_scene() {
    let good = scene_file("param.1.target r=2\nparam.1.speed f=3\nparam.1.label s=cube\n");
    let prog = scene_check(&good, SPIN).expect("attached script checks");
    assert!(prog.cx.funcs().iter().any(|f| prog.cx.name_str(f.name) == "find__scripts__spin"));

    let missing = scene_err(&scene_file("param.1.speed f=3\n"), SPIN);
    assert!(missing.contains("scripts/spin.j3s needs prop `target`"), "{missing}");
    assert!(missing.contains("--> scenes/main.j3scene:8"), "{missing}");

    let typed = scene_err(&scene_file("param.1.target r=2\nparam.1.speed s=fast\n"), SPIN);
    assert!(typed.contains("`speed` is `f32` in scripts/spin.j3s"), "{typed}");
    assert!(typed.contains("--> scenes/main.j3scene:14"), "{typed}");
    assert!(typed.contains("param.1.speed s=fast"), "{typed}");

    let unknown = scene_err(&scene_file("param.1.target r=2\nparam.1.sped f=3\n"), SPIN);
    assert!(unknown.contains("`sped` isn't a prop of scripts/spin.j3s"), "{unknown}");

    let hook = SPIN.replace("pub fn update(scene: &Scene, dt: f32)", "pub fn update(dt: f32)");
    let bad_hook = scene_err(&scene_file("param.1.target r=2\n"), &hook);
    assert!(bad_hook.contains("must be `pub fn update(scene: &Scene, dt: f32)`"), "{bad_hook}");

    let wrong_path = scene_file("param.1.target r=2\n").replace("s=scripts/spin.j3s", "s=scripts/spn.j3s");
    let path = scene_err(&wrong_path, SPIN);
    assert!(path.contains("no script at scripts/spn.j3s"), "{path}");
    assert!(path.contains("did you mean `scripts/spin.j3s`"), "{path}");
}

#[test]
fn scene_script_lookup_needs_an_attachment() {
    let detached = scene_file("").replace("node.0.script s=scripts/spin.j3s\n", "");
    let err = scene_err(&detached, SPIN);
    assert!(err.contains("no scene attaches scripts/spin.j3s"), "{err}");
}

#[test]
fn script_path_error_suggests_existing_script() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../speculative/11_script");
    let manifest = Manifest::load(&root.join("j3.toml")).expect("script manifest");
    let mut loader = Loader {
        files: Vec::new(),
        loaded: HashSet::new(),
        next_file: 1,
        game: GameBackend::Sdl,
        os: OsKind::Linux,
        src_dir: Some(manifest.src_dir()),
        package: Some(manifest.name),
        script_root: root,
        overlays: vec![(
            manifest.entry.clone(),
            "module script_demo; const Door = @script(\"scripts/dor.j3s\"); fn main() {}"
                .to_string(),
        )],
        scene_scan: None,
        ui_cfg: None,
        ui_gen: None,
        ui_err: None,
    };
    loader.load_path(&manifest.entry).expect("load main");
    let err = loader.load_imports().unwrap_err();
    assert!(err.contains("no script at scripts/dor.j3s"), "{err}");
    assert!(err.contains("did you mean `scripts/door.j3s`"), "{err}");
}

#[test]
fn script_references_join_import_cycles() {
    let module_a = ["script".to_string(), "scripts".to_string(), "a".to_string()];
    let module_b = ["script".to_string(), "scripts".to_string(), "b".to_string()];
    let a = "const B = @script(\"scripts/b.j3s\");";
    let b = "const A = @script(\"scripts/a.j3s\");";
    let files = vec![
        ParsedFile {
            path: "scripts/a.j3s".to_string(),
            src: a.to_string(),
            ast: parse::parse_script(a, FileId(1), &module_a).unwrap(),
            file_id: FileId(1),
        },
        ParsedFile {
            path: "scripts/b.j3s".to_string(),
            src: b.to_string(),
            ast: parse::parse_script(b, FileId(2), &module_b).unwrap(),
            file_id: FileId(2),
        },
    ];
    let err = check_import_cycles(&files).unwrap_err();
    assert!(
        err.contains("script.scripts.a -> script.scripts.b -> script.scripts.a"),
        "{err}"
    );
}

fn tool_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("j3c_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn tool_script_main_prints() {
    let dir = tool_dir("tool_main");
    let path = dir.join("greet.j3s");
    std::fs::write(
        &path,
        "struct Note { n: int; }\nuse j3.io.printLn;\npub fn main() {\n    let note = Note { n: 3 };\n    printLn(`note {note.n}`);\n}\n",
    )
    .unwrap();
    let prog = compile_path(&path).expect("tool script");
    let run = exec(&prog);
    assert_eq!(run.status, 0, "{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "note 3\n");
}

#[test]
fn tool_script_without_main_errors() {
    let dir = tool_dir("tool_nomain");
    let path = dir.join("spin.j3s");
    std::fs::write(&path, "pub fn update() {}\n").unwrap();
    let err = match compile_path(&path) {
        Ok(_) => panic!("expected a missing main"),
        Err(e) => e,
    };
    assert!(err.contains("pub fn main"), "{err}");
    assert!(err.contains("scene"), "{err}");
}

#[test]
fn tool_script_reads_args() {
    let dir = tool_dir("tool_args");
    let path = dir.join("echo.j3s");
    std::fs::write(
        &path,
        "use j3.io.printLn;\nuse j3.sys.arg;\npub fn main() { printLn(arg(1)); }\n",
    )
    .unwrap();
    let prog = compile_path(&path).expect("args script");
    let opts = llvm_opts();
    let run = emit::run_program_with(
        &prog.cx,
        &prog.modules,
        &opts,
        &["west".to_string()],
    )
    .expect("run");
    assert_eq!(run.status, 0, "{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "west\n");
}
