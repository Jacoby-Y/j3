use super::*;

/// `j3c size [path] [--target T]`: what is in the built binary. Reads it with `llvm-nm` /
/// `llvm-size` (ELF, PE, and Mach-O) or the binutils ones; a stripped binary has no symbols,
/// so build without `--release` to see the per-function breakdown.
pub(super) fn size_cmd(rest: &[String]) -> i32 {
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
    // A symbol cannot be split back into module and name (both can hold `_`), so each one is
    // looked up in the program that the same sources make.
    let owners = match compile_user(path) {
        Ok((prog, _)) => symbol_owners(&prog),
        Err(e) => {
            print_fail(&e);
            return 1;
        }
    };
    let mut groups: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
    for (size, name) in &funcs {
        let module = owners.get(name).map_or("(not in these sources)", |(m, _)| m.as_str());
        *groups.entry(module.to_string()).or_default() += size;
    }
    let mut groups: Vec<_> = groups.into_iter().collect();
    groups.sort_by(|a, b| b.1.cmp(&a.1));
    println!("code per module (stdlib modules are j3.*):");
    for (name, size) in groups.iter().take(12) {
        println!("  {size:>9}  {name}");
    }
    funcs.sort_by(|a, b| b.0.cmp(&a.0));
    println!("biggest functions:");
    for (size, name) in funcs.iter().take(15) {
        let shown = match owners.get(name) {
            Some((m, label)) => format!("{m}.{label}"),
            None => name.clone(),
        };
        println!("  {size:>9}  {shown}");
    }
    0
}

/// Linker symbol -> (module, function label) for every function in `prog`, named with the
/// same `emit::func_c_name` the C and LLVM emitters use. Methods are labelled `Struct.name`.
pub(super) fn symbol_owners(prog: &compile::Program) -> std::collections::HashMap<String, (String, String)> {
    let cx = &prog.cx;
    cx.funcs()
        .iter()
        .map(|f| {
            let module = f.module.iter().map(|p| cx.name_str(*p)).collect::<Vec<_>>().join(".");
            let name = cx.name_str(f.name);
            let label = match f.method_of {
                Some(sid) => format!("{}.{name}", cx.name_str(cx.struct_def(sid).name)),
                None => name.to_string(),
            };
            (emit::func_c_name(cx, f), (module, label))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_names_a_function_by_its_module_even_when_both_have_underscores() {
        let dir = std::env::temp_dir().join(format!("j3-size-owners-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("main.j3");
        std::fs::write(
            &file,
            "module closure_trace;\n\nfn more_value(x: int): int {\n    return x * 2;\n}\n\nfn main() {\n    more_value(2);\n}\n",
        )
        .unwrap();
        let prog = compile::compile_path(&file).expect("check");
        let owners = symbol_owners(&prog);
        let _ = std::fs::remove_dir_all(&dir);
        let hit = owners
            .iter()
            .find(|(sym, _)| sym.starts_with("j3_closure_trace_more_value_"))
            .unwrap_or_else(|| panic!("no symbol for more_value in {owners:?}"));
        assert_eq!(hit.1, &("closure_trace".to_string(), "more_value".to_string()));
    }
}

pub(super) fn build_cwd() -> i32 {
    status("checking");
    if let Some((m, opts)) = unchanged_project(None, "build") {
        return built_unchanged(m, &opts);
    }
    build_loaded(compile_user(None))
}

pub(super) fn build_path(path: &str) -> i32 {
    status("checking");
    if let Some((m, opts)) = unchanged_project(Some(path), "build") {
        return built_unchanged(m, &opts);
    }
    let started = std::time::Instant::now();
    let prog = compile_user(Some(path));
    timing("parse + check", started);
    build_loaded(prog)
}

/// Hash of everything a single-target build's output depends on: every checked source
/// file, the emit options, fast mode, and this j3c binary.
pub(super) fn source_key(prog: &compile::Program, opts: &emit::EmitOpts, fast: bool) -> String {
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

pub(super) fn source_stamp(opts: &emit::EmitOpts) -> Option<PathBuf> {
    let dir = emit::host::cache_dir(opts.cc.out.as_ref()?, &opts.target);
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("src.key"))
}

/// The output exists and was built from exactly these sources and options, so emit and
/// clang can be skipped. Scene-only edits in the editor land here.
pub(super) fn up_to_date(prog: &compile::Program, opts: &emit::EmitOpts, fast: bool) -> bool {
    let (Some(out), Some(stamp)) = (opts.cc.out.as_ref(), source_stamp(opts)) else { return false };
    out.exists() && std::fs::read_to_string(stamp).map(|k| k == source_key(prog, opts, fast)).unwrap_or(false)
}

pub(super) fn mark_built(prog: &compile::Program, opts: &emit::EmitOpts, fast: bool) {
    if let Some(stamp) = source_stamp(opts) {
        let _ = std::fs::write(stamp, source_key(prog, opts, fast));
    }
    mark_inputs();
}

/// `inputs.key` for this build, taken before parsing; written once the output matches it.
static INPUTS: std::sync::Mutex<Option<(PathBuf, String)>> = std::sync::Mutex::new(None);

/// Record that the output was built from the project files as they were when this build started.
pub(super) fn mark_inputs() {
    if let Some((stamp, key)) = INPUTS.lock().ok().and_then(|mut g| g.take()) {
        let _ = std::fs::write(stamp, key);
    }
}

/// Stat-only key of the project: every file and folder under the root (path, size, mtime),
/// except `target/` and dot folders, plus the emit options and this j3c. No key (no shortcut)
/// when `J3_STDLIB` is set.
fn inputs_key(m: &Manifest, opts: &emit::EmitOpts, fast: bool) -> Option<String> {
    use std::hash::{Hash, Hasher};
    if crate::stdlib::using_override() {
        return None;
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut stack = vec![m.root.clone()];
    while let Some(dir) = stack.pop() {
        let meta = std::fs::metadata(&dir).ok()?;
        dir.hash(&mut h);
        meta.modified().ok().hash(&mut h);
        let mut entries: Vec<_> = std::fs::read_dir(&dir).ok()?.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = e.file_name();
            let name = name.to_string_lossy();
            let path = e.path();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                if !name.starts_with('.') && !(dir == m.root && name == "target") {
                    stack.push(path);
                }
                continue;
            }
            let meta = std::fs::metadata(&path).ok()?;
            path.hash(&mut h);
            meta.len().hash(&mut h);
            meta.modified().ok().hash(&mut h);
        }
    }
    format!("{opts:?}").hash(&mut h);
    (fast || FAST.load(std::sync::atomic::Ordering::Relaxed)).hash(&mut h);
    crate::emit::incr::compiler_id().hash(&mut h);
    Some(format!("{:016x}", h.finish()))
}

/// A single-target project whose files have not changed since its output was built: parse and
/// check can be skipped. Also remembers this build's key for [`mark_inputs`]. `kind` ("build" or
/// "run") keeps one stamp per command: `run` writes its stamp only on the no-scripts path, so a
/// project with scene scripts always goes through the dev runner.
pub(super) fn unchanged_project(path: Option<&str>, kind: &str) -> Option<(Manifest, emit::EmitOpts)> {
    let Ok(crate::project::Input::Project(m)) = load_input(path) else { return None };
    if m.emit.targets.len() > 1 {
        return None;
    }
    let manifest = Some(m);
    let mut opts = emit_opts(&manifest).ok()?;
    let m = manifest?;
    if emit::host::target_os(&opts.target) == emit::host::TargetOs::Android {
        return None;
    }
    if opts.cc.out.is_none() {
        opts.cc.out = Some(PathBuf::from("target").join("a.out"));
    }
    crate::gen::before_build(&m.root);
    let fast = m.cc.fast || m.emit.fast;
    let key = inputs_key(&m, &opts, fast)?;
    let out = opts.cc.out.clone()?;
    let stamp = emit::host::cache_dir(&out, &opts.target).join(format!("inputs.{kind}.key"));
    let same = out.exists() && std::fs::read_to_string(&stamp).map(|k| k == key).unwrap_or(false);
    if let Ok(mut g) = INPUTS.lock() {
        *g = Some((stamp, key));
    }
    same.then_some((m, opts))
}

fn built_unchanged(m: Manifest, opts: &emit::EmitOpts) -> i32 {
    let out = opts.cc.out.clone().expect("out path");
    status("up to date");
    println!("{}", out.display());
    package(&Some(m), &[(opts.target.clone(), out)])
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

pub(super) static PROGRESS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// `run` and `build` turn this on so a long check or compile doesn't sit silent.
pub(super) fn progress_on() {
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

pub(super) fn build_samples_err() -> i32 {
    eprintln!("j3c: `build samples` is not a thing; use `run samples`");
    1
}

pub(super) fn build_loaded(prog: Result<(compile::Program, Option<Manifest>), String>) -> i32 {
    if let Ok((p, _)) = &prog {
        print_warnings(p);
    }
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

pub(super) fn build_inner(prog: Result<(compile::Program, Option<Manifest>), String>) -> i32 {
    match prog {
        Ok((prog, manifest)) => match emit_opts(&manifest) {
            Ok(mut opts) => {
                if let Some(m) = &manifest { crate::gen::before_build(&m.root); }
                let targets = manifest
                    .as_ref()
                    .map(|m| m.emit.targets.clone())
                    .unwrap_or_default();
                let android = targets.iter().chain(std::iter::once(&opts.target));
                for t in android.filter(|t| emit::host::target_os(t) == emit::host::TargetOs::Android) {
                    if let Err(e) = emit::android::ready(t) {
                        print_fail(&e);
                        return 1;
                    }
                }
                if targets.len() <= 1 {
                    if opts.cc.out.is_none() {
                        opts.cc.out = Some(PathBuf::from("target").join("a.out"));
                    }
                    let fast = manifest.as_ref().map(|m| m.cc.fast || m.emit.fast).unwrap_or(false);
                    let out = opts.cc.out.clone().expect("out path");
                    if up_to_date(&prog, &opts, fast) {
                        mark_inputs();
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
pub(super) fn package(manifest: &Option<Manifest>, built: &[(String, PathBuf)]) -> i32 {
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
pub(super) fn exit_reason(code: i32) -> String {
    match code {
        101 => " (panic)".into(),
        102 => " (runtime check failed)".into(),
        129..=159 => format!(" (killed by signal {})", code - 128),
        _ => String::new(),
    }
}

pub(super) fn report_run(result: emit::RunResult) -> i32 {
    print!("{}", String::from_utf8_lossy(&result.stdout));
    if result.status == 0 && !result.stderr.is_empty() {
        // a clean exit still shows what the program wrote to stderr (logs, fake-mode lines)
        eprint!("{}", String::from_utf8_lossy(&result.stderr));
    }
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

pub(super) fn link_built(prog: &compile::Program, opts: &emit::EmitOpts) -> i32 {
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
