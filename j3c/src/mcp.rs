//! `j3c -e mcp`: a Model Context Protocol server over stdio (newline-delimited JSON-RPC 2.0), so
//! an agent can build scenes, look at them, and play the game. It is a thin layer over the
//! same work `j3c -e scene`, `j3c -e render`, `j3c check`, and `j3c run` do.
//!
//! While `j3c editor` has the project open, scene tools act on the live editor instead of
//! the files: requests go through the editor's events file, each change is one undo step,
//! and the person watching sees it happen. Scenes the editor doesn't have open, and every
//! scene while it is closed, are edited as files.
//!
//! stdout carries only protocol messages; everything else goes to stderr.

use std::fs;
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::lsp::json::{self, Json};
use crate::scene::{self, Reply};

const PROTOCOLS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const USAGE: &str = "\
usage: j3c -e mcp [--project DIR]

Serves the Model Context Protocol on stdin/stdout for an agent working on a j3 project.
Without --project it follows the editor: the project `j3c editor` has open, else the
nearest j3.toml at or above the current directory. With --project DIR it stays on the
nearest j3.toml at or above DIR. Tools: scene_list,
scene_get, scene_add, scene_add_many, scene_set, scene_delete, place, undo, render, check, play, console, import,
asset_search, asset_online_search, asset_fetch, gen. `j3c -e mcp --tools` prints each tool's
arguments and the CLI command that does the same work.

Saving and undo depend on the editor. With `j3c editor` open on the project, scene edits go
to it live: each is one undo step there (Ctrl+Z), and nothing is saved until the person saves
or scene_set passes save: true. With it closed, each edit writes the scene file at once and
the undo tool puts the file back (20 steps). Every edit reply says which one happened.

Claude Code:  claude mcp add -s user j3 -- j3c -e mcp
.mcp.json:    {\"mcpServers\": {\"j3\": {\"command\": \"j3c\", \"args\": [\"-e\", \"mcp\"]}}}";

const INSTRUCTIONS: &str = "\
Tools for a j3 game project: scenes are trees of named objects (meshes, cameras, lights, \
groups, placed scenes) in scenes/*.j3scene. Name objects by name, by Parent/Child path, or \
by #id. Positions are local to the parent, rotations are degrees, y is up, and 1 unit is \
about 1 meter. Work in a loop: change the scene (scene_add/scene_set; scene_add_many for several objects at once; place to ground, stack, \
scatter, or lay out grid pieces like walls and floors instead of guessing heights), then look at it with \
render (use focus to frame one object, view=top to check layout), then fix what looks \
wrong. Run check before play. When the person has the editor open, your edits appear there \
as undoable steps and are not saved until they save (or you call scene_set with save). \
Without the editor, undo puts a scene file back as it was before your last change. \
Need a model, material, or sky the project lacks? asset_online_search, then asset_fetch.";

pub fn cmd(rest: &[String]) -> i32 {
    let mut project = None;
    let mut it = rest.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{USAGE}");
                return 0;
            }
            "--tools" => {
                print!("{}", tool_reference());
                return 0;
            }
            "--project" => match it.next() {
                Some(p) => project = Some(PathBuf::from(p)),
                None => {
                    eprintln!("j3c -e mcp: --project needs a directory");
                    return 1;
                }
            },
            other => {
                eprintln!("j3c -e mcp: unknown argument `{other}`");
                return 1;
            }
        }
    }
    let follow = project.is_none();
    let root = scene::find_root(&project.unwrap_or_else(|| PathBuf::from(".")));
    let mut server = Server { root, follow, next: 0, was_live: false };
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    serve(&mut server, stdin.lock(), &mut stdout.lock());
    0
}

struct Server {
    /// The project, or why there is none (reported by every tool call).
    root: Result<PathBuf, String>,
    /// No --project: use the project the editor has open, when one is.
    follow: bool,
    /// Counter for live editor request ids.
    next: u64,
    /// The last scene request went to the open editor, not the file.
    was_live: bool,
}

fn serve(server: &mut Server, input: impl BufRead, out: &mut impl Write) {
    for line in input.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() { continue; }
        let reply = match json::parse(&line) {
            Ok(msg) => server.handle(&msg),
            Err(e) => Some(error(Json::Null, -32700, &format!("parse error: {e}"))),
        };
        if let Some(reply) = reply {
            let _ = writeln!(out, "{}", reply.stringify());
            let _ = out.flush();
        }
    }
}

fn result(id: Json, value: Json) -> Json {
    Json::obj(vec![("jsonrpc", Json::Str("2.0".into())), ("id", id), ("result", value)])
}

fn error(id: Json, code: i64, message: &str) -> Json {
    Json::obj(vec![
        ("jsonrpc", Json::Str("2.0".into())),
        ("id", id),
        ("error", Json::obj(vec![("code", Json::Num(code as f64)), ("message", Json::Str(message.into()))])),
    ])
}

fn s(v: &str) -> Json { Json::Str(v.into()) }

/// A tool's answer: MCP content blocks, optional structured data, and whether it failed.
struct Answer {
    content: Vec<Json>,
    structured: Option<Json>,
    error: bool,
}

impl Answer {
    fn text(text: impl Into<String>) -> Answer {
        Answer { content: vec![text_block(text)], structured: None, error: false }
    }

    fn fail(text: impl Into<String>) -> Answer {
        Answer { content: vec![text_block(text)], structured: None, error: true }
    }

    fn json(self) -> Json {
        let mut pairs = vec![("content", Json::Arr(self.content)), ("isError", Json::Bool(self.error))];
        if let Some(data) = self.structured {
            pairs.push(("structuredContent", data));
        }
        Json::obj(pairs)
    }
}

fn text_block(text: impl Into<String>) -> Json {
    Json::obj(vec![("type", s("text")), ("text", Json::Str(text.into()))])
}

impl Server {
    fn handle(&mut self, msg: &Json) -> Option<Json> {
        let method = msg.get("method").and_then(Json::as_str);
        let id = msg.get("id").cloned();
        let Some(method) = method else {
            // A response to something we never ask; nothing to do.
            return None;
        };
        let Some(id) = id else {
            // Notifications (`notifications/initialized`, cancellations) need no answer.
            return None;
        };
        let params = msg.get("params").cloned().unwrap_or(Json::Obj(Vec::new()));
        Some(match method {
            "initialize" => {
                let asked = params.get("protocolVersion").and_then(Json::as_str).unwrap_or("");
                let version = PROTOCOLS.iter().find(|v| **v == asked).copied().unwrap_or(PROTOCOLS[0]);
                result(
                    id,
                    Json::obj(vec![
                        ("protocolVersion", s(version)),
                        ("capabilities", Json::obj(vec![("tools", Json::obj(vec![("listChanged", Json::Bool(false))]))])),
                        ("serverInfo", Json::obj(vec![("name", s("j3c")), ("version", s(env!("CARGO_PKG_VERSION")))])),
                        ("instructions", s(INSTRUCTIONS)),
                    ]),
                )
            }
            "ping" => result(id, Json::Obj(Vec::new())),
            "tools/list" => result(id, Json::obj(vec![("tools", tools())])),
            "tools/call" => {
                let Some(name) = params.get("name").and_then(Json::as_str) else {
                    return Some(error(id, -32602, "tools/call needs a tool name"));
                };
                let args = params.get("arguments").cloned().unwrap_or(Json::Obj(Vec::new()));
                if !TOOL_NAMES.contains(&name) {
                    return Some(error(id, -32602, &format!("unknown tool `{name}`")));
                }
                let mut answer = match self.call(name, &args) {
                    Ok(a) => a,
                    Err(e) => Answer::fail(e),
                };
                if std::env::current_exe().map(|p| !p.exists()).unwrap_or(false) {
                    answer.content.push(text_block("note: j3c was rebuilt since this server started; restart the MCP server to use the new one."));
                }
                result(id, answer.json())
            }
            _ => error(id, -32601, &format!("method not found: {method}")),
        })
    }

    fn root(&self) -> Result<PathBuf, String> {
        if self.follow {
            // ponytail: with two editors open, the one opened last wins.
            let open = crate::editor::cache_dir().and_then(|c| fs::read_to_string(c.join("open-project")).ok());
            if let Some(open) = open.map(|s| PathBuf::from(s.trim())) {
                if self.live(&open) { return Ok(open); }
            }
            return self.root.clone().map_err(|_| {
                "no project: open one with `j3c editor`, or start the server with --project DIR".to_string()
            });
        }
        self.root.clone()
    }

    fn call(&mut self, name: &str, args: &Json) -> Result<Answer, String> {
        let mut answer = self.run_tool(name, args)?;
        if EDITS.contains(&name) && !answer.error {
            answer.content.push(text_block(
                if self.was_live {
                    "(live in the open editor: one undo step there, unsaved until the person saves or scene_set passes save: true)"
                } else {
                    "(written to the scene file; the undo tool puts it back)"
                }
                .to_string(),
            ));
        }
        Ok(answer)
    }

    fn run_tool(&mut self, name: &str, args: &Json) -> Result<Answer, String> {
        let root = self.root()?;
        match name {
            "scene_list" => {
                let rel = scene_arg(&root, args)?;
                let mut reply = self.scene_request(&root, &rel, &["list".into()])?;
                if let Some(filter) = str_arg(args, "filter") {
                    let f = filter.to_lowercase();
                    reply.lines.retain(|l| l[0] != "node" || l.iter().take(4).any(|c| c.to_lowercase().contains(&f)));
                }
                let count = reply.lines.iter().filter(|l| l[0] == "node").count();
                let head = format!(
                    "{count} objects in {rel} (#id path kind asset, world position; rotations are degrees){}\n",
                    if self.was_live { ", live in the open editor" } else { "" }
                );
                Ok(answer_from(&reply, "list", &rel, head))
            }
            "scene_get" => {
                let rel = scene_arg(&root, args)?;
                let node = need_str(args, "node")?;
                let reply = self.scene_request(&root, &rel, &["get".into(), node])?;
                Ok(answer_from(&reply, "get", &rel, String::new()))
            }
            "scene_add" => {
                let rel = scene_arg(&root, args)?;
                let reply = self.scene_request(&root, &rel, &add_request(args)?)?;
                Ok(answer_from(&reply, "add", &rel, String::new()))
            }
            "scene_add_many" => {
                let rel = scene_arg(&root, args)?;
                let Some(Json::Arr(adds)) = args.get("adds") else { return Err("scene_add_many needs adds".into()) };
                let reqs = adds.iter().map(add_request).collect::<Result<Vec<_>, _>>()?;
                self.batch(&root, &rel, reqs)
            }
            "scene_set" => {
                let rel = scene_arg(&root, args)?;
                let node = need_str(args, "node")?;
                let fields = pairs(args.get("fields"))?;
                let save = args.get("save").and_then(as_bool).unwrap_or(false);
                let mut reply = if fields.is_empty() {
                    if !save { return Err("scene_set needs fields (or save: true)".into()); }
                    Reply { ok: true, lines: Vec::new() }
                } else {
                    let mut req = vec!["set".to_string(), node];
                    req.extend(fields);
                    self.scene_request(&root, &rel, &req)?
                };
                if save && reply.ok && self.live(&root) {
                    let saved = self.live_request(&root, &rel, &["save".into()])?;
                    reply.lines.extend(saved.lines);
                }
                Ok(answer_from(&reply, "set", &rel, String::new()))
            }
            "scene_delete" => {
                let rel = scene_arg(&root, args)?;
                let mut req = vec!["delete".to_string()];
                match args.get("nodes") {
                    Some(Json::Arr(items)) => {
                        for item in items { req.push(value_str(item)?); }
                    }
                    _ => req.push(need_str(args, "node").map_err(|_| "scene_delete needs node or nodes".to_string())?),
                }
                let reply = self.scene_request(&root, &rel, &req)?;
                Ok(answer_from(&reply, "delete", &rel, String::new()))
            }
            "place" => {
                let rel = scene_arg(&root, args)?;
                let op = need_str(args, "op")?;
                let mut req = vec![op.clone()];
                match op.as_str() {
                    "ground" => {
                        req.push(need_str(args, "node")?);
                        if let Some(on) = str_arg(args, "target") { req.push(format!("on={on}")); }
                    }
                    "ontop" => {
                        req.push(need_str(args, "node")?);
                        req.push(need_str(args, "target")?);
                    }
                    "scatter" => {
                        req.push(need_str(args, "what")?);
                        for key in ["area", "count", "seed", "scale", "yaw", "spacing", "parent", "name", "ground"] {
                            if let Some(v) = args.get(key) {
                                if !matches!(v, Json::Null) { req.push(format!("{key}={}", value_str(v)?)); }
                            }
                        }
                    }
                    "grid" => return self.batch(&root, &rel, grid_requests(args)?),
                    other => return Err(format!("op `{other}`: expected ground, ontop, scatter, or grid")),
                }
                let reply = self.scene_request(&root, &rel, &req)?;
                Ok(answer_from(&reply, &op, &rel, String::new()))
            }
            "undo" => {
                let rel = scene_arg(&root, args)?;
                if self.live(&root) {
                    return Err("the editor has this project open; ask the person to press Ctrl+Z there".into());
                }
                Ok(Answer::text(scene::undo(&root, &rel)?))
            }
            "render" => self.render(&root, args),
            "check" => check(&root, args),
            "play" => play(&root, args),
            "console" => console(&root, args, self.live(&root)),
            "import" => self.import(&root, args),
            "asset_search" => asset_search(&root, args),
            "asset_online_search" => online_search(args),
            "asset_fetch" => self.fetch(&root, args),
            "gen" => {
                let force = args.get("force").and_then(as_bool).unwrap_or(false);
                let report = crate::gen::run_all(&root, force);
                let mut text = format!("{} generator script(s) in assets/gen/.", report.scripts);
                for m in &report.made { text += &format!("\nmade {m}"); }
                for f in &report.failed { text += &format!("\nfailed: {f}"); }
                if report.made.is_empty() && report.failed.is_empty() && report.scripts > 0 { text += " All up to date."; }
                Ok(Answer { content: vec![text_block(text)], structured: None, error: !report.failed.is_empty() })
            }
            _ => Err(format!("unknown tool `{name}`")),
        }
    }

    /// Run `reqs` as one batch: one file write, or one undo step in the editor; all or nothing.
    fn batch(&mut self, root: &Path, rel: &str, reqs: Vec<Vec<String>>) -> Result<Answer, String> {
        if reqs.is_empty() { return Err("nothing to add".into()); }
        let count = reqs.len();
        let args = reqs.join(&scene::BATCH.to_string());
        let reply = self.scene_request(root, rel, &args)?;
        let head = if reply.ok { format!("added {count} objects in one step\n") } else { "nothing was added\n".into() };
        Ok(answer_from(&reply, "add", rel, head))
    }

    /// Whether `j3c editor` has this project open: it holds `target/j3-editor/lock`.
    fn live(&self, root: &Path) -> bool {
        let path = root.join("target/j3-editor/lock");
        let Ok(file) = fs::OpenOptions::new().write(true).open(path) else { return false };
        matches!(file.try_lock(), Err(fs::TryLockError::WouldBlock))
    }

    /// Run a `tool.j3` request on the live editor when it has `rel` open, else on the file.
    fn scene_request(&mut self, root: &Path, rel: &str, args: &[String]) -> Result<Reply, String> {
        self.was_live = false;
        if self.live(root) {
            let reply = self.live_request(root, rel, args)?;
            if !reply.lines.iter().any(|l| l[0] == "defer") {
                self.was_live = true;
                return Ok(reply);
            }
        }
        scene::run_tool(root, rel, args)
    }

    /// Send one command to the open editor and wait for its reply.
    fn live_request(&mut self, root: &Path, rel: &str, args: &[String]) -> Result<Reply, String> {
        let request = scene::request_text(args)?;
        self.next += 1;
        let id = format!("mcp{}-{}", std::process::id(), self.next);
        let replies = root.join("target/j3-editor/replies");
        let start = fs::metadata(&replies).map(|m| m.len()).unwrap_or(0);
        crate::editor::append_event(root, &format!("cmd\t{id}\t{rel}\t{request}\n"));
        let prefix = format!("{id}\t");
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            std::thread::sleep(Duration::from_millis(20));
            let mut text = String::new();
            if let Ok(mut f) = fs::File::open(&replies) {
                let mut all = Vec::new();
                let _ = f.read_to_end(&mut all);
                // A new editor session empties the file; then read it from the start.
                let from = if (all.len() as u64) < start { 0 } else { start as usize };
                text = String::from_utf8_lossy(&all[from..]).into_owned();
            }
            let mine: Vec<&str> = text.lines().filter_map(|l| l.strip_prefix(&prefix)).collect();
            if mine.last() == Some(&"end") {
                let body: String = mine[..mine.len() - 1].iter().map(|l| format!("{l}\n")).collect();
                return Ok(Reply::parse(&body, true));
            }
            if Instant::now() > deadline {
                // Don't fall back to the file: the editor may still apply it, or overwrite it on save.
                return Err("the open editor did not answer within 10 seconds (a modal dialog may be up); nothing was changed on disk".into());
            }
        }
    }

    fn render(&mut self, root: &Path, args: &Json) -> Result<Answer, String> {
        let rel = scene_arg(root, args)?;
        // Stale generated meshes would show the old shape.
        let failed = crate::gen::run_all(root, false).failed;
        if !failed.is_empty() { return Err(format!("a mesh generator failed:\n{}", failed.join("\n"))); }
        let mut opts = Vec::new();
        for key in ["camera", "view", "focus", "select"] {
            if let Some(v) = str_arg(args, key) { opts.push(format!("{key}={v}")); }
        }
        let w = args.get("width").and_then(Json::as_i64).unwrap_or(640);
        let h = args.get("height").and_then(Json::as_i64).unwrap_or(360);
        if !(16..=2048).contains(&w) || !(16..=2048).contains(&h) {
            return Err("width and height must be 16 to 2048".into());
        }
        opts.push(format!("size={w}x{h}"));
        let mut done = None;
        if self.live(root) {
            self.next += 1;
            let file = format!("target/j3-editor/render-{}.png", self.next);
            let mut req = vec!["render".to_string()];
            req.extend(opts.iter().cloned());
            req.push(format!("out={file}"));
            let reply = self.live_request(root, &rel, &req)?;
            if let Some(e) = reply.error() { return Err(e); }
            if let Some(l) = reply.lines.iter().find(|l| l[0] == "ok") {
                done = Some((l.get(1).cloned().unwrap_or(file), l.get(2).cloned().unwrap_or_default(), true));
            }
        }
        let (file, label, temp) = match done {
            Some(d) => d,
            None => {
                opts.push("out=target/j3-editor/render.png".into());
                let (file, label) = scene::render(root, &rel, &opts)?;
                (file, label, false)
            }
        };
        let path = root.join(&file);
        let png = fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        if temp { let _ = fs::remove_file(&path); }
        let png = crate::png::compact(&png).unwrap_or(png);
        Ok(Answer {
            content: vec![
                Json::obj(vec![("type", s("image")), ("data", Json::Str(crate::png::base64(&png))), ("mimeType", s("image/png"))]),
                text_block(format!("{rel}, {label}, {w}x{h}{}", if temp { ", live from the editor (unsaved edits included)" } else { "" })),
            ],
            structured: None,
            error: false,
        })
    }

    fn import(&mut self, root: &Path, args: &Json) -> Result<Answer, String> {
        let sources = import_sources(root, args)?;
        let bake = args.get("bake").and_then(as_bool).unwrap_or(false);
        let again = args.get("again").and_then(as_bool).unwrap_or(false);
        let dir = root.join("target/j3-editor");
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let mut lines = Vec::new();
        let mut kept = Vec::new();
        let mut failed = false;
        for (i, source) in sources.iter().enumerate() {
            let shown = source.strip_prefix(root).unwrap_or(source).display().to_string();
            let fragment = dir.join(format!("mcp-import-{}-{i}.j3scene", std::process::id()));
            let raw = source.to_string_lossy();
            // ponytail: matched by file name only; a different kit's tree.glb needs again:true.
            let existing = format!("assets/imported/{}.j3scene", crate::editor::import_name(&raw));
            if !again && root.join(&existing).is_file() {
                lines.push(format!("skipped {shown}: already imported as {existing} (again:true imports a copy)"));
                kept.push(existing);
                continue;
            }
            match crate::editor::import_asset(root, &fragment, &raw, bake)
                .and_then(|w| crate::editor::keep_import(root, &fragment, &raw).map(|f| (w, f)))
            {
                Ok((warnings, file)) => {
                    lines.push(format!("imported {shown} as {file} (a scene you can place)"));
                    lines.extend(warnings.iter().map(|w| format!("  warning: {w}")));
                    kept.push(file);
                }
                Err(e) => {
                    let _ = fs::remove_file(&fragment);
                    lines.push(format!("failed {shown}: {e}"));
                    failed = true;
                }
            }
        }
        let _ = crate::editor::generate_assets_catalog(root);
        let mut text = lines.join("\n");
        let place = args.get("place").and_then(as_bool).unwrap_or(true);
        if let ([file], 1, true) = (&kept[..], sources.len(), place) {
            let rel = scene_arg(root, args)?;
            let mut req = vec!["add".to_string(), "instance".to_string(), format!("scene={file}")];
            for key in ["name", "parent"] {
                if let Some(v) = str_arg(args, key) { req.push(format!("{key}={v}")); }
            }
            if let Some(at) = args.get("pos") { req.push(format!("pos={}", value_str(at)?)); }
            let reply = self.scene_request(root, &rel, &req)?;
            if let Some(e) = reply.error() { return Ok(Answer::fail(format!("{text}\nplacing it failed: {e}"))); }
            text += &format!("\n{}", scene::to_text(&reply).trim_end());
        }
        Ok(if failed { Answer::fail(text) } else { Answer::text(text) })
    }
}

impl Server {
    fn fetch(&mut self, root: &Path, args: &Json) -> Result<Answer, String> {
        use crate::online::Fetched;
        let (source, id) = (need_str(args, "source")?, need_str(args, "id")?);
        let file = str_arg(args, "file");
        let credited = "credited in assets/CREDITS.txt";
        match crate::online::fetch(root, &source, &id, file.as_deref())? {
            Fetched::Model(path) => {
                let mut args = args.clone();
                if let Json::Obj(pairs) = &mut args {
                    pairs.retain(|(k, _)| k != "paths");
                    pairs.push(("path".into(), Json::Str(path.to_string_lossy().into_owned())));
                }
                let mut answer = self.import(root, &args)?;
                answer.content.push(text_block(credited));
                Ok(answer)
            }
            Fetched::Material(rel) => Ok(Answer::text(format!("made {rel} (set material={rel} on a mesh); {credited}"))),
            Fetched::Sky(rel) => Ok(Answer::text(format!("made {rel}; add a builtin:sky mesh and set texture={rel}; {credited}"))),
            Fetched::Pack(files) => {
                let mut text = format!("{source}:{id} has {} models:\n", files.len());
                for f in files.iter().take(200) {
                    text += &format!("{}\n", f.file_name().unwrap_or_default().to_string_lossy());
                }
                if files.len() > 200 { text += "...\n"; }
                text += "Call asset_fetch again with file=<name> to import one, or import with paths from the structured result (a folder or wildcard works too).";
                let paths = files.iter().map(|f| Json::Str(f.to_string_lossy().into_owned())).collect();
                Ok(Answer { content: vec![text_block(text)], structured: Some(Json::obj(vec![("files", Json::Arr(paths))])), error: false })
            }
        }
    }
}

fn online_search(args: &Json) -> Result<Answer, String> {
    let source = str_arg(args, "source").unwrap_or_else(|| "any".into());
    let kind = str_arg(args, "kind").unwrap_or_else(|| "any".into());
    let hits = crate::online::search(&source, &str_arg(args, "query").unwrap_or_default(), &kind)?;
    let mut text = format!("{} results (all CC0; asset_fetch with source and id gets one)\n", hits.len());
    for h in &hits { text += &format!("{}  {}  {}  {}  ({})\n", h.source, h.id, h.kind, h.name, h.author); }
    let items = hits.iter().map(|h| {
        Json::obj(vec![
            ("source", s(h.source)), ("id", s(&h.id)), ("kind", s(h.kind)),
            ("name", s(&h.name)), ("author", s(&h.author)), ("thumb", s(&h.thumb)),
        ])
    });
    Ok(Answer { content: vec![text_block(text.trim_end().to_string())], structured: Some(Json::obj(vec![("assets", Json::Arr(items.collect()))])), error: false })
}

fn has_jpeg(dir: &Path) -> bool {
    fs::read_dir(dir).into_iter().flatten().flatten().any(|e| {
        let p = e.path();
        // file_type doesn't follow symlinks, so a link loop can't recurse forever.
        if e.file_type().is_ok_and(|t| t.is_dir()) { return has_jpeg(&p); }
        matches!(p.extension().and_then(|x| x.to_str()).map(str::to_ascii_lowercase).as_deref(), Some("jpg" | "jpeg"))
    })
}

/// The model files `path`/`paths` name: a file, a folder (its models, not recursive), or a
/// file name with `*` wildcards.
fn import_sources(root: &Path, args: &Json) -> Result<Vec<PathBuf>, String> {
    let mut given: Vec<String> = str_arg(args, "path").into_iter().collect();
    if let Some(Json::Arr(xs)) = args.get("paths") {
        given.extend(xs.iter().filter_map(|x| x.as_str().map(str::to_string)));
    }
    let is_model = |p: &Path| matches!(p.extension().and_then(|e| e.to_str()), Some("gltf" | "glb" | "j3mesh"));
    let mut out = Vec::new();
    for g in given {
        let p = if Path::new(&g).is_absolute() { PathBuf::from(&g) } else { root.join(&g) };
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        let listed = |dir: &Path, keep: &dyn Fn(&Path) -> bool| -> Vec<PathBuf> {
            let mut v: Vec<PathBuf> = fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| keep(p)).collect();
            v.sort();
            v
        };
        if p.is_dir() {
            out.extend(listed(&p, &|f| is_model(f)));
        } else if name.contains('*') {
            let dir = p.parent().unwrap_or(root);
            out.extend(listed(dir, &|f| is_model(f) && f.file_name().and_then(|n| n.to_str()).is_some_and(|n| wild(&name, n))));
        } else {
            out.push(p);
        }
    }
    if out.is_empty() { return Err("no .gltf, .glb, or .j3mesh files matched".into()); }
    if out.len() > 500 { return Err(format!("{} files matched; import at most 500 per call", out.len())); }
    Ok(out)
}

/// `name` matches `pat`, where `*` is any run of characters.
fn wild(pat: &str, name: &str) -> bool {
    match pat.split_once('*') {
        None => pat == name,
        Some((head, rest)) => {
            let Some(tail) = name.strip_prefix(head) else { return false };
            (0..=tail.len()).any(|i| tail.is_char_boundary(i) && wild(rest, &tail[i..]))
        }
    }
}

fn answer_from(reply: &Reply, op: &str, rel: &str, head: String) -> Answer {
    let body = scene::to_text(reply);
    Answer {
        content: vec![text_block(format!("{head}{}", body.trim_end()))],
        structured: Some(scene::to_json(op, rel, reply)),
        error: !reply.ok,
    }
}

fn as_bool(v: &Json) -> Option<bool> {
    match v {
        Json::Bool(b) => Some(*b),
        _ => None,
    }
}

fn str_arg(args: &Json, key: &str) -> Option<String> {
    match args.get(key) {
        None | Some(Json::Null) => None,
        Some(v) => value_str(v).ok().filter(|s| !s.is_empty()),
    }
}

fn need_str(args: &Json, key: &str) -> Result<String, String> {
    str_arg(args, key).ok_or_else(|| format!("missing `{key}`"))
}

/// A JSON value as a request value: numbers plainly, arrays as `x,y,z`.
fn value_str(v: &Json) -> Result<String, String> {
    Ok(match v {
        Json::Str(s) => s.clone(),
        Json::Num(n) => number(*n),
        Json::Bool(b) => b.to_string(),
        Json::Null => String::new(),
        Json::Arr(items) => items.iter().map(value_str).collect::<Result<Vec<_>, _>>()?.join(","),
        Json::Obj(_) => return Err("expected a value, not an object".into()),
    })
}

fn number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 { format!("{}", n as i64) } else { format!("{n}") }
}

/// `fields` as `key=value` request arguments.
fn pairs(fields: Option<&Json>) -> Result<Vec<String>, String> {
    match fields {
        None | Some(Json::Null) => Ok(Vec::new()),
        Some(Json::Obj(items)) => items.iter().map(|(k, v)| Ok(format!("{k}={}", value_str(v)?))).collect(),
        Some(_) => Err("fields must be an object of key: value".into()),
    }
}

/// The `add` request for one `scene_add` argument object.
fn add_request(args: &Json) -> Result<Vec<String>, String> {
    let mut req = vec!["add".to_string(), need_str(args, "kind")?];
    for key in ["name", "parent"] {
        if let Some(v) = str_arg(args, key) { req.push(format!("{key}={v}")); }
    }
    req.extend(pairs(args.get("fields"))?);
    Ok(req)
}

/// `place` op `grid`: one `add` per piece at `[x*cell, floor*height, z*cell]`, yaw `turn*90`.
// ponytail: sugar over `add` only (no bounds or snapping), so it never disagrees with tool.j3.
fn grid_requests(args: &Json) -> Result<Vec<Vec<String>>, String> {
    let num = |v: Option<&Json>, d: f64| v.and_then(Json::as_f64).unwrap_or(d);
    let cell = num(args.get("cell"), 1.0);
    let height = num(args.get("height"), cell);
    let parent = str_arg(args, "parent");
    let Some(Json::Arr(pieces)) = args.get("pieces") else { return Err("grid needs pieces".into()) };
    if pieces.len() > 2000 { return Err("grid takes at most 2000 pieces".into()); }
    let mut reqs = Vec::new();
    for p in pieces {
        let what = need_str(p, "what")?;
        let turn = num(p.get("turn"), 0.0);
        if turn.fract() != 0.0 { return Err(format!("turn {turn} is not a whole number of quarter turns")); }
        let mut req = vec!["add".to_string()];
        if what.ends_with(".j3scene") {
            req.extend(["instance".into(), format!("scene={what}")]);
        } else if what.ends_with(".j3mesh") || what.starts_with("builtin:") {
            req.extend(["mesh".into(), format!("asset={what}")]);
        } else if SHAPES.contains(&what.as_str()) {
            req.push(what);
        } else {
            return Err(format!("grid piece `{what}`: expected a .j3scene, .j3mesh, builtin: asset, or shape"));
        }
        let (x, z, floor) = (num(p.get("x"), 0.0), num(p.get("z"), 0.0), num(p.get("floor"), 0.0));
        req.push(format!("pos={},{},{}", number(x * cell), number(floor * height), number(z * cell)));
        req.push(format!("rot=0,{},0", number(turn * 90.0)));
        if let Some(n) = str_arg(p, "name") { req.push(format!("name={n}")); }
        if let Some(v) = &parent { req.push(format!("parent={v}")); }
        reqs.push(req);
    }
    Ok(reqs)
}

const SHAPES: &[&str] = &["cube", "sphere", "cylinder", "cone", "capsule", "torus", "quad", "plane", "sky"];

fn scene_arg(root: &Path, args: &Json) -> Result<String, String> {
    scene::scene_rel(root, str_arg(args, "scene").as_deref())
}

/// Scene problems and code problems together.
fn check(root: &Path, args: &Json) -> Result<Answer, String> {
    let scenes = match str_arg(args, "scene") {
        Some(s) => vec![scene::scene_rel(root, Some(&s))?],
        None => scene::all_scenes(root).unwrap_or_default(),
    };
    let mut lines = Vec::new();
    let mut issues = Vec::new();
    let mut errors: i64 = 0;
    let placed = scene::placed_scenes(root);
    for rel in &scenes {
        let reply = scene::run_tool(root, rel, &scene::check_args(root, rel, &placed))?;
        if let Some(e) = reply.error() {
            lines.push(format!("{rel}: error: {e}"));
            errors += 1;
        }
        for l in reply.lines.iter().filter(|l| l[0] == "issue") {
            let severity = l.get(1).cloned().unwrap_or_default();
            if severity == "error" { errors += 1; }
            let message = l[3..].join(" ");
            lines.push(format!("{rel}: {severity}: {message}"));
            issues.push(Json::obj(vec![
                ("severity", s(&severity)),
                ("file", s(rel)),
                ("node", Json::Num(l.get(2).and_then(|n| n.parse().ok()).unwrap_or(0.0))),
                ("message", Json::Str(message)),
            ]));
        }
    }
    if args.get("code").and_then(as_bool).unwrap_or(true) {
        let host = crate::editor::host_exe()?;
        let out = Command::new(host)
            .arg("check")
            .arg("--json")
            .arg(root)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("j3c check: {e}"))?;
        let text = String::from_utf8_lossy(&out.stdout);
        match text.lines().rev().find_map(|l| json::parse(l).ok()) {
            Some(result) if result.get("ok").and_then(as_bool) == Some(true) => {}
            Some(result) => {
                errors += 1;
                let message = result.get("message").and_then(Json::as_str).unwrap_or("does not compile").to_string();
                let file = result.get("path").and_then(Json::as_str).unwrap_or("").to_string();
                let line = result.get("line").and_then(Json::as_i64).unwrap_or(0);
                lines.push(format!("{file}:{line}: error: {message}"));
                if let Some(Json::Arr(notes)) = result.get("notes") {
                    for n in notes.iter().filter_map(Json::as_str) { lines.push(format!("  note: {n}")); }
                }
                if let Some(Json::Arr(helps)) = result.get("helps") {
                    for h in helps.iter().filter_map(Json::as_str) { lines.push(format!("  help: {h}")); }
                }
                issues.push(Json::obj(vec![
                    ("severity", s("error")),
                    ("file", Json::Str(file)),
                    ("line", Json::Num(line as f64)),
                    ("message", Json::Str(message)),
                ]));
            }
            None => {
                errors += 1;
                lines.push(format!("code: j3c check failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
            }
        }
    }
    if !crate::emit::host_lib_present("SDL2_image", &[]) && has_jpeg(&root.join("assets")) {
        let message = "SDL2_image is not installed, so JPEG textures show white (PNG works)";
        lines.push(format!("warning: {message}"));
        issues.push(Json::obj(vec![("severity", s("warning")), ("file", s("assets")), ("message", s(message))]));
    }
    let warnings = issues.len() as i64 - errors;
    let head = if lines.is_empty() {
        format!("No problems in {} scene(s) or the code.", scenes.len())
    } else {
        format!("{errors} error(s), {} warning(s):", warnings.max(0))
    };
    Ok(Answer {
        content: vec![text_block(std::iter::once(head).chain(lines).collect::<Vec<_>>().join("\n"))],
        structured: Some(Json::obj(vec![("ok", Json::Bool(errors == 0)), ("issues", Json::Arr(issues))])),
        error: false,
    })
}

/// Build, run the game headless for some frames, and return its last frame and output.
fn play(root: &Path, args: &Json) -> Result<Answer, String> {
    let rel = scene_arg(root, args)?;
    let frames = args.get("frames").and_then(Json::as_i64).unwrap_or(120).clamp(1, 36000);
    let timeout = args.get("timeout").and_then(Json::as_i64).unwrap_or(60).clamp(5, 600) as u64;
    let host = crate::editor::host_exe()?;
    let dir = root.join("target/j3-editor");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let started = Instant::now();
    let build = Command::new(&host).arg("build").arg(root).stdin(Stdio::null()).output().map_err(|e| format!("build: {e}"))?;
    let build_log = format!("{}{}", String::from_utf8_lossy(&build.stdout), String::from_utf8_lossy(&build.stderr));
    let _ = fs::write(dir.join("build.log"), &build_log);
    if !build.status.success() {
        let errors: Vec<&str> = build_log.lines().filter(|l| !l.trim().is_empty()).take(60).collect();
        return Ok(Answer::fail(format!("Build failed (full log in target/j3-editor/build.log):\n{}", errors.join("\n"))));
    }
    let exe = crate::project::Manifest::load(&root.join("j3.toml"))?.out_path();
    let exe = if exe.is_absolute() { exe } else { root.join(exe) };
    let shot = dir.join("play.png");
    let _ = fs::remove_file(&shot);
    let mut child = Command::new(&exe)
        .current_dir(root)
        .env("J3_ROOT", root)
        .env("J3_SCENE", &rel)
        .env("J3_FRAMES", frames.to_string())
        .env("J3_SCREENSHOT", &shot)
        .env("SDL_VIDEODRIVER", "offscreen")
        .env("SDL_AUDIODRIVER", "dummy")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("run {}: {e}", exe.display()))?;
    let readers: Vec<_> = [child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>), child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>)]
        .into_iter()
        .flatten()
        .map(|mut pipe| {
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                let _ = pipe.read_to_end(&mut buf);
                String::from_utf8_lossy(&buf).into_owned()
            })
        })
        .collect();
    let deadline = Instant::now() + Duration::from_secs(timeout);
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? { break Some(status); }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let output: String = readers.into_iter().filter_map(|r| r.join().ok()).collect();
    let _ = fs::write(dir.join("play.log"), &output);
    let tail: Vec<&str> = output.lines().rev().take(60).collect::<Vec<_>>().into_iter().rev().collect();
    let verdict = match status {
        None => format!("The game was still running after {timeout}s (frames: {frames}); stopped it. Does the loop call game.present and stop on game.quitRequested()?"),
        Some(s) if s.success() => format!("Ran {frames} frames of {rel} in {:.1}s.", started.elapsed().as_secs_f32()),
        Some(s) => format!("The game crashed ({s})."),
    };
    let mut text = verdict;
    if !tail.is_empty() {
        text += &format!("\nOutput (last {} lines; all in target/j3-editor/play.log):\n{}", tail.len(), tail.join("\n"));
    }
    let mut content = Vec::new();
    match fs::read(&shot) {
        Ok(png) => {
            let png = crate::png::compact(&png).unwrap_or(png);
            content.push(Json::obj(vec![("type", s("image")), ("data", Json::Str(crate::png::base64(&png))), ("mimeType", s("image/png"))]));
        }
        Err(_) => text += "\nNo screenshot was taken.",
    }
    content.push(text_block(text));
    Ok(Answer { content, structured: None, error: !matches!(status, Some(s) if s.success()) })
}

/// Recent game and build output: the editor's console while it is open, else the last `play`.
fn console(root: &Path, args: &Json, live: bool) -> Result<Answer, String> {
    let want = args.get("lines").and_then(Json::as_i64).unwrap_or(50).clamp(1, 500) as usize;
    let played = root.join("target/j3-editor/play.log").exists();
    let lines: Vec<String> = if live {
        let text = fs::read_to_string(root.join("target/j3-editor/events")).unwrap_or_default();
        text.lines()
            .filter_map(|l| {
                let mut parts = l.splitn(3, '\t');
                let kind = parts.next()?;
                let _scene = parts.next()?;
                let message = parts.next().unwrap_or("");
                matches!(kind, "out" | "err" | "build" | "stopped" | "run").then(|| format!("[{kind}] {message}"))
            })
            .collect()
    } else {
        fs::read_to_string(root.join("target/j3-editor/play.log")).unwrap_or_default().lines().map(str::to_string).collect()
    };
    let from = lines.len().saturating_sub(want);
    if lines.is_empty() {
        return Ok(Answer::text(if live {
            "The editor's console has no game output yet."
        } else if played {
            "The last play printed nothing."
        } else {
            "No output yet; run play first."
        }));
    }
    Ok(Answer::text(format!(
        "{} (last {} of {} lines):\n{}",
        if live { "Editor console" } else { "Last play" },
        lines.len() - from,
        lines.len(),
        lines[from..].join("\n")
    )))
}

const BUILTINS: &[(&str, &str)] = &[
    ("builtin:cube", "unit cube"),
    ("builtin:sphere", "unit sphere"),
    ("builtin:quad", "1x1 upright quad"),
    ("builtin:plane", "flat 1x1 ground plane (scale it)"),
    ("builtin:sky", "sky sphere with generated clouds"),
    ("builtin:cylinder", "unit cylinder"),
    ("builtin:cone", "unit cone"),
    ("builtin:capsule", "capsule, 1 tall and 0.5 wide"),
    ("builtin:torus", "flat ring, 1 wide"),
];

fn asset_kind(path: &str) -> &'static str {
    match Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("") {
        "j3mesh" => "mesh",
        "gltf" | "glb" => "model",
        "j3mat" => "material",
        "png" | "jpg" | "jpeg" => "image",
        "j3scene" => "scene",
        "wav" | "ogg" | "mp3" => "sound",
        "ttf" | "otf" => "font",
        "j3" => "code",
        "j3s" => "script",
        _ => "file",
    }
}

fn asset_search(root: &Path, args: &Json) -> Result<Answer, String> {
    let query = str_arg(args, "query").unwrap_or_default().to_lowercase();
    let kind = str_arg(args, "kind").unwrap_or_else(|| "any".into());
    let mut found: Vec<(String, &str, String)> = Vec::new();
    for (path, note) in BUILTINS {
        found.push((path.to_string(), "mesh", note.to_string()));
    }
    let mut stack = vec![root.join("assets"), root.join("scenes")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let rel = path.strip_prefix(root).map_err(|e| e.to_string())?.to_string_lossy().replace('\\', "/");
            let k = asset_kind(&rel);
            let note = if k == "model" { "import it first (import tool)".to_string() } else { String::new() };
            found.push((rel, k, note));
        }
    }
    for script in crate::compile::script_paths(root) {
        if !script.starts_with("assets/") && !script.starts_with("scenes/") {
            found.push((script, "script", "set script=<path> on an object".into()));
        }
    }
    found.retain(|(p, k, n)| {
        (kind == "any" || *k == kind) && (query.is_empty() || p.to_lowercase().contains(&query) || n.to_lowercase().contains(&query))
    });
    found.sort_by(|a, b| a.1.cmp(b.1).then(a.0.cmp(&b.0)));
    let total = found.len();
    found.truncate(200);
    let mut text = format!("{total} match(es){}\n", if total > 200 { ", first 200" } else { "" });
    for (p, k, n) in &found {
        text += &format!("{k:<9} {p}{}\n", if n.is_empty() { String::new() } else { format!("  ({n})") });
    }
    let items = found
        .iter()
        .map(|(p, k, n)| Json::obj(vec![("path", s(p)), ("kind", s(k)), ("note", s(n))]))
        .collect();
    Ok(Answer {
        content: vec![text_block(text.trim_end().to_string())],
        structured: Some(Json::obj(vec![("assets", Json::Arr(items))])),
        error: false,
    })
}

/// Tools that change a scene; their replies say whether the change went to the editor or the file.
const EDITS: &[&str] = &["scene_add", "scene_add_many", "scene_set", "scene_delete", "place"];

/// The CLI command doing each tool's work; "" when only MCP has it.
const CLI_SAME: &[(&str, &str)] = &[
    ("scene_list", "j3c -e scene list"),
    ("scene_get", "j3c -e scene get <object>|@scene"),
    ("scene_add", "j3c -e scene add <kind> key=value ..."),
    ("scene_add_many", ""),
    ("scene_set", "j3c -e scene set <object> key=value ..."),
    ("scene_delete", "j3c -e scene delete <object> ..."),
    ("place", "j3c -e scene ground|ontop|scatter ... (grid is MCP only)"),
    ("undo", "j3c -e scene undo"),
    ("render", "j3c -e render"),
    ("check", "j3c -e scene check --all, then j3c check"),
    ("play", "j3c run --frames N --screenshot F.png --headless"),
    ("console", ""),
    ("import", ""),
    ("asset_search", ""),
    ("asset_online_search", ""),
    ("asset_fetch", ""),
    ("gen", "j3c -e gen [--force]"),
];

/// `j3c -e mcp --tools`: every tool's arguments, from the same definitions as `tools/list`.
fn tool_reference() -> String {
    let mut out = String::from("MCP tools (`j3c -e mcp`), from the definitions `tools/list` sends. * marks a required argument.\n");
    let Json::Arr(list) = tools() else { return out };
    for tool in &list {
        let name = tool.get("name").and_then(Json::as_str).unwrap_or("");
        let title = tool.get("title").and_then(Json::as_str).unwrap_or("");
        out.push_str(&format!("\n{name} — {title}\n"));
        let cli = CLI_SAME.iter().find(|(n, _)| *n == name).map(|(_, c)| *c).unwrap_or("");
        out.push_str(&format!("    CLI: {}\n", if cli.is_empty() { "none; MCP only" } else { cli }));
        if let Some(d) = tool.get("description").and_then(Json::as_str) {
            out.push_str(&format!("    {d}\n"));
        }
        if let Some(schema) = tool.get("inputSchema") {
            push_args(&mut out, schema, "    ");
        }
    }
    out
}

fn push_args(out: &mut String, schema: &Json, indent: &str) {
    let Some(Json::Obj(props)) = schema.get("properties") else { return };
    let required: Vec<&str> = match schema.get("required") {
        Some(Json::Arr(r)) => r.iter().filter_map(Json::as_str).collect(),
        _ => Vec::new(),
    };
    for (key, prop) in props {
        let mut line = format!("{indent}{key}{}: {}", if required.contains(&key.as_str()) { "*" } else { "" }, type_text(prop));
        if let Some(Json::Arr(e)) = prop.get("enum") {
            let names: Vec<&str> = e.iter().filter_map(Json::as_str).collect();
            line.push_str(&format!(" ({})", names.join("|")));
        }
        if let Some(d) = prop.get("default") {
            line.push_str(&format!(" = {}", d.stringify()));
        }
        if let Some(d) = prop.get("description").and_then(Json::as_str) {
            line.push_str(&format!(" — {d}"));
        }
        out.push_str(&line);
        out.push('\n');
        let item = prop.get("items").unwrap_or(prop);
        if item.get("properties").is_some() {
            push_args(out, item, &format!("{indent}    "));
        }
    }
}

fn type_text(prop: &Json) -> String {
    let ty = prop.get("type").and_then(Json::as_str).unwrap_or("any");
    match (ty, prop.get("items").and_then(|i| i.get("type")).and_then(Json::as_str)) {
        ("array", Some(inner)) => format!("array of {inner}"),
        _ => ty.to_string(),
    }
}

const TOOL_NAMES: &[&str] = &[
    "scene_list", "scene_get", "scene_add", "scene_add_many", "scene_set", "scene_delete", "place", "undo", "render", "check", "play", "console", "import",
    "asset_search", "asset_online_search", "asset_fetch", "gen",
];

/// Tool definitions, as JSON text so they read like the schema they are.
fn tools() -> Json {
    json::parse(TOOLS).expect("tool definitions are valid JSON")
}

const TOOLS: &str = r#"[
{"name":"scene_list","title":"List scene objects",
 "description":"List every object in a scene: #id, Parent/Child path, kind (group, mesh, camera, light, instance), asset, world position, rotation (degrees), scale, script. Objects placed by an instance (another scene file) are not listed; edit that scene instead.",
 "inputSchema":{"type":"object","properties":{
   "scene":{"type":"string","description":"Scene file, e.g. scenes/main.j3scene. Default: the scene in j3.editor.toml."},
   "filter":{"type":"string","description":"Only objects whose id, path, or kind contains this text."}}},
 "annotations":{"readOnlyHint":true}},
{"name":"scene_get","title":"Get an object",
 "description":"All fields of one object, including its script's props (param.<prop>). Pass node \"@scene\" for scene lighting (sun, sky, fog, shadows, URP).",
 "inputSchema":{"type":"object","required":["node"],"properties":{
   "node":{"type":"string","description":"Name, Parent/Child path, #id, or @scene."},
   "scene":{"type":"string"}}},
 "annotations":{"readOnlyHint":true}},
{"name":"scene_add","title":"Add an object",
 "description":"Add an object and return its #id and path. Shapes (cube, sphere, cylinder, cone, capsule, torus, quad, plane, sky) are meshes with that built-in asset. Top-level meshes rest on y=0 unless pos is given; names are made unique. For a mesh file use kind mesh with fields.asset; to place another scene use kind instance with fields.scene.",
 "inputSchema":{"type":"object","required":["kind"],"properties":{
   "kind":{"type":"string","enum":["group","mesh","camera","light","instance","cube","sphere","cylinder","cone","capsule","torus","quad","plane","sky"]},
   "name":{"type":"string"},
   "parent":{"type":"string","description":"Parent object (name, path, or #id); default top level."},
   "fields":{"type":"object","description":"Same keys as scene_set, e.g. {\"pos\":[0,1,0],\"scale\":2,\"material\":\"assets/materials/wood.j3mat\"}."},
   "scene":{"type":"string"}}}},
{"name":"scene_add_many","title":"Add several objects",
 "description":"Add several objects in one step (one file write, or one undo step in the editor). Each item is a scene_add argument object. All or nothing: if one fails, none are added.",
 "inputSchema":{"type":"object","required":["adds"],"properties":{
   "adds":{"type":"array","items":{"type":"object","required":["kind"],"properties":{
     "kind":{"type":"string"},"name":{"type":"string"},"parent":{"type":"string"},"fields":{"type":"object"}}}},
   "scene":{"type":"string"}}}},
{"name":"scene_set","title":"Change an object",
 "description":"Set fields on an object (all or nothing). Keys: name; pos [x,y,z] (local to parent), x, y, z; rot [x,y,z] degrees; scale [x,y,z] or one number; parent (\"\" for top level; keeps local transform); asset (mesh file or builtin:cube|sphere|cylinder|cone|capsule|torus|quad|plane|sky); texture; material (.j3mat); scene (instance's scene file); script (a .j3s path, \"\" for none); param.<prop> (a prop of the object's script, typed by its props block); tint [r,g,b] or one number (meshes and placed scenes; multiplies color, 1 unchanged, above 1 allowed, e.g. [0.7,1,1.8] turns green teal); color [r,g,b] 0-1 and range (lights); fov, near, far (cameras); effects (cameras: kind=camera .j3fx files run in order, \"a.j3fx|b.j3fx\"; \"\" for none) and fx.<file stem>.<param> (their params); alpha opaque|cutout|blend; opacity; doubleSided; prop.<name> (on an instance: a prop its scene declares; \"\" goes back to the default). With node \"@scene\": prop.<name> = \"<type>:<default>\" declares a prop on this scene (type int, float, text, bool, material, mesh, scene; \"\" removes it), and sun [x,y,z] (direction toward the sun), sunColor, sky, fogStart, fogEnd, shadows, shadowSize, urp, urpBounces, urpSamples, urpShadows, urpShadowNear (meters traced sun shadows reach; the shadow map covers the rest; 0 no limit). save: true also saves the scene in the open editor.",
 "inputSchema":{"type":"object","required":["node"],"properties":{
   "node":{"type":"string","description":"Name, Parent/Child path, #id, or @scene."},
   "fields":{"type":"object"},
   "save":{"type":"boolean","description":"With the editor open, save the scene after the change (without it, edits stay unsaved in the editor)."},
   "scene":{"type":"string"}}}},
{"name":"scene_delete","title":"Delete objects",
 "description":"Delete objects and everything under them.",
 "inputSchema":{"type":"object","properties":{
   "node":{"type":"string"},
   "nodes":{"type":"array","items":{"type":"string"}},
   "scene":{"type":"string"}}},
 "annotations":{"destructiveHint":true}},
{"name":"place","title":"Place objects",
 "description":"Placement without coordinates. ground: drop or lift node so its lowest point rests on the surface below it (or on target only; y=0 if nothing is below). ontop: center node over target and rest it on target's top. scatter: add count copies of what (a shape like cube or cylinder, a .j3mesh or builtin: asset, or a .j3scene placed as instances) at random spots in area [x0,z0,x1,z1] (in parent's space) under a new Scatter group, each grounded, with random yaw and a scale between scale[0] and scale[1]; the same seed gives the same layout. grid: add pieces (walls, floors, roof parts) on a grid in one step: each piece's what (a .j3scene, .j3mesh, builtin: asset, or shape) goes at [x*cell, floor*height, z*cell] turned turn*90 degrees about y, under parent.",
 "inputSchema":{"type":"object","required":["op"],"properties":{
   "op":{"type":"string","enum":["ground","ontop","scatter","grid"]},
   "node":{"type":"string","description":"Object to move (ground, ontop)."},
   "target":{"type":"string","description":"ontop: object to put node on. ground: rest only on this object."},
   "what":{"type":"string","description":"scatter: shape, mesh asset, or scene file."},
   "area":{"type":"array","items":{"type":"number"},"minItems":4,"maxItems":4,"description":"scatter: x0, z0, x1, z1."},
   "count":{"type":"integer","default":10},
   "seed":{"type":"integer","default":1},
   "scale":{"type":"array","items":{"type":"number"},"minItems":1,"maxItems":2},
   "yaw":{"type":"boolean","default":true},
   "spacing":{"type":"number","description":"scatter: least distance between copies."},
   "parent":{"type":"string"},
   "name":{"type":"string","description":"scatter: base name of the copies."},
   "ground":{"type":"boolean","default":true},
   "cell":{"type":"number","default":1,"description":"grid: cell size."},
   "height":{"type":"number","description":"grid: floor height (default cell)."},
   "pieces":{"type":"array","maxItems":2000,"items":{"type":"object","required":["what"],"properties":{
     "what":{"type":"string"},"x":{"type":"number"},"z":{"type":"number"},"floor":{"type":"number"},
     "turn":{"type":"integer","description":"Quarter turns about y."},"name":{"type":"string"}}}},
   "scene":{"type":"string"}}}},
{"name":"undo","title":"Undo the last change",
 "description":"Put a scene file back as it was before the last change made through these tools (keeps 20 per scene). Only when the editor is closed; with it open, the person uses Ctrl+Z.",
 "inputSchema":{"type":"object","properties":{"scene":{"type":"string"}}},
 "annotations":{"destructiveHint":true}},
{"name":"render","title":"Render the scene",
 "description":"Draw the scene as the game would (lights, shadows, sky, URP) and return the image, so you can see what you built. Default: through the main camera, or an overview if there is none. view frames all objects from a side; focus frames one object (with view's side); select outlines an object in orange. With the editor open it renders the live, unsaved scene.",
 "inputSchema":{"type":"object","properties":{
   "scene":{"type":"string"},
   "camera":{"type":"string","description":"Camera object to look through."},
   "view":{"type":"string","enum":["iso","front","back","side","top"]},
   "focus":{"type":"string","description":"Object to frame."},
   "select":{"type":"string","description":"Object to outline."},
   "width":{"type":"integer","default":640},
   "height":{"type":"integer","default":360}}},
 "annotations":{"readOnlyHint":true}},
{"name":"check","title":"Check scenes and code",
 "description":"Find problems before playing: broken parents, missing mesh/material/texture/scene files, placement cycles, missing scripts, object fields pointing at deleted objects, same-named siblings, and J3 compile errors (with file and line).",
 "inputSchema":{"type":"object","properties":{
   "scene":{"type":"string","description":"One scene; default every scene under scenes/."},
   "code":{"type":"boolean","default":true,"description":"Also typecheck the project's code."}}},
 "annotations":{"readOnlyHint":true}},
{"name":"play","title":"Play the game",
 "description":"Build the game and run it without a window for some frames (scripts run), then return the last frame as an image with the game's output. For a 3D game the output ends with a J3STATS line: average fps, draw calls, triangles, and CPU/GPU milliseconds. Build errors come back instead of an image.",
 "inputSchema":{"type":"object","properties":{
   "scene":{"type":"string","description":"Scene to start in; default the project's main scene."},
   "frames":{"type":"integer","default":120,"description":"Frames to run (60 per second)."},
   "timeout":{"type":"integer","default":60,"description":"Seconds before the run is stopped."}}}},
{"name":"console","title":"Game output",
 "description":"Recent game and build output: the editor's console while the editor is open, else the last play run.",
 "inputSchema":{"type":"object","properties":{"lines":{"type":"integer","default":50}}},
 "annotations":{"readOnlyHint":true}},
{"name":"import","title":"Import a model",
 "description":"Import .gltf, .glb, or .j3mesh files into assets/ as reusable scenes (assets/imported/<name>.j3scene), one line back per model. path may be a folder (its models, not recursive) or contain * wildcards; paths takes a list. Only a single imported model is placed (unless place is false).",
 "inputSchema":{"type":"object","properties":{
   "path":{"type":"string","description":"Model file, folder, or wildcard like kit/wall*.glb, absolute or relative to the project."},
   "paths":{"type":"array","items":{"type":"string"}},
   "bake":{"type":"boolean","default":false,"description":"Sample each textured triangle's texture at its UV center into vertex colors and drop the texture (for palette kits like Kenney's)."},
   "place":{"type":"boolean","default":true},
   "again":{"type":"boolean","default":false,"description":"Import even when assets/imported/<name>.j3scene exists (makes <name>_2). By default that model is skipped and the existing scene is used."},
   "scene":{"type":"string"},
   "name":{"type":"string"},
   "parent":{"type":"string"},
   "pos":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3}}}},
{"name":"asset_search","title":"Find assets",
 "description":"Find meshes, models, materials, images, scenes, sounds, and scripts in the project, plus the built-in shapes. Paths are what asset, material, texture, scene, and script fields take.",
 "inputSchema":{"type":"object","properties":{
   "query":{"type":"string"},
   "kind":{"type":"string","enum":["any","mesh","model","material","image","scene","sound","font","script","code","file"]}}},
 "annotations":{"readOnlyHint":true}},
{"name":"asset_online_search","title":"Search online assets",
 "description":"Search free CC0 assets online: Poly Haven models and skies, ambientCG materials, and Kenney model packs (a pack is a zip of many low-poly models). One line per result: source, id, kind, name, author. Then asset_fetch with the source and id.",
 "inputSchema":{"type":"object","properties":{
   "query":{"type":"string","description":"Words to match, like chair, brick, or town. Empty lists everything (up to 40)."},
   "source":{"type":"string","enum":["any","polyhaven","ambientcg","kenney"],"default":"any"},
   "kind":{"type":"string","enum":["any","model","material","sky"],"default":"any"}}},
 "annotations":{"readOnlyHint":true}},
{"name":"asset_fetch","title":"Download an online asset",
 "description":"Download an asset asset_online_search found (cached in ~/.cache/j3/assets) and add it to the project, with a line in assets/CREDITS.txt. A Poly Haven model is imported like the import tool (and placed unless place is false). An ambientCG material becomes assets/materials/<id>.j3mat with its color texture. A Poly Haven sky becomes assets/skies/<id>.png for a builtin:sky mesh's texture. A Kenney pack without file lists its models; with file (a name from that list, with or without .glb) it imports that one. Kenney models look best with bake:true.",
 "inputSchema":{"type":"object","required":["source","id"],"properties":{
   "source":{"type":"string","enum":["polyhaven","ambientcg","kenney"]},
   "id":{"type":"string"},
   "file":{"type":"string","description":"Kenney only: the model in the pack to import."},
   "bake":{"type":"boolean","default":false},
   "place":{"type":"boolean","default":true},
   "again":{"type":"boolean","default":false},
   "scene":{"type":"string"},
   "name":{"type":"string"},
   "parent":{"type":"string"},
   "pos":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3}}}},
{"name":"gen","title":"Run mesh generators",
 "description":"Run the project's mesh generator scripts: each assets/gen/<name>.j3 builds a mesh in code with j3.gfx.meshgen (box, cylinder, cone, frustum, sphere, capsule, torus, lathe, extrude, heightField, tri, quad; color, glow; at, turn, size, push, pop) and ends with b.save(meshgen.output(\"assets/gen/<name>.j3mesh\")). Write the script with your file tools, then run this; use the .j3mesh as a mesh asset. Unchanged scripts are skipped unless force. Render and play run it too. `j3c doc j3.gfx.meshgen` has the API.",
 "inputSchema":{"type":"object","properties":{"force":{"type":"boolean","default":false}}}}
]"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn run(server: &mut Server, input: &str) -> Vec<Json> {
        let mut out = Vec::new();
        serve(server, input.as_bytes(), &mut out);
        String::from_utf8(out).unwrap().lines().map(|l| json::parse(l).unwrap()).collect()
    }

    #[test]
    fn handshake_and_tool_list() {
        let mut server = Server { root: Err("no project".into()), follow: false, next: 0, was_live: false };
        let replies = run(
            &mut server,
            concat!(
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#,
                "\n",
                r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
                "\n\n",
                r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
                "\n",
                r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"scene_list","arguments":{}}}"#,
                "\n",
                r#"{"jsonrpc":"2.0","id":4,"method":"nope"}"#,
                "\n",
                r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"nope"}}"#,
                "\n",
                "{not json\n",
            ),
        );
        assert_eq!(replies.len(), 6, "the notification gets no reply");
        let init = replies[0].get("result").unwrap();
        assert_eq!(init.get("protocolVersion").and_then(Json::as_str), Some("2025-03-26"));
        let tools = match replies[1].get("result").and_then(|r| r.get("tools")) {
            Some(Json::Arr(t)) => t.clone(),
            _ => panic!("no tools"),
        };
        let names: Vec<&str> = tools.iter().filter_map(|t| t.get("name").and_then(Json::as_str)).collect();
        assert_eq!(names, TOOL_NAMES);
        let cli: Vec<&str> = CLI_SAME.iter().map(|(n, _)| *n).collect();
        assert_eq!(cli, TOOL_NAMES, "CLI_SAME must name every tool, in order");
        assert!(EDITS.iter().all(|e| TOOL_NAMES.contains(e)));
        let text = tool_reference();
        for name in TOOL_NAMES {
            assert!(text.contains(&format!("\n{name} — ")), "{name} missing\n{text}");
        }
        assert!(text.contains("    adds*: array of object"), "{text}");
        assert!(text.contains("        kind*: string"), "{text}");
        assert!(text.contains("op*: string (ground|ontop|scatter|grid)"), "{text}");
        for t in &tools {
            assert!(t.get("inputSchema").and_then(|s| s.get("type")).is_some());
        }
        // A tool that fails reports it in the result, so the agent can read it.
        let call = replies[2].get("result").unwrap();
        assert!(matches!(call.get("isError"), Some(Json::Bool(true))));
        assert_eq!(replies[3].get("error").and_then(|e| e.get("code")).and_then(Json::as_i64), Some(-32601));
        assert_eq!(replies[4].get("error").and_then(|e| e.get("code")).and_then(Json::as_i64), Some(-32602));
        assert_eq!(replies[5].get("error").and_then(|e| e.get("code")).and_then(Json::as_i64), Some(-32700));
    }

    #[test]
    fn values_become_request_text() {
        let v = json::parse(r#"{"pos":[1,2.5,-3],"scale":2,"doubleSided":true,"name":"A"}"#).unwrap();
        assert_eq!(pairs(Some(&v)).unwrap(), vec!["pos=1,2.5,-3", "scale=2", "doubleSided=true", "name=A"]);
    }

    #[test]
    fn scene_grid_and_batch_requests() {
        let args = json::parse(r#"{"cell":2,"pieces":[{"what":"builtin:cube"},{"what":"builtin:cube","x":1},
            {"what":"builtin:cube","z":1},{"what":"builtin:cube","x":1,"z":1,"turn":1,"floor":1}]}"#).unwrap();
        let reqs = grid_requests(&args).unwrap();
        let at = |r: &Vec<String>| (r[3].clone(), r[4].clone());
        assert_eq!(at(&reqs[0]), ("pos=0,0,0".into(), "rot=0,0,0".into()));
        assert_eq!(at(&reqs[1]).0, "pos=2,0,0");
        assert_eq!(at(&reqs[2]).0, "pos=0,0,2");
        assert_eq!(at(&reqs[3]), ("pos=2,2,2".into(), "rot=0,90,0".into()));
        let bad = json::parse(r#"{"pieces":[{"what":"x.png"}]}"#).unwrap();
        assert!(grid_requests(&bad).is_err());
        let text = scene::request_text(&reqs[..2].join(&scene::BATCH.to_string())).unwrap();
        assert_eq!(text, "add\tmesh\tasset=builtin:cube\tpos=0,0,0\trot=0,0,0\x1eadd\tmesh\tasset=builtin:cube\tpos=2,0,0\trot=0,0,0");
        assert!(scene::request_text(&["name=a\x1eb".into()]).is_err());
    }

    #[test]
    fn wildcard_matches() {
        assert!(wild("wall*.glb", "wall-door.glb"));
        assert!(!wild("wall*.glb", "roof.glb"));
        assert!(wild("*", "x"));
        assert!(wild("a*b*c", "aXbYc") && !wild("a*b*c", "aXbY"));
    }

    #[test]
    fn import_skips_existing() {
        let root = std::env::temp_dir().join(format!("j3_mcp_reimport_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("assets/imported")).unwrap();
        fs::write(root.join("rock.j3mesh"), "# j3mesh 1\n0\n").unwrap();
        let mut server = Server { root: Err("unused".into()), follow: false, next: 0, was_live: false };
        let args = |extra: &str| json::parse(&format!(r#"{{"path":"rock.j3mesh","place":false{extra}}}"#)).unwrap();
        let text = |a: Answer| a.content[0].get("text").and_then(Json::as_str).unwrap().to_string();
        assert!(text(server.import(&root, &args("")).unwrap()).starts_with("imported"));
        let again = text(server.import(&root, &args("")).unwrap());
        assert!(again.starts_with("skipped") && again.contains("assets/imported/rock.j3scene"), "{again}");
        assert!(!root.join("assets/imported/rock_2.j3scene").exists());
        server.import(&root, &args(r#","again":true"#)).unwrap();
        assert!(root.join("assets/imported/rock_2.j3scene").is_file());
        fs::remove_dir_all(root).unwrap();
    }
}
