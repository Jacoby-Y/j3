//! Tiny argv parser. No clap — keeps /target small.

use std::path::{Path, PathBuf};

use crate::compile;
use crate::emit;
use crate::ir::dump::{dump_module, dump_program};
use crate::ir::samples;
use crate::ir::validate::validate_module;
use crate::project::{self, Input, Manifest};

const VERSION: &str = "0.0.1";

pub fn run(args: impl IntoIterator<Item = String>) -> i32 {
    let args: Vec<String> = args.into_iter().collect();
    if args.is_empty() {
        print_help();
        return 0;
    }

    EMIT.store(0, std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut g) = OPT.lock() {
        *g = None;
    }
    FAST.store(false, std::sync::atomic::Ordering::Relaxed);
    RELEASE.store(false, std::sync::atomic::Ordering::Relaxed);
    crate::project::FILE_ONLY.store(false, std::sync::atomic::Ordering::Relaxed);

    let cmd = args[0].as_str();
    let rest = &args[1..];
    match cmd {
        "help" | "--help" | "-h" => help_cmd(rest),
        "version" | "--version" | "-V" => {
            println!("j3c {VERSION}");
            if rest.iter().any(|a| a == "--verbose" || a == "-v") {
                print_provenance();
            }
            0
        }
        "dump-ir" => dispatch(rest, dump_ir_hello, dump_ir_samples, dump_ir_path, false),
        "dump-c" => dispatch(rest, dump_c_hello, dump_c_samples, dump_c_path, false),
        "dump-llvm" => dispatch(rest, dump_ll_hello, dump_ll_samples, dump_ll_path, false),
        "dump-bc" => match rest.first() {
            Some(path) => dump_bc_path(path),
            None => { eprintln!("usage: j3c dump-bc <file.j3s>"); 1 }
        },
        "dump-syntax" => dump_syntax(rest),
        "dump-ui" => match rest.first() {
            Some(path) => match crate::ui::dump(std::path::Path::new(path)) {
                Ok(t) => {
                    print!("{t}");
                    0
                }
                Err(e) => {
                    eprint!("{e}");
                    1
                }
            },
            None => {
                eprintln!("usage: j3c dump-ui <file.j3ui|project dir>");
                1
            }
        },
        "new" => crate::scaffold::cmd(rest),
        "editor" => crate::editor::cmd(rest),
        "-e" | "--editor" => editor_tool(rest),
        "check" => check_cmd(rest),
        "i18n" => crate::i18n::cmd(rest),
        "doc" => crate::doc::cmd(rest),
        "guide" => {
            print!("{}", crate::helptext::GUIDE);
            0
        }
        "run" if rest.iter().any(|a| a == "--help" || a == "-h") => {
            println!("usage: j3c run [path | file.j3 | tool.j3s] [--frames N] [--screenshot F.png] [--headless] [--hot] [--file]");
            println!("  --hot       rebuild and restart on .j3/.j3ui/.j3s edits; the old run stays up if the build fails (`j3c doc j3ui`).");
            println!("  --headless  offscreen video, silent audio: no window. Agents always pass it (`j3c doc testing`).");
            println!("  --frames N  stop an App after N frames; --screenshot saves the last one (relative to your cwd).");
            0
        }
        "run" => {
            progress_on();
            match take_run_flags(rest) {
                Ok(rest) => run_dispatch(&rest),
                Err(e) => {
                    eprintln!("j3c run: {e}");
                    1
                }
            }
        }
        "build" => {
            progress_on();
            dispatch(rest, build_cwd, build_samples_err, build_path, true)
        }
        "fmt" => fmt_cmd(rest),
        "size" => size_cmd(rest),
        "golden" => crate::golden::cmd(rest),
        "lsp" => crate::lsp::serve(),
        other => {
            eprintln!("j3c: unknown command `{other}`");
            if matches!(other, "scene" | "render" | "gen" | "mcp") {
                eprintln!("editor tools take -e: `j3c -e {other}`");
            } else {
                eprintln!("run `j3c help`");
            }
            1
        }
    }
}

/// `j3c -e <tool>`: the scene editor's tools, which work without its window.
fn editor_tool(args: &[String]) -> i32 {
    let rest = args.get(1..).unwrap_or(&[]);
    match args.first().map(String::as_str) {
        Some("scene") => crate::scene::cmd(rest),
        Some("render") => crate::scene::render_cmd(rest),
        Some("gen") => crate::gen::cmd(rest),
        Some("imgdiff") => crate::imgdiff::cmd(rest),
        Some("mcp") => crate::mcp::cmd(rest),
        Some("script-props") => script_props_cmd(rest),
        other => {
            if let Some(o) = other { eprintln!("j3c -e: unknown editor tool `{o}`"); }
            eprintln!("usage: j3c -e scene|render|gen|imgdiff|mcp|script-props ...  (or --editor)");
            1
        }
    }
}

/// `j3c -e script-props <scripts/door.j3s>`: the props the inspector edits, as JSON
/// (name, type, default, required). Run from the project root.
fn script_props_cmd(rest: &[String]) -> i32 {
    let Some(rel) = rest.first() else {
        eprintln!("usage: j3c -e script-props <path.j3s>");
        return 1;
    };
    let root = match project::resolve_cwd() {
        Ok(Input::Project(m)) => m.root,
        _ => PathBuf::from("."),
    };
    match crate::editor::script_props_json(&root, rel) {
        Ok(json) => {
            println!("{json}");
            0
        }
        Err(e) => {
            print_fail(&e);
            1
        }
    }
}

fn dispatch(
    rest: &[String],
    no_file: fn() -> i32,
    samples: fn() -> i32,
    file: fn(&str) -> i32,
    allow_fast: bool,
) -> i32 {
    let (fast, path) = peel_args(rest);
    if fast && !allow_fast {
        eprintln!("j3c: `--fast` is only for `run` / `build`");
        return 1;
    }
    FAST.store(fast, std::sync::atomic::Ordering::Relaxed);
    match path {
        Some("samples") => samples(),
        Some(p) => file(p),
        None => no_file(),
    }
}

static FAST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// `--release`: smallest binary (opt s, no `-g`, stripped, no unwind tables).
static RELEASE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn peel_args(rest: &[String]) -> (bool, Option<&str>) {
    let mut fast = false;
    let mut path = None;
    let mut i = 0;
    while i < rest.len() {
        let a = rest[i].as_str();
        if a == "--fast" {
            fast = true;
        } else if a == "--release" {
            RELEASE.store(true, std::sync::atomic::Ordering::Relaxed);
        } else if a == "--emit" {
            if let Some(v) = rest.get(i + 1) {
                EMIT.store(emit_code(v), std::sync::atomic::Ordering::Relaxed);
                i += 1;
            }
        } else if let Some(v) = a.strip_prefix("--emit=") {
            EMIT.store(emit_code(v), std::sync::atomic::Ordering::Relaxed);
        } else if a == "--opt" {
            if let Some(v) = rest.get(i + 1) {
                set_opt(v);
                i += 1;
            }
        } else if let Some(v) = a.strip_prefix("--opt=") {
            set_opt(v);
        } else if path.is_none() && !a.starts_with('-') {
            path = Some(rest[i].as_str());
        }
        i += 1;
    }
    (fast, path)
}

fn emit_code(s: &str) -> u8 {
    match s {
        "c" | "C" => 1,
        "llvm" | "LLVM" | "ll" => 2,
        _ => 0,
    }
}

fn set_opt(s: &str) {
    if let Ok(mut g) = OPT.lock() {
        *g = Some(s.to_string());
    }
}

static EMIT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
static OPT: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

fn help_cmd(rest: &[String]) -> i32 {
    match rest.first().map(|s| s.as_str()) {
        None => {
            print_help();
            0
        }
        Some("manifest") => {
            print!("{}", crate::helptext::MANIFEST);
            0
        }
        Some(other) => {
            eprintln!("j3c: unknown help topic `{other}`");
            eprintln!("  = help: `j3c help` or `j3c help manifest`");
            1
        }
    }
}

/// Strip `--frames N`, `--screenshot FILE`, and `--headless` from `run`'s arguments and pass
/// them to the program: `j3.app` stops after N frames and saves the last one as a PNG, and
/// headless runs use SDL's offscreen video and silent audio, so no window or sound appears.
fn take_run_flags(rest: &[String]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--frames" => {
                let n = it.next().ok_or("--frames needs a count")?;
                if n.parse::<u32>().map_or(true, |n| n == 0) {
                    return Err(format!("--frames wants a positive whole number, not `{n}`"));
                }
                std::env::set_var("J3_FRAMES", n);
            }
            "--screenshot" => {
                let path = PathBuf::from(it.next().ok_or("--screenshot needs a file")?);
                // The program runs in its project root; keep the path the user meant.
                let path = if path.is_absolute() {
                    path
                } else {
                    std::env::current_dir().map_err(|e| e.to_string())?.join(path)
                };
                std::env::set_var("J3_SCREENSHOT", path);
            }
            "--hot" => HOT.store(true, std::sync::atomic::Ordering::Relaxed),
            "--file" => crate::project::FILE_ONLY.store(true, std::sync::atomic::Ordering::Relaxed),
            "--headless" => {
                std::env::set_var("SDL_VIDEODRIVER", "offscreen");
                std::env::set_var("SDL_AUDIODRIVER", "dummy");
            }
            _ => out.push(arg.clone()),
        }
    }
    Ok(out)
}

fn print_help() {
    println!("j3c {VERSION} — J3 compiler");
    println!();
    println!("usage:");
    println!("    j3c <command> [file.j3 | project]");
    println!();
    println!("commands:");
    println!("    new [name]       write j3.toml, src/main.j3, AGENTS.md, CLAUDE.md (hello)");
    println!("    new [name] --app SDL window (links SDL2)");
    println!("    new [name] --template fps  first-person 3D game with a scene (SDL2 + GL)");
    println!("    new [name] --template ui   .j3ui components window (SDL2 + ttf + GL; `doc j3ui`)");
    println!("    editor [--new] [project]  open the 3D scene editor");
    println!("    -e scene <op> ... list/get/add/set/delete/check scene objects by name (`-e scene --help`)");
    println!("    -e render [opts] draw a scene to a PNG without a window (`-e render --help`)");
    println!("    -e gen [--force] run assets/gen/*.j3 mesh generators that changed");
    println!("    -e mcp [--project D] MCP server for agents: edit, render, check, play scenes");
    println!("    -e mcp --tools   every MCP tool's arguments and the CLI command doing the same");
    println!("    -e script-props <f.j3s>  a script's props as JSON (what the inspector edits)");
    println!("                     (-e is short for --editor)");
    println!("    guide            how to start a project");
    println!("    check [path]     typecheck; no emit or link");
    println!("    check --json     one JSON object on stdout");
    println!("    doc [module]     tasks and stdlib modules, or one module's public items (`doc --help`)");
    println!("    doc search <w>   public names containing a word, in any module");
    println!("    doc recipes      complete checked programs: 2D app, 3D scene, canvas, script, ...");
    println!("    doc lang         language");
    println!("    doc j3ui         .j3ui components (j3ui-immediate: @j3ui functions)");
    println!("    doc css          CSS subset for .j3ui");
    println!("    doc testing      headless run, screenshots, goldens, imgdiff");
    println!("    golden <dir> [names] [--bless]  headless goldens of a project: states in <dir>/golden.toml (`doc testing`)");
    println!("    -e imgdiff a.png b.png [--tol N] [--max N] [--out d.png]  pixel diff for goldens (exit 1 = differs)");
    println!("    build [path]     emit, compile, write `target/<name>`");
    println!("    run [path]       emit, compile, execute");
    println!("    run --frames N [--screenshot F.png] [--headless]  stop an App after N frames;");
    println!("                     F.png is relative to where you run j3c, not the project");
    println!("    run --file f.j3  run that one file even inside a project");
    println!("    run tool.j3s [args]  run that script's `pub fn main` once; args are `j3.sys.arg`");
    println!("    fmt [path]       pretty-print `.j3` and `.j3s` (same lexer; keeps comments)");
    println!("    fmt --write      rewrite files");
    println!("    fmt --check      exit 1 if formatting would change a file");
    println!("    lsp              language server (stdio JSON-RPC)");
    println!("    dump-ir [path]   parse + typecheck; print IR");
    println!("    dump-ir samples  print hand-built place-IR samples");
    println!("    dump-c [path]    print C from the program");
    println!("    dump-llvm [path] print LLVM IR from the program");
    println!("    dump-ui <file.j3ui|dir> print the J3 generated from .j3ui components");
    println!("    dump-bc <file.j3s> print script bytecode");
    println!("    dump-syntax [--ui] [f]  print TextMate grammar (lexer/parser names; --ui for .j3ui)");
    println!("    --fast           with run/build: strip bounds-check Abort; no `-g`");
    println!("    --emit llvm|c    backend (default llvm; override [emit] backend)");
    println!("    --opt N          0/1/2/3/s/z (override [emit] opt)");
    println!("    --release        with run/build: smallest binary (opt s, no `-g`, stripped; keeps checks); build also writes [dist] build_outputs (zip, deb: `j3c doc ship`)");
    println!("    size [path]      what is in `target/<name>`: sections, code per module, biggest functions");
    println!("    run samples      emit, compile, execute place tests");
    println!("    help manifest    print j3.toml keys");
    println!("    version          print version");
    println!("    version --verbose  docs id, stdlib source, binary path: spot a stale j3c");
    println!("    help             print this help");
    println!();
    println!("A path may be a `.j3` file, a `j3.toml`, or a directory with `j3.toml`.");
    println!("With no path, run, build, and check use ./j3.toml and stop if it is missing.");
    println!("dump-ir, dump-c, and dump-llvm still fall back to the built-in hello.");
    println!("emit: LLVM by default (clang); C is optional. Options live in [emit] of j3.toml.");
    println!("run/build of a project executes with cwd = package root (`J3_ROOT`).");
    println!("J3C_TIMINGS=1 prints how long each build phase took to stderr (`j3c help manifest`).");
    println!("`j3c guide` is the short start. `j3c doc` lists the standard library.");
}

/// `j3c version --verbose`: where this build's docs and sources come from.
fn print_provenance() {
    let from = |var: &str| match std::env::var(var) {
        Ok(v) if !v.is_empty() => format!("{var}={v}"),
        _ => "embedded in this binary".to_string(),
    };
    println!("docs     {} (`j3c doc` prints the same id; another id means other docs)", crate::stdlib::docs_id());
    println!("stdlib   {}", from("J3_STDLIB"));
    println!("editor   {}", from("J3_EDITOR_SRC"));
    match std::env::current_exe() {
        Ok(p) => println!("binary   {}", p.display()),
        Err(e) => println!("binary   unknown ({e})"),
    }
}

fn dump_syntax(rest: &[String]) -> i32 {
    let ui = rest.first().is_some_and(|a| a == "--ui");
    let rest = if ui { &rest[1..] } else { rest };
    let text = if ui { crate::parse::syntax::ui_tm_grammar() } else { crate::parse::syntax::tm_grammar() };
    match rest.first() {
        None => {
            print!("{text}");
            0
        }
        Some(path) => match std::fs::write(path, &text) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("j3c: write {path}: {e}");
                1
            }
        },
    }
}

fn load_samples() -> Result<samples::PlaceSamples, String> {
    let samples = samples::place_samples();
    match validate_module(&samples.cx, &samples.module) {
        Ok(()) => Ok(samples),
        Err(e) => Err(e.message),
    }
}

fn load_input(path: Option<&str>) -> Result<Input, String> {
    match path {
        None => project::resolve_cwd(),
        Some(p) => project::resolve(Path::new(p)),
    }
}

fn compile_input(input: &Input) -> Result<(compile::Program, Option<Manifest>), String> {
    match input {
        Input::Hello => Ok((compile::compile_path(&compile::default_hello_path())?, None)),
        Input::File(p) => Ok((compile::compile_path(p)?, None)),
        Input::Project(m) => Ok((compile::compile_manifest(m)?, Some(m.clone()))),
    }
}

fn compile_entry(path: Option<&str>) -> Result<(compile::Program, Option<Manifest>), String> {
    let (mut prog, manifest) = compile_input(&load_input(path)?)?;
    if FAST.load(std::sync::atomic::Ordering::Relaxed) {
        compile::apply_fast(&mut prog);
    }
    Ok((prog, manifest))
}

/// `run` / `build` / `check` with no path. A missing `j3.toml` is an error.
/// Dump commands keep the built-in hello via [`compile_entry`].
fn compile_user(path: Option<&str>) -> Result<(compile::Program, Option<Manifest>), String> {
    if path.is_none() {
        match project::resolve_cwd() {
            Ok(Input::Hello) => {
                return Err(
                    "no j3.toml in the current directory\n  = help: `j3c new`\n  = help: `j3c new --app`\n  = help: `j3c guide`"
                        .to_string(),
                );
            }
            Ok(_) => {}
            Err(e) => return Err(e),
        }
    }
    compile_entry(path)
}

fn dump_ir_hello() -> i32 {
    dump_ir_prog(compile_entry(None))
}

fn dump_ir_path(path: &str) -> i32 {
    dump_ir_prog(compile_entry(Some(path)))
}

fn dump_bc_path(path: &str) -> i32 {
    if !path.ends_with(".j3s") {
        eprintln!("j3c dump-bc: expected a .j3s file");
        return 1;
    }
    match compile::analyze_path(Path::new(path), &[]) {
        Ok(prog) => {
            let script = prog.modules.iter().find(|module| {
                module.structs.iter().any(|id| {
                    prog.cx.struct_def(*id).script.as_ref().is_some_and(|meta| {
                        Path::new(&meta.path).ends_with(path)
                    })
                })
            });
            match script.map(|module| emit::bc::emit_script(&prog.cx, module)) {
                Some(Ok(bc)) => {
                    let root = match project::resolve(Path::new(path)) {
                        Ok(Input::Project(manifest)) => manifest.root,
                        _ => Path::new(path).parent().unwrap_or(Path::new(".")).to_path_buf(),
                    };
                    if let Err(e) = bc.write_to(&root) {
                        eprintln!("j3c dump-bc: {e}");
                        return 1;
                    }
                    print!("{}", bc.dump());
                    0
                }
                Some(Err(e)) => { eprintln!("j3c dump-bc: {e}"); 1 }
                None => { eprintln!("j3c dump-bc: no script module for {path}"); 1 }
            }
        }
        Err(e) => { print_fail(&e); 1 }
    }
}

fn dump_ir_prog(prog: Result<(compile::Program, Option<Manifest>), String>) -> i32 {
    match prog {
        Ok((prog, _)) => {
            print!("{}", dump_program(&prog.cx, &prog.modules));
            0
        }
        Err(e) => {
            print_fail(&e);
            1
        }
    }
}

fn dump_ir_samples() -> i32 {
    match load_samples() {
        Ok(samples) => {
            print!("{}", dump_module(&samples.cx, &samples.module));
            0
        }
        Err(e) => {
            eprintln!("j3c: IR sample failed validation: {e}");
            1
        }
    }
}

fn dump_c_hello() -> i32 {
    dump_c_prog(compile_entry(None))
}

fn dump_c_path(path: &str) -> i32 {
    dump_c_prog(compile_entry(Some(path)))
}

fn dump_c_prog(prog: Result<(compile::Program, Option<Manifest>), String>) -> i32 {
    match prog {
        Ok((prog, _)) => match emit::emit_program(&prog.cx, &prog.modules) {
            Ok(c) => {
                print!("{c}");
                0
            }
            Err(e) => {
                eprintln!("j3c: emit C failed: {e}");
                1
            }
        },
        Err(e) => {
            eprintln!("j3c: {e}");
            1
        }
    }
}

fn dump_ll_hello() -> i32 {
    dump_ll_prog(compile_entry(None))
}

fn dump_ll_path(path: &str) -> i32 {
    dump_ll_prog(compile_entry(Some(path)))
}

fn dump_ll_prog(prog: Result<(compile::Program, Option<Manifest>), String>) -> i32 {
    match prog {
        Ok((prog, manifest)) => match emit::emit_llvm(&prog.cx, &prog.modules, &dump_opts(&manifest)) {
            Ok(ll) => {
                print!("{ll}");
                0
            }
            Err(e) => {
                eprintln!("j3c: emit LLVM failed: {e}");
                1
            }
        },
        Err(e) => {
            eprintln!("j3c: {e}");
            1
        }
    }
}

fn dump_ll_samples() -> i32 {
    match load_samples() {
        Ok(samples) => match emit::emit_llvm(
            &samples.cx,
            std::slice::from_ref(&samples.module),
            &emit::EmitOpts::default(),
        ) {
            Ok(ll) => {
                print!("{ll}");
                0
            }
            Err(e) => {
                eprintln!("j3c: emit LLVM failed: {e}");
                1
            }
        },
        Err(e) => {
            eprintln!("j3c: IR sample failed validation: {e}");
            1
        }
    }
}

fn dump_c_samples() -> i32 {
    match load_samples() {
        Ok(samples) => match emit::emit_c(&samples.cx, &samples.module) {
            Ok(c) => {
                print!("{c}");
                0
            }
            Err(e) => {
                eprintln!("j3c: emit C failed: {e}");
                1
            }
        },
        Err(e) => {
            eprintln!("j3c: IR sample failed validation: {e}");
            1
        }
    }
}

/// `run`'s positionals: flags, then one path, then arguments for the program.
fn run_dispatch(rest: &[String]) -> i32 {
    let (fast, path, args) = peel_run(rest);
    FAST.store(fast, std::sync::atomic::Ordering::Relaxed);
    match path {
        Some("samples") => run_samples(),
        Some(p) => run_user(Some(p), &args),
        None => run_user(None, &args),
    }
}

/// `run --hot`: rebuild and restart on source edits (`hot.rs`).
static HOT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn peel_run(rest: &[String]) -> (bool, Option<&str>, Vec<String>) {
    let mut fast = false;
    let mut path = None;
    let mut args = Vec::new();
    let mut i = 0;
    while i < rest.len() {
        let a = rest[i].as_str();
        if a == "--fast" {
            fast = true;
        } else if a == "--release" {
            RELEASE.store(true, std::sync::atomic::Ordering::Relaxed);
        } else if a == "--emit" {
            if let Some(v) = rest.get(i + 1) {
                EMIT.store(emit_code(v), std::sync::atomic::Ordering::Relaxed);
                i += 1;
            }
        } else if let Some(v) = a.strip_prefix("--emit=") {
            EMIT.store(emit_code(v), std::sync::atomic::Ordering::Relaxed);
        } else if a == "--opt" {
            if let Some(v) = rest.get(i + 1) {
                set_opt(v);
                i += 1;
            }
        } else if let Some(v) = a.strip_prefix("--opt=") {
            set_opt(v);
        } else if path.is_none() && !a.starts_with('-') {
            path = Some(rest[i].as_str());
        } else if path.is_some() {
            args.push(rest[i].clone());
        }
        i += 1;
    }
    (fast, path, args)
}

/// A project with scripts runs in dev mode (`devrun`); everything else runs as before.
/// A `.j3s` path is a one-shot tool, not the dev watcher.
fn run_user(path: Option<&str>, args: &[String]) -> i32 {
    status("checking");
    if let Ok(Input::Project(m)) = load_input(path) {
        let manifest = Some(m.clone());
        let opts = match emit_opts(&manifest) {
            Ok(opts) => opts,
            Err(e) => {
                print_fail(&e);
                return 1;
            }
        };
        crate::gen::before_build(&m.root);
        let fast = FAST.load(std::sync::atomic::Ordering::Relaxed);
        return match crate::devrun::prepare(&m, &opts, fast) {
            Ok(crate::devrun::Prepared::NoScripts(prog)) if HOT.load(std::sync::atomic::Ordering::Relaxed) => {
                crate::hot::run(&m, &opts, fast, Ok(*prog), args)
            }
            Ok(crate::devrun::Prepared::NoScripts(prog)) => run_loaded(Ok((*prog, manifest)), None, args),
            Ok(crate::devrun::Prepared::Dev(mut built)) => {
                if HOT.load(std::sync::atomic::Ordering::Relaxed) {
                    eprintln!("j3c: --hot ignored: this project has .j3s scripts, and its own watcher handles them (script edits swap live; .j3 and markup edits need a restart)");
                }
                match crate::devrun::watch(&m, &opts, fast, &mut built, args) {
                    Ok(code) => {
                        if code != 0 {
                            eprintln!("j3c: program exited {code}{}", exit_reason(code));
                        }
                        code
                    }
                    Err(e) => {
                        print_fail(&e);
                        1
                    }
                }
            }
            Err(e) if HOT.load(std::sync::atomic::Ordering::Relaxed) => {
                crate::hot::run(&m, &opts, fast, Err(e), args)
            }
            Err(e) => {
                print_fail(&e);
                1
            }
        };
    }
    run_loaded(compile_user(path), None, args)
}

pub(crate) fn print_error(e: &str) {
    print_fail(e);
}

/// `j3c size [path] [--target T]`: what is in the built binary. Reads it with `llvm-nm` /
/// `llvm-size` (ELF, PE, and Mach-O) or the binutils ones; a stripped binary has no symbols,
/// so build without `--release` to see the per-function breakdown.
fn size_cmd(rest: &[String]) -> i32 {
    let mut path = None;
    let mut target = String::new();
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--target" => {
                i += 1;
                target = rest.get(i).cloned().unwrap_or_default();
            }
            a if !a.starts_with('-') && path.is_none() => path = Some(a),
            _ => {}
        }
        i += 1;
    }
    let m = match load_input(path) {
        Ok(Input::Project(m)) => m,
        Ok(_) => {
            eprintln!("j3c size: needs a project (a directory with j3.toml)");
            return 1;
        }
        Err(e) => {
            print_fail(&e);
            return 1;
        }
    };
    let mut exe = if target.is_empty() { m.out_path() } else { m.out_path_for(&target) };
    if !exe.is_file() && exe.with_extension("exe").is_file() {
        exe = exe.with_extension("exe");
    }
    let Ok(meta) = std::fs::metadata(&exe) else {
        eprintln!("j3c size: {} is not built; run `j3c build` first", exe.display());
        return 1;
    };
    let tool = |names: &[&str], args: &[&str]| -> Option<String> {
        names.iter().find_map(|n| {
            let out = std::process::Command::new(n).args(args).arg(&exe).output().ok()?;
            out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
        })
    };
    println!("{}  {} bytes", exe.display(), meta.len());
    if let Some(s) = tool(&["llvm-size", "size"], &[]) {
        // `text data bss dec hex file`: code and read-only data, initialised data, zeroed memory.
        if let Some(n) = s.lines().nth(1).map(|l| l.split_whitespace().take(3).collect::<Vec<_>>()) {
            if let [text, data, bss] = n[..] {
                println!("code+rodata {text}  data {data}  bss {bss} (bss is memory, not file size)");
            }
        }
    }
    let Some(nm) = tool(&["llvm-nm", "nm"], &["-S", "--defined-only"]) else {
        eprintln!("j3c size: `llvm-nm` or `nm` not found");
        return 0;
    };
    let mut funcs: Vec<(u64, String)> = Vec::new();
    for l in nm.lines() {
        let p: Vec<&str> = l.split_whitespace().collect();
        if let [_, size, kind, name] = p[..] {
            let name = name.trim_start_matches('_');
            if kind.eq_ignore_ascii_case("t") && name.starts_with("j3_") {
                funcs.push((u64::from_str_radix(size, 16).unwrap_or(0), name.to_string()));
            }
        }
    }
    if funcs.is_empty() {
        println!("no symbols (stripped): build without --release to see code per module");
        return 0;
    }
    let mut groups: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
    for (size, name) in &funcs {
        let mut parts = name.split('_').skip(1);
        let first = parts.next().unwrap_or("");
        let key = if first == "j3" { format!("j3.{}", parts.next().unwrap_or("")) } else { first.to_string() };
        *groups.entry(key).or_default() += size;
    }
    let mut groups: Vec<_> = groups.into_iter().collect();
    groups.sort_by(|a, b| b.1.cmp(&a.1));
    println!("code per module (stdlib is j3.*; names are approximate):");
    for (name, size) in groups.iter().take(12) {
        println!("  {size:>9}  {name}");
    }
    funcs.sort_by(|a, b| b.0.cmp(&a.0));
    println!("biggest functions:");
    for (size, name) in funcs.iter().take(15) {
        let short = name.rsplit_once('_').map(|(a, _)| a).unwrap_or(name);
        println!("  {size:>9}  {}", short.trim_start_matches("j3_"));
    }
    0
}

fn print_fail(e: &str) {
    if let Ok(mut last) = crate::config::LAST_ERR.lock() {
        *last = e.to_string();
    }
    if e.contains("error:") || e.contains("warning:") || e.contains("-->") {
        eprint!("{e}");
        if !e.ends_with('\n') {
            eprintln!();
        }
    } else {
        eprintln!("j3c: {e}");
    }
    print_hint(e);
}

fn print_hint(text: &str) {
    let Some(hint) = crate::emit::failure_hint(text) else {
        return;
    };
    if text.contains(&hint) {
        return;
    }
    eprintln!("  = help: {hint}");
}

fn dump_opts(manifest: &Option<Manifest>) -> emit::EmitOpts {
    emit_opts(manifest).unwrap_or_else(|_| emit::EmitOpts::default())
}

fn emit_opts(manifest: &Option<Manifest>) -> Result<emit::EmitOpts, String> {
    let forced = EMIT.load(std::sync::atomic::Ordering::Relaxed) != 0;
    let mut opts = match manifest {
        Some(m) => emit::EmitOpts::from_manifest(m)?,
        None => emit::EmitOpts::default(),
    };
    match EMIT.load(std::sync::atomic::Ordering::Relaxed) {
        1 => opts.backend = emit::Backend::C,
        2 => opts.backend = emit::Backend::Llvm,
        _ => {}
    }
    if let Ok(g) = OPT.lock() {
        if let Some(o) = g.as_ref() {
            opts.cc.opt = o.clone();
        }
    }
    if FAST.load(std::sync::atomic::Ordering::Relaxed) {
        opts.cc.debug = false;
    }
    if RELEASE.load(std::sync::atomic::Ordering::Relaxed) {
        opts.cc.release = true;
        opts.cc.debug = false;
        opts.incremental = false;
        if !OPT.lock().map(|g| g.is_some()).unwrap_or(false) {
            opts.cc.opt = "s".to_string();
        }
    }
    opts.backend = emit::resolve_backend(opts.backend, forced, &opts.clang)?;
    Ok(opts)
}

fn run_loaded(
    prog: Result<(compile::Program, Option<Manifest>), String>,
    extra_cwd: Option<PathBuf>,
    args: &[String],
) -> i32 {
    match prog {
        Ok((prog, manifest)) => match emit_opts(&manifest) {
            Ok(mut opts) => {
                if let Some(m) = &manifest { crate::gen::before_build(&m.root); }
                if let Err(e) = crate::preflight::check(&prog, manifest.as_ref()) {
                    print_fail(&e);
                    return 1;
                }
                if opts.cwd.is_none() {
                    opts.cwd = extra_cwd;
                }
                let fast = manifest.as_ref().map(|m| m.cc.fast || m.emit.fast).unwrap_or(false);
                if up_to_date(&prog, &opts, fast) {
                    if let Some(exe) = &opts.cc.out {
                        status("up to date");
                        status("running");
                        return match emit::run_exe_in(exe, opts.cwd.as_deref(), args) {
                            Ok(result) => report_run(result),
                            Err(e) => {
                                print_fail(&e);
                                1
                            }
                        };
                    }
                }
                match emit::run_program_with(&prog.cx, &prog.modules, &opts, args) {
                    Ok(result) => {
                        mark_built(&prog, &opts, fast);
                        report_run(result)
                    }
                    Err(e) => {
                        print_fail(&e);
                        1
                    }
                }
            }
            Err(e) => {
                print_fail(&e);
                1
            }
        },
        Err(e) => {
            print_fail(&e);
            1
        }
    }
}

fn run_samples() -> i32 {
    match load_samples() {
        Ok(samples) => {
            let mut opts = emit::EmitOpts::default();
            opts.backend = emit::Backend::Llvm;
            if !emit::clang_available(&opts.clang) {
                opts.backend = emit::Backend::C;
            }
            match emit::run_program(&samples.cx, std::slice::from_ref(&samples.module), &opts) {
                Ok(result) => {
                    if result.status != 0 {
                        eprintln!("j3c: sample program exited {}", result.status);
                    }
                    result.status
                }
                Err(e) => {
                    eprintln!("j3c: {e}");
                    1
                }
            }
        }
        Err(e) => {
            eprintln!("j3c: IR sample failed validation: {e}");
            1
        }
    }
}

fn build_cwd() -> i32 {
    status("checking");
    build_loaded(compile_user(None))
}

fn build_path(path: &str) -> i32 {
    status("checking");
    let started = std::time::Instant::now();
    let prog = compile_user(Some(path));
    timing("parse + check", started);
    build_loaded(prog)
}

/// Hash of everything a single-target build's output depends on: every checked source
/// file, the emit options, fast mode, and this j3c binary.
fn source_key(prog: &compile::Program, opts: &emit::EmitOpts, fast: bool) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for f in &prog.cx.files {
        f.path.hash(&mut h);
        f.src.hash(&mut h);
    }
    format!("{opts:?}").hash(&mut h);
    (fast || FAST.load(std::sync::atomic::Ordering::Relaxed)).hash(&mut h);
    if let Some(meta) = crate::editor::host_exe().ok().and_then(|p| std::fs::metadata(p).ok()) {
        meta.len().hash(&mut h);
        meta.modified().ok().hash(&mut h);
    }
    format!("{:016x}", h.finish())
}

fn source_stamp(opts: &emit::EmitOpts) -> Option<PathBuf> {
    let dir = emit::host::cache_dir(opts.cc.out.as_ref()?, &opts.target);
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("src.key"))
}

/// The output exists and was built from exactly these sources and options, so emit and
/// clang can be skipped. Scene-only edits in the editor land here.
fn up_to_date(prog: &compile::Program, opts: &emit::EmitOpts, fast: bool) -> bool {
    let (Some(out), Some(stamp)) = (opts.cc.out.as_ref(), source_stamp(opts)) else { return false };
    out.exists() && std::fs::read_to_string(stamp).map(|k| k == source_key(prog, opts, fast)).unwrap_or(false)
}

fn mark_built(prog: &compile::Program, opts: &emit::EmitOpts, fast: bool) {
    if let Some(stamp) = source_stamp(opts) {
        let _ = std::fs::write(stamp, source_key(prog, opts, fast));
    }
}

/// With `J3C_TIMINGS` set, print how long a build phase took to stderr.
/// `run` / `build` keep a 3-line status instead, so timings stay quiet there.
pub(crate) fn timing(phase: &str, since: std::time::Instant) {
    // An indented name is a step inside a phase; `compilation.log` keeps those.
    if phase.starts_with("  ") {
        crate::phase::step(phase, since.elapsed().as_secs_f64());
    }
    if PROGRESS.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    if std::env::var_os("J3C_TIMINGS").is_some() {
        eprintln!("j3c: {phase}: {:.3}s", since.elapsed().as_secs_f64());
    }
}

static PROGRESS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// `run` and `build` turn this on so a long check or compile doesn't sit silent.
fn progress_on() {
    PROGRESS.store(true, std::sync::atomic::Ordering::Relaxed);
    crate::phase::enable();
}

/// Old one-line messages, routed to the two-line status in `phase`: a known message starts
/// an outer phase, anything else is inner progress.
pub(crate) fn status(msg: &str) {
    match msg {
        "checking" => crate::phase::begin("parse"),
        "emitting" => crate::phase::begin("emit"),
        "compiling" => crate::phase::begin("compile"),
        "linking" => crate::phase::begin("link"),
        "packaging" => crate::phase::begin("package"),
        "running" => crate::phase::begin("running"),
        "up to date" => crate::phase::begin("up to date"),
        _ => crate::phase::detail(msg),
    }
}

fn build_samples_err() -> i32 {
    eprintln!("j3c: `build samples` is not a thing; use `run samples`");
    1
}

fn build_loaded(prog: Result<(compile::Program, Option<Manifest>), String>) -> i32 {
    if !crate::config::compilation_tracking() {
        return build_inner(prog);
    }
    let started = std::time::Instant::now();
    let (root, modules, opts) = match &prog {
        Ok((p, m)) => (
            m.as_ref().map(|m| m.root.clone()).unwrap_or_default(),
            p.modules.len(),
            emit_opts(m).ok(),
        ),
        Err(_) => (PathBuf::new(), 0, None),
    };
    let code = build_inner(prog);
    let (opt, backend, target) = opts
        .map(|o| (o.cc.opt.clone(), format!("{:?}", o.backend).to_lowercase(), o.target.clone()))
        .unwrap_or_default();
    crate::config::log_compile(&crate::config::Compile {
        root: &root,
        opt: &opt,
        backend: &backend,
        target: &target,
        modules,
        secs: crate::phase::elapsed().max(started.elapsed().as_secs_f64()),
        phases: crate::phase::report(),
        ok: code == 0,
    });
    code
}

fn build_inner(prog: Result<(compile::Program, Option<Manifest>), String>) -> i32 {
    match prog {
        Ok((prog, manifest)) => match emit_opts(&manifest) {
            Ok(mut opts) => {
                if let Some(m) = &manifest { crate::gen::before_build(&m.root); }
                let targets = manifest
                    .as_ref()
                    .map(|m| m.emit.targets.clone())
                    .unwrap_or_default();
                if targets.len() <= 1 {
                    if opts.cc.out.is_none() {
                        opts.cc.out = Some(PathBuf::from("target").join("a.out"));
                    }
                    let fast = manifest.as_ref().map(|m| m.cc.fast || m.emit.fast).unwrap_or(false);
                    let out = opts.cc.out.clone().expect("out path");
                    if up_to_date(&prog, &opts, fast) {
                        status("up to date");
                        println!("{}", out.display());
                    } else {
                        let built = link_built(&prog, &opts);
                        if built != 0 {
                            return built;
                        }
                        mark_built(&prog, &opts, fast);
                    }
                    return package(&manifest, &[(opts.target.clone(), out)]);
                }
                let m = manifest.as_ref().expect("target list");
                // Targets share nothing (own objects, own output), so build them side by side:
                // each one's long pole leaves most cores idle.
                let failed = std::thread::scope(|scope| {
                    let jobs: Vec<_> = targets
                        .iter()
                        .map(|triple| {
                            let mut opts = opts.clone();
                            scope.spawn(move || {
                                crate::phase::set_tag(triple);
                                crate::phase::target(triple);
                                let built = match compile::compile_manifest_target(m, triple) {
                                    Ok(mut p) => {
                                        if FAST.load(std::sync::atomic::Ordering::Relaxed) {
                                            compile::apply_fast(&mut p);
                                        }
                                        p
                                    }
                                    Err(e) => {
                                        print_fail(&e);
                                        return 1;
                                    }
                                };
                                opts.target = triple.clone();
                                opts.cc.out = Some(m.out_path_for(triple));
                                link_built(&built, &opts)
                            })
                        })
                        .collect();
                    jobs.into_iter().map(|j| j.join().unwrap_or(1)).any(|code| code != 0)
                });
                if failed {
                    return 1;
                }
                let outs: Vec<_> = targets.iter().map(|t| (t.clone(), m.out_path_for(t))).collect();
                package(&manifest, &outs)
            }
            Err(e) => {
                print_fail(&e);
                1
            }
        },
        Err(e) => {
            print_fail(&e);
            1
        }
    }
}

/// `[dist] build_outputs` on a `--release` build: zip / `.deb` each target into `target/dist/`.
fn package(manifest: &Option<Manifest>, built: &[(String, PathBuf)]) -> i32 {
    let Some(m) = manifest.as_ref().filter(|m| !m.dist.outputs.is_empty()) else { return 0 };
    if !RELEASE.load(std::sync::atomic::Ordering::Relaxed) {
        return 0;
    }
    status("packaging");
    match crate::dist::package(m, built) {
        Ok(zips) => {
            for z in zips {
                println!("{}", z.display());
            }
            0
        }
        Err(e) => {
            print_fail(&e);
            1
        }
    }
}

/// What a non-zero exit code means (`panic` 101, runtime check 102, signal 128+n).
fn exit_reason(code: i32) -> String {
    match code {
        101 => " (panic)".into(),
        102 => " (runtime check failed)".into(),
        129..=159 => format!(" (killed by signal {})", code - 128),
        _ => String::new(),
    }
}

fn report_run(result: emit::RunResult) -> i32 {
    print!("{}", String::from_utf8_lossy(&result.stdout));
    if result.status != 0 {
        eprintln!("j3c: program exited {}{}", result.status, exit_reason(result.status));
        let err = String::from_utf8_lossy(&result.stderr);
        if err.is_empty() {
            eprintln!("j3c: it wrote nothing to stderr");
        } else {
            eprintln!("{err}");
        }
        print_hint(&err);
    }
    result.status
}

fn link_built(prog: &compile::Program, opts: &emit::EmitOpts) -> i32 {
    match emit::compile_program(&prog.cx, &prog.modules, opts) {
        Ok(exe) => {
            println!("{}", exe.display());
            0
        }
        Err(e) => {
            if opts.target.is_empty() {
                print_fail(&e);
            } else {
                print_fail(&format!("target {}:\n{e}", opts.target));
            }
            1
        }
    }
}

fn check_cmd(rest: &[String]) -> i32 {
    let mut json = false;
    let mut strict = false;
    let mut path = None;
    for a in rest {
        if a == "--json" {
            json = true;
        } else if a == "--strict" {
            strict = true;
        } else if a == "--help" || a == "-h" {
            println!("usage: j3c check [--json] [--strict] [path]\n\ntypechecks the project, then reports missing libraries and files, and (with [ui]) CSS lint\nwarnings for theme.css and every component <style> (`j3c doc css`, Lint).\n  --json    one JSON object: ok/entry (and css / i18n arrays of warnings) or the error\n  --strict  exit 1 when the CSS lint or i18n check finds anything (default: warnings only)\nAlso warns `file:line:col: message [i18n-missing]` when a `t(\"key\")`/`tf` key is not in the default locale or a locale has a key the default lacks (`j3c doc i18n`).\nSilence: /* j3-lint-ignore rule */ before a line, or [ui] lint = false / lint-off = [\"rule\"] in j3.toml.");
            return 0;
        } else if a.starts_with('-') {
            eprintln!("j3c: unknown flag `{a}`");
            eprintln!("  = help: `j3c check [--json] [--strict] [path]`");
            return 1;
        } else if path.is_none() {
            path = Some(a.as_str());
        } else {
            eprintln!("j3c: unexpected argument `{a}`");
            return 1;
        }
    }
    let t_check = std::time::Instant::now();
    let res = compile_user(path);
    timing("check total", t_check);
    match res {
        Ok((prog, manifest)) => {
            if let Err(e) = crate::preflight::check(&prog, manifest.as_ref()) {
                if json {
                    println!("{}", json_err(&e));
                } else {
                    print_fail(&e);
                }
                return 1;
            }
            let entry = match &manifest {
                Some(m) => m.entry.display().to_string(),
                None => path.unwrap_or(".").to_string(),
            };
            // CSS lint: warnings only (exit code unchanged unless --strict)
            let css = match manifest.as_ref().map(crate::csslint::lint_project) {
                Some(Ok(w)) => w,
                Some(Err(e)) => {
                    eprintln!("note: css lint skipped: {e}");
                    Vec::new()
                }
                None => Vec::new(),
            };
            let i18n = manifest.as_ref().map(|m| crate::i18n::lint(&m.root)).unwrap_or_default();
            if json {
                println!("{}", check_json(&entry, &css, &i18n));
            } else {
                for w in css.iter().chain(&i18n) {
                    println!("{w}");
                }
                println!("checked {entry}");
            }
            if strict && !(css.is_empty() && i18n.is_empty()) { 1 } else { 0 }
        }
        Err(e) => {
            if json {
                println!("{}", json_err(&e));
            } else {
                print_fail(&e);
            }
            1
        }
    }
}

/// The `check --json` line for a project that typechecks; `css` and `i18n` are the warning arrays (omitted when empty).
fn check_json(entry: &str, css: &[String], i18n: &[String]) -> String {
    let arr = |key: &str, v: &[String]| if v.is_empty() { String::new() } else { format!(",\"{key}\":[{}]", v.iter().map(|w| json_str(w)).collect::<Vec<_>>().join(",")) };
    format!("{{\"ok\":true,\"entry\":{}{}{}}}", json_str(entry), arr("css", css), arr("i18n", i18n))
}

fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_err(text: &str) -> String {
    let mut message = String::new();
    let mut path = String::new();
    let mut line = 0usize;
    let mut column = 0usize;
    let mut notes = Vec::new();
    let mut helps = Vec::new();
    for raw in text.lines() {
        let t = raw.trim();
        if let Some(rest) = t.strip_prefix("error:") {
            if message.is_empty() {
                message = rest.trim().to_string();
            }
        } else if let Some(rest) = t.strip_prefix("-->") {
            let rest = rest.trim();
            if let Some((p, l, c)) = split_loc(rest) {
                path = p;
                line = l;
                column = c;
            } else if path.is_empty() {
                path = rest.to_string();
            }
        } else if let Some(rest) = t.strip_prefix("= note:") {
            notes.push(rest.trim().to_string());
        } else if let Some(rest) = t.strip_prefix("= help:") {
            helps.push(rest.trim().to_string());
        }
    }
    if message.is_empty() {
        message = text.trim().to_string();
    }
    let notes = json_list(&notes);
    let helps = json_list(&helps);
    format!(
        "{{\"ok\":false,\"message\":{},\"path\":{},\"line\":{line},\"column\":{column},\"notes\":{notes},\"helps\":{helps}}}",
        json_str(&message),
        json_str(&path),
    )
}

fn json_list(items: &[String]) -> String {
    let mut out = String::from("[");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&json_str(item));
    }
    out.push(']');
    out
}

fn split_loc(s: &str) -> Option<(String, usize, usize)> {
    let (rest, col) = s.rsplit_once(':')?;
    let (path, line) = rest.rsplit_once(':')?;
    Some((path.to_string(), line.parse().ok()?, col.parse().ok()?))
}

fn fmt_cmd(rest: &[String]) -> i32 {
    let mut write = false;
    let mut check = false;
    let mut path = None;
    for a in rest {
        if a == "--write" || a == "-w" {
            write = true;
        } else if a == "--check" {
            check = true;
        } else if !a.starts_with('-') && path.is_none() {
            path = Some(a.as_str());
        }
    }
    let files = match collect_j3(path) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("j3c: {e}");
            return 1;
        }
    };
    if files.is_empty() {
        eprintln!("j3c: no `.j3` or `.j3s` files to format");
        return 1;
    }
    let mut dirty = 0;
    for file in &files {
        let src = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("j3c: read {}: {e}", file.display());
                return 1;
            }
        };
        let out = match crate::fmt::format_src(&src) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("j3c: fmt {}: {e}", file.display());
                return 1;
            }
        };
        if out == src {
            if path.is_some() && !write && !check && files.len() == 1 {
                print!("{out}");
            }
            continue;
        }
        dirty += 1;
        if check {
            eprintln!("j3c: {} needs formatting", file.display());
            continue;
        }
        if write {
            if let Err(e) = std::fs::write(file, &out) {
                eprintln!("j3c: write {}: {e}", file.display());
                return 1;
            }
        } else if files.len() == 1 {
            print!("{out}");
        } else {
            println!("// --- {} ---", file.display());
            print!("{out}");
        }
    }
    if check && dirty > 0 {
        1
    } else {
        0
    }
}

fn collect_j3(path: Option<&str>) -> Result<Vec<PathBuf>, String> {
    match path {
        None => {
            let here = std::env::current_dir().map_err(|e| format!("cwd: {e}"))?;
            let src = here.join("src");
            if src.is_dir() {
                list_project_sources(&here)
            } else {
                list_j3(&here)
            }
        }
        Some(p) => {
            let p = PathBuf::from(p);
            if p.is_dir() {
                let src = p.join("src");
                if src.is_dir() {
                    list_project_sources(&p)
                } else {
                    list_j3(&p)
                }
            } else {
                Ok(vec![p])
            }
        }
    }
}

/// `src/**/*.j3` plus every `.j3s` under the project root (scripts can live anywhere).
fn list_project_sources(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = list_j3(&root.join("src"))?;
    for rel in crate::compile::script_paths(root) {
        let path = root.join(rel);
        if !out.contains(&path) {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

fn list_j3(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    walk_j3(dir, &mut out)?;
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_without_project_points_at_new() {
        let err = match compile_user(None) {
            Err(e) => e,
            Ok(_) => panic!("expected no project in the compiler directory"),
        };
        assert!(err.contains("j3c new"), "{err}");
        assert!(err.contains("j3c guide"), "{err}");
        assert_eq!(run(["run".into()]), 1);
        assert_eq!(run(["check".into()]), 1);
    }

    #[test]
    fn check_json_on_bad_file() {
        let dir = std::env::temp_dir().join(format!(
            "j3-check-json-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("bad.j3");
        std::fs::write(&file, "module bad;\nfn main() { noSuch(); }\n").unwrap();
        let err = match compile_user(Some(file.to_str().unwrap())) {
            Err(e) => e,
            Ok(_) => panic!("bad.j3 should not typecheck"),
        };
        let json = json_err(&err);
        assert!(json.contains("\"ok\":false"), "{json}");
        assert!(json.contains("\"line\":2"), "{json}");
        assert!(json.contains("noSuch") || json.contains("unknown"), "{json}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn check_json_carries_i18n_warnings_and_strict_fails_on_them() {
        let w = vec!["warning: a.j3:1:1: key `x` is not in locales/en.txt [i18n-missing]".to_string()];
        let j = check_json("src/main.j3", &[], &w);
        assert_eq!(j, "{\"ok\":true,\"entry\":\"src/main.j3\",\"i18n\":[\"warning: a.j3:1:1: key `x` is not in locales/en.txt [i18n-missing]\"]}");
        assert_eq!(check_json("e", &[], &[]), "{\"ok\":true,\"entry\":\"e\"}");
        let dir = std::env::temp_dir().join(format!("j3-check-i18n-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("locales")).unwrap();
        std::fs::write(dir.join("j3.toml"), "[package]\nname = \"i18nc\"\nentry = \"src/main.j3\"\n").unwrap();
        std::fs::write(dir.join("locales/en.txt"), "known = Known\n").unwrap();
        std::fs::write(dir.join("src/main.j3"), "module i18nc;\nfn t(k: string): string { return k; }\nfn main() {\n    t(\"known\");\n    t(\"unknown\");\n}\n").unwrap();
        let path = dir.display().to_string();
        assert_eq!(run(["check".into(), path.clone()]), 0);
        assert_eq!(run(["check".into(), "--strict".into(), path]), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_stops_when_check_fails() {
        let dir = std::env::temp_dir().join(format!("j3-run-gate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("j3.toml"),
            "[package]\nname = \"gate\"\nentry = \"src/main.j3\"\n\n[link]\nlibs = [\"SDL2\"]\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("src/main.j3"),
            "module gate;\n\nuse j3.game.tryLoadFont;\n\nfn main() {\n    let font = tryLoadFont(\"nope.ttf\", 12);\n    if font.raw == 0 {\n        return;\n    }\n}\n",
        )
        .unwrap();
        let path = dir.display().to_string();
        assert_eq!(run(["check".into(), path.clone()]), 1);
        assert_eq!(run(["run".into(), path]), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_skips_unchanged_sources() {
        let dir = std::env::temp_dir().join(format!("j3-run-skip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("j3.toml"), "[package]\nname = \"skip\"\nentry = \"src/main.j3\"\n").unwrap();
        std::fs::write(&dir.join("src/main.j3"), "module skip;\n\nfn main() {\n}\n").unwrap();
        let path = dir.display().to_string();
        let exe = dir.join("target/skip");
        assert_eq!(run(["run".into(), path.clone()]), 0);
        let first = std::fs::metadata(&exe).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(run(["run".into(), path]), 0);
        assert_eq!(
            std::fs::metadata(&exe).unwrap().modified().unwrap(),
            first,
            "unchanged run relinked"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn incremental_edit_recompiles_one_module() {
        let dir = std::env::temp_dir().join(format!("j3-incr-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("j3.toml"),
            "[package]\nname = \"incr\"\nentry = \"src/main.j3\"\n\n[emit]\nbackend = llvm\nopt = 0\ndebug = true\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("src/main.j3"),
            "module incr;\n\nuse j3.io.printLn;\nuse helper.word;\n\nfn main() {\n    printLn(word());\n}\n",
        )
        .unwrap();
        let helper = dir.join("src/helper.j3");
        std::fs::write(&helper, "module helper;\n\npub fn word(): string {\n    return \"one\";\n}\n").unwrap();
        let path = dir.display().to_string();
        assert_eq!(build_path(&path), 0);
        let exe = dir.join("target/incr");
        let first = std::process::Command::new(&exe).output().unwrap();
        assert!(
            String::from_utf8_lossy(&first.stdout).contains("one"),
            "{}",
            String::from_utf8_lossy(&first.stderr)
        );
        let (prog, manifest) = compile_user(Some(&path)).unwrap();
        let mut opts = emit_opts(&manifest).unwrap();
        opts.cc.opt = "2".into();
        opts.incremental = false;
        opts.cc.out = Some(dir.join("target/incr-o2"));
        let whole = emit::run_program_with(&prog.cx, &prog.modules, &opts, &[]).unwrap();
        assert_eq!(whole.stdout, first.stdout, "per-module output differed from one clang invocation");

        let before = obj_mtimes(&dir);
        std::fs::write(&helper, "module helper;\n\npub fn word(): string {\n    return \"two\";\n}\n").unwrap();
        assert_eq!(build_path(&path), 0);
        let second = std::process::Command::new(&exe).output().unwrap();
        assert!(String::from_utf8_lossy(&second.stdout).contains("two"));
        let after = obj_mtimes(&dir);
        for (name, modified) in &before {
            if name.contains("helper") {
                continue;
            }
            let same = after.iter().find(|(n, _)| n == name).map(|(_, t)| *t);
            assert_eq!(same, Some(*modified), "{name} was rebuilt");
        }
        let helper_before: Vec<_> = before.iter().filter(|(n, _)| n.contains("helper")).map(|(n, _)| n.clone()).collect();
        let helper_after: Vec<_> = after.iter().filter(|(n, _)| n.contains("helper")).map(|(n, _)| n.clone()).collect();
        assert_eq!(helper_after.len(), 1, "{helper_after:?}");
        assert_ne!(helper_before, helper_after, "helper object was reused after an edit");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn incremental_all_splits_optimized_builds() {
        let dir = std::env::temp_dir().join(format!("j3-incr-all-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("j3.toml"),
            "[package]\nname = \"incrall\"\nentry = \"src/main.j3\"\n\n[emit]\nbackend = llvm\nopt = 2\nincremental = \"all\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("src/main.j3"),
            "module incrall;\n\nuse j3.io.printLn;\n\nfn main() {\n    printLn(\"split\");\n}\n",
        )
        .unwrap();
        assert_eq!(build_path(&dir.display().to_string()), 0);
        let out = std::process::Command::new(dir.join("target/incrall")).output().unwrap();
        assert!(String::from_utf8_lossy(&out.stdout).contains("split"));
        assert!(!obj_mtimes(&dir).is_empty(), "opt 2 with incremental = all made no per-module objects");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn obj_mtimes(root: &std::path::Path) -> Vec<(String, std::time::SystemTime)> {
        let mut out = Vec::new();
        let host = root.join("target/.j3c/host");
        let dir = std::fs::read_dir(&host).unwrap().flatten().map(|e| e.path().join("obj")).find(|d| d.is_dir()).unwrap();
        for ent in std::fs::read_dir(&dir).unwrap().flatten() {
            let name = ent.file_name().to_string_lossy().to_string();
            if !name.ends_with(".o") {
                continue;
            }
            out.push((name, ent.metadata().unwrap().modified().unwrap()));
        }
        out.sort();
        out
    }

    #[test]
    fn build_skips_unchanged_sources() {
        let dir = std::env::temp_dir().join(format!("j3-build-skip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("j3.toml"), "[package]\nname = \"skip\"\nentry = \"src/main.j3\"\n").unwrap();
        let main = dir.join("src/main.j3");
        std::fs::write(&main, "module skip;\n\nfn main() {\n}\n").unwrap();
        let path = dir.display().to_string();
        let exe = dir.join("target/skip");
        assert_eq!(build_path(&path), 0);
        let first = std::fs::metadata(&exe).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(build_path(&path), 0);
        assert_eq!(std::fs::metadata(&exe).unwrap().modified().unwrap(), first, "unchanged build relinked");
        std::fs::write(&main, "module skip;\n\nfn main() {\n    let x = 1;\n}\n").unwrap();
        assert_eq!(build_path(&path), 0);
        assert_ne!(std::fs::metadata(&exe).unwrap().modified().unwrap(), first, "edit didn't rebuild");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn manifest_help_lists_package_name() {
        assert!(crate::helptext::MANIFEST.contains("name"));
        assert!(crate::helptext::MANIFEST.contains("J3_STDLIB"));
        assert!(crate::helptext::GUIDE.contains("j3c new"));
    }
}

fn walk_j3(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let ents = std::fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
    for ent in ents.flatten() {
        let p = ent.path();
        if p.is_dir() {
            walk_j3(&p, out)?;
        } else if matches!(p.extension().and_then(|e| e.to_str()), Some("j3" | "j3s")) {
            out.push(p);
        }
    }
    Ok(())
}
