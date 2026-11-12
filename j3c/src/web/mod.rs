//! `j3c -e web`: serve and run web builds (plan 06). `serve DIR` serves a built folder; `run DIR` serves
//! it, opens it (headless Chrome with `--headless`), streams the program's output here and exits with
//! its code. Building a project for the web is plan 06 chunk 2 (needs emsdk): until then `run` takes a
//! folder that already has an `index.html`.

pub mod serve;

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serve::{Server, State};

/// The `--shell-file` page for emcc: canvas, status line, and the `/__j3/*` client.
pub const SHELL: &str = include_str!("shell.html");

const USAGE: &str = "usage: j3c -e web serve DIR [--port N]\n       j3c -e web run DIR [--frames N] [--screenshot F] [--headless] [--open] [--secs N] [--port N]";

pub fn cmd(args: &[String]) -> i32 {
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        println!("  serve   serve DIR (an index.html with its .js/.wasm/.data) on 127.0.0.1 until Ctrl+C");
        println!("  run     serve DIR and run it: --headless uses headless Chrome (SwiftShader WebGL2), --open the");
        println!("          default browser, neither prints the URL. Output streams here; the exit code is the program's.");
        println!("  --frames N, --screenshot F   as `j3c run`: F is written when the program saves it");
        println!("  --secs N   give up after N seconds (default 120; exit 124)");
        return if args.is_empty() { 1 } else { 0 };
    }
    let r = Opts::parse(&args[1..]).and_then(|o| match args[0].as_str() {
        "serve" => serve_forever(&o),
        "run" => run(&o),
        other => Err(format!("unknown command `{other}`\n{USAGE}")),
    });
    match r {
        Ok(code) => code,
        Err(e) => {
            eprintln!("j3c -e web: {e}");
            1
        }
    }
}

#[derive(Default)]
struct Opts {
    dir: PathBuf,
    frames: Option<u32>,
    shot: Option<PathBuf>,
    headless: bool,
    open: bool,
    secs: u64,
    port: u16,
}

impl Opts {
    fn parse(args: &[String]) -> Result<Opts, String> {
        let mut o = Opts { dir: ".".into(), secs: 120, ..Opts::default() };
        let mut it = args.iter();
        let num = |v: Option<&String>, what: &str| -> Result<u64, String> {
            v.and_then(|v| v.parse().ok()).filter(|n| *n > 0).ok_or(format!("{what} needs a positive whole number"))
        };
        while let Some(a) = it.next() {
            match a.as_str() {
                "--frames" => o.frames = Some(num(it.next(), "--frames")? as u32),
                "--screenshot" => {
                    let p = PathBuf::from(it.next().ok_or("--screenshot needs a file")?);
                    o.shot = Some(std::env::current_dir().map_err(|e| e.to_string())?.join(p));
                }
                "--headless" => o.headless = true,
                "--open" => o.open = true,
                "--secs" => o.secs = num(it.next(), "--secs")?,
                "--port" => o.port = num(it.next(), "--port")? as u16,
                f if f.starts_with('-') => return Err(format!("unknown option `{f}`\n{USAGE}")),
                d => o.dir = PathBuf::from(d),
            }
        }
        if !o.dir.join("index.html").is_file() {
            return Err(format!(
                "{} has no index.html: give a built web folder (building a project for the web needs emsdk, plan 06 chunk 2)",
                o.dir.display()
            ));
        }
        Ok(o)
    }
}

/// Env vars the page copies into `ENV`: our `J3_*` and `SDL_*` (not the SDL drivers: the browser has
/// its own), with `J3_FRAMES` from `--frames` and the screenshot named by its file name only (the page
/// writes it at `/` and uploads it).
fn page_env(o: &Opts) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = std::env::vars()
        .filter(|(k, _)| (k.starts_with("J3_") || k.starts_with("SDL_")) && k != "SDL_VIDEODRIVER" && k != "SDL_AUDIODRIVER")
        .filter(|(k, _)| k != "J3_ROOT" && k != "J3_STDLIB")
        .collect();
    let mut set = |k: &str, val: String| {
        v.retain(|(n, _)| n != k);
        v.push((k.to_string(), val));
    };
    if let Some(n) = o.frames {
        set("J3_FRAMES", n.to_string());
    }
    if let Some(name) = o.shot.as_ref().and_then(|p| p.file_name()) {
        set("J3_SCREENSHOT", format!("/{}", name.to_string_lossy()));
    }
    if let Some((path, at)) = std::env::var("J3_CAPTURE").ok().and_then(|c| c.rsplit_once(':').map(|(a, b)| (a.to_string(), b.to_string()))) {
        let name = Path::new(&path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        set("J3_CAPTURE", format!("/{name}:{at}"));
    }
    v.sort();
    v
}

fn server(o: &Opts, out: &Path) -> (Server, Arc<Mutex<State>>) {
    let state = Arc::new(Mutex::new(State::default()));
    let srv = Server {
        root: o.dir.clone(),
        out: out.to_path_buf(),
        env_json: serve::env_json(&page_env(o)),
        state: state.clone(),
        echo: true,
    };
    (srv, state)
}

fn serve_forever(o: &Opts) -> Result<i32, String> {
    let (srv, _) = server(o, &o.dir);
    let port = serve::start(srv, o.port)?;
    println!("serving {} at http://127.0.0.1:{port}/  (Ctrl+C stops)", o.dir.display());
    loop {
        std::thread::park();
    }
}

fn run(o: &Opts) -> Result<i32, String> {
    let out = std::env::temp_dir().join(format!("j3-web-run-{}", std::process::id()));
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let (srv, state) = server(o, &out);
    let port = serve::start(srv, o.port)?;
    let url = format!("http://127.0.0.1:{port}/");
    let mut browser = if o.headless {
        Some(chrome(&url, &out)?)
    } else {
        if o.open {
            let _ = Command::new("xdg-open").arg(&url).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
        }
        println!("open {url}");
        None
    };
    let end = Instant::now() + Duration::from_secs(o.secs);
    let code = loop {
        if let Some(c) = state.lock().unwrap().exit {
            break Some(c);
        }
        if Instant::now() >= end {
            break None;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    if let Some(b) = browser.as_mut() {
        let _ = b.kill();
        let _ = b.wait();
    }
    let moved = o.shot.as_ref().map(|dst| {
        let src = out.join(dst.file_name().unwrap_or_default());
        std::fs::copy(&src, dst).map(|_| ()).map_err(|e| format!("screenshot {} was not uploaded: {e}", dst.display()))
    });
    let _ = std::fs::remove_dir_all(&out);
    let Some(code) = code else {
        eprintln!("j3c -e web run: no exit after {} s (raise --secs; a page error shows above)", o.secs);
        return Ok(124);
    };
    if let Some(Err(e)) = moved {
        if code == 0 {
            return Err(e);
        }
    }
    Ok(code)
}

/// Headless Chrome on `url` with SwiftShader WebGL2, its own profile under `tmp`, autoplay allowed.
fn chrome(url: &str, tmp: &Path) -> Result<Child, String> {
    let bin = ["google-chrome", "chromium", "chromium-browser", "google-chrome-stable"]
        .into_iter()
        .find(|b| Command::new(b).arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok())
        .ok_or("no google-chrome or chromium on PATH (needed for --headless)")?;
    Command::new(bin)
        .args([
            "--headless=new",
            "--no-sandbox",
            "--enable-unsafe-swiftshader",
            "--use-angle=swiftshader",
            "--autoplay-policy=no-user-gesture-required",
            "--no-first-run",
            "--window-size=1280,720",
        ])
        .arg(format!("--user-data-dir={}", tmp.join("chrome").display()))
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot start {bin}: {e}"))
}
