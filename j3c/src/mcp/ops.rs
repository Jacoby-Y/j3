use super::*;

pub(super) fn online_search(args: &Json) -> Result<Answer, String> {
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

pub(super) fn has_jpeg(dir: &Path) -> bool {
    fs::read_dir(dir).into_iter().flatten().flatten().any(|e| {
        let p = e.path();
        // file_type doesn't follow symlinks, so a link loop can't recurse forever.
        if e.file_type().is_ok_and(|t| t.is_dir()) { return has_jpeg(&p); }
        matches!(p.extension().and_then(|x| x.to_str()).map(str::to_ascii_lowercase).as_deref(), Some("jpg" | "jpeg"))
    })
}

/// The model files `path`/`paths` name: a file, a folder (its models, not recursive), or a
/// file name with `*` wildcards.
pub(super) fn import_sources(root: &Path, args: &Json) -> Result<Vec<PathBuf>, String> {
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
pub(super) fn wild(pat: &str, name: &str) -> bool {
    match pat.split_once('*') {
        None => pat == name,
        Some((head, rest)) => {
            let Some(tail) = name.strip_prefix(head) else { return false };
            (0..=tail.len()).any(|i| tail.is_char_boundary(i) && wild(rest, &tail[i..]))
        }
    }
}

pub(super) fn answer_from(reply: &Reply, op: &str, rel: &str, head: String) -> Answer {
    let body = scene::to_text(reply);
    Answer {
        content: vec![text_block(format!("{head}{}", body.trim_end()))],
        structured: Some(scene::to_json(op, rel, reply)),
        error: !reply.ok,
    }
}

pub(super) fn as_bool(v: &Json) -> Option<bool> {
    match v {
        Json::Bool(b) => Some(*b),
        _ => None,
    }
}

pub(super) fn str_arg(args: &Json, key: &str) -> Option<String> {
    match args.get(key) {
        None | Some(Json::Null) => None,
        Some(v) => value_str(v).ok().filter(|s| !s.is_empty()),
    }
}

pub(super) fn need_str(args: &Json, key: &str) -> Result<String, String> {
    str_arg(args, key).ok_or_else(|| format!("missing `{key}`"))
}

/// A JSON value as a request value: numbers plainly, arrays as `x,y,z`.
pub(super) fn value_str(v: &Json) -> Result<String, String> {
    Ok(match v {
        Json::Str(s) => s.clone(),
        Json::Num(n) => number(*n),
        Json::Bool(b) => b.to_string(),
        Json::Null => String::new(),
        Json::Arr(items) => items.iter().map(value_str).collect::<Result<Vec<_>, _>>()?.join(","),
        Json::Obj(_) => return Err("expected a value, not an object".into()),
    })
}

pub(super) fn number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 { format!("{}", n as i64) } else { format!("{n}") }
}

/// `fields` as `key=value` request arguments.
pub(super) fn pairs(fields: Option<&Json>) -> Result<Vec<String>, String> {
    match fields {
        None | Some(Json::Null) => Ok(Vec::new()),
        Some(Json::Obj(items)) => items.iter().map(|(k, v)| Ok(format!("{k}={}", value_str(v)?))).collect(),
        Some(_) => Err("fields must be an object of key: value".into()),
    }
}

/// The `add` request for one `scene_add` argument object.
pub(super) fn add_request(args: &Json) -> Result<Vec<String>, String> {
    let mut req = vec!["add".to_string(), need_str(args, "kind")?];
    for key in ["name", "parent"] {
        if let Some(v) = str_arg(args, key) { req.push(format!("{key}={v}")); }
    }
    req.extend(pairs(args.get("fields"))?);
    Ok(req)
}

/// `place` op `grid`: one `add` per piece at `[x*cell, floor*height, z*cell]`, yaw `turn*90`.
// ponytail: sugar over `add` only (no bounds or snapping), so it never disagrees with tool.j3.
pub(super) fn grid_requests(args: &Json) -> Result<Vec<Vec<String>>, String> {
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
        let mut req = scene::add_what(&what)?;
        let (x, z, floor) = (num(p.get("x"), 0.0), num(p.get("z"), 0.0), num(p.get("floor"), 0.0));
        req.push(format!("pos={},{},{}", number(x * cell), number(floor * height), number(z * cell)));
        req.push(format!("rot=0,{},0", number(turn * 90.0)));
        if let Some(n) = str_arg(p, "name") { req.push(format!("name={n}")); }
        if let Some(v) = &parent { req.push(format!("parent={v}")); }
        reqs.push(req);
    }
    Ok(reqs)
}

pub(super) fn scene_arg(root: &Path, args: &Json) -> Result<String, String> {
    scene::scene_rel(root, str_arg(args, "scene").as_deref())
}

/// Scene problems and code problems together.
pub(super) fn check(root: &Path, args: &Json) -> Result<Answer, String> {
    let scenes = match str_arg(args, "scene") {
        Some(s) => vec![scene::scene_rel(root, Some(&s))?],
        None => scene::all_scenes(root).unwrap_or_default(),
    };
    let mut lines = Vec::new();
    let mut issues = Vec::new();
    let mut errors: i64 = 0;
    let placed = scene::placed_scenes(root);
    for rel in &scenes {
        let reply = scene::check_scene(root, rel, &placed)?;
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
            Some(result) if result.get("ok").and_then(as_bool) == Some(true) => {
                if let Some(Json::Arr(ws)) = result.get("warnings") {
                    for w in ws {
                        code_issue(w, "warning", &mut lines, &mut issues);
                    }
                }
                // CSS lint lines (`warning: file:line:col: message [rule]`) ride along in `css`
                // (`i18n` carries the `[i18n-missing]` ones the same way)
                for key in ["css", "i18n"] {
                    let Some(Json::Arr(css)) = result.get(key) else { continue };
                    for w in css.iter().filter_map(Json::as_str) {
                        lines.push(w.to_string());
                        let message = w.strip_prefix("warning: ").unwrap_or(w);
                        issues.push(Json::obj(vec![("severity", s("warning")), ("file", s("code")), ("message", Json::Str(message.to_string()))]));
                    }
                }
            }
            Some(result) => {
                // `errors` holds every one; older binaries only had the top-level fields
                let all = match result.get("errors") {
                    Some(Json::Arr(es)) if !es.is_empty() => es.clone(),
                    _ => vec![result.clone()],
                };
                for e in &all {
                    errors += 1;
                    code_issue(e, "error", &mut lines, &mut issues);
                }
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

/// One `j3c check --json` diagnostic (`message`, `path`, `line`, `notes`, `helps`) into the reply.
fn code_issue(d: &Json, severity: &str, lines: &mut Vec<String>, issues: &mut Vec<Json>) {
    let message = d.get("message").and_then(Json::as_str).unwrap_or("does not compile").to_string();
    let file = d.get("path").and_then(Json::as_str).unwrap_or("").to_string();
    let line = d.get("line").and_then(Json::as_i64).unwrap_or(0);
    lines.push(format!("{file}:{line}: {severity}: {message}"));
    for key in ["notes", "helps"] {
        if let Some(Json::Arr(xs)) = d.get(key) {
            let tag = if key == "notes" { "note" } else { "help" };
            for x in xs.iter().filter_map(Json::as_str) {
                lines.push(format!("  {tag}: {x}"));
            }
        }
    }
    issues.push(Json::obj(vec![
        ("severity", Json::Str(severity.to_string())),
        ("file", Json::Str(file)),
        ("line", Json::Num(line as f64)),
        ("message", Json::Str(message)),
    ]));
}

/// Build, run the game headless for some frames, and return its last frame and output.
pub(super) fn play(root: &Path, args: &Json) -> Result<Answer, String> {
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
        .envs(crate::dist::steam_dev_env(root))
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
pub(super) fn console(root: &Path, args: &Json, live: bool) -> Result<Answer, String> {
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

pub(super) const BUILTINS: &[(&str, &str)] = &[
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

pub(super) fn asset_kind(path: &str) -> &'static str {
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

pub(super) fn asset_search(root: &Path, args: &Json) -> Result<Answer, String> {
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
