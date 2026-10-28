use super::*;

/// `run`'s positionals: flags, then one path, then arguments for the program.
pub(super) fn run_dispatch(rest: &[String]) -> i32 {
    let (fast, path, args) = peel_run(rest);
    FAST.store(fast, std::sync::atomic::Ordering::Relaxed);
    match path {
        Some("samples") => run_samples(),
        Some(p) => run_user(Some(p), &args),
        None => run_user(None, &args),
    }
}

/// `run --hot`: rebuild and restart on source edits (`hot.rs`).
pub(super) static HOT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(super) fn peel_run(rest: &[String]) -> (bool, Option<&str>, Vec<String>) {
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
pub(super) fn run_user(path: Option<&str>, args: &[String]) -> i32 {
    status("checking");
    if !HOT.load(std::sync::atomic::Ordering::Relaxed) {
        if let Some((_, opts)) = unchanged_project(path, "run") {
            let exe = opts.cc.out.clone().expect("out path");
            status("up to date");
            status("running");
            return match emit::run_exe_in(&exe, opts.cwd.as_deref(), args) {
                Ok(result) => report_run(result),
                Err(e) => {
                    print_fail(&e);
                    1
                }
            };
        }
    }
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

pub(super) fn print_fail(e: &str) {
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

pub(super) fn print_hint(text: &str) {
    let Some(hint) = crate::emit::failure_hint(text) else {
        return;
    };
    if text.contains(&hint) {
        return;
    }
    eprintln!("  = help: {hint}");
}

pub(super) fn dump_opts(manifest: &Option<Manifest>) -> emit::EmitOpts {
    emit_opts(manifest).unwrap_or_else(|_| emit::EmitOpts::default())
}

pub(super) fn emit_opts(manifest: &Option<Manifest>) -> Result<emit::EmitOpts, String> {
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

pub(super) fn run_loaded(
    prog: Result<(compile::Program, Option<Manifest>), String>,
    extra_cwd: Option<PathBuf>,
    args: &[String],
) -> i32 {
    match prog {
        Ok((prog, manifest)) => match emit_opts(&manifest) {
            Ok(mut opts) => {
                if emit::host::target_os(&opts.target) == emit::host::TargetOs::Android {
                    print_fail("this is an Android target, which can't run on this machine; build an apk to run it on a device or emulator");
                    return 1;
                }
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
                        mark_inputs();
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

pub(super) fn run_samples() -> i32 {
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
