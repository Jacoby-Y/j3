//! `j3c golden <dir> [name...] [--bless]`: headless golden test of a project, states from `<dir>/golden.toml`.
//!
//! ```toml
//! [defaults]                  # for every state: frame (30), tol, max, frames, timeout (seconds)
//! frame = 30
//! [state.browser]             # golden/browser.png; capture lands in target/golden/browser.png
//! text = "target/ui-a11y.txt" # optional: also compare this file with golden/browser.txt
//! [state.browser.env]
//! DESK_GOLDEN = "browser"
//! [check.steady]              # a run with no capture: rc 0 and no line starting with FAIL
//! frames = 200
//! [check.steady.env]
//! J3_STEADY = "60:120"
//! ```
//! Each run is `j3c run <dir> --headless --frames N` with a fixed step, in a child process.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub struct Tbl {
    /// "defaults", "state" or "check"
    pub kind: String,
    pub name: String,
    pub kv: Vec<(String, String)>,
    pub env: Vec<(String, String)>,
}

impl Tbl {
    fn get(&self, k: &str) -> Option<&str> {
        self.kv.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str())
    }
}

const KEYS: [&str; 6] = ["frame", "tol", "max", "frames", "timeout", "text"];

/// `[defaults]`, `[state.NAME]`, `[check.NAME]`, each with an optional `[state.NAME.env]`; parsed by the same
/// TOML subset reader as `j3.toml` (`project::toml_doc`). Tables come back in file order, defaults first.
pub fn parse(src: &str) -> Result<Vec<Tbl>, String> {
    let (heads, vals) = crate::project::toml_doc(src).map_err(|e| e.replacen("j3.toml:", "golden.toml:", 1))?;
    let mut out = vec![Tbl { kind: "defaults".into(), name: String::new(), kv: vec![], env: vec![] }];
    let mut seen: Vec<&str> = Vec::new();
    for (h, line) in &heads {
        let e = |m: String| format!("golden.toml:{line}: {m}");
        if seen.contains(&h.as_str()) {
            return Err(e(format!("[{h}] appears twice")));
        }
        seen.push(h);
        let p: Vec<&str> = h.split('.').collect();
        match p[..] {
            ["defaults"] => {}
            [k @ ("state" | "check"), n] if !n.is_empty() => out.push(Tbl { kind: k.into(), name: n.into(), kv: vec![], env: vec![] }),
            [k @ ("state" | "check"), n, "env"] if out.iter().any(|t| t.kind == k && t.name == n) => {}
            _ => return Err(e(format!("unknown section [{h}] (want [defaults], [state.NAME], [check.NAME], or NAME.env under them)"))),
        }
    }
    for (sec, k, v, line) in vals {
        let e = |m: String| format!("golden.toml:{line}: {m}");
        let p: Vec<&str> = sec.split('.').collect();
        if p[0] == "defaults" {
            if k == "text" || !KEYS.contains(&k.as_str()) {
                return Err(e(format!("unknown key `{k}` in [defaults] (want frame, tol, max, frames, timeout)")));
            }
            out[0].kv.push((k, v));
            continue;
        }
        let t = out.iter_mut().find(|t| t.kind == p[0] && t.name == p[1]).unwrap();
        if p.len() == 3 {
            t.env.push((k, v));
            continue;
        }
        if !KEYS.contains(&k.as_str()) {
            return Err(e(format!("unknown key `{k}` in [{sec}] (want {})", KEYS.join(", "))));
        }
        if k != "text" && v.parse::<u32>().is_err() {
            return Err(e(format!("`{k}` wants a whole number, not `{v}`")));
        }
        if t.kv.iter().any(|(a, _)| *a == k) {
            return Err(e(format!("`{k}` is set twice in [{sec}]")));
        }
        t.kv.push((k, v));
    }
    // a duplicate name across kinds would make `j3c golden NAME` ambiguous
    for (i, t) in out.iter().enumerate().skip(1) {
        if out[1..i].iter().any(|u| u.name == t.name) {
            let line = heads.iter().find(|(h, _)| *h == format!("{}.{}", t.kind, t.name)).map_or(0, |h| h.1);
            return Err(format!("golden.toml:{line}: the name `{}` is used twice", t.name));
        }
    }
    Ok(out)
}

/// The pass/fail decision for one image state. `diff` is `None` when the capture is missing.
pub fn judge(golden_exists: bool, diff: Option<Result<(usize, usize), String>>, max: usize) -> Result<(), String> {
    let Some(d) = diff else { return Err("no capture: the run wrote no image (see target/golden/<name>.log)".into()) };
    if !golden_exists {
        return Err("no golden yet: look at target/golden/<name>.png, then `j3c golden <dir> --bless`".into());
    }
    let (n, _) = d?;
    if n > max { Err(format!("{n} pixels differ")) } else { Ok(()) }
}

/// A check run passes at rc 0 with no line starting `FAIL`.
pub fn judge_check(rc: Option<i32>, out: &str) -> Result<(), String> {
    let Some(rc) = rc else { return Err("timed out".into()) };
    if let Some(l) = out.lines().find(|l| l.starts_with("FAIL")) {
        return Err(l.to_string());
    }
    if rc != 0 { Err(format!("rc {rc}")) } else { Ok(()) }
}

/// Wait for the child (and its process group on unix); past `limit` kill them all and return `None`.
fn wait_or_kill(mut ch: std::process::Child, limit: Duration) -> Option<i32> {
    let start = Instant::now();
    loop {
        match ch.try_wait() {
            Ok(Some(s)) => return s.code(),
            Ok(None) if start.elapsed() > limit => {
                #[cfg(unix)]
                let _ = Command::new("kill").args(["-KILL", "--", &format!("-{}", ch.id())]).status();
                let _ = ch.kill();
                let _ = ch.wait();
                return None;
            }
            _ => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

fn run(dir: &Path, t: &Tbl, d: &Tbl, name: &str, frames: Option<u32>, capture: Option<(&str, u32)>) -> (Option<i32>, String) {
    let log = dir.join("target/golden").join(format!("{name}.log"));
    let f = std::fs::File::create(&log).ok();
    let mut c = Command::new(std::env::current_exe().unwrap_or_else(|_| "j3c".into()));
    c.current_dir(dir).args(["run", ".", "--headless"]);
    if let Some(n) = frames {
        c.args(["--frames", &n.to_string()]);
    }
    c.env("SDL_VIDEODRIVER", "offscreen").env("SDL_AUDIODRIVER", "dummy").env("__GL_SHADER_DISK_CACHE", "0").env("J3_FIXED_DT", "0.016");
    c.env_remove("J3_CAPTURE").env_remove("J3_STEADY"); // an inherited one must not leak in; the toml may set them
    for (k, v) in d.env.iter().chain(&t.env) {
        c.env(k, v);
    }
    if let Some((p, fr)) = capture {
        c.env("J3_CAPTURE", format!("{p}:{fr}"));
    }
    if let Some(f) = f {
        c.stdout(f.try_clone().unwrap()).stderr(f);
    }
    let limit = Duration::from_secs(t.get("timeout").or(d.get("timeout")).and_then(|s| s.parse().ok()).unwrap_or(300));
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut c, 0); // so a timeout kills the program `j3c run` started too
    let Ok(ch) = c.stdin(Stdio::null()).spawn() else { return (Some(127), "could not start j3c".into()) };
    let rc = wait_or_kill(ch, limit);
    (rc, std::fs::read_to_string(&log).unwrap_or_default())
}

pub fn cmd(args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--help" || a == "-h") || args.is_empty() {
        println!("usage: j3c golden <dir> [name...] [--bless]");
        println!("  Runs each [[state]] of <dir>/golden.toml headless, captures target/golden/<name>.png and compares it");
        println!("  with golden/<name>.png (PASS/FAIL per state; exit 1 on any FAIL). Names run only those states.");
        println!("  --bless copies the captures over the goldens (only the named states, else all). [[check]] runs execute");
        println!("  only without --bless. Format and recipe: `j3c doc testing`.");
        return if args.is_empty() { 2 } else { 0 };
    }
    let bless = args.iter().any(|a| a == "--bless");
    let pos: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let dir = Path::new(pos[0]);
    let src = match std::fs::read_to_string(dir.join("golden.toml")) {
        Ok(s) => s,
        Err(_) => {
            eprintln!("j3c golden: no golden.toml in {}", dir.display());
            return 2;
        }
    };
    let tables = match parse(&src) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("j3c golden: {e}");
            return 2;
        }
    };
    let (d, rest) = tables.split_first().unwrap();
    let names = &pos[1..];
    if let Some(n) = names.iter().find(|n| !rest.iter().any(|t| t.name == n.as_str())) {
        eprintln!("j3c golden: no state or check named `{n}`");
        return 2;
    }
    let _ = std::fs::create_dir_all(dir.join("target/golden"));
    let _ = std::fs::create_dir_all(dir.join("golden"));
    let num = |t: &Tbl, k: &str, dflt: usize| t.get(k).or(d.get(k)).and_then(|s| s.parse().ok()).unwrap_or(dflt);
    let mut fail = false;
    for t in rest {
        let name = t.name.as_str();
        if !names.is_empty() && !names.iter().any(|n| n.as_str() == name) {
            continue;
        }
        if t.kind == "check" {
            if bless { continue }
            let (rc, out) = run(dir, t, d, name, t.get("frames").and_then(|s| s.parse().ok()), None);
            match judge_check(rc, &out) {
                Ok(()) => println!("PASS {name}"),
                Err(e) => { println!("FAIL {name} ({e}; target/golden/{name}.log)"); fail = true }
            }
            continue;
        }
        if t.kind != "state" {
            continue;
        }
        let frame = num(t, "frame", 30) as u32;
        let png = format!("target/golden/{name}.png");
        let _ = std::fs::remove_file(dir.join(&png));
        let text = t.get("text");
        if let Some(x) = text { let _ = std::fs::remove_file(dir.join(x)); }
        let (rc, _) = run(dir, t, d, name, Some(num(t, "frames", frame as usize + 2) as u32), Some((&png, frame)));
        if rc.is_none() {
            println!("FAIL {name} (timed out and killed; target/golden/{name}.log)");
            fail = true;
            continue;
        }
        let gold = dir.join(format!("golden/{name}.png"));
        let cap = dir.join(&png);
        let tol = num(t, "tol", 0) as i32;
        if bless {
            if cap.exists() && std::fs::copy(&cap, &gold).is_ok() {
                println!("blessed {name}");
            } else {
                println!("FAIL {name} (no capture to bless)");
                fail = true;
            }
            if let Some(x) = text {
                if std::fs::copy(dir.join(x), dir.join(format!("golden/{name}.txt"))).is_err() {
                    println!("FAIL {name} (the run wrote no {x} to bless)");
                    fail = true;
                }
            }
            continue;
        }
        let d_img = cap.exists().then(|| {
            if !gold.exists() { return Ok((0, 0)) }
            crate::imgdiff::diff(gold.to_str().unwrap(), cap.to_str().unwrap(), tol, dir.join(format!("target/golden/{name}.diff.png")).to_str()).map_err(|e| e.1)
        });
        let mut r = judge(gold.exists(), d_img, num(t, "max", 0));
        if r.is_ok() {
            if let Some(x) = text {
                let a = std::fs::read_to_string(dir.join(format!("golden/{name}.txt"))).unwrap_or_default();
                let b = std::fs::read_to_string(dir.join(x)).unwrap_or_default();
                if a != b {
                    let n = a.lines().zip(b.lines()).take_while(|(p, q)| p == q).count() + 1;
                    r = Err(format!("{x} differs from golden/{name}.txt at line {n}"));
                }
            }
        }
        match r {
            Ok(()) => println!("PASS {name}"),
            Err(e) => { println!("FAIL {name} ({e})"); fail = true }
        }
    }
    i32::from(fail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_defaults_states_and_env() {
        let t = parse("[defaults]\nframe = 4 # x\n[state.a]\ntol = 2\n[state.a.env]\nA = \"1\"\nB = \"x, y\"\n[check.c]\n[state.d-e]\n").unwrap();
        assert_eq!(t.len(), 4);
        assert_eq!(t[0].get("frame"), Some("4"));
        assert_eq!((t[1].kind.as_str(), t[1].name.as_str(), t[1].get("tol")), ("state", "a", Some("2")));
        assert_eq!(t[1].env, vec![("A".to_string(), "1".to_string()), ("B".to_string(), "x, y".to_string())]);
        assert_eq!((t[2].kind.as_str(), t[3].name.as_str()), ("check", "d-e"));
    }

    #[test]
    fn malformed_files_name_the_line() {
        let e = |s: &str| parse(s).err().unwrap_or_default();
        assert!(e("[state.a]\nfram = 3\n").starts_with("golden.toml:2: unknown key `fram`"));
        assert!(e("[state.a]\n[state.b]\n[state.a]\n").starts_with("golden.toml:3: [state.a] appears twice"));
        assert!(e("[state.a]\n[check.a]\n").starts_with("golden.toml:2: the name `a` is used twice"));
        assert!(e("[state.a]\ntol = \"x\n").starts_with("golden.toml:2:"));
        assert!(e("[state.a]\ntol = high\n").starts_with("golden.toml:2: `tol` wants a whole number"));
        assert!(e("[state.a]\ntol = [\"1\"]\n").starts_with("golden.toml:2:"));
        assert!(e("[stat.a]\n").starts_with("golden.toml:1: unknown section"));
        assert!(e("[state.a]\ntol = 1\ntol = 2\n").starts_with("golden.toml:3: `tol` is set twice"));
    }

    #[cfg(unix)]
    #[test]
    fn a_hung_child_and_its_children_are_killed() {
        use std::os::unix::process::CommandExt;
        let mut c = Command::new("sh");
        c.args(["-c", "sleep 30 & echo $! > /tmp/j3_golden_kill_test.pid; wait"]).process_group(0);
        let ch = c.spawn().unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(wait_or_kill(ch, Duration::from_millis(100)), None);
        let pid = std::fs::read_to_string("/tmp/j3_golden_kill_test.pid").unwrap();
        std::thread::sleep(Duration::from_millis(200));
        let st = std::fs::read_to_string(format!("/proc/{}/stat", pid.trim())).unwrap_or_default();
        assert!(st.is_empty() || st.contains(") Z"), "grandchild still alive: {st}"); // a zombie is dead, just unreaped
        let _ = std::fs::remove_file("/tmp/j3_golden_kill_test.pid");
    }

    #[test]
    fn verdicts() {
        assert!(judge(true, Some(Ok((0, 9))), 0).is_ok());
        assert!(judge(true, Some(Ok((3, 9))), 2).unwrap_err().contains("3 pixels"));
        assert!(judge(true, Some(Ok((3, 9))), 3).is_ok());
        assert!(judge(false, Some(Ok((0, 0))), 0).unwrap_err().contains("--bless"));
        assert!(judge(true, None, 0).is_err());
        assert!(judge(true, Some(Err("size differs".into())), 0).is_err());
        assert!(judge_check(Some(0), "PASS a\n").is_ok());
        assert!(judge_check(Some(0), "FAIL b\n").is_err());
        assert!(judge_check(Some(1), "").unwrap_err().contains("rc 1"));
        assert!(judge_check(None, "").is_err());
    }
}
