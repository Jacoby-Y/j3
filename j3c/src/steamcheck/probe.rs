//! Struct layouts, callback ids and constants against the SDK headers: a C++ probe of
//! `static_assert`s, compiled with `-fsyntax-only` once per pack rule (Linux = pack 4, Windows
//! mingw = pack 8). A failed assert is asked again as `J3V<value>` (an incomplete type) so the
//! compiler prints the SDK's number.

use super::consts::{member, CONSTS};
use super::scan::Scan;
use super::Finding;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

#[derive(Clone, Copy, PartialEq)]
enum What {
    Pack,
    Layout,
    Callback,
    Const,
}

struct Item {
    what: What,
    tag: String,
    loc: String,
    expr: String,
    ours: i64,
}

fn items(sc: &Scan, pack: i64, first: bool, out: &mut Vec<Finding>) -> Vec<Item> {
    let mut v = vec![Item { what: What::Pack, tag: "pack".into(), loc: String::new(), expr: "J3_PACK".into(), ours: pack }];
    for r in &sc.rows {
        let (expr, tag) = if r.field == "SIZE" {
            (format!("sizeof({})", r.name), format!("{}.SIZE", r.name))
        } else {
            (format!("offsetof({}, {})", r.name, member(&r.name, &r.field)), format!("{}.{}", r.name, r.field))
        };
        v.push(Item { what: What::Layout, tag, loc: r.loc.to_string(), expr, ours: if pack == 4 { r.p4 } else { r.p8 } });
    }
    if first {
        for (name, id, loc) in &sc.callbacks {
            v.push(Item { what: What::Callback, tag: name.clone(), loc: loc.to_string(), expr: format!("{name}::k_iCallback"), ours: *id });
        }
        for (name, id, loc) in &sc.cb_consts {
            if !sc.callbacks.iter().any(|c| c.1 == *id) {
                out.push(Finding::err(format!("{loc}: {name} = {id} has no row in callbackId (callbacks.j3), so steam-check cannot verify it")));
            }
        }
        for (name, cexpr) in CONSTS {
            match sc.consts.get(*name) {
                Some((val, loc)) => v.push(Item { what: What::Const, tag: format!("{name} ({cexpr})"), loc: loc.to_string(), expr: cexpr.to_string(), ours: *val }),
                None => out.push(Finding::err(format!("steam-check table names {name} but j3.steam has no such const; fix or remove the row in steamcheck/consts.rs"))),
            }
        }
    }
    v
}

const HEAD: &[&str] = &[
    "#include <stddef.h>",
    "#include <stdint.h>",
    "#include <steam/steam_api.h>",
    "#if __has_include(<steam/isteamnetworkingsockets.h>)",
    "#include <steam/isteamnetworkingsockets.h>",
    "#endif",
    "#if __has_include(<steam/isteamnetworkingmessages.h>)",
    "#include <steam/isteamnetworkingmessages.h>",
    "#endif",
    "#if __has_include(<steam/isteamnetworkingutils.h>)",
    "#include <steam/isteamnetworkingutils.h>",
    "#endif",
    "#if defined(VALVE_CALLBACK_PACK_SMALL)",
    "enum { J3_PACK = 4 };",
    "#elif defined(VALVE_CALLBACK_PACK_LARGE)",
    "enum { J3_PACK = 8 };",
    "#else",
    "#error neither VALVE_CALLBACK_PACK_SMALL nor VALVE_CALLBACK_PACK_LARGE is defined",
    "#endif",
    "template <long long X> struct J3V;",
];

/// The probe text and, for each line that checks something, its index in `items`.
fn source(items: &[&Item], ask: bool) -> (String, HashMap<usize, usize>) {
    let mut lines: Vec<String> = HEAD.iter().map(|s| s.to_string()).collect();
    let mut at = HashMap::new();
    for (i, it) in items.iter().enumerate() {
        lines.push(if ask {
            format!("J3V<(long long)({})> j3v{i};", it.expr)
        } else {
            format!("static_assert((long long)({}) == {}LL, \"j3 probe\");", it.expr, it.ours)
        });
        at.insert(lines.len(), i);
    }
    (lines.join("\n") + "\n", at)
}

struct Diag {
    line: usize,
    probe: bool,
    msg: String,
}

fn diags(stderr: &str) -> Vec<Diag> {
    let mut out = Vec::new();
    for l in stderr.lines() {
        let Some(i) = l.find(": error:").or_else(|| l.find(": fatal error:")) else { continue };
        let mut it = l[..i].rsplitn(3, ':');
        let (_col, line, file) = (it.next(), it.next(), it.next().unwrap_or(""));
        out.push(Diag {
            line: line.and_then(|s| s.parse().ok()).unwrap_or(0),
            probe: file.ends_with("probe.cpp"),
            msg: l[i + 2..].trim_start_matches("error:").trim_start_matches("fatal error:").trim().to_string(),
        });
    }
    out
}

pub(super) fn which(exe: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(exe).is_file()))
}

/// (compiler name, label) for the Linux pass: clang++, else g++.
pub(super) fn linux_cc() -> Option<&'static str> {
    ["clang++", "g++", "c++"].into_iter().find(|c| which(c))
}

fn run_cc(cc: &str, dir: &Path, public: &Path, text: &str) -> Result<(Vec<Diag>, String), String> {
    let file = dir.join("probe.cpp");
    std::fs::write(&file, text).map_err(|e| format!("cannot write {}: {e}", file.display()))?;
    let mut cmd = Command::new(cc);
    cmd.args(["-std=c++17", "-fsyntax-only", "-Wno-invalid-offsetof", "-w"]);
    if cc.contains("clang") {
        cmd.arg("-ferror-limit=0");
    }
    let o = cmd.arg("-I").arg(public).arg(&file).output().map_err(|e| format!("cannot run {cc}: {e}"))?;
    let err = String::from_utf8_lossy(&o.stderr).to_string();
    let d = diags(&err);
    if d.is_empty() && !o.status.success() {
        return Err(format!("{cc} failed: {}", err.lines().next().unwrap_or("no output")));
    }
    Ok((d, err))
}

/// One compile pass; `pack` is 4 (Linux) or 8 (Windows).
fn pass(cc: &str, label: &str, pack: i64, public: &Path, sc: &Scan, first: bool, out: &mut Vec<Finding>) {
    static RUN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = RUN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("j3-steamcheck-{}-{n}-{pack}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    pass_in(cc, label, pack, public, sc, first, &dir, out);
    let _ = std::fs::remove_dir_all(&dir);
}

#[allow(clippy::too_many_arguments)]
fn pass_in(cc: &str, label: &str, pack: i64, public: &Path, sc: &Scan, first: bool, dir: &Path, out: &mut Vec<Finding>) {
    let all = items(sc, pack, first, out);
    let refs: Vec<&Item> = all.iter().collect();
    let (text, at) = source(&refs, false);
    let (ds, _) = match run_cc(cc, dir, public, &text) {
        Ok(r) => r,
        Err(e) => return out.push(Finding::err(format!("{label}: {e}"))),
    };
    let mut failed = Vec::new();
    for d in &ds {
        match (d.probe, at.get(&d.line)) {
            (true, Some(&i)) if d.msg.contains("static assertion") || d.msg.contains("static_assert") => failed.push(i),
            (true, Some(&i)) => out.push(Finding::err(format!("{}: {}: {} (probe error: {}; if the SDK renamed it, fix the row or consts.rs)", refs[i].loc, refs[i].tag, label, d.msg))),
            _ => {
                out.push(Finding::err(format!("{label}: the SDK headers do not compile with {cc}: {}", d.msg)));
                return;
            }
        }
    }
    let sdk = ask(cc, dir, public, &refs, &failed);
    for i in &failed {
        let it = refs[*i];
        let theirs = sdk.get(i).map_or("?".to_string(), |v| v.to_string());
        let text = match it.what {
            What::Pack => format!("{label}: probe expected pack {} but {cc} activates pack {theirs}; the layout results of this pass are not trustworthy (wrong compiler target)", it.ours),
            What::Layout => format!("{}: {}: ours {}, SDK {theirs} ({label}, pack {pack})", it.loc, it.tag, it.ours),
            What::Callback => format!("{}: {}: callback id ours {}, SDK {theirs}", it.loc, it.tag, it.ours),
            What::Const => format!("{}: {}: ours {}, SDK {theirs}", it.loc, it.tag, it.ours),
        };
        out.push(Finding::err(text));
    }
    let macro_name = if pack == 4 { "VALVE_CALLBACK_PACK_SMALL" } else { "VALVE_CALLBACK_PACK_LARGE" };
    let wrong = failed.iter().any(|i| refs[*i].what == What::Pack);
    if !wrong {
        out.push(Finding::info(format!("{label}: {cc}, {macro_name} (pack {pack}), {} checks", refs.len() - 1)));
    }
}

/// The SDK's value for each failed item, from `J3V<value>` in the compiler's message.
fn ask(cc: &str, dir: &Path, public: &Path, refs: &[&Item], failed: &[usize]) -> HashMap<usize, i64> {
    let mut got = HashMap::new();
    if failed.is_empty() {
        return got;
    }
    let sub: Vec<&Item> = failed.iter().map(|&i| refs[i]).collect();
    let (text, at) = source(&sub, true);
    let Ok((ds, _)) = run_cc(cc, dir, public, &text) else { return got };
    for d in ds {
        if let (Some(&k), Some(p)) = (at.get(&d.line), d.msg.find("J3V<")) {
            let n: String = d.msg[p + 4..].chars().take_while(|c| c.is_ascii_digit() || *c == '-').collect();
            if let Ok(v) = n.parse() {
                got.insert(failed[k], v);
            }
        }
    }
    got
}

/// Both pack passes. `windows` false skips the mingw pass.
pub fn check(public: &Path, sc: &Scan, windows: bool, out: &mut Vec<Finding>) {
    match linux_cc() {
        Some(cc) => pass(cc, "linux", 4, public, sc, true, out),
        None => out.push(Finding::err("no C++ compiler found (tried clang++, g++, c++): cannot check layouts, callback ids or constants; install clang++ or pass --no-compile")),
    }
    if !windows {
        return;
    }
    let mingw = "x86_64-w64-mingw32-g++";
    if which(mingw) {
        pass(mingw, "windows", 8, public, sc, false, out);
    } else {
        out.push(Finding::err(format!("{mingw} not found: the Windows (pack 8) layouts were not checked; install mingw-w64 or pass --no-windows")));
    }
}
