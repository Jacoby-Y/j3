//! Crash and abort reports: source context for failed checks, addresses rewritten to `function at file:line`.

use super::*;

/// A failed runtime check's `/abs/path/file.j3:85: reason` line, with the path made relative
/// to the project root and the source line (plus one above and below) under it.
/// `None` when the line is not one, or the file cannot be read (stdlib, moved file).
pub(crate) fn abort_context(line: &str, root: &Path) -> Option<String> {
    let body = line.strip_prefix("runtime check failed: ").unwrap_or(line);
    let (head, reason) = body.split_once(": ")?;
    let (file, num) = head.rsplit_once(':')?;
    let num: usize = num.parse().ok()?;
    if !(file.ends_with(".j3") || file.ends_with(".j3s")) || num == 0 {
        return None;
    }
    let path = Path::new(file);
    let text = std::fs::read_to_string(if path.is_absolute() { path.to_path_buf() } else { root.join(path) }).ok()?;
    let shown = path.strip_prefix(root).unwrap_or(path).display();
    let lines: Vec<&str> = text.lines().collect();
    if num > lines.len() {
        return None;
    }
    let width = (num + 1).to_string().len();
    let mut out = format!("{shown}:{num}: {reason}");
    for n in num.saturating_sub(1).max(1)..=(num + 1).min(lines.len()) {
        let mark = if n == num { '>' } else { ' ' };
        out.push_str(&format!("\n{mark} {n:>width$} | {}", lines[n - 1]));
    }
    Some(out)
}

fn signal_name(n: i32) -> &'static str {
    match n {
        4 => "SIGILL",
        6 => "SIGABRT",
        7 => "SIGBUS",
        8 => "SIGFPE",
        11 => "SIGSEGV",
        _ => "signal",
    }
}

fn hex(s: &str) -> Option<u64> {
    u64::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok()
}

/// Rewrite the crash handler's `j3 crash:` / `j3 pc:` / `j3 at:` lines (raw addresses) into
/// `function at file:line` using `nm` and `addr2line` when installed. A failed check's walk
/// (`j3 base:` + `j3 at:` with no `j3 crash:`) becomes a `called from` list. `None` when `err`
/// has neither, so ordinary stderr passes through untouched.
pub(crate) fn symbolize(exe: &Path, err: &str) -> Option<String> {
    let mut sig = None;
    let mut base = None;
    let mut addrs: Vec<u64> = Vec::new();
    let mut rest = String::new();
    for line in err.lines() {
        if let Some(v) = line.strip_prefix("j3 crash: signal ") {
            sig = v.trim().parse::<i32>().ok();
        } else if let Some(v) = line.strip_prefix("j3 base: ") {
            base = hex(v);
        } else if let Some(v) = line.strip_prefix("j3 pc: ").or_else(|| line.strip_prefix("j3 at: ")) {
            addrs.extend(hex(v));
        } else {
            rest.push_str(line);
            rest.push('\n');
        }
    }
    if sig.is_none() && addrs.is_empty() {
        return None;
    }
    let mut out = rest;
    let names = base.and_then(|b| addr_names(exe, b, &addrs, sig.is_some()));
    let shown: Vec<String> = addrs
        .iter()
        .enumerate()
        .map(|(i, a)| match &names {
            Some(n) => n[i].clone(),
            None => format!("{a:#x}"),
        })
        .collect();
    // folded runs of one frame: (text, count); stop at the runtime's own entry frames
    let mut runs: Vec<(String, usize)> = Vec::new();
    for t in shown {
        if t.starts_with("j3.sys.start ") || t.starts_with("main at ") || t.starts_with("?? ") {
            break;
        }
        match runs.last_mut() {
            Some((p, n)) if *p == t => *n += 1,
            _ => runs.push((t, 1)),
        }
    }
    match sig {
        Some(sig) => {
            // ponytail: heuristic (the walk stops at 32 frames); the exact test is the fault
            // address against the stack guard page (siginfo si_addr)
            let deep = runs.iter().find(|(_, n)| *n >= 16);
            match deep {
                Some((f, _)) if sig == 11 => {
                    let name = f.split(" at ").next().unwrap_or(f);
                    out.push_str(&format!("crash: stack overflow (SIGSEGV): endless recursion in `{name}`?\n"));
                }
                _ => out.push_str(&format!("crash: {} ({sig})\n", signal_name(sig))),
            }
        }
        None => out.push_str("called from:\n"),
    }
    for (t, n) in runs {
        out.push_str(&format!("  {t}"));
        if n > 1 {
            out.push_str(&format!("  (x{n})"));
        }
        out.push('\n');
    }
    Some(out)
}

/// `function at file:line` for each runtime address; needs `nm` (to find the load offset from
/// `main`) and `addr2line`. Return addresses (all but a crash's first, the faulting pc) step back a byte.
fn addr_names(exe: &Path, runtime_main: u64, addrs: &[u64], first_is_pc: bool) -> Option<Vec<String>> {
    let nm = Command::new("nm").arg(exe).output().ok()?;
    let nm = String::from_utf8_lossy(&nm.stdout).into_owned();
    let static_main = nm.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let (addr, kind, name) = (it.next()?, it.next()?, it.next()?);
        (name == "main" && kind.eq_ignore_ascii_case("t")).then(|| hex(addr)).flatten()
    })?;
    let offset = runtime_main.wrapping_sub(static_main);
    let mut cmd = Command::new("addr2line");
    cmd.args(["-f", "-C", "-p", "-e"]).arg(exe);
    for (i, a) in addrs.iter().enumerate() {
        let a = a.wrapping_sub(offset).wrapping_sub(u64::from(i > 0 || !first_is_pc));
        cmd.arg(format!("{a:#x}"));
    }
    let out = cmd.output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let names: Vec<String> = text.lines().map(|l| l.to_string()).collect();
    (names.len() == addrs.len()).then_some(names)
}
