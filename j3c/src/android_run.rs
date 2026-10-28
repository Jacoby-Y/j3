//! `j3c -e android run [dir] [--serial S] [--secs N] [--no-build]`: build the project's APK (the
//! same build as `j3c build --release`, so `[dist] build_outputs` needs `"apk"`), install it on one
//! adb device, start it, and print its log until the app exits or `--secs` pass.
//! Every `adb` gets `stdin = null`: nothing here waits for input.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use crate::emit::android::sdk_root;
use crate::project::{self, Input};

const USAGE: &str = "usage: j3c -e android run [dir] [--serial S] [--secs N] [--no-build]";

pub fn cmd(args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        println!("  Builds the APK of dir's project (as `j3c build --release`; `[dist] build_outputs` needs \"apk\"),");
        println!("  installs it on the connected device, starts it, and prints its log (logcat --pid).");
        println!("  --serial S  the device to use when several are connected (see `adb devices`)");
        println!("  --secs N    stop after N seconds (default 60); also stops when the app exits");
        println!("  --no-build  install the APK already in target/dist instead of building");
        return 0;
    }
    match args.first().map(String::as_str) {
        Some("run") => match Opts::parse(&args[1..]) {
            Ok(o) => match run(&o) {
                Ok(code) => code,
                Err(e) => {
                    eprintln!("{e}");
                    1
                }
            },
            Err(e) => {
                eprintln!("j3c -e android run: {e}");
                eprintln!("{USAGE}");
                1
            }
        },
        Some(other) => {
            eprintln!("j3c -e android: unknown command `{other}`");
            eprintln!("{USAGE}");
            1
        }
        None => {
            eprintln!("{USAGE}");
            1
        }
    }
}

struct Opts {
    dir: String,
    serial: Option<String>,
    secs: u64,
    build: bool,
}

impl Opts {
    fn parse(args: &[String]) -> Result<Opts, String> {
        let mut o = Opts { dir: ".".into(), serial: None, secs: 60, build: true };
        let mut dirs = Vec::new();
        let mut it = args.iter();
        while let Some(a) = it.next() {
            match a.as_str() {
                "--serial" => o.serial = Some(it.next().ok_or("--serial needs a device serial")?.clone()),
                "--secs" => {
                    o.secs = it
                        .next()
                        .and_then(|v| v.parse().ok())
                        .filter(|n| *n >= 1)
                        .ok_or("--secs needs a whole number of seconds, 1 or more")?;
                }
                "--no-build" => o.build = false,
                f if f.starts_with('-') => return Err(format!("unknown option `{f}`")),
                d => dirs.push(d.to_string()),
            }
        }
        if dirs.len() > 1 {
            return Err("give one project folder".into());
        }
        if let Some(d) = dirs.pop() {
            o.dir = d;
        }
        Ok(o)
    }
}

/// The whole command. Each error is one line and is printed as it is. `Ok(1)` is a failed build,
/// which has already printed why.
fn run(o: &Opts) -> Result<i32, String> {
    let m = match project::resolve(Path::new(&o.dir))? {
        Input::Project(m) => m,
        _ => return Err(format!("{} is not a project folder: it needs a j3.toml", o.dir)),
    };
    if o.build && crate::cli::run(vec!["build".to_string(), "--release".to_string(), o.dir.clone()]) != 0 {
        return Ok(1);
    }
    let apk = crate::dist::apk::path(&m);
    if !apk.is_file() {
        return Err(format!("no APK at {}: add \"apk\" to [dist] build_outputs in j3.toml", apk.display()));
    }
    let apk_str = apk.to_str().ok_or_else(|| format!("APK path is not UTF-8: {}", apk.display()))?;

    let adb = adb_bin();
    let listed = ok_stdout(&adb_run(&adb, None, &["devices"])?, "devices")?;
    let serial = pick_device(&parse_devices(&listed), o.serial.as_deref())?;
    let serial = serial.as_str();

    let pkg = m.android.package.as_str();
    let activity = format!("{pkg}/.J3Activity");
    let steps: [(&str, &[&str]); 4] = [
        ("install", &["install", "-r", apk_str]),
        ("force-stop", &["shell", "am", "force-stop", pkg]),
        ("logcat -c", &["logcat", "-c"]),
        ("start", &["shell", "am", "start", "-W", "-n", activity.as_str()]),
    ];
    for (what, args) in steps {
        ok_stdout(&adb_run(&adb, Some(serial), args)?, what)?;
    }

    let pid = pid_of(&adb, serial, pkg)?
        .ok_or_else(|| format!("{pkg} has no process after start: it may have crashed (`adb logcat -d` shows why)"))?;
    let pid_arg = format!("--pid={pid}");
    let mut logcat = adb_command(&adb, Some(serial), &["logcat", "-v", "brief", pid_arg.as_str()])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("cannot run adb logcat: {e}"))?;
    let end = Instant::now() + Duration::from_secs(o.secs);
    let followed = follow(&adb, serial, pkg, &mut logcat, end);
    // Always stop and reap logcat. A child that already exited makes kill fail, which is fine here.
    let _ = logcat.kill();
    let _ = logcat.wait();
    followed?;
    Ok(0)
}

/// Checks the app's process once a second until it is gone, `end` passes, or logcat stops by itself
/// (that is an error: the log stopped while the app may still run).
fn follow(adb: &Path, serial: &str, pkg: &str, logcat: &mut Child, end: Instant) -> Result<(), String> {
    loop {
        if let Ok(Some(status)) = logcat.try_wait() {
            return Err(format!("adb logcat stopped early ({status})"));
        }
        let now = Instant::now();
        if now >= end {
            return Ok(());
        }
        std::thread::sleep(end.min(now + Duration::from_secs(1)) - now);
        if pid_of(adb, serial, pkg)?.is_none() {
            return Ok(());
        }
    }
}

/// The device to use: the only one, or `--serial` when it is connected.
fn pick_device(devices: &[String], want: Option<&str>) -> Result<String, String> {
    match (devices, want) {
        ([], _) => Err("no Android device: connect one with USB debugging on, or start an emulator".into()),
        (_, Some(s)) => devices
            .iter()
            .find(|d| d.as_str() == s)
            .cloned()
            .ok_or_else(|| format!("no Android device {s}: adb lists {}", devices.join(", "))),
        ([one], None) => Ok(one.clone()),
        (many, None) => Err(format!("{} Android devices ({}): pick one with --serial S", many.len(), many.join(", "))),
    }
}

/// Serials of the devices `adb devices` lists as ready, the lines `<serial>\tdevice`. The header and
/// the `offline` and `unauthorized` lines are skipped.
fn parse_devices(out: &str) -> Vec<String> {
    out.lines()
        .filter_map(|line| {
            let (serial, state) = line.trim_end().split_once('\t')?;
            (state.trim() == "device" && !serial.trim().is_empty()).then(|| serial.trim().to_string())
        })
        .collect()
}

/// The app's process id, or None when it has no process. `pidof` exits 1 with no output when the app
/// is not running. Output on stderr is adb's own failure (the device went away), which is an error.
fn pid_of(adb: &Path, serial: &str, pkg: &str) -> Result<Option<String>, String> {
    let out = adb_run(adb, Some(serial), &["shell", "pidof", pkg])?;
    if !out.status.success() {
        let err = text(&out.stderr);
        return if err.is_empty() { Ok(None) } else { Err(format!("adb shell pidof failed: {err}")) };
    }
    Ok(text(&out.stdout).split_whitespace().next().map(str::to_string))
}

/// `$ANDROID_HOME/platform-tools/adb` when it exists (`ANDROID_HOME` defaults to `~/Android/Sdk`),
/// else `adb` from PATH.
fn adb_bin() -> PathBuf {
    sdk_root().map(|s| s.join("platform-tools/adb")).filter(|p| p.is_file()).unwrap_or_else(|| PathBuf::from("adb"))
}

/// An `adb` command with stdin closed. `-s serial` goes first when given.
fn adb_command(adb: &Path, serial: Option<&str>, args: &[&str]) -> Command {
    let mut c = Command::new(adb);
    c.stdin(Stdio::null());
    if let Some(s) = serial {
        c.arg("-s").arg(s);
    }
    c.args(args);
    c
}

fn adb_run(adb: &Path, serial: Option<&str>, args: &[&str]) -> Result<Output, String> {
    adb_command(adb, serial, args)
        .output()
        .map_err(|e| format!("cannot run {}: {e} (install Android platform-tools, or put adb on PATH)", adb.display()))
}

/// stdout of a successful `adb` call. A non-zero exit fails with its stderr (or stdout when stderr is
/// empty). `install` and `am start` can print `Failure ...` or `Error: ...` with exit 0, which fails
/// too. Stderr of a successful call is forwarded, so adb's daemon notice shows.
fn ok_stdout(out: &Output, what: &str) -> Result<String, String> {
    let (so, err) = (text(&out.stdout), text(&out.stderr));
    if !out.status.success() {
        return Err(format!("adb {what} failed: {}", if err.is_empty() { &so } else { &err }));
    }
    if !err.is_empty() {
        eprintln!("{err}");
    }
    if let Some(line) = so.lines().find(|l| l.starts_with("Failure") || l.starts_with("Error")) {
        return Err(format!("adb {what} failed: {line}"));
    }
    Ok(so)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::parse_devices;

    #[test]
    fn parse_devices_cases() {
        assert!(parse_devices("List of devices attached\n\n").is_empty());
        assert_eq!(parse_devices("List of devices attached\nemulator-5554\tdevice\n\n"), ["emulator-5554"]);
        assert!(parse_devices("List of devices attached\nA1\tunauthorized\nB2\toffline\n").is_empty());
        assert_eq!(
            parse_devices("List of devices attached\nA1\tdevice\nB2\toffline\nC3\tunauthorized\n"),
            ["A1"]
        );
        assert_eq!(parse_devices("List of devices attached\r\nA1\tdevice\r\nB2\tdevice\r\n\r\n"), ["A1", "B2"]);
    }
}
