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
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("j3hot-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn map(items: &[(&str, u64)]) -> BTreeMap<PathBuf, u64> {
        items.iter().map(|(p, h)| (PathBuf::from(p), *h)).collect()
    }

    #[test]
    fn debounce_coalesces_a_burst_into_one_build() {
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        let mut d = Debounce::new(map(&[("a", 1)]), ms(150), t0);
        assert!(!d.poll(map(&[("a", 1)]), t0 + ms(500)), "no change, no build");
        assert!(!d.poll(map(&[("a", 2)]), t0 + ms(600)), "changed just now");
        assert!(!d.poll(map(&[("a", 3), ("b", 1)]), t0 + ms(700)), "still changing: timer restarts");
        assert!(!d.poll(map(&[("a", 3), ("b", 1)]), t0 + ms(800)));
        assert!(d.poll(map(&[("a", 3), ("b", 1)]), t0 + ms(850)), "quiet for 150 ms");
        assert!(!d.poll(map(&[("a", 3), ("b", 1)]), t0 + ms(2000)), "one build per burst");
        // an edit that lands while a build runs is seen by the next poll
        assert!(!d.poll(map(&[("a", 4), ("b", 1)]), t0 + ms(2100)));
        assert!(d.poll(map(&[("a", 4), ("b", 1)]), t0 + ms(2300)));
    }

    #[test]
    fn style_edits_do_not_count_but_markup_does() {
        let root = tmp("scan");
        let f = root.join("ui.j3ui");
        std::fs::write(&f, "<p>hi</p><style>p { color: red }</style><script>let a = 1;</script>").unwrap();
        std::fs::write(root.join("theme.css"), "a{}").unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(root.join("target/x.j3"), "").unwrap();
        let mut s = Scanner::default();
        let a = s.scan(&[root.clone()]);
        assert_eq!(a.len(), 1, "only ui.j3ui: no css, no target/");
        std::fs::write(&f, "<p>hi</p><style>p { color: blue }</style><script>let a = 1;</script>").unwrap();
        assert_eq!(s.scan(&[root.clone()]), a);
        std::fs::write(&f, "<p>hi!</p><style>p { color: blue }</style><script>let a = 1;</script>").unwrap();
        assert_ne!(s.scan(&[root.clone()]), a);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A stand-in program: waits for the quit file, then records its state env and exits;
    /// `done` makes it exit on its own.
    fn fake(root: &Path, paths: &HotPaths, tag: &str) -> Result<Child, String> {
        let script = format!(
            "d={d}; echo $J3_HOT_STATE > $d/{tag}.state; while [ ! -e \"$J3_HOT_QUIT\" ] && [ ! -e $d/done ]; do sleep 0.02; done; \
             if [ -e \"$J3_HOT_QUIT\" ]; then echo saved-{tag} > \"$J3_HOT_STATE\"; exit 0; fi; exit 7",
            d = root.display()
        );
        let mut c = Command::new("sh");
        c.arg("-c").arg(script).stderr(Stdio::null());
        for (k, v) in paths.envs() {
            c.env(k, v);
        }
        c.spawn().map_err(|e| e.to_string())
    }

    #[test]
    fn failed_build_keeps_the_old_process_and_a_good_one_restarts_with_the_state_path() {
        let root = tmp("sup");
        std::fs::write(root.join("a.j3"), "1").unwrap();
        let paths = HotPaths::new(&root);
        let builds = std::cell::RefCell::new(Vec::<&str>::new());
        let spawns = std::cell::RefCell::new(0);
        let reports = std::cell::RefCell::new(Vec::<String>::new());
        let r2 = root.clone();
        let r3 = root.clone();
        // the editor: bad edit, then a good one, then the program is told to finish
        let editor = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            std::fs::write(r2.join("a.j3"), "bad").unwrap();
            std::thread::sleep(Duration::from_millis(900));
            std::fs::write(r2.join("a.j3"), "good").unwrap();
            std::thread::sleep(Duration::from_millis(1200));
            std::fs::write(r2.join("done"), "").unwrap();
        });
        let mut build = || -> Result<PathBuf, String> {
            let n = builds.borrow().len();
            if n == 0 {
                builds.borrow_mut().push("fail");
                Err("a.j3:1: boom".into())
            } else {
                builds.borrow_mut().push("ok");
                Ok(PathBuf::from("exe"))
            }
        };
        let mut spawn = |_: &Path, p: &HotPaths| {
            *spawns.borrow_mut() += 1;
            let tag = if *spawns.borrow() == 1 { "one" } else { "two" };
            fake(&r3, p, tag)
        };
        let mut report = |e: &str| reports.borrow_mut().push(e.to_string());
        let no_stop = std::sync::atomic::AtomicBool::new(false);
        let mut hooks = Hooks { build: &mut build, spawn: &mut spawn, report: &mut report, stop: &no_stop };
        let code = supervise(&[root.clone()], &paths, Ok(PathBuf::from("exe")), &mut hooks, Duration::from_millis(100), Duration::from_secs(5)).unwrap();
        editor.join().unwrap();
        assert_eq!(code, 7, "the second program ended on its own");
        assert_eq!(*builds.borrow(), ["fail", "ok"]);
        assert_eq!(*reports.borrow(), ["a.j3:1: boom"]);
        assert_eq!(*spawns.borrow(), 2, "failed build spawned nothing");
        // both got the same handshake paths; the first saved state on its way out
        let want = paths.state.to_string_lossy().to_string();
        assert_eq!(std::fs::read_to_string(root.join("one.state")).unwrap().trim(), want);
        assert_eq!(std::fs::read_to_string(root.join("two.state")).unwrap().trim(), want);
        assert!(!paths.quit.exists(), "quit file removed after the restart");
        assert_eq!(std::fs::read_to_string(&paths.state).unwrap().trim(), "saved-one");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn only_real_top_level_style_blocks_are_stripped() {
        let a = without_styles("<p/><style>x</style><b/>");
        assert_eq!(a, "<p/><b/>");
        assert_eq!(without_styles("<styles>keep</styles>"), "<styles>keep</styles>");
        assert_eq!(without_styles("<style media=\"x\">a</style>z"), "z");
        let code = "<script>let s = \"<style>not css</style>\";</script><style>c</style>";
        assert_eq!(without_styles(code), "<script>let s = \"<style>not css</style>\";</script>");
        assert_eq!(without_styles("<p/><style>unclosed"), "<p/>");
    }

    #[test]
    fn unchanged_files_are_not_reread() {
        let root = tmp("gate");
        for i in 0..20 {
            std::fs::write(root.join(format!("f{i}.j3")), format!("{i}")).unwrap();
        }
        let mut s = Scanner::default();
        let a = s.scan(&[root.clone()]);
        assert_eq!(s.rehashed, 20);
        for _ in 0..10 {
            assert_eq!(s.scan(&[root.clone()]), a);
        }
        assert_eq!(s.rehashed, 20, "ten more polls read nothing");
        std::fs::write(root.join("f3.j3"), "changed!").unwrap(); // size differs
        assert_ne!(s.scan(&[root.clone()]), a);
        assert_eq!(s.rehashed, 21, "only the changed file");
        let _ = std::fs::remove_dir_all(&root);
    }

    fn run_sup(root: &Path, builds: &mut dyn FnMut(usize) -> Result<PathBuf, String>, edit: impl FnOnce(PathBuf) + Send + 'static) -> (Vec<String>, Vec<PathBuf>, i32) {
        let paths = HotPaths::new(root);
        let reports = std::cell::RefCell::new(Vec::<String>::new());
        let spawned = std::cell::RefCell::new(Vec::<PathBuf>::new());
        let n = std::cell::Cell::new(0usize);
        let r2 = root.to_path_buf();
        let editor = std::thread::spawn(move || edit(r2));
        let mut build = || {
            n.set(n.get() + 1);
            builds(n.get())
        };
        let mut spawn = |exe: &Path, p: &HotPaths| {
            spawned.borrow_mut().push(exe.to_path_buf());
            fake(root, p, "x")
        };
        let mut report = |e: &str| reports.borrow_mut().push(e.to_string());
        let no_stop = std::sync::atomic::AtomicBool::new(false);
        let mut hooks = Hooks { build: &mut build, spawn: &mut spawn, report: &mut report, stop: &no_stop };
        let code = supervise(&[root.to_path_buf()], &paths, Ok(PathBuf::from("exe0")), &mut hooks, Duration::from_millis(100), Duration::from_secs(5)).unwrap();
        editor.join().unwrap();
        (reports.into_inner(), spawned.into_inner(), code)
    }

    #[test]
    fn a_save_during_a_build_is_built_before_the_restart() {
        let root = tmp("during");
        std::fs::write(root.join("a.j3"), "1").unwrap();
        let mut builds = |n: usize| {
            if n == 1 {
                // the user saves again while build 1 runs
                std::fs::write(root.join("a.j3"), "2 newer").unwrap();
                std::thread::sleep(Duration::from_millis(300));
            }
            Ok(PathBuf::from(format!("exe{n}")))
        };
        let (_, spawned, _) = run_sup(&root.clone(), &mut builds, {
            let root = root.clone();
            move |_| {
                std::thread::sleep(Duration::from_millis(400));
                std::fs::write(root.join("a.j3"), "1 edited").unwrap();
                std::thread::sleep(Duration::from_millis(3000));
                std::fs::write(root.join("done"), "").unwrap();
            }
        });
        // exe0 at start, then only the build made from the newest sources (exe2)
        assert_eq!(spawned, [PathBuf::from("exe0"), PathBuf::from("exe2")]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_panicking_build_is_reported_and_the_watcher_lives() {
        let root = tmp("panic");
        std::fs::write(root.join("a.j3"), "1").unwrap();
        let mut builds = |n: usize| -> Result<PathBuf, String> {
            if n == 1 {
                panic!("boom in check");
            }
            Ok(PathBuf::from("exe2"))
        };
        let (reports, spawned, _) = run_sup(&root.clone(), &mut builds, {
            let root = root.clone();
            move |_| {
                std::thread::sleep(Duration::from_millis(300));
                std::fs::write(root.join("a.j3"), "bad").unwrap();
                std::thread::sleep(Duration::from_millis(1000));
                std::fs::write(root.join("a.j3"), "good").unwrap();
                std::thread::sleep(Duration::from_millis(1500));
                std::fs::write(root.join("done"), "").unwrap();
            }
        });
        assert_eq!(reports.len(), 1);
        assert!(reports[0].contains("crashed") && reports[0].contains("boom in check"), "{reports:?}");
        assert_eq!(spawned, [PathBuf::from("exe0"), PathBuf::from("exe2")]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn stop_flag_kills_the_child() {
        let root = tmp("stop");
        let paths = HotPaths::new(&root);
        let stop = std::sync::atomic::AtomicBool::new(false);
        let pid = std::cell::Cell::new(0u32);
        let mut build = || Err("never".to_string());
        let mut spawn = |_: &Path, _: &HotPaths| {
            let c = Command::new("sleep").arg("30").spawn().map_err(|e| e.to_string())?;
            pid.set(c.id());
            Ok(c)
        };
        let mut report = |_: &str| {};
        let mut hooks = Hooks { build: &mut build, spawn: &mut spawn, report: &mut report, stop: &stop };
        std::thread::scope(|sc| {
            sc.spawn(|| {
                std::thread::sleep(Duration::from_millis(400));
                stop.store(true, std::sync::atomic::Ordering::Relaxed);
            });
            let code = supervise(&[root.clone()], &paths, Ok(PathBuf::from("x")), &mut hooks, Duration::from_millis(100), Duration::from_secs(1)).unwrap();
            assert_eq!(code, 130);
        });
        assert!(!Path::new(&format!("/proc/{}", pid.get())).exists(), "child reaped and gone");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_program_that_ignores_quit_is_killed() {
        let root = tmp("kill");
        let paths = HotPaths::new(&root);
        let mut c = Command::new("sleep").arg("30").spawn().unwrap();
        let t = Instant::now();
        stop(&mut c, &paths, Duration::from_millis(200));
        assert!(t.elapsed() < Duration::from_secs(5));
        assert!(c.try_wait().unwrap().is_some());
        let _ = std::fs::remove_dir_all(&root);
    }
}
