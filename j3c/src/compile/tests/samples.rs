//! Hello, the speculative samples and examples run on both backends.

use super::*;

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
        (
            "lang/small_gaps.j3",
            false,
            "scaled=6\ntotal=3 rest=2\nzero\nother 1\ntwo\nother 3\nr0 c0\nr1 c0\na=1\na=2\nflags=3 low=3 inv=252 all=255\ndir=2 turn=3\narray 0 15 9 10\n",
        ),
        (
            "lang/literals.j3",
            false,
            "kw=1230 two\nzero sum=0 x=0+4 y=0 on=false\nu64 max=true top=1 low=true\nfloats 1000000000 2 100 3\nshift 16 4\niter total=78 runes=3 sum=8694\nsplit 4 3 lines 2 high half all other 255\n",
        ),
        (
            "lang/ufcs.j3",
            false,
            "sum=13 chain=52\nabs=2.5 count=3 len=5\n",
        ),
        ("lang/extend.j3", false, "counter=15 double=30\nbox=20 hi\n"),
        (
            "lang/generic_fns.j3",
            false,
            "first=4 2.5 hi\nmax=7 2.5 9\nswap=2 1\nchain=9 apply=36 two=5\nexplicit=6\n",
        ),
        (
            "lang/traits.j3",
            false,
            "3 | 70 | 2\ntrue false true true\n9 6 true false\ntotal=43 true\ntag=203 direct=203 true\n",
        ),
        (
            "lang/dyn.j3",
            false,
            "llvm 6 102 calls=2\nllvm lines=102\nc lines=7\ntotal=6 calls=4\npass=15 picked=c named=c up=llvm\n",
        ),
        (
            "lang/simd.j3",
            false,
            "2.5 4.5 6.5 8.5\n1 2 2.5 2.5\n4 3 2 1\n10\n10 4\n2147483647 -2147483648 1 -1\n2 2 6 16\n6 12\n2 4 6\n",
        ),
        (
            "lang/chars.j3",
            false,
            "a=97 smile=128512 e=233\n10 13 9 0 92 39 34 127\nwide=65 upper=65 same=true\nn=42\nfound=3\n",
        ),
        (
            "lang/patterns.j3",
            false,
            "name name name digit space other\nadd\nmul\nnegative -3\nnumber 8\nother\n4 9 -1\nscore=143\n",
        ),
        (
            "lang/variants.j3",
            false,
            "eval=15\nnamed=4 width=5\nExpr.Named(x, Span { lo: 4, hi: 9 }, true)\nExpr.Bin(BinOp.Add, &Expr, &Expr)\nlo=1 end=7\n",
        ),
        (
            "lang/match_expr.j3",
            false,
            "cost=9\ndigit space other\narea=12 12 -1\nradius=5 0\nsquare rect circle dot\ntotal=12\nnot add\n",
        ),
        (
            "lang/embed.j3",
            false,
            "greeting 27 bytes: hello from a file\nline two\n\nblob len=7 nul=0 ff=255 sum=57756288667\nb.txt: 6\ngreeting.txt: 27\ninline second\n",
        ),
        (
            "lang/text_fmt.j3",
            false,
            "i=-5 u=-1 f=1.5 x=0.1 b=true Tok { name: let, at: Span { lo: 4, hi: 7 }, hot: true }\ni=-5 u=-1 f=1.5 x=0.1 b=true Tok { name: let, at: Span { lo: 4, hi: 7 }, hot: true }\nKind.Num(3, 2.5) Kind.Word(w) Kind.End done\n18446744073709551615 ff 0.1 \u{1F600}A   end\nlen=580\n",
        ),
        ("lang/f16.j3", false, "0.25 0.875 1.5\n65504 true\n2048 2052 0.0999756\n2 8 12\n"),
        (
            "lang/shader.j3",
            false,
            "vert 827 bytes, frag 430 bytes\nuniforms: uScreen uAtlas\nattributes: 4\n  loc 0 comps 2 offset 0 stride 8 divisor 0\n  loc 1 comps 2 offset 0 stride 32 divisor 1\n  loc 2 comps 2 offset 8 stride 32 divisor 1\n  loc 3 comps 4 offset 16 stride 32 divisor 1\nhash -8512555711321048290 1809110793748906764\n",
        ),
        (
            "lang/collections.j3",
            false,
            "fn if let\nsorted 1 3 9 has3=true at5=2 pop=9\na=1 b=3 c=1\ntotal=5\neval=3 uses=3 Expr.Add(&Expr, &Expr)\nx=0 y=1 again=0 str=y\n",
        ),
        (
            "lang/tuples.j3",
            false,
            "q=3 r=2 t=2,1\n2.5 1 2.5 hi ((1, 2.5), hi)\nfound at 1 -> 50\n5 6 0\ntrue true true\nlen=2 at=home total=12\n",
        ),
        (
            "lang/derive.j3",
            false,
            "true false true Key { name: x, n: 2 } true\ntrue true true true Shape.Rect(1, 2)\nShape.Dot Shape.Circle(2) Shape.Circle(4) Shape.Rect(0, 1)\nlen=2 a=5 pair=true same-as-tuple=true\nopt=true false true true map=12\n",
        ),
        (
            "lang/boxed_closures.j3",
            false,
            "add5=6 add7=8 via stack param=15\nsnap=101 base=0\ncounter=3 n=0\nlist total=60 double=42 global=61\npool 11 22 33 on main=0 here=1\n",
        ),
        (
            "lang/closures.j3",
            false,
            "11 12 total=3\napply=40 thunk=15\ntotal=303\nhold=33\nnested=27 total=305\nmethod=15 n=15\nplain=2 0\nearly=7 d=101\n",
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
