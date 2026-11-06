//! Hot reload tests: debounce, scanner, supervisor.

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
