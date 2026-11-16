//! Package dist outputs (`[dist] build_outputs` names a dep's `[dist_outputs]` tool): after the built-ins,
//! each runs once via `pkgtool` with
//! `--dist <abs target/dist> --built <triple>=<abs exe>... --stage <triple>=<abs folder>... --name N --version V [--release]`
//! (an empty triple is the host; Android triples have no `--stage`). Its stdout is echoed live; the last
//! non-empty line, when it names an existing folder (relative: to the project root), joins the build's
//! "wrote" list. A non-zero exit fails the build with that code. A built-in name (`zip`, ...) is never a
//! package output; a dep providing one gets a warning here.

use crate::project::{dist_provider, Manifest, Output};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;

pub(super) fn run_all(
    m: &Manifest,
    built: &[(String, PathBuf)],
    stages: &[(String, PathBuf)],
    dist: &Path,
    out: &mut Vec<PathBuf>,
) -> Result<(), (String, i32)> {
    // A built-in wins over a dep's output of the same name.
    for (n, b) in &Output::ALL {
        if let Some((d, _)) = m.dist.outputs.contains(b).then(|| dist_provider(&m.deps, n)).flatten() {
            eprintln!("j3c: warning: dist.build_outputs: `{n}` is built in; package `{}`'s [dist_outputs] {n} does not run", d.name);
        }
    }
    let names = m.dist.outputs.iter().filter_map(|o| if let Output::Pkg(n) = o { Some(n) } else { None });
    for name in names {
        // `Manifest` load checked every name (`project::check_outputs`).
        let (dep, tool) = dist_provider(&m.deps, name).ok_or_else(|| (format!("dist output `{name}`: no package provides it"), 1))?;
        let mut args = vec!["--dist".to_string(), dist.display().to_string()];
        for (t, exe) in built {
            let exe = std::path::absolute(exe).map_err(|e| (format!("{}: {e}", exe.display()), 1))?;
            args.extend(["--built".to_string(), format!("{t}={}", exe.display())]);
        }
        for (t, dir) in stages {
            args.extend(["--stage".to_string(), format!("{t}={}", dir.display())]);
        }
        args.extend(["--name".to_string(), m.name.clone(), "--version".to_string(), m.version.clone()]);
        if crate::cli::RELEASE.load(std::sync::atomic::Ordering::Relaxed) {
            args.push("--release".into());
        }
        let label = format!("dist output `{name}` (package `{}`, {})", dep.name, tool.file.display());
        let mut child = crate::pkgtool::command(m, dep, tool, &args, &[])
            .map_err(|e| (e, 1))?
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| (format!("{label}: {e}"), 1))?;
        let mut last = String::new();
        let stdout = child.stdout.take().expect("piped stdout");
        // bytes, not `lines()`: non-UTF-8 output is shown lossily, and a read error still waits for the child
        let (mut rd, mut buf) = (BufReader::new(stdout), Vec::new());
        let mut read_err = None;
        loop {
            buf.clear();
            match rd.read_until(b'\n', &mut buf) {
                Ok(0) => break,
                Ok(_) => {}
                Err(e) => {
                    read_err = Some(e);
                    break;
                }
            }
            let line = String::from_utf8_lossy(&buf);
            let line = line.trim_end_matches(['\n', '\r']).to_string();
            println!("{line}");
            let _ = std::io::stdout().flush();
            if !line.trim().is_empty() {
                last = line;
            }
        }
        if read_err.is_some() {
            let _ = child.kill();
        }
        let status = child.wait().map_err(|e| (format!("{label}: {e}"), 1))?;
        if let Some(e) = read_err {
            return Err((format!("{label}: read stdout: {e}"), 1));
        }
        let code = crate::pkgtool::exit_code(status, dep, tool);
        if code != 0 {
            return Err((format!("{label} failed with exit code {code}"), code));
        }
        if !last.is_empty() {
            let folder = m.root.join(last.trim());
            if folder.is_dir() {
                out.push(folder);
            } else {
                eprintln!("j3c: warning: {label}: last stdout line is not a folder, not listed: {}", last.trim());
            }
        }
    }
    Ok(())
}
