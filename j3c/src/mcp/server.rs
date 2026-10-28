use super::*;

impl Server {
    pub(super) fn handle(&mut self, msg: &Json) -> Option<Json> {
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
                        ("instructions", Json::Str(instructions(self.root().ok().as_deref()))),
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
        for h in self.coach.hints(name, args, answer.error) { answer.content.push(text_block(h)); }
        Ok(answer)
    }

    fn run_tool(&mut self, name: &str, args: &Json) -> Result<Answer, String> {
        let root = self.root()?;
        match name {
            "scene_list" => {
                let rel = scene_arg(&root, args)?;
                let mut req = vec!["list".to_string()];
                let filter = str_arg(args, "filter");
                let glob = filter.as_deref().is_some_and(|f| f.contains(['*', '?']));
                if glob { req.push(format!("filter={}", filter.as_deref().unwrap_or(""))); }
                if str_arg(args, "group_by").as_deref() == Some("prefix") { req.push("group=prefix".into()); }
                let tree = args.get("tree").and_then(as_bool).unwrap_or(false);
                if tree { req.push("tree=1".into()); }
                let mut reply = self.scene_request(&root, &rel, &req)?;
                if let Some(filter) = filter.filter(|_| !glob) {
                    let f = filter.to_lowercase();
                    reply.lines.retain(|l| l[0] != "node" || l.iter().take(4).any(|c| c.to_lowercase().contains(&f)));
                }
                let count = reply.lines.iter().filter(|l| l[0] == "node").count();
                let groups = reply.lines.iter().filter(|l| l[0] == "group").count();
                let head = if tree {
                    format!("objects in {rel} as the scene file writes them (indent is depth; `scene_apply` takes these lines back){}\n", if self.was_live { ", live in the open editor" } else { "" })
                } else if groups > 0 {
                    format!("{groups} name prefixes in {rel} (count, prefix)\n")
                } else {
                    format!(
                        "{count} objects in {rel} (#id path kind asset, world position; rotations are degrees){}\n",
                        if self.was_live { ", live in the open editor" } else { "" }
                    )
                };
                Ok(answer_from(&reply, "list", &rel, head))
            }
            "scene_get" => {
                let rel = scene_arg(&root, args)?;
                let node = need_str(args, "node")?;
                let reply = self.scene_request(&root, &rel, &["get".into(), node])?;
                Ok(answer_from(&reply, "get", &rel, String::new()))
            }
            "scene_apply" => {
                let rel = scene_arg(&root, args)?;
                let lines = need_str(args, "lines")?;
                let req = scene::apply_args(&lines, str_arg(args, "parent").as_deref())?;
                let reply = self.scene_request(&root, &rel, &req)?;
                Ok(answer_from(&reply, "apply", &rel, String::new()))
            }
            "scene_split" => {
                let rel = scene_arg(&root, args)?;
                let mut req = vec!["split".to_string()];
                match args.get("nodes") {
                    Some(Json::Arr(items)) => { for item in items { req.push(value_str(item)?); } }
                    _ => req.push(need_str(args, "node").map_err(|_| "scene_split needs node or nodes".to_string())?),
                }
                req.push(format!("out={}", need_str(args, "out")?));
                for key in ["instance", "center"] {
                    if args.get(key).and_then(as_bool).unwrap_or(false) { req.push(format!("{key}=1")); }
                }
                let reply = self.scene_request(&root, &rel, &req)?;
                Ok(answer_from(&reply, "split", &rel, String::new()))
            }
            "scene_inline" => {
                let rel = scene_arg(&root, args)?;
                let reply = self.scene_request(&root, &rel, &["inline".into(), need_str(args, "node")?])?;
                Ok(answer_from(&reply, "inline", &rel, String::new()))
            }
            "scene_fmt" => {
                let rel = scene_arg(&root, args)?;
                let reply = self.scene_request(&root, &rel, &["fmt".into()])?;
                Ok(answer_from(&reply, "fmt", &rel, String::new()))
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
                    "ring" | "stairs" | "spiral" | "wall" => {
                        let Json::Obj(items) = args else { return Err("place needs an object of arguments".into()) };
                        let mut kv = Vec::new();
                        for (k, v) in items {
                            if !matches!(k.as_str(), "op" | "scene") && !matches!(v, Json::Null) { kv.push(format!("{k}={}", value_str(v)?)); }
                        }
                        return self.batch(&root, &rel, scene::pattern_requests(&op, &kv)?);
                    }
                    other => return Err(format!("op `{other}`: expected ground, ontop, scatter, grid, ring, stairs, spiral, or wall")),
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
            "scene_stats" => {
                let rel = scene_arg(&root, args)?;
                let reply = self.scene_request(&root, &rel, &["stats".into()])?;
                Ok(answer_from(&reply, "stats", &rel, String::new()))
            }
            "scene_diff" => {
                let rel = scene_arg(&root, args)?;
                let d = scene::diff(&root, &rel, str_arg(args, "against").as_deref())?;
                Ok(Answer::text(format!("{rel}: {}", d.text(80).trim_end())))
            }
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
        for key in ["camera", "view", "focus", "select", "at", "yaw", "pitch", "fov", "hide", "ambient", "ortho"] {
            match args.get(key) {
                None | Some(Json::Null) => {}
                Some(v) => opts.push(format!("{key}={}", value_str(v)?)),
            }
        }
        if let Some(Json::Arr(views)) = args.get("views") {
            let views = views.iter().map(value_str).collect::<Result<Vec<_>, _>>()?;
            opts.push(format!("views={}", views.join(";")));
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

    pub(super) fn import(&mut self, root: &Path, args: &Json) -> Result<Answer, String> {
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
