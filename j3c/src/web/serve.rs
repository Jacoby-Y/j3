//! The dev server for web builds: static files from one folder plus the `/__j3/*` endpoints the shell
//! page talks to. `std::net`, one connection at a time, `Connection: close` (enough for one tab).
//! COOP/COEP are always sent, so a later pthreads build (plan 06 chunk 13) works here too.
//!
//! - `GET /__j3/env` the env JSON the page copies into Emscripten's `ENV` (`{"dev":true,...}`)
//! - `POST /__j3/log?fd=1|2` body is program output, printed to our stdout/stderr
//! - `POST /__j3/file?name=F` body is a file the program wrote (a screenshot), saved as `out/F`
//! - `POST /__j3/exit?code=N` the program ended; [`State::exit`] holds the code

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// What the endpoints filled in, shared with the thread that serves.
#[derive(Default)]
pub struct State {
    pub exit: Option<i32>,
    /// Program output kept for tests (also printed).
    pub log: String,
}

pub struct Server {
    pub root: PathBuf,
    /// Where `/__j3/file` uploads land.
    pub out: PathBuf,
    /// The JSON `/__j3/env` returns.
    pub env_json: String,
    pub state: Arc<Mutex<State>>,
    /// Print program output (off in tests).
    pub echo: bool,
}

/// Bind `127.0.0.1:port` (0 picks a free one) and serve `srv` on a thread. Returns the port.
pub fn start(srv: Server, port: u16) -> Result<u16, String> {
    let l = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("cannot listen on port {port}: {e}"))?;
    let port = l.local_addr().map_err(|e| e.to_string())?.port();
    let srv = Arc::new(srv);
    std::thread::spawn(move || {
        for conn in l.incoming().flatten() {
            let _ = handle(&srv, conn);
        }
    });
    Ok(port)
}

fn handle(srv: &Server, mut conn: TcpStream) -> std::io::Result<()> {
    let mut rd = BufReader::new(conn.try_clone()?);
    let mut line = String::new();
    rd.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("/").to_string();
    let mut len = 0usize;
    loop {
        let mut h = String::new();
        if rd.read_line(&mut h)? == 0 || h.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                len = v.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0u8; len];
    rd.read_exact(&mut body)?;
    let (status, mime, bytes) = respond(srv, &method, &target, &body);
    write!(
        conn,
        "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCross-Origin-Opener-Policy: same-origin\r\n\
         Cross-Origin-Embedder-Policy: require-corp\r\nCross-Origin-Resource-Policy: same-origin\r\nCache-Control: no-store\r\n\
         Connection: close\r\n\r\n",
        bytes.len()
    )?;
    conn.write_all(&bytes)?;
    conn.flush()
}

/// One request's status line, MIME type and body. Split from the socket so tests call it directly.
pub fn respond(srv: &Server, method: &str, target: &str, body: &[u8]) -> (&'static str, &'static str, Vec<u8>) {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let arg = |k: &str| query.split('&').find_map(|kv| kv.strip_prefix(k).and_then(|r| r.strip_prefix('=')));
    let ok = ("200 OK", "text/plain", Vec::new());
    let bad = |m: &str| ("400 Bad Request", "text/plain", m.as_bytes().to_vec());
    match (method, path) {
        ("GET", "/__j3/env") => ("200 OK", "application/json", srv.env_json.clone().into_bytes()),
        ("POST", "/__j3/log") => {
            let text = String::from_utf8_lossy(body);
            if srv.echo {
                if arg("fd") == Some("2") {
                    eprint!("{text}");
                } else {
                    print!("{text}");
                    let _ = std::io::stdout().flush();
                }
            }
            srv.state.lock().unwrap().log.push_str(&text);
            ok
        }
        ("POST", "/__j3/file") => {
            let Some(name) = arg("name").filter(|n| plain_name(n)) else { return bad("name must be a plain file name") };
            match std::fs::write(srv.out.join(name), body) {
                Ok(()) => ok,
                Err(e) => ("500 Internal Server Error", "text/plain", e.to_string().into_bytes()),
            }
        }
        ("POST", "/__j3/exit") => {
            let Some(code) = arg("code").and_then(|c| c.parse().ok()) else { return bad("code must be a number") };
            srv.state.lock().unwrap().exit = Some(code);
            ok
        }
        ("GET", _) => match file_under(&srv.root, path) {
            Some(p) => match std::fs::read(&p) {
                Ok(b) => ("200 OK", mime(&p), b),
                Err(_) => ("404 Not Found", "text/plain", b"not found".to_vec()),
            },
            None => ("403 Forbidden", "text/plain", b"outside the served folder".to_vec()),
        },
        _ => ("405 Method Not Allowed", "text/plain", Vec::new()),
    }
}

/// A file name with no folder part, not `.`/`..`, not empty.
fn plain_name(n: &str) -> bool {
    !n.is_empty() && n != "." && n != ".." && !n.contains(['/', '\\', '%', '\0'])
}

/// `root` + the URL path; `None` when a part is `..` (or encoded, which we do not decode). `/` is `index.html`.
fn file_under(root: &Path, url: &str) -> Option<PathBuf> {
    let mut p = root.to_path_buf();
    for part in url.split('/').filter(|s| !s.is_empty()) {
        if part == ".." || part == "." || part.contains(['%', '\\', '\0']) {
            return None;
        }
        p.push(part);
    }
    if p.is_dir() {
        p.push("index.html");
    }
    Some(p)
}

fn mime(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript",
        "wasm" => "application/wasm",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "css" => "text/css",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
}

/// `{"dev":true,"env":{"K":"V",...}}` with JSON string escaping.
pub fn env_json(vars: &[(String, String)]) -> String {
    let esc = |s: &str| {
        let mut o = String::from("\"");
        for c in s.chars() {
            match c {
                '"' => o.push_str("\\\""),
                '\\' => o.push_str("\\\\"),
                c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
                c => o.push(c),
            }
        }
        o.push('"');
        o
    };
    let kv: Vec<String> = vars.iter().map(|(k, v)| format!("{}:{}", esc(k), esc(v))).collect();
    format!("{{\"dev\":true,\"env\":{{{}}}}}", kv.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(dir: &Path) -> Server {
        Server {
            root: dir.join("site"),
            out: dir.join("out"),
            env_json: env_json(&[("J3_FRAMES".into(), "3".into()), ("Q".into(), "a\"b".into())]),
            state: Arc::new(Mutex::new(State::default())),
            echo: false,
        }
    }

    #[test]
    fn serves_files_and_endpoints() {
        let dir = std::env::temp_dir().join(format!("j3-web-serve-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("site/sub")).unwrap();
        std::fs::create_dir_all(dir.join("out")).unwrap();
        std::fs::write(dir.join("site/index.html"), "<p>hi</p>").unwrap();
        std::fs::write(dir.join("site/sub/a.wasm"), [0u8, 97, 115, 109]).unwrap();
        std::fs::write(dir.join("secret.txt"), "no").unwrap();
        let srv = server(&dir);

        let (s, m, b) = respond(&srv, "GET", "/", b"");
        assert_eq!((s, m, b.as_slice()), ("200 OK", "text/html; charset=utf-8", b"<p>hi</p>".as_slice()));
        assert_eq!(respond(&srv, "GET", "/sub/a.wasm?v=1", b"").1, "application/wasm");
        assert_eq!(respond(&srv, "GET", "/nope.js", b"").0, "404 Not Found");
        assert_eq!(respond(&srv, "GET", "/../secret.txt", b"").0, "403 Forbidden");
        assert_eq!(respond(&srv, "GET", "/sub/%2e%2e/../secret.txt", b"").0, "403 Forbidden");

        let env = String::from_utf8(respond(&srv, "GET", "/__j3/env", b"").2).unwrap();
        assert_eq!(env, r#"{"dev":true,"env":{"J3_FRAMES":"3","Q":"a\"b"}}"#);

        respond(&srv, "POST", "/__j3/log?fd=1", b"hello\n");
        assert_eq!(srv.state.lock().unwrap().log, "hello\n");

        assert_eq!(respond(&srv, "POST", "/__j3/file?name=shot.png", b"PNG").0, "200 OK");
        assert_eq!(std::fs::read(dir.join("out/shot.png")).unwrap(), b"PNG");
        for bad in ["../x", "/etc/x", "a/b", "..", ""] {
            assert_eq!(respond(&srv, "POST", &format!("/__j3/file?name={bad}"), b"x").0, "400 Bad Request", "{bad}");
        }

        assert_eq!(respond(&srv, "POST", "/__j3/exit?code=x", b"").0, "400 Bad Request");
        assert_eq!(srv.state.lock().unwrap().exit, None);
        respond(&srv, "POST", "/__j3/exit?code=101", b"");
        assert_eq!(srv.state.lock().unwrap().exit, Some(101));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn socket_sends_isolation_headers() {
        let dir = std::env::temp_dir().join(format!("j3-web-sock-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("site")).unwrap();
        std::fs::write(dir.join("site/index.html"), "ok").unwrap();
        let port = start(server(&dir), 0).unwrap();
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        let mut reply = String::new();
        s.read_to_string(&mut reply).unwrap();
        assert!(reply.starts_with("HTTP/1.1 200 OK"), "{reply}");
        assert!(reply.contains("Cross-Origin-Opener-Policy: same-origin"), "{reply}");
        assert!(reply.contains("Cross-Origin-Embedder-Policy: require-corp"), "{reply}");
        assert!(reply.ends_with("\r\n\r\nok"), "{reply}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
