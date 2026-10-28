//! Build phases: the two-line live status on stderr and the timing breakdown that
//! `target/compilation.log` keeps.
//!
//! Line one is the outer phase and how far through the pipeline it is (`[3/6] emit`), line two
//! is the inner progress (`module 12/271 j3.scene3d`). Both update in place on a terminal.
//! Nothing records or draws until `enable`, so the LSP and the editor pay nothing.

use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// The outer phases of a build in order. A name not listed here (`running`) has no count.
const OUTER: [&str; 6] = ["parse", "check", "emit", "compile", "link", "package"];

/// Inner updates closer together than this are dropped, so a fast loop does not flood stderr.
const REDRAW_GAP_MS: u128 = 40;

struct Rec {
    /// 0 outer phase, 1 step inside it, 2 detail of a step.
    depth: u8,
    name: String,
    secs: Option<f64>,
}

struct State {
    on: bool,
    started: Option<Instant>,
    recs: Vec<Rec>,
    /// The open outer phase: its index in `recs` and when it began.
    open: Option<(usize, Instant)>,
    line1: String,
    line2: String,
    /// Lines currently on the terminal, so the next update can move back over them.
    drawn: usize,
    tty: bool,
    last_draw: Option<Instant>,
}

fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(State {
            on: false,
            started: None,
            recs: Vec::new(),
            open: None,
            line1: String::new(),
            line2: String::new(),
            drawn: 0,
            tty: std::io::IsTerminal::is_terminal(&std::io::stderr()),
            last_draw: None,
        })
    })
}

/// Start recording and drawing. `run` and `build` call this once.
pub fn enable() {
    if let Ok(mut s) = state().lock() {
        if !s.on {
            s.on = true;
            s.started = Some(Instant::now());
        }
    }
}

/// Seconds since `enable`, or 0 when it was never called.
pub fn elapsed() -> f64 {
    state().lock().ok().and_then(|s| s.started).map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0)
}

thread_local! {
    static TAG: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// Label this thread's inner lines (the target it builds) when several build at once.
pub fn set_tag(tag: &str) {
    TAG.with(|t| *t.borrow_mut() = tag.to_string());
}

/// Begin an outer phase. Beginning the one already open does nothing.
pub fn begin(name: &str) {
    let Ok(mut s) = state().lock() else { return };
    if !s.on {
        return;
    }
    if let Some((i, _)) = s.open {
        if s.recs[i].name == name {
            return;
        }
    }
    close(&mut s);
    s.recs.push(Rec { depth: 0, name: name.to_string(), secs: None });
    let index = s.recs.len() - 1;
    s.open = Some((index, Instant::now()));
    s.line1 = match OUTER.iter().position(|p| *p == name) {
        Some(i) => format!("j3c: [{}/{}] {name}", i + 1, OUTER.len()),
        None => format!("j3c: {name}"),
    };
    s.line2.clear();
    if s.tty {
        redraw(&mut s);
    } else {
        eprintln!("{}", s.line1);
    }
}

/// Mark which target the phases after this belong to (builds with several `target`s).
pub fn target(triple: &str) {
    let Ok(mut s) = state().lock() else { return };
    if !s.on {
        return;
    }
    close(&mut s);
    s.recs.push(Rec { depth: 0, name: format!("target {triple}"), secs: None });
}

/// The inner line: what the open phase is doing right now. Not recorded.
pub fn detail(msg: &str) {
    let Ok(mut s) = state().lock() else { return };
    if !s.on {
        return;
    }
    let tagged;
    let msg = match TAG.with(|t| t.borrow().clone()) {
        t if t.is_empty() => msg,
        t => {
            tagged = format!("[{t}] {msg}");
            &tagged
        }
    };
    if !s.tty {
        // Piped output: a plain line every 2s, only in the phases that can sit for a while.
        let slow = s.open.map(|(i, _)| matches!(s.recs[i].name.as_str(), "compile" | "link")).unwrap_or(false);
        if slow && s.last_draw.map(|t| t.elapsed().as_secs() >= 2).unwrap_or(true) {
            eprintln!("      {msg}");
            s.last_draw = Some(Instant::now());
        }
        return;
    }
    s.line2 = format!("      {msg}");
    if s.last_draw.map(|t| t.elapsed().as_millis() < REDRAW_GAP_MS).unwrap_or(false) {
        return;
    }
    redraw(&mut s);
}

/// Run `cmd` to completion like `Command::output`, updating the inner line with the elapsed
/// seconds so one long clang or link call is not silent.
pub fn run_ticking(cmd: &mut std::process::Command, what: &str) -> std::io::Result<std::process::Output> {
    use std::io::Read;
    use std::process::Stdio;
    let t = Instant::now();
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    fn read<R: Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut v = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut v);
            }
            v
        })
    }
    let (h_out, h_err) = (read(child.stdout.take()), read(child.stderr.take()));
    let status = loop {
        if let Some(st) = child.try_wait()? {
            break st;
        }
        detail(&format!("{what}: {}s", t.elapsed().as_secs()));
        // Short polls: a fast link (mold, ~30 ms) should not wait out a long tick.
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    Ok(std::process::Output { status, stdout: h_out.join().unwrap_or_default(), stderr: h_err.join().unwrap_or_default() })
}

/// A finished step inside the open phase, with its time.
pub fn step(name: &str, secs: f64) {
    let Ok(mut s) = state().lock() else { return };
    if s.on && s.open.is_some() {
        s.recs.push(Rec { depth: 1, name: name.trim().to_string(), secs: Some(secs) });
    }
}

/// A line of context under the open phase (cache hits, the slowest modules), with no time.
pub fn note(text: &str) {
    let Ok(mut s) = state().lock() else { return };
    if s.on && s.open.is_some() {
        s.recs.push(Rec { depth: 1, name: text.to_string(), secs: None });
    }
}

/// Close the open phase and return the breakdown for `compilation.log`, or "" if nothing ran.
pub fn report() -> String {
    let Ok(mut s) = state().lock() else { return String::new() };
    close(&mut s);
    let mut out = String::new();
    for r in &s.recs {
        let indent = " ".repeat(6 + 2 * r.depth as usize);
        match r.secs {
            Some(t) => out += &format!("{indent}{:<w$}{t:>7.2}s\n", r.name, w = 44usize.saturating_sub(indent.len())),
            None => out += &format!("{indent}{}\n", r.name),
        }
    }
    out
}

fn close(s: &mut State) {
    if let Some((i, since)) = s.open.take() {
        s.recs[i].secs = Some(since.elapsed().as_secs_f64());
    }
}

/// Columns of stderr's terminal (Linux ioctl, no crates); 80 when unknown.
fn term_width() -> usize {
    #[cfg(target_os = "linux")]
    {
        #[repr(C)]
        struct Ws {
            rows: u16,
            cols: u16,
            x: u16,
            y: u16,
        }
        extern "C" {
            fn ioctl(fd: i32, req: u64, ...) -> i32;
        }
        let mut w = Ws { rows: 0, cols: 0, x: 0, y: 0 };
        // 0x5413 = TIOCGWINSZ
        if unsafe { ioctl(2, 0x5413, &mut w as *mut Ws) } == 0 && w.cols > 0 {
            return w.cols as usize;
        }
    }
    80
}

fn redraw(s: &mut State) {
    // A line wider than the terminal wraps, and then moving up by line count lands too low.
    let cols = term_width().saturating_sub(1).max(10);
    let clipped: Vec<String> = [&s.line1, &s.line2].into_iter().filter(|l| !l.is_empty()).map(|l| l.chars().take(cols).collect()).collect();
    let visible: Vec<&str> = clipped.iter().map(|l| l.as_str()).collect();
    let mut err = std::io::stderr().lock();
    if s.drawn > 0 {
        let _ = write!(err, "\x1b[{}A", s.drawn);
    }
    let rows = s.drawn.max(visible.len());
    for i in 0..rows {
        match visible.get(i) {
            Some(line) => {
                let _ = writeln!(err, "\x1b[2K{line}");
            }
            None => {
                let _ = writeln!(err, "\x1b[2K");
            }
        }
    }
    if rows > visible.len() {
        let _ = write!(err, "\x1b[{}A\x1b[J", rows - visible.len());
    }
    let _ = err.flush();
    s.drawn = visible.len();
    s.last_draw = Some(Instant::now());
}

#[cfg(test)]
mod tests {
    #[test]
    fn report_nests_steps_under_their_phase() {
        super::enable();
        super::begin("check");
        super::step("  collect", 0.5);
        super::begin("emit");
        let text = super::report();
        // The recorder is process-global and `enable` turns it on for parallel compile tests too,
        // so find our lines instead of assuming they are the first three.
        let at = |w: &str| text.lines().find(|l| l.trim_start().starts_with(&format!("{w} "))).unwrap_or_else(|| panic!("{w}: {text}"));
        assert!(at("collect").starts_with("        ") && !at("check").starts_with("        "), "{text}");
        at("emit");
    }
}
