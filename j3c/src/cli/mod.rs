//! Tiny argv parser. No clap — keeps /target small.

use std::path::{Path, PathBuf};

use crate::compile;
use crate::emit;
use crate::ir::dump::{dump_module, dump_program};
use crate::ir::samples;
use crate::ir::validate::validate_module;
use crate::project::{self, Input, Manifest};

mod build;
mod check;
mod dump;
mod runner;

pub(crate) use build::*;
use check::*;
use dump::*;
pub(crate) use runner::*;

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
        Some("android") => crate::android_run::cmd(rest),
        Some("script-props") => script_props_cmd(rest),
        other => {
            if let Some(o) = other { eprintln!("j3c -e: unknown editor tool `{o}`"); }
            eprintln!("usage: j3c -e scene|render|gen|imgdiff|mcp|android|script-props ...  (or --editor)");
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
pub(crate) static RELEASE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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
    println!("    -e android run [dir] build the APK, install it on an adb device, start it, print its log (`-e android --help`)");
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
    println!("    --release        with run/build: smallest binary (opt s, no `-g`, stripped; keeps checks); build also writes [dist] build_outputs (zip, deb: `j3c doc ship`); `j3.build.RELEASE`/`DEV` say which");
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

#[cfg(test)]
mod tests;
