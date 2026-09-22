//! Load `.j3` files, parse, typecheck. Always pulls in `j3.sys` for `start`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::check::{self, ParsedFile};

pub use crate::check::Program;
use crate::parse;
use crate::project::Manifest;
use crate::span::FileId;
use crate::stdlib::stdlib_dir;

#[derive(Clone, Copy)]
enum GameBackend {
    Sdl,
    Headless,
}

#[derive(Clone, Copy)]
enum OsKind {
    Linux,
    Windows,
    Macos,
}

fn os_kind(target: &str) -> OsKind {
    let t = target.to_ascii_lowercase();
    if t.contains("windows") {
        OsKind::Windows
    } else if t.contains("apple") || t.contains("darwin") {
        OsKind::Macos
    } else {
        OsKind::Linux
    }
}

pub fn compile_path(entry: &Path) -> Result<Program, String> {
    load_and_check(entry, GameBackend::Sdl, true, None, "")
}

pub fn compile_path_target(entry: &Path, target: &str) -> Result<Program, String> {
    load_and_check(entry, GameBackend::Sdl, true, None, target)
}

pub fn compile_path_headless(entry: &Path) -> Result<Program, String> {
    load_and_check(entry, GameBackend::Headless, true, None, "")
}

pub fn compile_manifest(manifest: &Manifest) -> Result<Program, String> {
    compile_manifest_target(manifest, manifest.emit.primary())
}

pub fn compile_manifest_target(manifest: &Manifest, target: &str) -> Result<Program, String> {
    load_and_check(
        &manifest.entry,
        GameBackend::Sdl,
        true,
        Some(manifest),
        target,
    )
}

pub fn check_path_lib(entry: &Path) -> Result<Program, String> {
    load_and_check(entry, GameBackend::Sdl, false, None, "")
}

fn load_and_check(
    entry: &Path,
    game: GameBackend,
    require_main: bool,
    project: Option<&Manifest>,
    target: &str,
) -> Result<Program, String> {
    let mut loader = Loader {
        files: Vec::new(),
        loaded: HashSet::new(),
        next_file: 1,
        game,
        os: os_kind(target),
        src_dir: project.map(|m| m.src_dir()),
        package: project.map(|m| m.name.clone()),
        overlays: Vec::new(),
    };
    loader.load_path(entry)?;
    loader.load_module(&["j3".to_string(), "sys".to_string()])?;
    loader.load_imports()?;
    check_import_cycles(&loader.files)?;
    let srcs: Vec<(String, String)> = loader
        .files
        .iter()
        .map(|f| (f.path.clone(), f.src.clone()))
        .collect();
    let checked = if require_main {
        check::check_files(loader.files)
    } else {
        check::check_files_lib(loader.files)
    };
    let mut prog = checked.map_err(|e| e.render(&srcs))?;
    if project.map(|m| m.cc.fast || m.emit.fast).unwrap_or(false) {
        crate::ir::fast::strip_aborts(&mut prog.cx);
    }
    Ok(prog)
}

pub fn apply_fast(prog: &mut Program) {
    crate::ir::fast::strip_aborts(&mut prog.cx);
}

/// Typecheck for the LSP / editor. No `main` required. `overlays` replace disk.
pub fn analyze_path(
    path: &Path,
    overlays: &[(PathBuf, String)],
) -> Result<Program, String> {
    analyze_checked(path, overlays).map_err(|e| e.message)
}

/// Same as [`analyze_path`], but keeps the check span for the LSP.
pub fn analyze_checked(
    path: &Path,
    overlays: &[(PathBuf, String)],
) -> Result<Program, crate::check::CheckError> {
    let project = match crate::project::resolve(path) {
        Ok(crate::project::Input::Project(m)) => Some(m),
        _ => None,
    };
    let entry = project
        .as_ref()
        .map(|m| m.entry.clone())
        .unwrap_or_else(|| path.to_path_buf());
    let target = project
        .as_ref()
        .map(|m| m.emit.primary().to_string())
        .unwrap_or_default();
    let mut loader = Loader {
        files: Vec::new(),
        loaded: HashSet::new(),
        next_file: 1,
        game: GameBackend::Sdl,
        os: os_kind(&target),
        src_dir: project.as_ref().map(|m| m.src_dir()),
        package: project.as_ref().map(|m| m.name.clone()),
        overlays: overlays.to_vec(),
    };
    loader
        .load_path(&entry)
        .map_err(|e| crate::check::CheckError::new(path.display().to_string(), crate::span::Span::dummy(), e))?;
    if path != entry.as_path() {
        let _ = loader.load_path(path);
    }
    loader
        .load_module(&["j3".to_string(), "sys".to_string()])
        .map_err(|e| crate::check::CheckError::new(path.display().to_string(), crate::span::Span::dummy(), e))?;
    loader
        .load_imports()
        .map_err(|e| crate::check::CheckError::new(path.display().to_string(), crate::span::Span::dummy(), e))?;
    check_import_cycles(&loader.files).map_err(|e| {
        crate::check::CheckError::new(path.display().to_string(), crate::span::Span::dummy(), e)
    })?;
    check::check_files_lib(loader.files)
}

const HELLO_SRC: &str = include_str!("../examples/hello.j3");

pub fn default_hello_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/hello.j3")
}

fn builtin_hello(path: &Path) -> bool {
    path == default_hello_path()
}

struct Loader {
    files: Vec<ParsedFile>,
    loaded: HashSet<String>,
    next_file: u32,
    game: GameBackend,
    os: OsKind,
    src_dir: Option<PathBuf>,
    package: Option<String>,
    overlays: Vec<(PathBuf, String)>,
}

impl Loader {
    fn load_path(&mut self, path: &Path) -> Result<(), String> {
        let text = self.read_text(path)?;
        self.parse_text(path.display().to_string(), text)
    }

    fn read_text(&self, path: &Path) -> Result<String, String> {
        for (p, src) in &self.overlays {
            if same_path(p, path) {
                return Ok(src.clone());
            }
        }
        if let Some(src) = crate::stdlib::embedded(path) {
            return Ok(src.to_string());
        }
        if builtin_hello(path) {
            return Ok(HELLO_SRC.to_string());
        }
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))
    }

    fn parse_text(&mut self, path: String, text: String) -> Result<(), String> {
        let file_id = FileId(self.next_file);
        self.next_file += 1;
        let ast = match parse::parse(&text, file_id) {
            Ok(f) => f,
            Err(e) => return Err(parse::format_error(&path, &text, &e)),
        };
        let key = ast.module_key();
        if !self.loaded.insert(key) {
            return Ok(());
        }
        self.files.push(ParsedFile {
            path,
            src: text,
            ast,
            file_id,
        });
        Ok(())
    }

    fn load_module(&mut self, parts: &[String]) -> Result<(), String> {
        let key = parts.join(".");
        if self.loaded.contains(&key) {
            return Ok(());
        }
        let path = self.module_path(parts);
        if !source_exists(&path) {
            return Err(format!(
                "cannot find module `{}` (looked for {})",
                key,
                path.display()
            ));
        }
        self.load_path(&path)
    }

    fn module_path(&self, parts: &[String]) -> PathBuf {
        if parts.len() == 2 && parts[0] == "j3" && parts[1] == "game" {
            let name = match self.game {
                GameBackend::Sdl => "game.j3",
                GameBackend::Headless => "game_headless.j3",
            };
            return stdlib_dir().join("j3").join(name);
        }
        if parts.len() == 2 && parts[0] == "j3" && parts[1] == "sys" {
            let name = match self.os {
                OsKind::Linux => "sys.j3",
                OsKind::Windows => "sys_windows.j3",
                OsKind::Macos => "sys_macos.j3",
            };
            return stdlib_dir().join("j3").join(name);
        }
        if let Some(path) = self.project_module(parts) {
            return path;
        }
        module_file(parts)
    }

    fn project_module(&self, parts: &[String]) -> Option<PathBuf> {
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

    fn load_imports(&mut self) -> Result<(), String> {
        let mut i = 0;
        while i < self.files.len() {
            let uses: Vec<(Vec<String>, Option<Vec<String>>)> = self.files[i]
                .ast
                .items
                .iter()
                .filter_map(|it| match it {
                    parse::Item::Use(u) => Some((
                        u.path.iter().map(|p| p.name.clone()).collect(),
                        u.names
                            .as_ref()
                            .map(|ns| ns.iter().map(|n| n.name.clone()).collect()),
                    )),
                    parse::Item::UseLib(_) => None,
                    _ => None,
                })
                .collect();
            for (path, names) in uses {
                if names.is_some() {
                    self.load_module(&path)?;
                } else if source_exists(&self.module_path(&path)) {
                    self.load_module(&path)?;
                } else if path.len() >= 2 {
                    self.load_module(&path[..path.len() - 1])?;
                } else {
                    return Err(format!("cannot resolve `use {}`", path.join(".")));
                }
            }
            i += 1;
        }
        Ok(())
    }
}

fn check_import_cycles(files: &[ParsedFile]) -> Result<(), String> {
    let keys: Vec<String> = files.iter().map(|f| f.ast.module_key()).collect();
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();
    for f in files {
        let from = f.ast.module_key();
        for it in &f.ast.items {
            if let parse::Item::Use(u) = it {
                if let Some(to) = use_mod_key(u, &keys) {
                    if to != from {
                        graph.entry(from.clone()).or_default().push(to);
                    }
                }
            }
        }
    }
    for start in &keys {
        if let Some(cycle) = cycle_from(&graph, start) {
            return Err(format!("import cycle: {}", cycle.join(" -> ")));
        }
    }
    Ok(())
}

fn use_mod_key(u: &parse::UseItem, loaded: &[String]) -> Option<String> {
    let parts: Vec<String> = u.path.iter().map(|p| p.name.clone()).collect();
    if u.names.is_some() {
        let k = parts.join(".");
        if loaded.iter().any(|x| x == &k) {
            return Some(k);
        }
        return None;
    }
    let as_mod = parts.join(".");
    if loaded.iter().any(|k| k == &as_mod) {
        return Some(as_mod);
    }
    if parts.len() >= 2 {
        let k = parts[..parts.len() - 1].join(".");
        if loaded.iter().any(|x| x == &k) {
            return Some(k);
        }
    }
    None
}

fn cycle_from(graph: &HashMap<String, Vec<String>>, start: &str) -> Option<Vec<String>> {
    fn dfs(
        graph: &HashMap<String, Vec<String>>,
        node: &str,
        stack: &mut Vec<String>,
        on_stack: &mut HashSet<String>,
    ) -> Option<Vec<String>> {
        if on_stack.contains(node) {
            let i = stack.iter().position(|s| s == node)?;
            let mut c = stack[i..].to_vec();
            c.push(node.to_string());
            return Some(c);
        }
        if stack.iter().any(|s| s == node) {
            return None;
        }
        stack.push(node.to_string());
        on_stack.insert(node.to_string());
        if let Some(nexts) = graph.get(node) {
            for n in nexts {
                if let Some(c) = dfs(graph, n, stack, on_stack) {
                    return Some(c);
                }
            }
        }
        on_stack.remove(node);
        stack.pop();
        None
    }
    dfs(graph, start, &mut Vec::new(), &mut HashSet::new())
}

fn source_exists(path: &Path) -> bool {
    crate::stdlib::embedded(path).is_some() || builtin_hello(path) || path.is_file()
}

fn module_file(parts: &[String]) -> PathBuf {
    join_mod(&stdlib_dir(), parts)
}

fn same_path(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a.as_os_str() == b.as_os_str(),
    }
}

fn join_mod(root: &Path, parts: &[String]) -> PathBuf {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit;
    use crate::ir::dump::dump_program;

    fn llvm_opts() -> emit::EmitOpts {
        let mut o = emit::EmitOpts::default();
        o.backend = emit::Backend::Llvm;
        if !emit::clang_available(&o.clang) {
            o.backend = emit::Backend::C;
        }
        o
    }

    fn exec(prog: &Program) -> emit::RunResult {
        emit::run_program(&prog.cx, &prog.modules, &llvm_opts()).expect("emit/run")
    }

    #[test]
    fn hello_j3_prints_hello_world() {
        let prog = compile_path(&default_hello_path()).expect("compile hello.j3");
        let dump = dump_program(&prog.cx, &prog.modules);
        assert!(dump.contains("printLn"), "{dump}");
        assert!(dump.contains("@syscall("), "{dump}");
        assert!(dump.contains("Hello, world!"), "{dump}");
        assert!(dump.contains("fn start()"), "{dump}");

        let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
        assert!(c.contains("#line "), "missing #line in C:\n{c}");
        let ll = emit::emit_llvm(&prog.cx, &prog.modules, &emit::EmitOpts::default()).expect("emit llvm");
        assert!(ll.contains("target triple"), "{ll}");
        assert!(ll.contains("define i32 @main()"), "{ll}");
        let run = exec(&prog);
        assert_eq!(
            run.status,
            0,
            "stderr {}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&run.stdout), "Hello, world!\n");
    }

    #[test]
    fn c_backend_still_runs_hello() {
        let prog = compile_path(&default_hello_path()).expect("compile hello.j3");
        let mut opts = emit::EmitOpts::default();
        opts.backend = emit::Backend::C;
        let run = emit::run_program(&prog.cx, &prog.modules, &opts).expect("c run");
        assert_eq!(
            run.status,
            0,
            "stderr {}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&run.stdout), "Hello, world!\n");
    }

    #[test]
    fn c_backend_runs_02_data() {
        let prog = compile_path(&speculative("02_data.j3")).expect("compile 02");
        let mut opts = emit::EmitOpts::default();
        opts.backend = emit::Backend::C;
        let run = emit::run_program(&prog.cx, &prog.modules, &opts).expect("c run 02");
        assert_eq!(
            run.status,
            0,
            "stderr {}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            "len=4.47214 player.pos=Vec2 { x: 12, y: 9 }\n"
        );
    }

    #[test]
    fn c_backend_runs_08_headless() {
        let prog = compile_path_headless(&speculative("08_game_loop.j3")).expect("compile 08");
        let mut opts = emit::EmitOpts::default();
        opts.backend = emit::Backend::C;
        let run = emit::run_program(&prog.cx, &prog.modules, &opts).expect("c run 08");
        assert_eq!(
            run.status,
            0,
            "stderr {}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&run.stdout), "bye\n");
    }

    #[test]
    fn hud_quad_static_init_has_no_union() {
        let prog = compile_path(&speculative("08_game_loop.j3")).expect("compile 08 sdl");
        let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
        let line = c
            .lines()
            .find(|l| l.contains("HUD_QUAD"))
            .expect("HUD_QUAD line missing");
        assert!(
            !line.contains("union"),
            "HUD_QUAD static init used union:\n{line}"
        );
    }

    #[test]
    fn analyze_hello_and_overlay() {
        let path = default_hello_path();
        let prog = analyze_path(&path, &[]).expect("analyze");
        assert!(prog.cx.files.iter().any(|f| f.path.contains("hello.j3")));
        let src = std::fs::read_to_string(&path).unwrap().replace("Hello, world!", "Hi");
        let overlay = vec![(path.clone(), src)];
        let prog2 = analyze_path(&path, &overlay).expect("overlay");
        let dump = dump_program(&prog2.cx, &prog2.modules);
        assert!(dump.contains("Hi"), "{dump}");
    }

    #[test]
    fn project_emit_cwd_is_package_root() {
        let toml = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("examples/bounce/j3.toml");
        let m = crate::project::Manifest::load(&toml).expect("toml");
        let opts = emit::EmitOpts::from_manifest(&m).expect("opts");
        assert_eq!(opts.cwd.as_ref().unwrap(), &m.root);
    }

    fn speculative(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("speculative")
            .join(name)
    }

    #[test]
    fn basics_j3_runs() {
        let prog = compile_path(&speculative("01_basics.j3")).expect("compile 01");
        let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
        assert!(!c.contains("({"), "GNU statement-expr in C:\n{c}");
        let run = exec(&prog);
        assert_eq!(
            run.status,
            0,
            "stderr {}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            "Hello, world!\nHello, Coby, how are you?\n"
        );
    }

    #[test]
    fn control_j3_runs() {
        let prog = compile_path(&speculative("04_control.j3")).expect("compile 04");
        let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
        assert!(!c.contains("({"), "GNU statement-expr in C:\n{c}");
        let run = exec(&prog);
        assert_eq!(
            run.status,
            0,
            "stderr {}\nstdout {}",
            String::from_utf8_lossy(&run.stderr),
            String::from_utf8_lossy(&run.stdout)
        );
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            "hp=3\nn=3\nn=2\nn=1\nabs=4\ni=0\ni=1\ni=3\nhello player\nleaving main\n"
        );
    }

    #[test]
    fn data_j3_runs() {
        let prog = compile_path(&speculative("02_data.j3")).expect("compile 02");
        let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
        let run = exec(&prog);
        assert_eq!(
            run.status,
            0,
            "stderr {}\nstdout {}\nc:\n{c}",
            String::from_utf8_lossy(&run.stderr),
            String::from_utf8_lossy(&run.stdout)
        );
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            "len=4.47214 player.pos=Vec2 { x: 12, y: 9 }\n"
        );
    }

    #[test]
    fn embedding_j3_runs() {
        let prog = compile_path(&speculative("03_embedding.j3")).expect("compile 03");
        let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
        let run = exec(&prog);
        assert_eq!(
            run.status,
            0,
            "stderr {}\nstdout {}\nc:\n{c}",
            String::from_utf8_lossy(&run.stderr),
            String::from_utf8_lossy(&run.stdout)
        );
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            "x=10, width=1\nx=10, width=1\n"
        );
    }

    #[test]
    fn arrays_j3_runs() {
        let prog = compile_path(&speculative("05_arrays.j3")).expect("compile 05");
        let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
        let run = exec(&prog);
        assert_eq!(
            run.status,
            0,
            "stderr {}\nstdout {}\nc:\n{c}",
            String::from_utf8_lossy(&run.stderr),
            String::from_utf8_lossy(&run.stdout)
        );
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            "sum=102 mid0=21\nxs[0]=11\nxs[1]=21\nxs[2]=30\nxs[3]=40\nhi Coby bytes=4\n"
        );
    }

    #[test]
    fn enums_j3_runs() {
        let prog = compile_path(&speculative("06_enums_errors.j3")).expect("compile 06");
        let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
        let run = exec(&prog);
        assert_eq!(
            run.status,
            0,
            "stderr {}\nstdout {}\nc:\n{c}",
            String::from_utf8_lossy(&run.stderr),
            String::from_utf8_lossy(&run.stdout)
        );
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            "north\nhit 2\nmissing\n"
        );
    }

    #[test]
    fn memory_j3_runs() {
        let prog = compile_path(&speculative("07_memory.j3")).expect("compile 07");
        let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
        let run = exec(&prog);
        assert_eq!(
            run.status,
            0,
            "stderr {}\nstdout {}\nc:\n{c}",
            String::from_utf8_lossy(&run.stderr),
            String::from_utf8_lossy(&run.stdout)
        );
        assert_eq!(String::from_utf8_lossy(&run.stdout), "entities=1 hits=1\n");
    }

    #[test]
    fn game_loop_j3_runs() {
        let prog = compile_path_headless(&speculative("08_game_loop.j3")).expect("compile 08");
        let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
        let run = exec(&prog);
        assert_eq!(
            run.status,
            0,
            "stderr {}\nstdout {}\nc:\n{c}",
            String::from_utf8_lossy(&run.stderr),
            String::from_utf8_lossy(&run.stdout)
        );
        assert_eq!(String::from_utf8_lossy(&run.stdout), "bye\n");
    }

    #[test]
    fn both_backends_run_01_through_08() {
        let cases: &[(&str, bool, &str)] = &[
            (
                "01_basics.j3",
                false,
                "Hello, world!\nHello, Coby, how are you?\n",
            ),
            (
                "02_data.j3",
                false,
                "len=4.47214 player.pos=Vec2 { x: 12, y: 9 }\n",
            ),
            ("03_embedding.j3", false, "x=10, width=1\nx=10, width=1\n"),
            (
                "04_control.j3",
                false,
                "hp=3\nn=3\nn=2\nn=1\nabs=4\ni=0\ni=1\ni=3\nhello player\nleaving main\n",
            ),
            (
                "05_arrays.j3",
                false,
                "sum=102 mid0=21\nxs[0]=11\nxs[1]=21\nxs[2]=30\nxs[3]=40\nhi Coby bytes=4\n",
            ),
            ("06_enums_errors.j3", false, "north\nhit 2\nmissing\n"),
            ("07_memory.j3", false, "entities=1 hits=1\n"),
            ("08_game_loop.j3", true, "bye\n"),
        ];

        for &(name, headless, expected) in cases {
            let prog = if headless {
                compile_path_headless(&speculative(name)).expect(name)
            } else {
                compile_path(&speculative(name)).expect(name)
            };

            let mut llvm_opts = emit::EmitOpts::default();
            llvm_opts.backend = emit::Backend::Llvm;
            if !emit::clang_available(&llvm_opts.clang) {
                panic!("clang required for dual-backend test");
            }
            let llvm_run =
                emit::run_program(&prog.cx, &prog.modules, &llvm_opts).expect("llvm run");

            let mut c_opts = emit::EmitOpts::default();
            c_opts.backend = emit::Backend::C;
            let c_run = emit::run_program(&prog.cx, &prog.modules, &c_opts).expect("c run");

            assert_eq!(
                llvm_run.status,
                0,
                "{name} llvm stderr {}",
                String::from_utf8_lossy(&llvm_run.stderr)
            );
            assert_eq!(
                c_run.status,
                0,
                "{name} c stderr {}",
                String::from_utf8_lossy(&c_run.stderr)
            );
            let llvm_out = String::from_utf8_lossy(&llvm_run.stdout);
            let c_out = String::from_utf8_lossy(&c_run.stdout);
            assert_eq!(llvm_out.as_ref(), expected, "{name} llvm stdout");
            assert_eq!(c_out.as_ref(), expected, "{name} c stdout");
            assert_eq!(
                llvm_out, c_out,
                "{name} llvm vs c\nllvm stderr {}\nc stderr {}",
                String::from_utf8_lossy(&llvm_run.stderr),
                String::from_utf8_lossy(&c_run.stderr)
            );
        }
    }

    #[test]
    fn signed_div_min_is_a_branch() {
        let path = std::env::temp_dir().join(format!(
            "j3c_signed_div_min_{}.j3",
            std::process::id()
        ));
        std::fs::write(
            &path,
            // Plan wrote `-9223372036854775808`; parser rejects that literal
            // (unary-minus of an out-of-range positive). Equivalent MIN:
            r#"module t;
fn main() {
    let a = -9223372036854775807 - 1;
    let b = -1;
    let q = a / b;
    let _ = q;
}
"#,
        )
        .expect("write temp");
        let prog = compile_path(&path).expect("compile");
        let _ = std::fs::remove_file(&path);
        let ll = emit::emit_llvm(&prog.cx, &prog.modules, &emit::EmitOpts::default())
            .expect("emit llvm");
        assert!(ll.contains("sdiv "), "{ll}");
        assert!(ll.contains("phi "), "{ll}");
        for line in ll.lines() {
            assert!(
                !(line.contains("select i1") && line.contains("-9223372036854775808")),
                "select+min on one line:\n{line}\n{ll}"
            );
        }
    }

    #[test]
    fn defer_is_block_scoped() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/defer_scope.j3");
        let prog = compile_path(&path).expect("compile defer_scope");
        let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
        let run = exec(&prog);
        assert_eq!(
            run.status,
            0,
            "stderr {}\nstdout {}\nc:\n{c}",
            String::from_utf8_lossy(&run.stderr),
            String::from_utf8_lossy(&run.stdout)
        );
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            "a\ninner\nb\ni=0\nd\nd\ndone\n"
        );
    }

    #[test]
    fn stdlib_game_j3_typechecks() {
        let game = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stdlib/j3/game.j3");
        let prog = check_path_lib(&game).expect("check shipped j3.game");
        assert!(
            prog.modules.iter().any(|m| {
                m.name.len() == 2
                    && prog.cx.name_str(m.name[0]) == "j3"
                    && prog.cx.name_str(m.name[1]) == "game"
            }),
            "expected j3.game module"
        );
        let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
        assert!(c.contains("use lib \"SDL2\""), "{c}");

        check_path_lib(&speculative("09_stdlib_game.j3")).expect("check 09");
    }

    #[test]
    fn stdlib_option_result_typecheck() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stdlib/j3");
        check_path_lib(&dir.join("option.j3")).expect("check j3.option");
        check_path_lib(&dir.join("result.j3")).expect("check j3.result");
        check_path_lib(&dir.join("rand.j3")).expect("check j3.rand");
        check_path_lib(&dir.join("math.j3")).expect("check j3.math");
        check_path_lib(&dir.join("ecs.j3")).expect("check j3.ecs");
        check_path_lib(&dir.join("app.j3")).expect("check j3.app");
        check_path_lib(&dir.join("collide.j3")).expect("check j3.collide");
        check_path_lib(&dir.join("res.j3")).expect("check j3.res");
        check_path_lib(&dir.join("scene.j3")).expect("check j3.scene");
        check_path_lib(&dir.join("save.j3")).expect("check j3.save");
        check_path_lib(&dir.join("tile.j3")).expect("check j3.tile");
        check_path_lib(&dir.join("fx.j3")).expect("check j3.fx");
        check_path_lib(&dir.join("debug.j3")).expect("check j3.debug");
        check_path_lib(&dir.join("ui.j3")).expect("check j3.ui");
        check_path_lib(&speculative("10_j3ui.j3")).expect("check 10 j3ui");
        check_path_lib(&dir.join("assets.j3")).expect("check j3.assets");
        check_path_lib(&dir.join("gfx.j3")).expect("check j3.gfx");
        check_path_lib(&dir.join("mesh.j3")).expect("check j3.mesh");
        check_path_lib(&dir.join("voxel.j3")).expect("check j3.voxel");
    }

    #[test]
    fn math_rand_j3_runs() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/math_rand.j3");
        let prog = compile_path(&path).expect("compile math_rand");
        let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
        let run = exec(&prog);
        assert_eq!(
            run.status,
            0,
            "stderr {}\nstdout {}\nc:\n{c}",
            String::from_utf8_lossy(&run.stderr),
            String::from_utf8_lossy(&run.stdout)
        );
        assert_eq!(String::from_utf8_lossy(&run.stdout), "n=12\n");
    }

    #[test]
    fn mesh_fmt_j3_runs() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/mesh.j3");
        let prog = compile_path(&path).expect("compile mesh");
        let run = exec(&prog);
        assert_eq!(
            run.status,
            0,
            "stderr {}\nstdout {}",
            String::from_utf8_lossy(&run.stderr),
            String::from_utf8_lossy(&run.stdout)
        );
        assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
    }

    #[test]
    fn ray_aabb_j3_runs() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/ray_aabb.j3");
        let prog = compile_path(&path).expect("compile ray_aabb");
        let run = exec(&prog);
        assert_eq!(
            run.status,
            0,
            "stderr {}\nstdout {}",
            String::from_utf8_lossy(&run.stderr),
            String::from_utf8_lossy(&run.stdout)
        );
        assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
    }

    #[test]
    fn voxel_grid_j3_runs() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/voxel_grid.j3");
        let prog = compile_path(&path).expect("compile voxel_grid");
        let run = exec(&prog);
        assert_eq!(
            run.status,
            0,
            "stderr {}\nstdout {}",
            String::from_utf8_lossy(&run.stderr),
            String::from_utf8_lossy(&run.stdout)
        );
        assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
    }

    #[test]
    fn voxel_ao_j3_runs() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/voxel_ao.j3");
        let prog = compile_path(&path).expect("compile voxel_ao");
        let run = exec(&prog);
        assert_eq!(
            run.status,
            0,
            "stderr {}\nstdout {}",
            String::from_utf8_lossy(&run.stderr),
            String::from_utf8_lossy(&run.stdout)
        );
        assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
    }

    #[test]
    fn minecraft_project_typechecks() {
        let toml = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("examples/minecraft/j3.toml");
        let m = crate::project::Manifest::load(&toml).expect("j3.toml");
        assert_eq!(m.name, "minecraft");
        compile_manifest(&m).expect("compile minecraft");
    }

    #[test]
    fn ecs_basic_j3_runs() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/ecs_basic.j3");
        let prog = compile_path(&path).expect("compile ecs_basic");
        let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
        let run = exec(&prog);
        assert_eq!(
            run.status,
            0,
            "stderr {}\nstdout {}\nc:\n{c}",
            String::from_utf8_lossy(&run.stderr),
            String::from_utf8_lossy(&run.stdout)
        );
        assert_eq!(String::from_utf8_lossy(&run.stdout), "ok\n");
    }

    #[test]
    fn import_cycle_is_error() {
        let a = parse_file(1, "a.j3", "module a;\nuse b;\nfn main() {}\n");
        let b = parse_file(2, "b.j3", "module b;\nuse a;\n");
        let err = check_import_cycles(&[a, b]).unwrap_err();
        assert!(err.contains("import cycle"), "{err}");
    }

    fn parse_file(id: u32, path: &str, src: &str) -> ParsedFile {
        let ast = parse::parse(src, FileId(id)).expect(path);
        ParsedFile {
            path: path.to_string(),
            src: src.to_string(),
            ast,
            file_id: FileId(id),
        }
    }

    #[test]
    fn occur_mutual_is_error() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/honesty/occur_mutual.j3");
        let err = match compile_path(&path) {
            Ok(_) => panic!("expected occur error"),
            Err(e) => e,
        };
        assert!(err.contains("recursive field"), "{err}");
    }

    #[test]
    fn occur_array_is_error() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/honesty/occur_array.j3");
        let err = match compile_path(&path) {
            Ok(_) => panic!("expected occur error"),
            Err(e) => e,
        };
        assert!(err.contains("recursive field"), "{err}");
    }

    #[test]
    fn left_literal_takes_right_numeric_type() {
        let src = "\
module t;
fn scale(far: f32): f32 {
    return 2 * far;
}
fn below(far: f32): bool {
    return 0 < far;
}
fn keep(): float {
    return 2.0 + 2;
}
fn main() {}
";
        check::check_files(vec![parse_file(1, "t.j3", src)]).expect("left literal");
    }

    #[test]
    fn inclusive_for_runs() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/inclusive_for.j3");
        let prog = compile_path(&path).expect("compile inclusive_for");
        let run = exec(&prog);
        assert_eq!(
            run.status,
            0,
            "stderr {}\nstdout {}",
            String::from_utf8_lossy(&run.stderr),
            String::from_utf8_lossy(&run.stdout)
        );
        assert_eq!(String::from_utf8_lossy(&run.stdout), "0123\n");
    }

    #[test]
    fn for_index_ends_with_the_loop() {
        let src = "\
module t;
fn main() {
    let n = 0;
    for i in 0..2 {
        n = n + i;
    }
    for i in 0..2 {
        n = n + i;
    }
}
";
        check::check_files(vec![parse_file(1, "t.j3", src)]).expect("two fors");
    }

    #[test]
    fn let_then_for_still_shadows() {
        let src = "\
module t;
fn main() {
    let i = 0;
    for i in 0..2 {
    }
}
";
        let err = match check::check_files(vec![parse_file(1, "t.j3", src)]) {
            Ok(_) => panic!("expected shadowing"),
            Err(e) => e,
        };
        assert!(err.message.contains("shadowing"), "{}", err.message);
    }

    #[test]
    fn default_fields_fill_omitted() {
        let src = "\
module t;
struct Hit {
    ok: bool = false;
    x: int = 0;
}
fn main() {
    let a = Hit {};
    let b = Hit { ok: true };
    if a.ok || a.x != 0 || !b.ok || b.x != 0 {
        return;
    }
}
";
        check::check_files(vec![parse_file(1, "t.j3", src)]).expect("defaults");
    }

    #[test]
    fn omitted_hit_texture_mesh_use_zeros() {
        let path = std::env::temp_dir().join(format!(
            "j3c_omitted_hit_texture_mesh_{}.j3",
            std::process::id()
        ));
        let src = "\
module t;
use j3.voxel.Hit;
use j3.game.Texture;
use j3.gfx.Mesh;
use j3.io.printLn;
fn main() {
    let h = Hit {};
    let tex = Texture {};
    let mesh = Mesh {};
    if h.ok || h.x != 0 || h.face != 0 {
        printLn(\"hit\");
    }
    if tex.w != 0 || tex.h != 0 {
        printLn(\"tex\");
    }
    if mesh.count != 0 || mesh.vao != 0 {
        printLn(\"mesh\");
    }
}
";
        std::fs::write(&path, src).expect("write temp");
        let result = compile_path(&path);
        let _ = std::fs::remove_file(&path);
        let prog = result.expect("compile");
        let run = exec(&prog);
        assert_eq!(
            run.status,
            0,
            "stderr {}\nstdout {}",
            String::from_utf8_lossy(&run.stderr),
            String::from_utf8_lossy(&run.stdout)
        );
        assert_eq!(String::from_utf8_lossy(&run.stdout), "");
    }

    #[test]
    fn missing_field_without_default_is_error() {
        let src = "\
module t;
struct Pair {
    a: int = 0;
    b: int;
}
fn main() {
    let p = Pair {};
}
";
        let err = match check::check_files(vec![parse_file(1, "t.j3", src)]) {
            Ok(_) => panic!("expected missing field"),
            Err(e) => e,
        };
        assert!(err.message.contains("missing field `b`"), "{}", err.message);
    }

    #[test]
    fn generic_field_default_is_error() {
        let src = "\
module t;
struct Box<T> {
    v: T = 0;
}
fn main() {}
";
        let err = match check::check_files(vec![parse_file(1, "t.j3", src)]) {
            Ok(_) => panic!("expected generic default error"),
            Err(e) => e,
        };
        assert!(
            err.message.contains("default on a generic field"),
            "{}",
            err.message
        );
    }

    #[test]
    fn non_literal_field_default_is_error() {
        let src = "\
module t;
struct Hit {
    x: int = 1 + 2;
}
fn main() {}
";
        let err = match check::check_files(vec![parse_file(1, "t.j3", src)]) {
            Ok(_) => panic!("expected literal default error"),
            Err(e) => e,
        };
        assert!(
            err.message.contains("field default must be a literal"),
            "{}",
            err.message
        );
    }

    #[test]
    fn two_literals_still_default_from_the_left() {
        let src = "\
module t;
fn main() {
    let x = 2 + 2.0;
}
";
        let err = match check::check_files(vec![parse_file(1, "t.j3", src)]) {
            Ok(_) => panic!("expected arithmetic mismatch"),
            Err(e) => e,
        };
        assert!(
            err.message.contains("arithmetic type mismatch"),
            "{}",
            err.message
        );
    }

    #[test]
    fn private_use_is_error() {
        let hidden = include_str!("../examples/honesty/hidden.j3");
        let user = include_str!("../examples/honesty/private_use.j3");
        let files = vec![
            parse_file(1, "hidden.j3", hidden),
            parse_file(2, "private_use.j3", user),
        ];
        let err = match check::check_files(files) {
            Ok(_) => panic!("expected private error"),
            Err(e) => e,
        };
        assert!(err.message.contains("private"), "{}", err.message);
    }

    #[test]
    fn type_mismatch_is_rustc_shaped() {
        let files = vec![parse_file(
            1,
            "t.j3",
            "module t;\nfn main() {\n    let x: int = \"hi\";\n}\n",
        )];
        let err = match check::check_files(files) {
            Ok(_) => panic!("expected type error"),
            Err(e) => e.render(&[("t.j3".into(), "module t;\nfn main() {\n    let x: int = \"hi\";\n}\n".into())]),
        };
        assert!(err.contains("error:"), "{err}");
        assert!(err.contains("expected `int`"), "{err}");
        assert!(err.contains("found `string`"), "{err}");
        assert!(err.contains("--> t.j3:"), "{err}");
        assert!(err.contains("let x: int"), "{err}");
    }

    #[test]
    fn unknown_name_suggests_typo() {
        let src = "module t;\nfn main() {\n    let start = 1;\n    let x = statr;\n}\n";
        let files = vec![parse_file(1, "t.j3", src)];
        let err = match check::check_files(files) {
            Ok(_) => panic!("expected unknown name"),
            Err(e) => e,
        };
        assert!(err.message.contains("unknown name `statr`"), "{}", err.message);
        assert!(
            err.helps.iter().any(|h| h.contains("`start`")),
            "{:?}",
            err.helps
        );
    }

    #[test]
    fn generic_method_error_is_in_defining_file() {
        let boxy = include_str!("../examples/honesty/gen_box.j3");
        let user = include_str!("../examples/honesty/gen_use.j3");
        let files = vec![
            parse_file(1, "boxy.j3", boxy),
            parse_file(2, "user.j3", user),
        ];
        let err = match check::check_files(files) {
            Ok(_) => panic!("expected generic body error"),
            Err(e) => e,
        };
        assert_eq!(err.path, "boxy.j3", "{}", err.message);
        assert!(
            err.notes.iter().any(|n| n.contains("instantiating")),
            "{:?}",
            err.notes
        );
        assert!(
            err.message.contains("expected `int`") && err.message.contains("found `string`"),
            "{}",
            err.message
        );
    }

    #[test]
    fn singleton_init_runs() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/singleton_init.j3");
        let prog = compile_path(&path).expect("compile singleton");
        let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
        assert!(c.contains("7ll") || c.contains("= 7"), "{c}");
        let run = exec(&prog);
        assert_eq!(run.status, 0, "{}", String::from_utf8_lossy(&run.stderr));
        assert_eq!(String::from_utf8_lossy(&run.stdout), "n=7\n");
    }

    #[test]
    fn fast_strips_abort() {
        let prog = compile_path(&speculative("05_arrays.j3")).expect("compile 05");
        let slow_ir = dump_program(&prog.cx, &prog.modules);
        assert!(slow_ir.contains("abort;"), "{slow_ir}");
        let mut fast_prog = compile_path(&speculative("05_arrays.j3")).expect("compile 05");
        apply_fast(&mut fast_prog);
        let fast_ir = dump_program(&fast_prog.cx, &fast_prog.modules);
        assert!(!fast_ir.contains("abort;"), "{fast_ir}");
    }

    #[test]
    fn fast_keeps_div0() {
        let src = r#"
module t;
fn main() {
    let n = 1;
    let d = 0;
    let q = n / d;
    let _ = q;
}
"#;
        let mut prog = check::check_files(vec![parse_file(1, "t.j3", src)]).expect("check");
        let slow_ir = dump_program(&prog.cx, &prog.modules);
        assert!(slow_ir.contains("abort div0;"), "{slow_ir}");
        apply_fast(&mut prog);
        let fast_ir = dump_program(&prog.cx, &prog.modules);
        assert!(fast_ir.contains("abort div0;"), "{fast_ir}");
    }

    #[test]
    fn read_file_j3_runs() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/read_file.j3");
        let prog = compile_path(&path).expect("compile read_file");
        let run = exec(&prog);
        assert_eq!(
            run.status,
            0,
            "stderr {}\nstdout {}",
            String::from_utf8_lossy(&run.stderr),
            String::from_utf8_lossy(&run.stdout)
        );
        assert_eq!(String::from_utf8_lossy(&run.stdout), "len=11\n");
    }

    #[test]
    fn bounce_project_typechecks() {
        let toml = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("examples/bounce/j3.toml");
        let m = crate::project::Manifest::load(&toml).expect("j3.toml");
        assert_eq!(m.name, "bounce");
        let prog = compile_manifest(&m).expect("compile bounce");
        let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
        assert!(c.contains("use lib \"SDL2\""), "{c}");
        let ll = emit::emit_llvm(&prog.cx, &prog.modules, &emit::EmitOpts::default()).expect("emit llvm");
        assert!(ll.contains("; use lib \"SDL2\""), "{ll}");
        assert_eq!(m.emit.backend, "llvm");
        assert!(
            prog.modules.iter().any(|mod_| {
                prog.cx.name_str(mod_.name[0]) == "bounce"
                    && (mod_.name.len() == 1 || prog.cx.name_str(mod_.name[1]) == "world")
            }),
            "expected bounce + bounce.world"
        );
        assert!(
            prog.modules.iter().any(|mod_| {
                prog.cx.name_str(mod_.name[0]) == "j3"
                    && mod_.name.len() > 1
                    && prog.cx.name_str(mod_.name[1]) == "app"
            }),
            "expected j3.app"
        );
    }

    #[test]
    fn defense_project_typechecks() {
        let toml = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("examples/defense/j3.toml");
        let m = crate::project::Manifest::load(&toml).expect("j3.toml");
        assert_eq!(m.name, "defense");
        let prog = compile_manifest(&m).expect("compile defense");
        emit::emit_program(&prog.cx, &prog.modules).expect("emit");
        assert!(
            prog.modules.iter().any(|mod_| {
                prog.cx.name_str(mod_.name[0]) == "defense"
                    && mod_.name.len() > 1
                    && prog.cx.name_str(mod_.name[1]) == "world"
            }),
            "expected defense.world"
        );
    }

    #[test]
    fn gallery_project_typechecks() {
        let toml = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("examples/gallery/j3.toml");
        let m = crate::project::Manifest::load(&toml).expect("j3.toml");
        assert_eq!(m.name, "gallery");
        let prog = compile_manifest(&m).expect("compile gallery");
        let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
        assert!(c.contains("use lib \"SDL2\""), "{c}");
        assert!(
            prog.modules.iter().any(|mod_| {
                prog.cx.name_str(mod_.name[0]) == "gallery"
                    && (mod_.name.len() == 1 || prog.cx.name_str(mod_.name[1]) == "world")
            }),
            "expected gallery + gallery.world"
        );
    }

    #[test]
    fn island_project_typechecks() {
        let toml = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("examples/island/j3.toml");
        let m = crate::project::Manifest::load(&toml).expect("j3.toml");
        assert_eq!(m.name, "island");
        let prog = compile_manifest(&m).expect("compile island");
        let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
        assert!(c.contains("use lib \"GL\""), "{c}");
        assert!(c.contains("use lib \"SDL2\""), "{c}");
        let ll = emit::emit_llvm(&prog.cx, &prog.modules, &emit::EmitOpts::default()).expect("emit llvm");
        assert!(ll.contains("; use lib \"GL\""), "{ll}");
        assert_eq!(m.emit.backend, "llvm");
        assert!(
            prog.modules.iter().any(|mod_| {
                prog.cx.name_str(mod_.name[0]) == "island"
                    && (mod_.name.len() == 1 || prog.cx.name_str(mod_.name[1]) == "world")
            }),
            "expected island + island.world"
        );
        assert!(
            prog.modules.iter().any(|mod_| {
                prog.cx.name_str(mod_.name[0]) == "j3"
                    && mod_.name.len() > 1
                    && prog.cx.name_str(mod_.name[1]) == "gfx"
            }),
            "expected j3.gfx"
        );
    }

    #[test]
    fn dungeon_project_typechecks() {
        let toml = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("examples/dungeon/j3.toml");
        let m = crate::project::Manifest::load(&toml).expect("j3.toml");
        assert_eq!(m.name, "dungeon");
        let prog = compile_manifest(&m).expect("compile dungeon");
        let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
        assert!(c.contains("use lib \"SDL2\""), "{c}");
        let ll = emit::emit_llvm(&prog.cx, &prog.modules, &emit::EmitOpts::default()).expect("emit llvm");
        assert!(ll.contains("; use lib \"SDL2\""), "{ll}");
        assert_eq!(m.emit.backend, "llvm");
        assert!(
            prog.modules.iter().any(|mod_| {
                prog.cx.name_str(mod_.name[0]) == "dungeon"
                    && mod_.name.len() > 1
                    && prog.cx.name_str(mod_.name[1]) == "map"
            }),
            "expected dungeon.map"
        );
        assert!(
            prog.modules.iter().any(|mod_| {
                prog.cx.name_str(mod_.name[0]) == "j3"
                    && mod_.name.len() > 1
                    && prog.cx.name_str(mod_.name[1]) == "ecs"
            }),
            "expected j3.ecs"
        );
        assert!(
            prog.modules.iter().any(|mod_| {
                prog.cx.name_str(mod_.name[0]) == "j3"
                    && mod_.name.len() > 1
                    && prog.cx.name_str(mod_.name[1]) == "app"
            }),
            "expected j3.app"
        );
    }

    fn honesty(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("examples/honesty")
            .join(name)
    }

    fn run_ok(path: &Path, stdout: &str) {
        let prog = compile_path(path).expect("compile");
        let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
        assert!(!c.contains("({"), "GNU statement-expr in C:\n{c}");
        let run = exec(&prog);
        assert_eq!(
            run.status,
            0,
            "stderr {}\nstdout {}\nc:\n{c}",
            String::from_utf8_lossy(&run.stderr),
            String::from_utf8_lossy(&run.stdout)
        );
        assert_eq!(String::from_utf8_lossy(&run.stdout), stdout);
    }

    #[test]
    fn short_circuit_skips_oob() {
        run_ok(&honesty("short_circuit.j3"), "ok\n");
    }

    #[test]
    fn if_expr_skips_untaken_arm() {
        run_ok(&honesty("if_expr_skip.j3"), "ok\n");
    }

    #[test]
    fn defer_does_not_clobber_return() {
        run_ok(&honesty("defer_return.j3"), "ok\n");
    }

    #[test]
    fn vec2_assign_op_calls_operator() {
        run_ok(&honesty("vec2_add_eq.j3"), "ok\n");
    }

    #[test]
    fn div_by_zero_aborts() {
        let prog = compile_path(&honesty("div0.j3")).expect("compile");
        let run = exec(&prog);
        assert_ne!(run.status, 0, "div0 should abort");
    }

    #[test]
    fn const_div_by_zero_is_error() {
        let files = vec![parse_file(
            1,
            "t.j3",
            "module t;\nconst X = 1 / 0;\nfn main() {}\n",
        )];
        let err = match check::check_files(files) {
            Ok(_) => panic!("expected /0 error"),
            Err(e) => e,
        };
        assert!(err.message.contains("division by zero"), "{}", err.message);
    }

    #[test]
    fn string_index_is_not_an_lvalue() {
        let src = "module t;\nfn main() {\n    let s = \"hi\";\n    let p = &s[0];\n}\n";
        let files = vec![parse_file(1, "t.j3", src)];
        let err = match check::check_files(files) {
            Ok(_) => panic!("expected &s[0] error"),
            Err(e) => e,
        };
        assert!(
            err.message.contains("cannot take `&`") || err.message.contains("not a place"),
            "{}",
            err.message
        );
    }

    #[test]
    fn string_slice_store_is_error() {
        let src = "module t;\nfn main() {\n    let s = \"hi\";\n    let mid = s[0..1];\n    mid[0] = 65;\n}\n";
        let files = vec![parse_file(1, "t.j3", src)];
        let err = match check::check_files(files) {
            Ok(_) => panic!("expected string slice store error"),
            Err(e) => e,
        };
        assert!(
            err.message.contains("cannot assign") || err.message.contains("not a place"),
            "{}",
            err.message
        );
    }

    #[test]
    fn arena_field_is_error() {
        let err = match compile_path(&honesty("arena_field.j3")) {
            Ok(_) => panic!("expected Arena field error"),
            Err(e) => e,
        };
        assert!(err.contains("Arena by value") || err.contains("&Arena"), "{err}");
    }

    #[test]
    fn windows_hello_uses_kernel32() {
        let prog = compile_path_target(&default_hello_path(), "x86_64-pc-windows-gnu")
            .expect("compile windows hello");
        let dump = dump_program(&prog.cx, &prog.modules);
        assert!(!dump.contains("@syscall("), "{dump}");
        let mut opts = emit::EmitOpts::default();
        opts.target = "x86_64-pc-windows-gnu".to_string();
        let ll = emit::emit_llvm(&prog.cx, &prog.modules, &opts).expect("llvm");
        assert!(ll.contains("target triple = \"x86_64-pc-windows-gnu\""));
        assert!(ll.contains("CreateFileA"), "{ll}");
        assert!(ll.contains("VirtualAlloc"), "{ll}");
        assert!(!ll.contains("@syscall"), "{ll}");
        assert!(ll.contains("DILocation"), "{ll}");
        assert!(ll.contains("hello.j3"), "{ll}");
        let c = emit::emit_program(&prog.cx, &prog.modules).expect("c");
        assert!(!c.contains("unistd.h"), "{c}");
        assert!(!c.contains("j3_syscall"), "{c}");
        if mingw_clang() {
            emit::compile_text(&ll, &opts).expect("link windows hello");
        }
    }

    #[test]
    fn macos_hello_uses_libc() {
        let prog = compile_path_target(&default_hello_path(), "arm64-apple-darwin")
            .expect("compile macos hello");
        let mut opts = emit::EmitOpts::default();
        opts.target = "arm64-apple-darwin".to_string();
        let ll = emit::emit_llvm(&prog.cx, &prog.modules, &opts).expect("llvm");
        assert!(ll.contains("target triple = \"arm64-apple-darwin\""));
        assert!(ll.contains("@mmap"), "{ll}");
        assert!(!ll.contains("@syscall"), "{ll}");
    }

    #[test]
    fn llvm_line_info_follows_debug_flag() {
        let prog = compile_path(&default_hello_path()).expect("compile");
        let mut opts = emit::EmitOpts::default();
        let ll = emit::emit_llvm(&prog.cx, &prog.modules, &opts).expect("llvm");
        assert!(ll.contains("DILocation(line: 5"), "{ll}");
        assert!(ll.contains("hello.j3"), "{ll}");
        opts.cc.debug = false;
        let bare = emit::emit_llvm(&prog.cx, &prog.modules, &opts).expect("llvm");
        assert!(!bare.contains("DILocation"), "{bare}");
    }

    #[test]
    fn llvm_names_locals_when_debug() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/mesh.j3");
        let prog = compile_path(&path).expect("compile mesh");
        let mut opts = emit::EmitOpts::default();
        let ll = emit::emit_llvm(&prog.cx, &prog.modules, &opts).expect("llvm");
        assert!(ll.contains("DILocalVariable(name: \"q\""), "{ll}");
        assert!(ll.contains("llvm.dbg.declare"), "{ll}");
        assert!(ll.contains("= alloca "), "{ll}");
        opts.cc.debug = false;
        let bare = emit::emit_llvm(&prog.cx, &prog.modules, &opts).expect("llvm");
        assert!(!bare.contains("DILocalVariable"), "{bare}");
        assert!(!bare.contains("llvm.dbg.declare"), "{bare}");
    }

    #[test]
    fn c_declares_exit_without_syscall() {
        let prog = compile_path_target(
            &speculative("01_basics.j3"),
            "x86_64-pc-windows-gnu",
        )
        .expect("windows 01");
        let c = emit::emit_program(&prog.cx, &prog.modules).expect("c");
        assert!(!c.contains("j3_syscall"), "{c}");
        assert!(c.contains("void _exit(int);"), "{c}");
    }

    #[test]
    fn target_list_picks_a_sys_file_per_triple() {
        let toml = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("examples/dungeon/j3.toml");
        let mut m = crate::project::Manifest::load(&toml).expect("j3.toml");
        m.emit.targets = vec![
            "x86_64-pc-windows-gnu".to_string(),
            "x86_64-pc-linux-gnu".to_string(),
        ];
        assert_eq!(m.emit.primary(), "x86_64-pc-linux-gnu");
        let host = compile_manifest(&m).expect("host");
        let host_ir = dump_program(&host.cx, &host.modules);
        assert!(host_ir.contains("@syscall("), "{host_ir}");
        let win = compile_manifest_target(&m, "x86_64-pc-windows-gnu").expect("windows");
        let win_ir = dump_program(&win.cx, &win.modules);
        assert!(!win_ir.contains("@syscall("), "{win_ir}");
        assert!(win_ir.contains("CreateFileA") || win_ir.contains("kernel32"), "{win_ir}");
    }

    #[test]
    fn windows_dungeon_checks_and_missing_sdl_is_a_link_error() {
        let toml = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("examples/dungeon/j3.toml");
        let mut m = crate::project::Manifest::load(&toml).expect("j3.toml");
        m.emit.targets = vec!["x86_64-pc-windows-gnu".to_string()];
        let prog = compile_manifest(&m).expect("typecheck windows dungeon");
        let opts = emit::EmitOpts::from_manifest(&m).expect("opts");
        let ll = emit::emit_llvm(&prog.cx, &prog.modules, &opts).expect("llvm");
        assert!(ll.contains("CreateFileA"), "{ll}");
        assert!(!ll.contains("@syscall"), "{ll}");
        if !mingw_clang() {
            return;
        }
        match emit::compile_text(&ll, &opts) {
            Ok(_) => {}
            Err(e) => {
                let err = e.to_lowercase();
                assert!(
                    err.contains("cannot find")
                        || err.contains("undefined")
                        || err.contains("unresolved")
                        || err.contains("library not found")
                        || err.contains("-lsdl2"),
                    "{e}"
                );
            }
        }
    }

    fn mingw_clang() -> bool {
        let dir = std::env::temp_dir();
        let src = dir.join(format!("j3c_mingw_probe_{}.c", std::process::id()));
        let obj = dir.join(format!("j3c_mingw_probe_{}.o", std::process::id()));
        if std::fs::write(&src, "int main(void){return 0;}\n").is_err() {
            return false;
        }
        let ok = std::process::Command::new("clang")
            .args(["--target=x86_64-pc-windows-gnu", "-c"])
            .arg("-o")
            .arg(&obj)
            .arg(&src)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        let _ = std::fs::remove_file(&src);
        let _ = std::fs::remove_file(&obj);
        ok
    }

    fn user_dump(prog: &Program) -> String {
        let mut out = String::new();
        for module in &prog.modules {
            if prog.cx.name_str(module.name[0]) == "j3" {
                continue;
            }
            crate::ir::dump::dump_module_into(&prog.cx, module, &mut out);
            out.push('\n');
        }
        out
    }

    fn assert_golden(name: &str, got: &str) {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/ir/goldens")
            .join(name);
        if std::env::var("J3C_WRITE_GOLDENS").is_ok() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, got).unwrap();
            return;
        }
        let want = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!("missing golden {}: {e} (J3C_WRITE_GOLDENS=1 to create)", path.display())
        });
        assert_eq!(want, got, "golden {name} drifted");
    }

    #[test]
    fn ir_goldens_01_through_08_and_samples() {
        let samples = crate::ir::samples::place_samples();
        assert_golden(
            "samples.ir",
            &crate::ir::dump::dump_module(&samples.cx, &samples.module),
        );
        for name in [
            "01_basics.j3",
            "02_data.j3",
            "03_embedding.j3",
            "04_control.j3",
            "05_arrays.j3",
            "06_enums_errors.j3",
            "07_memory.j3",
            "08_game_loop.j3",
        ] {
            let prog = if name == "08_game_loop.j3" {
                compile_path_headless(&speculative(name)).expect(name)
            } else {
                compile_path(&speculative(name)).expect(name)
            };
            let stem = name.trim_end_matches(".j3");
            assert_golden(&format!("{stem}.ir"), &user_dump(&prog));
        }
    }
}
