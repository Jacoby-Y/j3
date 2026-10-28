use super::*;

/// This j3c binary. After a rebuild replaces it on disk, Linux reports the old path with
/// " (deleted)"; `/proc/self/exe` still opens the running (old) binary, so a long-lived
/// `j3c -e mcp` keeps working with the toolchain it started with.
// ponytail: the server keeps using the old j3c until restarted; mcp.rs says so.
pub(crate) fn host_exe() -> Result<PathBuf, String> {
    Ok(host_or_self(std::env::current_exe().map_err(|e| e.to_string())?))
}

pub(super) fn host_or_self(path: PathBuf) -> PathBuf {
    if path.exists() || !cfg!(target_os = "linux") { return path; }
    PathBuf::from("/proc/self/exe")
}

/// Build the editor frontend once per j3c binary, stdlib, and frontend source, shared by
/// every project. `J3_EDITOR_SRC` reads the frontend from disk instead of the embedded copy.
pub(crate) fn frontend(host: &Path) -> Result<(PathBuf, fs::File), String> {
    let sources: Vec<(String, String)> = match std::env::var("J3_EDITOR_SRC") {
        Ok(dir) if !dir.is_empty() => {
            let mut out = Vec::new();
            for entry in fs::read_dir(&dir).map_err(|e| format!("J3_EDITOR_SRC {dir}: {e}"))? {
                let path = entry.map_err(|e| e.to_string())?.path();
                if path.extension().and_then(|s| s.to_str()) != Some("j3") { continue; }
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                let text = fs::read_to_string(&path).map_err(|e| format!("J3_EDITOR_SRC {}: {e}", path.display()))?;
                out.push((name, text));
            }
            out.sort();
            out
        }
        _ => FRONTEND.iter().map(|(n, s)| (n.to_string(), s.to_string())).collect(),
    };
    let mut hasher = DefaultHasher::new();
    sources.hash(&mut hasher);
    let meta = fs::metadata(host).map_err(|e| e.to_string())?;
    (meta.len(), meta.modified().ok()).hash(&mut hasher);
    if crate::stdlib::using_override() {
        hash_dir(&crate::stdlib::stdlib_dir(), &mut hasher);
    }
    let cache = cache_dir().ok_or("no XDG_CACHE_HOME or HOME for the editor build")?;
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let key = format!("{:016x}", hasher.finish());
    let dir = cache.join(&key);
    let exe = dir.join("target/j3_editor");
    if !exe.exists() {
        let temp = (0..100)
            .find_map(|attempt| {
                let path = cache.join(format!("{key}.tmp-{}-{attempt}", std::process::id()));
                match fs::create_dir(&path) {
                    Ok(()) => Some(Ok(path)),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => None,
                    Err(e) => Some(Err(format!("editor build directory: {e}"))),
                }
            })
            .ok_or("could not reserve an editor build directory")??;
        let built = (|| -> Result<(), String> {
            fs::create_dir_all(temp.join("src")).map_err(|e| e.to_string())?;
            for (name, text) in &sources {
                fs::write(temp.join("src").join(name), text).map_err(|e| e.to_string())?;
            }
            fs::write(temp.join("font.ttf"), FONT).map_err(|e| e.to_string())?;
            fs::write(temp.join("j3.toml"), "[package]\nname = \"j3_editor\"\nentry = \"src/main.j3\"\n\n[link]\nlibs = [\"SDL2\", \"GL\"]\n")
                .map_err(|e| e.to_string())?;
            // Output goes to stderr: `j3c -e mcp` keeps stdout for the protocol.
            #[cfg(unix)]
            let err_fd = std::io::stderr().as_fd().try_clone_to_owned();
            #[cfg(windows)]
            let err_fd = std::io::stderr().as_handle().try_clone_to_owned();
            let status = Command::new(host)
                .arg("build")
                .arg(&temp)
                .stdout(Stdio::from(err_fd.map_err(|e| e.to_string())?))
                .status()
                .map_err(|e| format!("build editor: {e}"))?;
            if !status.success() { return Err("editor build failed".into()); }
            if !temp.join("target/j3_editor").is_file() {
                return Err("editor build did not produce target/j3_editor".into());
            }
            Ok(())
        })();
        if let Err(error) = built {
            let _ = fs::remove_dir_all(&temp);
            return Err(error);
        }
        if let Err(error) = fs::rename(&temp, &dir) {
            if !exe.is_file() {
                let _ = fs::remove_dir_all(&temp);
                return Err(format!("publish editor build: {error}"));
            }
            // Another launch published the same build first.
            let _ = fs::remove_dir_all(&temp);
        }
    }
    let lock = match cache_lock(&dir) {
        Ok(lock) => lock,
        Err(_) if !exe.exists() => return frontend(host),
        Err(error) => return Err(error),
    };
    lock.lock_shared().map_err(|e| format!("editor cache lock: {e}"))?;
    if !exe.is_file() {
        drop(lock);
        let _ = fs::remove_dir_all(&dir);
        return frontend(host);
    }
    cleanup_frontends(&cache, &dir);
    Ok((dir, lock))
}

pub(super) fn cache_lock(dir: &Path) -> Result<fs::File, String> {
    fs::OpenOptions::new().create(true).read(true).write(true)
        .open(dir.join("in-use.lock"))
        .map_err(|e| format!("editor cache lock: {e}"))
}

/// `~/.cache/j3` (or `$XDG_CACHE_HOME/j3`).
pub(crate) fn cache_root() -> Option<PathBuf> {
    std::env::var_os("XDG_CACHE_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .map(|c| c.join("j3"))
}

/// `~/.cache/j3/editor`: editor builds, and which project the editor last opened.
pub(crate) fn cache_dir() -> Option<PathBuf> {
    cache_root().map(|r| r.join("editor"))
}

/// Remember `root` as the open project, for `j3c -e mcp` without `--project`.
pub(super) fn note_open(root: &Path) {
    let Some(cache) = cache_dir() else { return };
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let _ = fs::create_dir_all(&cache);
    let _ = fs::write(cache.join("open-project"), root.to_string_lossy().as_bytes());
}

pub(super) fn project_lock(root: &Path) -> Result<fs::File, String> {
    let dir = root.join("target/j3-editor");
    fs::create_dir_all(&dir).map_err(|e| format!("editor lock: {e}"))?;
    let lock = fs::OpenOptions::new().create(true).write(true)
        .open(dir.join("lock"))
        .map_err(|e| format!("editor lock: {e}"))?;
    lock.try_lock().map_err(|e| match e {
        fs::TryLockError::WouldBlock => "editor already open for this project".to_string(),
        fs::TryLockError::Error(e) => format!("editor lock: {e}"),
    })?;
    Ok(lock)
}

/// Keep the 3 newest builds, the current one, and any another process has locked. Every
/// rebuild of j3c makes a new key, so an age gate would keep dozens.
pub(super) fn cleanup_frontends(cache: &Path, current: &Path) {
    let Ok(entries) = fs::read_dir(cache) else { return };
    let now = std::time::SystemTime::now();
    let mut builds: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let path = entry.path();
        if name.contains(".tmp-") {
            // A build killed before its own cleanup ran; live builds finish in minutes.
            let stale = fs::metadata(&path).and_then(|m| m.modified()).ok()
                .and_then(|time| now.duration_since(time).ok())
                .is_some_and(|age| age > std::time::Duration::from_secs(60 * 60));
            if stale { let _ = fs::remove_dir_all(&path); }
            continue;
        }
        if name.len() != 16 || !name.bytes().all(|c| c.is_ascii_hexdigit()) { continue; }
        if let Some(time) = fs::metadata(path.join("target/j3_editor")).ok().and_then(|m| m.modified().ok()) {
            builds.push((time, path));
        }
    }
    builds.sort_by(|a, b| b.0.cmp(&a.0));
    for (position, (_, path)) in builds.into_iter().enumerate() {
        if position < 3 || path == current { continue; }
        let Ok(lock) = cache_lock(&path) else { continue };
        if lock.try_lock().is_err() { continue; }
        // Recheck after locking: a launch that found the exe missing rebuilds it itself.
        if path.join("target/j3_editor").is_file() {
            let _ = fs::remove_dir_all(path);
        }
    }
}

/// Every file's path and bytes under `dir`, in a stable order.
pub(super) fn hash_dir(dir: &Path, hasher: &mut DefaultHasher) {
    let Ok(read) = fs::read_dir(dir) else { return };
    let mut entries: Vec<PathBuf> = read.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        path.hash(hasher);
        if path.is_dir() {
            hash_dir(&path, hasher);
        } else if let Ok(bytes) = fs::read(&path) {
            bytes.hash(hasher);
        }
    }
}
