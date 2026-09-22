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

    let cmd = args[0].as_str();
    let rest = &args[1..];
    match cmd {
        "help" | "--help" | "-h" => help_cmd(rest),
        "version" | "--version" | "-V" => {
            println!("j3c {VERSION}");
            0
        }
        "dump-ir" => dispatch(rest, dump_ir_hello, dump_ir_samples, dump_ir_path, false),
        "dump-c" => dispatch(rest, dump_c_hello, dump_c_samples, dump_c_path, false),
        "dump-llvm" => dispatch(rest, dump_ll_hello, dump_ll_samples, dump_ll_path, false),
        "dump-syntax" => dump_syntax(rest),
        "new" => crate::scaffold::cmd(rest),
        "check" => check_cmd(rest),
        "doc" => crate::doc::cmd(rest),
        "guide" => {
            print!("{}", crate::helptext::GUIDE);
            0
        }
        "run" => dispatch(rest, run_cwd, run_samples, run_path, true),
        "build" => dispatch(rest, build_cwd, build_samples_err, build_path, true),
        "fmt" => fmt_cmd(rest),
        "lsp" => crate::lsp::serve(),
        other => {
            eprintln!("j3c: unknown command `{other}`");
            eprintln!("run `j3c help`");
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

fn peel_args(rest: &[String]) -> (bool, Option<&str>) {
    let mut fast = false;
    let mut path = None;
    let mut i = 0;
    while i < rest.len() {
        let a = rest[i].as_str();
        if a == "--fast" {
            fast = true;
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

fn print_help() {
    println!("j3c {VERSION} — J3 compiler");
    println!();
    println!("usage:");
    println!("    j3c <command> [file.j3 | project]");
    println!();
    println!("commands:");
    println!("    new [name]       write j3.toml, src/main.j3, and AGENTS.md (hello)");
    println!("    new [name] --app SDL window (links SDL2, image, mixer, ttf)");
    println!("    guide            how to start a project");
    println!("    check [path]     typecheck; no emit or link");
    println!("    check --json     one JSON object on stdout");
    println!("    doc [module]     list stdlib modules, or one module's public items");
    println!("    build [path]     emit, compile, write `target/<name>`");
    println!("    run [path]       emit, compile, execute");
    println!("    fmt [path]       pretty-print `.j3` (same lexer; keeps comments)");
    println!("    fmt --write      rewrite files");
    println!("    fmt --check      exit 1 if formatting would change a file");
    println!("    lsp              language server (stdio JSON-RPC)");
    println!("    dump-ir [path]   parse + typecheck; print IR");
    println!("    dump-ir samples  print hand-built place-IR samples");
    println!("    dump-c [path]    print C from the program");
    println!("    dump-llvm [path] print LLVM IR from the program");
    println!("    dump-syntax [f]  print TextMate grammar (lexer/parser names)");
    println!("    --fast           with run/build: strip bounds-check Abort; no `-g`");
    println!("    --emit llvm|c    backend (default llvm; override [emit] backend)");
    println!("    --opt N          0/1/2/3/s/z (override [emit] opt)");
    println!("    run samples      emit, compile, execute place tests");
    println!("    help manifest    print j3.toml keys");
    println!("    version          print version");
    println!("    help             print this help");
    println!();
    println!("A path may be a `.j3` file, a `j3.toml`, or a directory with `j3.toml`.");
    println!("With no path, run, build, and check use ./j3.toml and stop if it is missing.");
    println!("dump-ir, dump-c, and dump-llvm still fall back to the built-in hello.");
    println!("emit: LLVM by default (clang); C is optional. Options live in [emit] of j3.toml.");
    println!("run/build of a project executes with cwd = package root (`J3_ROOT`).");
    println!("`j3c guide` is the short start. `j3c doc` lists the standard library.");
}

fn dump_syntax(rest: &[String]) -> i32 {
    let text = crate::parse::syntax::tm_grammar();
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

fn run_cwd() -> i32 {
    run_loaded(compile_user(None), None)
}

fn run_path(path: &str) -> i32 {
    run_loaded(compile_user(Some(path)), None)
}

fn print_fail(e: &str) {
    if e.contains("error:") || e.contains("warning:") || e.contains("-->") {
        eprint!("{e}");
        if !e.ends_with('\n') {
            eprintln!();
        }
    } else {
        eprintln!("j3c: {e}");
    }
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
    opts.backend = emit::resolve_backend(opts.backend, forced, &opts.clang)?;
    Ok(opts)
}

fn run_loaded(
    prog: Result<(compile::Program, Option<Manifest>), String>,
    extra_cwd: Option<PathBuf>,
) -> i32 {
    match prog {
        Ok((prog, manifest)) => match emit_opts(&manifest) {
            Ok(mut opts) => {
                if opts.cwd.is_none() {
                    opts.cwd = extra_cwd;
                }
                match emit::run_program(&prog.cx, &prog.modules, &opts) {
                    Ok(result) => {
                        print!("{}", String::from_utf8_lossy(&result.stdout));
                        if result.status != 0 {
                            eprintln!("j3c: program exited {}", result.status);
                            let err = String::from_utf8_lossy(&result.stderr);
                            if !err.is_empty() {
                                eprintln!("{err}");
                            }
                        }
                        result.status
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
    build_loaded(compile_user(None))
}

fn build_path(path: &str) -> i32 {
    build_loaded(compile_user(Some(path)))
}

fn build_samples_err() -> i32 {
    eprintln!("j3c: `build samples` is not a thing; use `run samples`");
    1
}

fn build_loaded(prog: Result<(compile::Program, Option<Manifest>), String>) -> i32 {
    match prog {
        Ok((prog, manifest)) => match emit_opts(&manifest) {
            Ok(mut opts) => {
                let targets = manifest
                    .as_ref()
                    .map(|m| m.emit.targets.clone())
                    .unwrap_or_default();
                if targets.len() <= 1 {
                    if opts.cc.out.is_none() {
                        opts.cc.out = Some(PathBuf::from("target").join("a.out"));
                    }
                    return link_built(&prog, &opts);
                }
                let m = manifest.as_ref().expect("target list");
                for triple in &targets {
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
                    if link_built(&built, &opts) != 0 {
                        return 1;
                    }
                }
                0
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

fn link_built(prog: &compile::Program, opts: &emit::EmitOpts) -> i32 {
    match emit::emit_text(&prog.cx, &prog.modules, opts) {
        Ok(text) => match emit::compile_text(&text, opts) {
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
        },
        Err(e) => {
            eprintln!("j3c: emit failed: {e}");
            1
        }
    }
}

fn check_cmd(rest: &[String]) -> i32 {
    let mut json = false;
    let mut path = None;
    for a in rest {
        if a == "--json" {
            json = true;
        } else if a.starts_with('-') {
            eprintln!("j3c: unknown flag `{a}`");
            eprintln!("  = help: `j3c check --json [path]`");
            return 1;
        } else if path.is_none() {
            path = Some(a.as_str());
        } else {
            eprintln!("j3c: unexpected argument `{a}`");
            return 1;
        }
    }
    match compile_user(path) {
        Ok((_, manifest)) => {
            let entry = match &manifest {
                Some(m) => m.entry.display().to_string(),
                None => path.unwrap_or(".").to_string(),
            };
            if json {
                println!(
                    "{{\"ok\":true,\"entry\":{}}}",
                    json_str(&entry)
                );
            } else {
                println!("checked {entry}");
            }
            0
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
        eprintln!("j3c: no `.j3` files to format");
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
                list_j3(&src)
            } else {
                list_j3(&here)
            }
        }
        Some(p) => {
            let p = PathBuf::from(p);
            if p.is_dir() {
                let src = p.join("src");
                if src.is_dir() {
                    list_j3(&src)
                } else {
                    list_j3(&p)
                }
            } else {
                Ok(vec![p])
            }
        }
    }
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
        } else if p.extension().and_then(|e| e.to_str()) == Some("j3") {
            out.push(p);
        }
    }
    Ok(())
}
