//! `j3c run --hot`: rebuild and restart the program when `.j3`, `.j3ui` markup/script, `.j3s`
//! or `j3.toml` change. `<style>` blocks and theme.css are not watched here: the running
//! program reloads those itself (`j3.ui.domhot`).
//!
//! The old process keeps running while the new build is made in this process. A failed build
//! prints its errors and changes nothing. A good one restarts the child: j3c creates the
//! `J3_HOT_QUIT` file, the program (`j3.app`) sees it within a few frames, writes the window
//! (`J3_HOT_WIN`) and, if the app opted in with `App.hot`, its state (`J3_HOT_STATE`), and
//! exits; the new process reads both back. Nothing of this runs without those variables.
//!
//! Watched: the project folder (not `target/`, `.git`, hidden or `node_modules` folders) and,
//! when `J3_STDLIB` is set, that folder; the embedded stdlib cannot change under a running `j3c`.
//!
//! ponytail: a build in flight is not cancelled by a newer save (the next poll just builds
//! again); polling every 100 ms instead of inotify (no deps); no in-window error banner.

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::{Duration, Instant, SystemTime};

/// Quiet time after the last change before a rebuild starts.
pub const DEBOUNCE: Duration = Duration::from_millis(150);
/// How long the old program gets to save and exit before it is killed.
const GRACE: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(100);

/// The three files of the restart handshake, under `target/.j3hot`.
pub struct HotPaths {
    pub quit: PathBuf,
    pub win: PathBuf,
    pub state: PathBuf,
}

impl HotPaths {
    pub fn new(root: &Path) -> HotPaths {
        let dir = root.join("target/.j3hot");
        let _ = std::fs::create_dir_all(&dir);
        HotPaths { quit: dir.join("quit"), win: dir.join("window"), state: dir.join("state") }
    }

    /// The environment a program is started with.
    pub fn envs(&self) -> [(&'static str, &Path); 3] {
        [("J3_HOT_QUIT", &self.quit), ("J3_HOT_WIN", &self.win), ("J3_HOT_STATE", &self.state)]
    }

    /// Forget a previous session (a fresh `j3c run --hot` starts fresh).
    pub fn clear(&self) {
        for p in [&self.quit, &self.win, &self.state] {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// Source text with the top-level `<style>...</style>` blocks removed: a style edit must not
/// restart. `<script>` bodies are skipped over (a `"<style>"` string there is code), and a tag
/// only counts when `<style` is followed by `>` or whitespace (`<styles>` is a component).
fn without_styles(src: &str) -> String {
    let mut out = String::new();
    let mut rest = src;
    loop {
        let style = rest.match_indices("<style").find(|(i, _)| {
            matches!(rest[i + 6..].chars().next(), Some(c) if c == '>' || c.is_whitespace())
        });
        let script = rest.find("<script");
        match (style, script) {
            (Some((a, _)), s) if s.map_or(true, |s| a < s) => {
                out.push_str(&rest[..a]);
                match rest[a..].find("</style>") {
                    Some(b) => rest = &rest[a + b + "</style>".len()..],
                    None => return out,
                }
            }
            (_, Some(sc)) => {
                // keep the whole script block
                let end = rest[sc..].find("</script>").map_or(rest.len(), |e| sc + e + "</script>".len());
                out.push_str(&rest[..end]);
                rest = &rest[end..];
            }
            _ => {
                out.push_str(rest);
                return out;
            }
        }
    }
}

fn watched(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some("j3" | "j3ui" | "j3s") => true,
        _ => path.file_name().map(|n| n == "j3.toml").unwrap_or(false),
    }
}

/// Content hashes of the watched files. A file is re-read only when its size or mtime moved,
/// and compared by content, so a generated file rewritten with the same text is no change.
#[derive(Default)]
pub struct Scanner {
    cache: BTreeMap<PathBuf, (u64, Option<SystemTime>, u64)>,
    /// Files read and hashed so far (the mtime/size gate keeps this at the number of changes).
    pub rehashed: usize,
}

impl Scanner {
    pub fn scan(&mut self, roots: &[PathBuf]) -> BTreeMap<PathBuf, u64> {
        let mut files = Vec::new();
        for r in roots {
            walk(r, &mut files);
        }
        let mut out = BTreeMap::new();
        for f in files {
            let Ok(meta) = std::fs::metadata(&f) else { continue };
            let stamp = (meta.len(), meta.modified().ok());
            let hash = match self.cache.get(&f) {
                Some(c) if (c.0, c.1) == stamp => c.2,
                _ => {
                    self.rehashed += 1;
                    let text = std::fs::read(&f).unwrap_or_default();
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    if f.extension().map(|e| e == "j3ui").unwrap_or(false) {
                        without_styles(&String::from_utf8_lossy(&text)).hash(&mut h);
                    } else {
                        text.hash(&mut h);
                    }
                    let v = h.finish();
                    self.cache.insert(f.clone(), (stamp.0, stamp.1, v));
                    v
                }
            };
            out.insert(f, hash);
        }
        out
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name();
        let name = name.to_string_lossy();
        if p.is_dir() {
            if name != "target" && !name.starts_with('.') && name != "node_modules" {
                walk(&p, out);
            }
        } else if watched(&p) {
            out.push(p);
        }
    }
}

/// Turns a stream of scans into "rebuild now" moments: waits until the files have been still
/// for [`DEBOUNCE`] (coalescing a burst of saves into one build), and compares with the scan
/// the last build started from, so an edit made during a build triggers the next one.
pub struct Debounce {
    built: BTreeMap<PathBuf, u64>,
    cur: BTreeMap<PathBuf, u64>,
    since: Instant,
    quiet: Duration,
}

impl Debounce {
    pub fn new(first: BTreeMap<PathBuf, u64>, quiet: Duration, now: Instant) -> Debounce {
        Debounce { built: first.clone(), cur: first, since: now, quiet }
    }

    /// Note the latest scan without deciding anything.
    pub fn observe(&mut self, scan: BTreeMap<PathBuf, u64>, now: Instant) {
        if scan != self.cur {
            self.cur = scan;
            self.since = now;
        }
    }

    /// No change is waiting: the latest scan is the one the last build started from.
    pub fn stable(&self) -> bool {
        self.cur == self.built
    }

    /// True once when a change has settled; the build then starts from this scan.
    pub fn poll(&mut self, scan: BTreeMap<PathBuf, u64>, now: Instant) -> bool {
        self.observe(scan, now);
        if self.cur != self.built && now.duration_since(self.since) >= self.quiet {
            self.built = self.cur.clone();
            return true;
        }
        false
    }
}

/// What the supervisor needs from the outside, so tests can stand in for clang and the game.
pub struct Hooks<'a> {
    /// Rebuild; the new executable, or the error text to show.
    pub build: &'a mut dyn FnMut() -> Result<PathBuf, String>,
    /// Start the program from an executable with the handshake environment.
    pub spawn: &'a mut dyn FnMut(&Path, &HotPaths) -> Result<Child, String>,
    /// Show a build error.
    pub report: &'a mut dyn FnMut(&str),
    /// Set to leave the loop (signals); the child is killed.
    pub stop: &'a std::sync::atomic::AtomicBool,
}

/// Ask the child to save and exit; kill it after `grace`.
fn stop(child: &mut Child, paths: &HotPaths, grace: Duration) {
    let _ = std::fs::remove_file(&paths.state);
    let _ = std::fs::write(&paths.quit, "");
    let end = Instant::now() + grace;
    while Instant::now() < end {
        if matches!(child.try_wait(), Ok(Some(_))) {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    if matches!(child.try_wait(), Ok(None)) {
        eprintln!("j3c: the program did not exit in time; killed it (no state kept). A program with its own main loop: add app.hotPoll() to the loop to save state");
        let _ = child.kill();
    }
    let _ = child.wait();
    let _ = std::fs::remove_file(&paths.quit);
}

/// Set by SIGINT/SIGTERM (`run` installs the handlers): stop the child and return.
pub static STOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn panic_text(p: Box<dyn std::any::Any + Send>) -> String {
    let what = p
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default();
    format!("the compiler crashed while building: {what}")
}

/// Run until the program exits by itself (window closed, `--frames` done, crash): its code.
/// `first` is the initial executable, or the initial build error (then it waits for an edit).
/// A build that finishes while newer edits are waiting is not started: those are built first,
/// so a restart always runs the newest sources.
pub fn supervise(
    roots: &[PathBuf],
    paths: &HotPaths,
    first: Result<PathBuf, String>,
    hooks: &mut Hooks,
    quiet: Duration,
    grace: Duration,
) -> Result<i32, String> {
    paths.clear();
    let mut scanner = Scanner::default();
    let mut deb = Debounce::new(scanner.scan(roots), quiet, Instant::now());
    let mut child: Option<Child> = None;
    let mut ready: Option<PathBuf> = None;
    match first {
        Ok(exe) => child = Some((hooks.spawn)(&exe, paths)?),
        Err(e) => {
            (hooks.report)(&e);
            eprintln!("j3c: waiting for a fix");
        }
    }
    loop {
        if hooks.stop.load(std::sync::atomic::Ordering::Relaxed) {
            if let Some(mut c) = child.take() {
                let _ = c.kill();
                let _ = c.wait();
            }
            return Ok(130);
        }
        if let Some(c) = child.as_mut() {
            if let Some(status) = c.try_wait().map_err(|e| format!("wait: {e}"))? {
                return Ok(crate::emit::host::exit_code(&status));
            }
        }
        std::thread::sleep(POLL);
        if deb.poll(scanner.scan(roots), Instant::now()) {
            eprintln!("j3c: change detected, rebuilding");
            // a panic in the compiler must not end the watcher
            let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (hooks.build)()))
                .unwrap_or_else(|p| Err(panic_text(p)));
            match built {
                Err(e) => {
                    ready = None;
                    (hooks.report)(&e);
                    eprintln!("j3c: build failed; {}", if child.is_some() { "kept the running program" } else { "waiting for a fix" });
                }
                Ok(exe) => ready = Some(exe),
            }
        }
        deb.observe(scanner.scan(roots), Instant::now());
        // restart only when nothing newer than this build is waiting (the poll above has seen
        // any save made during the build)
        if deb.stable() {
            if let Some(exe) = ready.take() {
                if let Some(mut old) = child.take() {
                    stop(&mut old, paths, grace);
                }
                eprintln!("j3c: restarting");
                child = Some((hooks.spawn)(&exe, paths)?);
            }
        }
    }
}

#[cfg(unix)]
extern "C" {
    fn signal(sig: i32, handler: usize) -> usize;
    fn _exit(code: i32) -> !;
}

#[cfg(unix)]
extern "C" fn on_signal(_: i32) {
    // a second signal while the first is being handled (e.g. mid-build) leaves at once
    if STOP.swap(true, std::sync::atomic::Ordering::Relaxed) {
        unsafe { _exit(130) }
    }
}

/// SIGINT and SIGTERM stop the child instead of orphaning it.
#[cfg(unix)]
fn trap_signals() {
    unsafe {
        signal(2, on_signal as *const () as usize);
        signal(15, on_signal as *const () as usize);
    }
}
#[cfg(not(unix))]
fn trap_signals() {}

/// The real thing for a project: builds with the normal pipeline (incremental when enabled),
/// runs a private copy of the executable so the next build can replace the original.
pub fn run(
    m: &crate::project::Manifest,
    opts: &crate::emit::EmitOpts,
    fast: bool,
    first: Result<crate::check::Program, String>,
    args: &[String],
) -> i32 {
    let paths = HotPaths::new(&m.root);
    let finish = |prog: crate::check::Program| -> Result<PathBuf, String> {
        crate::preflight::check(&prog, Some(m))?;
        crate::emit::compile_program(&prog.cx, &prog.modules, opts)
    };
    let mut n = 0;
    let mut build = || -> Result<PathBuf, String> {
        crate::gen::before_build(&m.root);
        let mut prog = crate::compile::compile_manifest_dev(m)?;
        if fast {
            crate::compile::apply_fast(&mut prog);
        }
        if crate::compile::dev::has_scripts(&prog) {
            return Err("the program has `.j3s` scripts now; rerun `j3c run` (scripts have their own watcher)".into());
        }
        finish(prog)
    };
    let first = first.and_then(&finish);
    let mut spawn = |exe: &Path, p: &HotPaths| -> Result<Child, String> {
        // ponytail: old copies pile up in target/.j3hot until the next `j3c run --hot` clears nothing; a few MB each
        n += 1;
        let copy = p.quit.with_file_name(format!("run-{}{}", n % 2, exe.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default()));
        let _ = std::fs::remove_file(&copy);
        std::fs::copy(exe, &copy).map_err(|e| format!("copy {}: {e}", exe.display()))?;
        crate::devrun::launch(&copy, &m.root, args, &p.envs())
    };
    let mut report = |e: &str| crate::cli::print_error(e);
    crate::cli::status("running");
    eprintln!("j3c: hot mode: rebuilding on .j3 / .j3ui / .j3s edits");
    let mut hooks = Hooks { build: &mut build, spawn: &mut spawn, report: &mut report, stop: &STOP };
    trap_signals();
    let mut roots = vec![m.root.clone()];
    if let Some(dir) = std::env::var_os("J3_STDLIB") {
        roots.push(PathBuf::from(dir));
    }
    match supervise(&roots, &paths, first, &mut hooks, DEBOUNCE, GRACE) {
        Ok(code) => {
            crate::devrun::report_crash();
            if code != 0 {
                eprintln!("j3c: program exited {code}");
            }
            code
        }
        Err(e) => {
            crate::cli::print_error(&e);
            1
        }
    }
}

#[cfg(all(test, unix))]
mod tests;
