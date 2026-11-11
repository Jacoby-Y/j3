//! Where a module's file lives: stdlib, project `src/`, deps; and path helpers.

use super::*;

impl Loader {
    pub(super) fn module_path(&self, parts: &[String]) -> PathBuf {
        if parts.len() == 2 && parts[0] == "j3" && parts[1] == "game" {
            let name = match self.game {
                GameBackend::Sdl => "game.j3",
                GameBackend::Headless => "game_headless.j3",
            };
            return stdlib_dir().join("j3").join(name);
        }
        if parts.len() == 2 && parts[0] == "j3" && parts[1] == "build" {
            // `--release` swaps in the file with `RELEASE = true`, `DEV = false`
            let release = crate::cli::RELEASE.load(std::sync::atomic::Ordering::Relaxed);
            return stdlib_dir().join("j3").join(if release { "build_release.j3" } else { "build.j3" });
        }
        if parts.len() == 2 && parts[0] == "j3" && parts[1] == "sys" {
            let name = match self.os {
                OsKind::Linux => "sys.j3",
                OsKind::Windows => "sys_windows.j3",
                OsKind::Macos => "sys_macos.j3",
                OsKind::Android => "sys_android.j3",
            };
            return stdlib_dir().join("j3").join(name);
        }
        if parts.len() == 2 && parts[0] == "j3" && parts[1] == "threadsys" {
            let name = match self.os {
                OsKind::Windows => "threadsys_windows.j3",
                OsKind::Android => "threadsys_android.j3",
                OsKind::Linux | OsKind::Macos => "threadsys.j3",
            };
            return stdlib_dir().join("j3").join(name);
        }
        if parts.len() == 2 && parts[0] == "j3" && parts[1] == "instance" {
            // x86-64 Linux syscall numbers: other systems get the stub (one copy, never a message)
            let name = match self.os {
                OsKind::Linux => "instance.j3",
                OsKind::Windows | OsKind::Macos | OsKind::Android => "instance_other.j3",
            };
            return stdlib_dir().join("j3").join(name);
        }
        if parts.len() == 2 && parts[0] == "j3" && parts[1] == "netsys" {
            let name = match self.os {
                OsKind::Linux => "netsys.j3",
                OsKind::Windows => "netsys_windows.j3",
                OsKind::Macos => "netsys_macos.j3",
                OsKind::Android => "netsys_android.j3",
            };
            return stdlib_dir().join("j3").join(name);
        }
        if let Some(path) = self.dep_module(parts) {
            return path;
        }
        if let Some(path) = self.project_module(parts) {
            return path;
        }
        module_file(parts)
    }

    pub(super) fn dep_module(&self, parts: &[String]) -> Option<PathBuf> {
        let dep = self.deps.iter().find(|d| parts.first() == Some(&d.name))?;
        if parts.len() == 1 {
            // `use hello;` is the package's root module, `src/hello.j3`
            return Some(dep.root.join(format!("{}.j3", dep.name))).filter(|p| p.is_file());
        }
        Some(join_mod(&dep.root, &parts[1..])).filter(|p| p.is_file())
    }

    /// A dep file must say `module <dep>.<path>;`, as project files say `module <package>.<path>;`.
    pub(super) fn dep_module_line(&self, key: &str, path: &str) -> Result<(), String> {
        let Some(dep) = self.deps.iter().find(|d| Path::new(path).starts_with(&d.root)) else {
            return Ok(());
        };
        let found = self.files.iter().find(|f| f.path == path).map(|f| f.ast.module_key());
        match found {
            Some(k) if k != key => Err(format!("{path}: imported as `{key}` but says `module {k};`; write `module {key};` (dep `{}`)", dep.name)),
            _ => Ok(()),
        }
    }

    pub(super) fn project_module(&self, parts: &[String]) -> Option<PathBuf> {
        let src = self.src_dir.as_ref()?;
        if parts.first().map(|s| s.as_str()) == Some("j3") {
            return None;
        }
        let direct = join_mod(src, parts);
        if direct.is_file() {
            return Some(direct);
        }
        if let Some(pkg) = &self.package {
            if parts.first() == Some(pkg) && parts.len() > 1 {
                let stripped = join_mod(src, &parts[1..]);
                if stripped.is_file() {
                    return Some(stripped);
                }
            }
        }
        None
    }
}

pub(in crate::compile) fn source_exists(path: &Path) -> bool {
    crate::stdlib::embedded(path).is_some() || builtin_hello(path) || path.is_file()
}

pub(super) fn module_file(parts: &[String]) -> PathBuf {
    join_mod(&stdlib_dir(), parts)
}

pub(in crate::compile) fn same_path(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a.as_os_str() == b.as_os_str(),
    }
}

pub(crate) fn join_mod(root: &Path, parts: &[String]) -> PathBuf {
    let mut p = root.to_path_buf();
    for (i, part) in parts.iter().enumerate() {
        if i + 1 == parts.len() {
            p.push(format!("{part}.j3"));
        } else {
            p.push(part);
        }
    }
    p
}
