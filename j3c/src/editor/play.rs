use super::*;

/// Stop the running game, if any. Returns true when one was running.
pub(super) fn kill_game(running: &mut Option<Child>) -> bool {
    let Some(mut child) = running.take() else { return false };
    let alive = matches!(child.try_wait(), Ok(None));
    let _ = child.kill();
    let _ = child.wait();
    alive
}

/// Send a failed build's error lines to the console (the full log stays in build.log).
pub(super) fn report_build_errors(root: &Path, log: &str) {
    let _ = fs::write(root.join("target/j3-editor/build.log"), log);
    let project = format!("{}/", root.display());
    let errors = log.lines().filter(|s| s.contains("error") || s.trim_start().starts_with("-->"));
    let mut sent = 0;
    for line in errors.take(30) {
        event(root, "build", "", &line.trim().replace(&project, ""));
        sent += 1;
    }
    if sent == 0 {
        event(root, "build", "", "Build failed; see target/j3-editor/build.log");
    }
}

/// A game with scripts builds in dev mode: script edits never rebuild it. Anything else
/// builds with `j3c build`. Returns the program to launch and, in dev mode, its scripts.
pub(super) fn build_for_play(root: &Path, host: &Path) -> Result<(PathBuf, Option<crate::devrun::Built>), ()> {
    let manifest = match Manifest::load(&root.join("j3.toml")) {
        Ok(m) => m,
        Err(e) => {
            event(root, "msg", "", &e);
            return Err(());
        }
    };
    let opts = crate::emit::EmitOpts::from_manifest(&manifest).and_then(|mut o| {
        o.backend = crate::emit::resolve_backend(o.backend, false, &o.clang)?;
        Ok(o)
    });
    let prepared = opts.and_then(|opts| crate::devrun::prepare(&manifest, &opts, false));
    match prepared {
        Ok(crate::devrun::Prepared::Dev(built)) => return Ok((built.exe.clone(), Some(built))),
        Ok(crate::devrun::Prepared::NoScripts(_)) => {}
        Err(e) => {
            report_build_errors(root, &e);
            event(root, "msg", "", "Build failed");
            return Err(());
        }
    }
    let build = match Command::new(host).arg("build").arg(root).output() {
        Ok(out) => out,
        Err(e) => {
            event(root, "msg", "", &format!("build game: {e}"));
            return Err(());
        }
    };
    let mut log = String::from_utf8_lossy(&build.stderr).to_string();
    log.push_str(&String::from_utf8_lossy(&build.stdout));
    if !build.status.success() {
        report_build_errors(root, &log);
        event(root, "msg", "", "Build failed");
        return Err(());
    }
    let _ = fs::write(root.join("target/j3-editor/build.log"), &log);
    let project = format!("{}/", root.display());
    for line in log.lines().filter(|s| s.contains("warning")).take(30) {
        event(root, "out", "", &line.trim().replace(&project, ""));
    }
    Ok((manifest.out_path(), None))
}

pub(super) fn spawn_game(root: &Path, exe: &Path, scene: &str) -> Result<Child, String> {
    let mut process = Command::new(exe)
        .current_dir(root)
        .env("J3_ROOT", root)
        .env("J3_SCENE", scene)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(out) = process.stdout.take() { forward(root, out, "out"); }
    if let Some(err) = process.stderr.take() { forward(root, err, "err"); }
    Ok(process)
}

pub(super) fn script_times(built: &Option<crate::devrun::Built>) -> Vec<Option<std::time::SystemTime>> {
    built
        .as_ref()
        .map(|b| {
            b.scripts
                .iter()
                .map(|p| fs::metadata(p).and_then(|m| m.modified()).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Build the game and (re)launch it on `scene`. The lock is the "Play is busy" flag and holds
/// the game; after launch it is released between checks so Stop and Play can reach the game.
/// In dev mode a script edit swaps into the running game, or relaunches it when its layout
/// changed, without compiling the game again.
pub(super) fn play(root: &Path, host: &Path, game: &Mutex<Option<Child>>, scene: &str) {
    let Ok(mut running) = game.try_lock() else {
        event(root, "msg", "", "Already building");
        return;
    };
    if kill_game(&mut running) {
        event(root, "stopped", "", "Game stopped");
    }
    event(root, "msg", "", "Building...");
    event(root, "out", "", &format!("--- Play {scene} ---"));
    let started = std::time::Instant::now();
    let Ok((exe, mut built)) = build_for_play(root, host) else { return };
    event(root, "out", "", &format!("Built in {:.1}s", started.elapsed().as_secs_f32()));
    let mut pid = match spawn_game(root, &exe, scene) {
        Ok(process) => {
            let pid = process.id();
            *running = Some(process);
            event(root, "run", "", "Game launched");
            pid
        }
        Err(e) => return event(root, "msg", "", &format!("Could not launch game: {e}")),
    };
    drop(running);
    let mut seen = script_times(&built);
    let manifest = Manifest::load(&root.join("j3.toml")).ok();
    // Report the game closing by itself. Stop and restart clear or replace the child, which ends this.
    loop {
        std::thread::sleep(std::time::Duration::from_millis(250));
        let Ok(mut running) = game.lock() else { return };
        let Some(child) = running.as_mut().filter(|c| c.id() == pid) else { return };
        match child.try_wait() {
            Ok(None) => {}
            Ok(Some(status)) => {
                *running = None;
                if status.success() { return event(root, "stopped", "", "Game closed"); }
                let how = match status.code() {
                    Some(code) => format!("exit {code}"),
                    None => "killed by a signal".to_string(),
                };
                return event(root, "stopped", "crash", &format!("Game crashed ({how})"));
            }
            Err(_) => {
                *running = None;
                return event(root, "stopped", "", "Game closed");
            }
        }
        let now = script_times(&built);
        if now == seen {
            continue;
        }
        seen = now;
        let (Some(m), Some(old)) = (manifest.as_ref(), built.as_ref()) else { continue };
        let opts = crate::emit::EmitOpts::from_manifest(m).and_then(|mut o| {
            o.backend = crate::emit::resolve_backend(o.backend, false, &o.clang)?;
            Ok(o)
        });
        match opts.and_then(|opts| crate::devrun::prepare(m, &opts, false)) {
            Ok(crate::devrun::Prepared::Dev(next)) => {
                let same_layout = next.layouts.iter().all(|(path, (layout, _))| {
                    old.layouts.get(path).map(|(was, _)| was == layout).unwrap_or(true)
                });
                if next.rebuilt || !same_layout {
                    kill_game(&mut running);
                    match spawn_game(root, &next.exe, scene) {
                        Ok(process) => {
                            pid = process.id();
                            *running = Some(process);
                            event(root, "run", "", "Script layout changed; relaunched the game");
                        }
                        Err(e) => return event(root, "msg", "", &format!("Could not relaunch game: {e}")),
                    }
                } else {
                    crate::devrun::bump_reload(root);
                    event(root, "out", "", "Swapped script code");
                }
                seen = next.scripts.iter().map(|p| fs::metadata(p).and_then(|m| m.modified()).ok()).collect();
                built = Some(next);
            }
            Ok(crate::devrun::Prepared::NoScripts(_)) => {}
            Err(e) => {
                report_build_errors(root, &e);
                event(root, "msg", "", "Script error; the game keeps its old code");
            }
        }
    }
}

/// Send each line the game writes to `pipe` to the editor's console as a `kind` event.
// ponytail: 500 lines a second per stream, the rest counted and skipped, so a game
// printing every frame can't flood the events file.
pub(super) fn forward(root: &Path, pipe: impl std::io::Read + Send + 'static, kind: &'static str) {
    let root = root.to_path_buf();
    std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(pipe);
        let mut line = Vec::new();
        let mut window = std::time::Instant::now();
        let (mut sent, mut skipped) = (0u32, 0u32);
        loop {
            line.clear();
            match std::io::BufRead::read_until(&mut reader, b'\n', &mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            if window.elapsed().as_secs() >= 1 {
                if skipped > 0 { event(&root, kind, "", &format!("... {skipped} lines skipped")); }
                window = std::time::Instant::now();
                sent = 0;
                skipped = 0;
            }
            if sent >= 500 {
                if skipped == 0 { event(&root, kind, "", "... too much output; skipping the rest of this second"); }
                skipped += 1;
                continue;
            }
            sent += 1;
            let text = String::from_utf8_lossy(&line);
            event(&root, kind, "", text.trim_end());
        }
        if skipped > 0 { event(&root, kind, "", &format!("... {skipped} lines skipped")); }
    });
}

/// Import `path` into the editor's tab `tab`: as objects pasted in (an `import` event), or with
/// `as_scene` as its own scene file placed once (`placed`).
pub(super) fn import_into(root: &Path, path: &str, tab: &str, as_scene: bool) {
    static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let file = format!("import-{}.j3scene", COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1);
    let fragment = root.join("target/j3-editor").join(&file);
    let result = import_asset(root, &fragment, path, false).and_then(|w| {
        let placed = if as_scene { Some(keep_import(root, &fragment, path)?) } else { None };
        generate_assets_catalog(root).map(|_| (w, placed))
    });
    match result {
        Ok((w, Some(scene))) if tab.is_empty() => {
            let more = if w.is_empty() { String::new() } else { format!(" ({} import warning{})", w.len(), if w.len() == 1 { "" } else { "s" }) };
            event(root, "refresh", "", &format!("Added {scene}{more}; drag it from Assets into a scene"));
        }
        Ok((w, placed)) => {
            let note = if w.is_empty() {
                "Asset imported".to_string()
            } else {
                format!("Imported with {} warning{}: {}", w.len(), if w.len() == 1 { "" } else { "s" }, w[0])
            };
            match placed {
                Some(scene) => append_event(root, &format!("placed\t{tab}\t{scene}\t{}\n", note.replace(['\n', '\r', '\t'], " "))),
                None => import_event(root, tab, &file, &note),
            }
        }
        Err(e) => {
            eprintln!("j3c editor: {e}");
            event(root, "msg", "", &e);
        }
    }
}

/// Write `target/j3-editor/online` (rows of source, id, kind, name, author, thumbnail path;
/// `pack` names the pack they came from, else empty) in `save` format and tell the editor.
pub(super) fn online_results(root: &Path, rows: &[[String; 6]], pack: &str, message: &str) {
    let mut out = format!("online.count={}\nonline.pack s={pack}\n", rows.len());
    for (i, row) in rows.iter().enumerate() {
        for (key, v) in ["source", "id", "kind", "name", "author", "thumb"].iter().zip(row) {
            out += &format!("online.{i}.{key} s={}\n", v.replace(['\n', '\r', '\t'], " "));
        }
    }
    match fs::write(root.join("target/j3-editor/online"), out) {
        Ok(()) => event(root, "online", "", message),
        Err(e) => event(root, "msg", "", &format!("online results: {e}")),
    }
}

/// Ask the desktop for a model file and hand the path back to the editor to import.
pub(super) fn pick(root: &Path, tab: &str) {
    // ponytail: zenity, so Linux desktops only; drag-and-drop works everywhere.
    let out = Command::new("zenity")
        .args(["--file-selection", "--title=Import model", "--file-filter=Models | *.gltf *.glb *.j3mesh"])
        .output();
    match out {
        Ok(out) if out.status.success() => {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !path.is_empty() {
                event(root, "picked", tab, &path);
            }
        }
        Ok(_) => {}
        Err(_) => event(root, "msg", "", "No file picker (install zenity), or drop the file on the window"),
    }
}

/// Regenerate both catalogs, as at startup.
pub(super) fn rescan(root: &Path) -> Result<(), String> {
    for line in crate::gen::run_all(root, false).failed {
        event(root, "build", "", &format!("mesh generator failed: {line}"));
    }
    generate_catalog(root)?;
    generate_assets_catalog(root)
}

/// Width over height of the game window, for the editor's camera preview: the numbers in
/// `.create("Title", W, H, ...)`, else 16:9.
// ponytail: a text scan of src/main.j3; games that size the window another way get 16:9.
pub(super) fn game_aspect(entry: &str) -> f32 {
    let size = entry.split(".create(\"").skip(1).find_map(|rest| {
        let mut parts = rest.split_once("\",")?.1.split(',');
        let w: f32 = parts.next()?.trim().parse().ok()?;
        let h: f32 = parts.next()?.trim().parse().ok()?;
        (w > 0.0 && h > 0.0).then_some(w / h)
    });
    size.unwrap_or(16.0 / 9.0)
}

pub(super) fn launch(root: &Path, scene: &Path) -> Result<(), String> {
    let host = host_exe()?;
    let (dir, _cache_lock) = frontend(&host)?;
    fs::create_dir_all(root.join("target/j3-editor")).map_err(|e| e.to_string())?;
    // A fresh log each session, so the editor never replays the last one's messages.
    fs::write(root.join("target/j3-editor/events"), "").map_err(|e| e.to_string())?;
    fs::write(root.join("target/j3-editor/replies"), "").map_err(|e| e.to_string())?;
    let _ = fs::remove_file(root.join("target/j3-editor/online"));
    let rel = scene.strip_prefix(root).map_err(|e| e.to_string())?;
    let mut child = Command::new(dir.join("target/j3_editor"))
        .current_dir(root)
        .env("J3_EDITOR_SCENE", rel)
        .env("J3_EDITOR_FONT", dir.join("font.ttf"))
        .env("J3_EDITOR_ASPECT", game_aspect(&fs::read_to_string(root.join("src/main.j3")).unwrap_or_default()).to_string())
        .env("J3_HOST", &host)
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("launch editor: {e}"))?;
    let stdout = child.stdout.take().ok_or("editor stdout unavailable")?;
    let game: Arc<Mutex<Option<Child>>> = Arc::default();
    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(|e| e.to_string())?;
        if let Some(rest) = line.strip_prefix("J3EDITOR REPLY ") {
            // An answer to a `j3c -e mcp` command; the server polls this file for its id.
            if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(root.join("target/j3-editor/replies")) {
                let _ = f.write_all(format!("{rest}\n").as_bytes());
            }
        } else if let Some(rest) = line.strip_prefix("J3EDITOR PLAY") {
            let (root, host, game) = (root.to_path_buf(), host.clone(), game.clone());
            let scene = rest.trim().to_string();
            std::thread::spawn(move || play(&root, &host, &game, &scene));
        } else if line == "J3EDITOR STOP" {
            match game.try_lock() {
                Ok(mut running) => {
                    if kill_game(&mut running) {
                        event(root, "stopped", "", "Game stopped");
                    }
                }
                Err(_) => event(root, "msg", "", "Still building; Stop once the game is up"),
            }
        } else if let Some(tab) = line.strip_prefix("J3EDITOR PICK") {
            let (root, tab) = (root.to_path_buf(), tab.trim().to_string());
            std::thread::spawn(move || pick(&root, &tab));
        } else if let Some(rest) = line.strip_prefix("J3EDITOR RENAME ") {
            let (old, new) = rest.split_once('\t').unwrap_or((rest, ""));
            match rename_asset(root, old, new) {
                Ok(n) => append_event(root, &format!("renamed\t{old}\t{new}\tRenamed; updated {n} scene file{}\n", if n == 1 { "" } else { "s" })),
                Err(e) => event(root, "msg", "", &e),
            }
        } else if let Some(rel) = line.strip_prefix("J3EDITOR DELETE ") {
            match delete_asset(root, rel) {
                Ok(trash) => append_event(root, &format!("deleted\t{rel}\tMoved to {trash}\n")),
                Err(e) => event(root, "msg", "", &e),
            }
        } else if line == "J3EDITOR RESCAN" {
            match rescan(root) {
                Ok(()) => event(root, "refresh", "", "Assets and scripts reloaded"),
                Err(e) => event(root, "msg", "", &e),
            }
        } else if let Some(name) = line.strip_prefix("J3EDITOR SCRIPT ") {
            match create_script(root, name) {
                Ok(rel) => event(root, "refresh", "", &format!("Made {rel}")),
                Err(e) => {
                    eprintln!("j3c editor: {e}");
                    event(root, "msg", "", &e);
                }
            }
        } else if let Some(rest) = line.strip_prefix("J3EDITOR IMPORT ") {
            // `<asset>\t<scene>[\tscene]`; the scene part is the editor's open tab, and a
            // trailing `scene` keeps the import as its own scene file to place.
            let mut parts = rest.split('\t');
            let path = parts.next().unwrap_or("");
            let tab = parts.next().unwrap_or("");
            import_into(root, path, tab, parts.next() == Some("scene"));
        } else if let Some(rest) = line.strip_prefix("J3EDITOR FETCH ") {
            // `<source>\t<kind>\t<query>`: search online; the results land in target/j3-editor/online.
            let mut parts = rest.splitn(3, '\t').map(str::to_string);
            let (source, kind) = (parts.next().unwrap_or_default(), parts.next().unwrap_or_default());
            let (root, query) = (root.to_path_buf(), parts.next().unwrap_or_default());
            std::thread::spawn(move || match crate::online::search(&source, &query, &kind) {
                Ok(hits) => {
                    // Thumbnails download side by side; a missing one just shows no picture.
                    let thumbs: Vec<Option<PathBuf>> = std::thread::scope(|s| {
                        let jobs: Vec<_> = hits.iter().map(|h| s.spawn(|| crate::online::thumb(h))).collect();
                        jobs.into_iter().map(|j| j.join().ok().flatten()).collect()
                    });
                    let rows: Vec<[String; 6]> = hits.iter().zip(thumbs).map(|(h, t)| {
                        let thumb = t.map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
                        [h.source.to_string(), h.id.clone(), h.kind.to_string(), h.name.clone(), h.author.clone(), thumb]
                    }).collect();
                    let n = rows.len();
                    online_results(&root, &rows, "", &format!("{n} result{}", if n == 1 { "" } else { "s" }));
                }
                Err(e) => event(&root, "msg", "", &e),
            });
        } else if let Some(rest) = line.strip_prefix("J3EDITOR GET ") {
            // `<source>\t<id>\t<tab>[\tscene]`: download an online result and import it like IMPORT;
            // an empty tab with `scene` only adds the reusable scene to the project.
            let mut parts = rest.split('\t').map(str::to_string);
            let (source, id) = (parts.next().unwrap_or_default(), parts.next().unwrap_or_default());
            let tab = parts.next().unwrap_or_default();
            let as_scene = parts.next().as_deref() == Some("scene");
            let root = root.to_path_buf();
            std::thread::spawn(move || {
                use crate::online::Fetched;
                match crate::online::fetch(&root, &source, &id, None) {
                    Ok(Fetched::Model(path)) => import_into(&root, &path.to_string_lossy(), &tab, as_scene),
                    Ok(Fetched::Material(rel) | Fetched::Sky(rel)) => {
                        let _ = generate_assets_catalog(&root);
                        event(&root, "refresh", "", &format!("Made {rel}"));
                    }
                    Ok(Fetched::Pack(files)) => {
                        let thumbs = crate::online::pack_thumbs(&source, &id, &files);
                        let rows: Vec<[String; 6]> = files.iter().zip(thumbs).map(|(f, t)| {
                            let name = f.file_name().unwrap_or_default().to_string_lossy().into_owned();
                            let thumb = t.map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
                            [source.clone(), format!("{id}/{name}"), "model".into(), name, "Kenney".into(), thumb]
                        }).collect();
                        online_results(&root, &rows, &id, &format!("{} models in {id}", rows.len()));
                    }
                    Err(e) => event(&root, "msg", "", &e),
                }
            });
        } else {
            println!("{line}");
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    // The game belongs to this session, so it closes with the editor.
    if let Ok(mut running) = game.lock() {
        kill_game(&mut running);
    }
    if !status.success() {
        return Err(format!("editor exited with {status}"));
    }
    Ok(())
}
