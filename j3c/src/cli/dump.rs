use super::*;

pub(super) fn dump_syntax(rest: &[String]) -> i32 {
    const USAGE: &str = "usage: j3c dump-syntax [--ui] [file]";
    if rest.iter().any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        println!("  --ui   print the .j3ui grammar instead; no file prints to stdout");
        return 0;
    }
    if let Some(bad) = rest.iter().find(|a| a.starts_with('-') && a.as_str() != "--ui") {
        eprintln!("j3c dump-syntax: unknown flag `{bad}`; {USAGE}");
        return 1;
    }
    let ui = rest.iter().any(|a| a == "--ui");
    let text = if ui { crate::parse::syntax::ui_tm_grammar() } else { crate::parse::syntax::tm_grammar() };
    match rest.iter().find(|a| !a.starts_with('-')) {
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

pub(super) fn load_samples() -> Result<samples::PlaceSamples, String> {
    let samples = samples::place_samples();
    match validate_module(&samples.cx, &samples.module) {
        Ok(()) => Ok(samples),
        Err(e) => Err(e.message),
    }
}

pub(super) fn load_input(path: Option<&str>) -> Result<Input, String> {
    match path {
        None => project::resolve_cwd(),
        Some(p) => project::resolve(Path::new(p)),
    }
}

pub(super) fn compile_input(input: &Input) -> Result<(compile::Program, Option<Manifest>), String> {
    match input {
        Input::Hello => Ok((compile::compile_path(&compile::default_hello_path())?, None)),
        Input::File(p) => Ok((compile::compile_path(p)?, None)),
        Input::Project(m) => Ok((compile::compile_manifest(m)?, Some(m.clone()))),
    }
}

pub(super) fn compile_entry(path: Option<&str>) -> Result<(compile::Program, Option<Manifest>), String> {
    let (mut prog, manifest) = compile_input(&load_input(path)?)?;
    if FAST.load(std::sync::atomic::Ordering::Relaxed) {
        compile::apply_fast(&mut prog);
    }
    Ok((prog, manifest))
}

/// `run` / `build` / `check` with no path. A missing `j3.toml` is an error.
/// Dump commands keep the built-in hello via [`compile_entry`].
pub(super) fn compile_user(path: Option<&str>) -> Result<(compile::Program, Option<Manifest>), String> {
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

pub(super) fn dump_ir_hello() -> i32 {
    dump_ir_prog(compile_entry(None))
}

pub(super) fn dump_ir_path(path: &str) -> i32 {
    dump_ir_prog(compile_entry(Some(path)))
}

pub(super) fn dump_bc_path(path: &str) -> i32 {
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

pub(super) fn dump_ir_prog(prog: Result<(compile::Program, Option<Manifest>), String>) -> i32 {
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

pub(super) fn dump_ir_samples() -> i32 {
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

pub(super) fn dump_c_hello() -> i32 {
    dump_c_prog(compile_entry(None))
}

pub(super) fn dump_c_path(path: &str) -> i32 {
    dump_c_prog(compile_entry(Some(path)))
}

pub(super) fn dump_c_prog(prog: Result<(compile::Program, Option<Manifest>), String>) -> i32 {
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

pub(super) fn dump_ll_hello() -> i32 {
    dump_ll_prog(compile_entry(None))
}

pub(super) fn dump_ll_path(path: &str) -> i32 {
    dump_ll_prog(compile_entry(Some(path)))
}

pub(super) fn dump_ll_prog(prog: Result<(compile::Program, Option<Manifest>), String>) -> i32 {
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

pub(super) fn dump_ll_samples() -> i32 {
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

pub(super) fn dump_c_samples() -> i32 {
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
