//! `j3c -e steam-check --sdk DIR`: verify `j3.steam` against the real Steamworks SDK (not shipped
//! here). Compares every `SteamAPI_*` symbol, call arity and types, accessor versions, struct
//! layouts (both pack rules), callback ids and named constants with the SDK's headers.
//! Only reads DIR; the C++ probe lives in a temp dir.

mod calls;
mod consts;
mod header;
mod probe;
mod scan;
#[cfg(test)]
mod tests;

use scan::Src;
use std::path::{Path, PathBuf};

pub struct Finding {
    pub error: bool,
    pub info: bool,
    pub text: String,
}

impl Finding {
    pub fn new(error: bool, text: String) -> Finding {
        Finding { error, info: false, text }
    }
    pub fn err(text: impl Into<String>) -> Finding {
        Finding::new(true, text.into())
    }
    pub fn warn(text: impl Into<String>) -> Finding {
        Finding::new(false, text.into())
    }
    pub fn info(text: impl Into<String>) -> Finding {
        Finding { error: false, info: true, text: text.into() }
    }
    pub fn line(&self) -> String {
        let tag = if self.error { "error" } else if self.info { "info" } else { "warning" };
        format!("{tag}: {}", self.text)
    }
}

#[derive(Clone, Copy)]
pub struct Opts {
    pub compile: bool,
    pub windows: bool,
}

const USAGE: &str = "usage: j3c -e steam-check --sdk DIR [--no-compile] [--no-windows]";

const HELP: &str = "usage: j3c -e steam-check --sdk DIR [--no-compile] [--no-windows]
  Checks j3.steam against the Steamworks SDK in DIR (the unzipped SDK: DIR/sdk/public/steam/steam_api_flat.h).
  Symbols and accessor versions, call arity and types, struct layouts for both pack rules, callback ids,
  constants. Layouts need clang++ (or g++) and x86_64-w64-mingw32-g++; DIR is only read.
  --no-compile  skip the C++ probe (symbols and calls only)    --no-windows  skip the mingw (pack 8) pass
  Prints one error:/warning: line per finding and a summary; exit 1 on any error, 2 on usage.";

pub fn cmd(args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{HELP}");
        return 0;
    }
    let (mut sdk, mut opts) = (None, Opts { compile: true, windows: true });
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--sdk" => sdk = it.next().map(PathBuf::from),
            "--no-compile" => opts.compile = false,
            "--no-windows" => opts.windows = false,
            _ => {
                eprintln!("{USAGE}");
                return 2;
            }
        }
    }
    let Some(sdk) = sdk else {
        eprintln!("{USAGE}");
        return 2;
    };
    let Some(public) = find_public(&sdk) else {
        eprintln!("steam-check: no steam/steam_api_flat.h under {} (expected DIR/sdk/public/steam/ or DIR/public/steam/)", sdk.display());
        return 2;
    };
    println!("sdk: {} ({})", public.display(), version(&sdk).unwrap_or_else(|| "version unknown".into()));
    let srcs = scan::sources();
    if srcs.is_empty() {
        eprintln!("steam-check: no j3.steam sources found (J3_STDLIB={})", crate::stdlib::stdlib_dir().display());
        return 2;
    }
    let found = check(&public, &srcs, opts);
    let (mut errs, mut warns) = (0, 0);
    for f in &found {
        println!("{}", f.line());
        errs += f.error as usize;
        warns += (!f.error && !f.info) as usize;
    }
    println!("steam-check: {errs} errors, {warns} warnings");
    (errs > 0) as i32
}

/// The `public` directory (the one holding `steam/`) for DIR given as the zip root, `sdk/`, or `public/`.
pub fn find_public(dir: &Path) -> Option<PathBuf> {
    [dir.join("sdk/public"), dir.join("public"), dir.to_path_buf()].into_iter().find(|p| p.join("steam/steam_api_flat.h").is_file())
}

/// The SDK version from a `Readme.txt` near DIR (a line mentioning the SDK and `x.y`).
fn version(dir: &Path) -> Option<String> {
    for d in [dir.to_path_buf(), dir.join("sdk"), dir.join("..")] {
        // The Readme has a Latin-1 copyright sign, so read bytes, not UTF-8.
        let Ok(bytes) = std::fs::read(d.join("Readme.txt")) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        for l in text.lines().take(40) {
            let is_ver = |w: &str| w.trim_start_matches('v').split('.').count() >= 2 && w.trim_start_matches('v').chars().next().is_some_and(|c| c.is_ascii_digit());
            // "Steamworks SDK 1.xx" or the changelog head "v1.65 23th July 2026".
            if (l.contains("SDK") && l.split_whitespace().any(is_ver)) || l.trim().split_whitespace().next().is_some_and(|w| w.starts_with('v') && is_ver(w)) {
                return Some(l.trim().chars().take(80).collect());
            }
        }
    }
    None
}

/// All checks. `public` is the SDK's `public` directory; `srcs` the `j3.steam` files.
pub fn check(public: &Path, srcs: &[Src], opts: Opts) -> Vec<Finding> {
    let mut out = Vec::new();
    let sc = scan::scan(srcs);
    let flat = match std::fs::read_to_string(public.join("steam/steam_api_flat.h")) {
        Ok(t) => t,
        Err(e) => return vec![Finding::err(format!("cannot read steam_api_flat.h: {e}"))],
    };
    let mut decls = header::parse_flat(&flat);
    // Core calls (Init, Shutdown, ManualDispatch, the pipe) are declared in these, not the flat header.
    for h in ["steam/steam_api.h", "steam/steam_api_internal.h"] {
        if let Ok(t) = std::fs::read_to_string(public.join(h)) {
            for (k, d) in header::parse_flat(&t) {
                decls.entry(k).or_insert(d);
            }
        }
    }
    if decls.is_empty() {
        out.push(Finding::err("steam_api_flat.h has no `S_API ... SteamAPI_...( ... );` declarations; is this the SDK's flat header?"));
        return out;
    }
    let mut types = header::Types::default();
    types.add_text(&flat);
    if let Ok(rd) = std::fs::read_dir(public.join("steam")) {
        for e in rd.flatten() {
            if e.path().extension().is_some_and(|x| x == "h") {
                if let Ok(t) = std::fs::read_to_string(e.path()) {
                    types.add_text(&t);
                }
            }
        }
    }
    out.push(Finding::info(format!("{} symbol uses, {} accessor rows, {} layout rows, {} callbacks", sc.sites.len(), sc.accessors.len(), sc.rows.len(), sc.callbacks.len())));
    calls::check(&sc, &decls, &types, &mut out);
    for (name, loc) in &sc.pubs {
        if !consts::CONSTS.iter().any(|c| c.0 == name) && !consts::UNCHECKED.iter().any(|c| c.0 == name) {
            out.push(Finding::err(format!("{loc}: pub const {name} is in neither CONSTS nor UNCHECKED in steamcheck/consts.rs; add it with its SDK name, or list it with a reason")));
        }
    }
    if opts.compile {
        probe::check(public, &sc, opts.windows, &mut out);
    }
    out
}
