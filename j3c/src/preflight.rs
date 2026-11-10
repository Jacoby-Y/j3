//! After typecheck, `j3c check` asks whether this machine can run the program.
//! A missing linked library, a missing dlopen library, or a missing file passed
//! to a loader is an error here. The program is not executed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::check::Program;
use crate::diag::Diagnostic;
use crate::emit::{failure_hint, host_lib_present};
use crate::ir::body::Block;
use crate::ir::cx::IrCx;
use crate::ir::entity::FuncKind;
use crate::ir::ids::LocalId;
use crate::ir::operand::{ConstKind, Operand};
use crate::ir::place::PlaceRoot;
use crate::ir::rvalue::{Rvalue, RvalueKind};
use crate::ir::stmt::{Stmt, StmtKind};
use crate::project::Manifest;
use crate::span::Span;

struct Api {
    link_name: &'static str,
    soname: &'static str,
    /// Index of the path argument, when the call loads a file.
    path_arg: Option<usize>,
    /// Missing library or file aborts the process.
    fatal: bool,
}

pub fn check(prog: &Program, manifest: Option<&Manifest>) -> Result<(), String> {
    let diags = collect(prog, manifest);
    if diags.is_empty() {
        return Ok(());
    }
    let sources: Vec<(String, String)> = prog
        .cx
        .files
        .iter()
        .map(|f| (f.path.clone(), f.src.clone()))
        .collect();
    let mut out = String::new();
    for diag in diags {
        out.push_str(&diag.render(&sources));
        if !out.ends_with('\n') {
            out.push('\n');
        }
    }
    Err(out)
}

fn collect(prog: &Program, manifest: Option<&Manifest>) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let mut seen = Vec::new();
    let search: Vec<String> = manifest
        .map(|m| {
            m.link.search.iter().map(|dir| {
                let path = PathBuf::from(dir);
                if path.is_absolute() { dir.clone() }
                else { m.root.join(dir).display().to_string() }
            }).collect()
        })
        .unwrap_or_default();
    let root = asset_root(prog, manifest);
    let probe_libs = manifest
        .map(|m| !is_foreign(m.emit.primary()))
        .unwrap_or(true);

    if probe_libs {
        let mut libs = link_libs(&prog.cx);
        if let Some(m) = manifest {
            for lib in &m.link.libs {
                if !libs.iter().any(|l| l == lib) {
                    libs.push(lib.clone());
                }
            }
        }
        for lib in libs {
            if host_lib_present(&lib, &search) {
                continue;
            }
            push_once(
                &mut diags,
                &mut seen,
                format!("lib:{lib}"),
                lib_diag(manifest, &lib),
            );
        }
    }

    for func in prog.cx.funcs() {
        let FuncKind::J3 { body } = &func.kind else {
            continue;
        };
        let Some(file) = prog.cx.file(func.span.file) else {
            continue;
        };
        if file.path.contains("stdlib/j3/") {
            continue;
        }
        let literals = string_locals(&body.block);
        walk_block(&body.block, &mut |rv| {
            let RvalueKind::Call { func: callee, args } = &rv.kind else {
                return;
            };
            let Some(id) = fn_addr(callee) else {
                return;
            };
            let called = prog.cx.func(id);
            let module = module_name(&prog.cx, &called.module);
            let name = prog.cx.name_str(called.name);
            let Some(api) = api(&module, name) else {
                return;
            };
            // Image loads fall back to the J3 PNG decoder, so SDL2_image is optional.
            if probe_libs && api.link_name != "SDL2_image" && !host_lib_present(api.link_name, &search) {
                push_once(
                    &mut diags,
                    &mut seen,
                    format!("call:{}:{}", name, api.soname),
                    call_lib_diag(prog, rv.span, name, &api),
                );
            }
            let Some(index) = api.path_arg else {
                return;
            };
            let Some(arg) = args.get(index) else {
                return;
            };
            let Some(bytes) = literal_bytes(arg, &literals) else {
                return;
            };
            let text = String::from_utf8_lossy(&bytes);
            if text.is_empty() {
                return;
            }
            let full = resolve_asset(&root, &text);
            if full.is_file() {
                return;
            }
            push_once(
                &mut diags,
                &mut seen,
                format!("file:{name}:{text}"),
                call_file_diag(prog, rv.span, name, &api, &text),
            );
        });
    }
    diags
}

fn push_once(diags: &mut Vec<Diagnostic>, seen: &mut Vec<String>, key: String, diag: Diagnostic) {
    if seen.iter().any(|k| k == &key) {
        return;
    }
    seen.push(key);
    diags.push(diag);
}

fn lib_diag(manifest: Option<&Manifest>, lib: &str) -> Diagnostic {
    let path = manifest
        .map(|m| m.root.join("j3.toml").display().to_string())
        .unwrap_or_else(|| "j3.toml".to_string());
    let msg = format!("cannot find library `{lib}`");
    let mut diag = Diagnostic::error(path, Span::dummy(), msg.clone());
    if let Some(hint) = failure_hint(&msg) {
        diag = diag.help(hint);
    }
    diag.note("j3c check does not run the program; this library is required to link it")
}

fn call_lib_diag(prog: &Program, span: Span, name: &str, api: &Api) -> Diagnostic {
    let path = file_path(prog, span);
    let msg = if api.fatal {
        format!("`{name}` panics unless `{}` is installed", api.soname)
    } else {
        format!(
            "`{name}` returns an empty handle because `{}` is not installed",
            api.soname
        )
    };
    let mut diag = Diagnostic::error(path, span, msg);
    if let Some(hint) = failure_hint(api.soname) {
        diag = diag.help(hint);
    }
    if !api.fatal {
        diag = diag.note("the process keeps running, and this load draws nothing");
    }
    diag
}

fn call_file_diag(prog: &Program, span: Span, name: &str, api: &Api, file: &str) -> Diagnostic {
    let path = file_path(prog, span);
    let msg = if api.fatal {
        format!("`{name}` panics because `{file}` is not a file")
    } else {
        format!("`{name}` returns an empty handle because `{file}` is not a file")
    };
    let mut diag = Diagnostic::error(path, span, msg).help(
        "paths are relative to the project directory; put the file there or change the path",
    );
    if !api.fatal {
        diag = diag.note("the process keeps running, and this load draws nothing");
    }
    diag
}

fn file_path(prog: &Program, span: Span) -> String {
    prog.cx
        .file(span.file)
        .map(|f| f.path.clone())
        .unwrap_or_default()
}

fn asset_root(prog: &Program, manifest: Option<&Manifest>) -> PathBuf {
    if let Some(m) = manifest {
        return m.root.clone();
    }
    for func in prog.cx.funcs() {
        if func.module.first().is_some_and(|n| prog.cx.name_str(*n) == "j3") {
            continue;
        }
        if let Some(file) = prog.cx.file(func.span.file) {
            if let Some(dir) = Path::new(&file.path).parent() {
                return dir.to_path_buf();
            }
        }
    }
    PathBuf::from(".")
}

fn resolve_asset(root: &Path, literal: &str) -> PathBuf {
    let path = Path::new(literal);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn is_foreign(target: &str) -> bool {
    let t = target.to_ascii_lowercase();
    t.contains("windows") || t.contains("apple") || t.contains("darwin")
}

fn link_libs(cx: &IrCx) -> Vec<String> {
    let mut libs = Vec::new();
    for func in cx.funcs() {
        let FuncKind::Lib { lib, .. } = &func.kind else {
            continue;
        };
        let name = cx.name_str(*lib);
        if name == "m" {
            continue;
        }
        if !libs.iter().any(|l| l == name) {
            libs.push(name.to_string());
        }
    }
    libs
}

fn api(module: &str, name: &str) -> Option<Api> {
    if module != "j3.game" {
        return None;
    }
    let font = Api {
        link_name: "SDL2_ttf",
        soname: "libSDL2_ttf-2.0.so.0",
        path_arg: Some(0),
        fatal: false,
    };
    let image = Api {
        link_name: "SDL2_image",
        soname: "libSDL2_image-2.0.so.0",
        path_arg: Some(0),
        fatal: false,
    };
    let sound = Api {
        link_name: "SDL2_mixer",
        soname: "libSDL2_mixer-2.0.so.0",
        path_arg: Some(0),
        fatal: false,
    };
    Some(match name {
        "loadFont" => Api { fatal: true, ..font },
        "tryLoadFont" | "tryLoadFontC" => font,
        "loadTexture" => Api { fatal: true, ..image },
        "tryLoadTexture" | "tryLoadTextureC" | "loadRgba" => image,
        "loadSound" | "loadMusic" => Api { fatal: true, ..sound },
        "tryLoadSound" | "tryLoadSoundC" | "tryLoadMusic" | "tryLoadMusicC" => sound,
        "setVolume" | "setMusicVolume" | "stop" | "stopMusic" => Api {
            path_arg: None,
            fatal: true,
            ..sound
        },
        _ => return None,
    })
}

fn module_name(cx: &IrCx, module: &[crate::intern::Name]) -> String {
    module
        .iter()
        .map(|n| cx.name_str(*n))
        .collect::<Vec<_>>()
        .join(".")
}

fn fn_addr(op: &Operand) -> Option<crate::ir::ids::FuncId> {
    let Operand::Const(c) = op else {
        return None;
    };
    match c.kind {
        ConstKind::FnAddr(id) => Some(id),
        _ => None,
    }
}

fn literal_bytes(op: &Operand, locals: &HashMap<LocalId, Vec<u8>>) -> Option<Vec<u8>> {
    match op {
        Operand::Const(c) => match &c.kind {
            ConstKind::Bytes(bytes) => Some(bytes.clone()),
            _ => None,
        },
        Operand::Copy(place) => {
            if !place.proj.is_empty() {
                return None;
            }
            let PlaceRoot::Local(id) = place.root else {
                return None;
            };
            locals.get(&id).cloned()
        }
    }
}

fn string_locals(block: &Block) -> HashMap<LocalId, Vec<u8>> {
    let mut out = HashMap::new();
    collect_string_locals(block, &mut out);
    out
}

fn collect_string_locals(block: &Block, out: &mut HashMap<LocalId, Vec<u8>>) {
    for stmt in &block.stmts {
        match &stmt.kind {
            StmtKind::Assign { place, rvalue } => {
                if place.proj.is_empty() {
                    if let PlaceRoot::Local(id) = place.root {
                        if let RvalueKind::Use(Operand::Const(c)) = &rvalue.kind {
                            if let ConstKind::Bytes(bytes) = &c.kind {
                                out.insert(id, bytes.clone());
                            }
                        }
                    }
                }
            }
            StmtKind::If {
                then_block,
                else_block,
                ..
            } => {
                collect_string_locals(then_block, out);
                if let Some(else_block) = else_block {
                    collect_string_locals(else_block, out);
                }
            }
            StmtKind::While { body, .. } | StmtKind::ForRange { body, .. } | StmtKind::Defer { body } => {
                collect_string_locals(body, out);
            }
            StmtKind::Block(body) => collect_string_locals(body, out),
            StmtKind::Switch { cases, default, .. } => {
                for (_, b) in cases {
                    collect_string_locals(b, out);
                }
                collect_string_locals(default, out);
            }
            _ => {}
        }
    }
}

fn walk_block(block: &Block, visit: &mut dyn FnMut(&Rvalue)) {
    for stmt in &block.stmts {
        walk_stmt(stmt, visit);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::compile::compile_manifest;
    use crate::project::Manifest;

    #[test]
    fn hello_has_nothing_to_probe() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/programs/hello.j3");
        let prog = crate::compile::compile_path(&path).expect("hello");
        super::check(&prog, None).expect("hello links nothing extra");
    }

    #[test]
    fn missing_font_paths_are_errors() {
        let dir = std::env::temp_dir().join(format!("j3-preflight-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("j3.toml"),
            "[package]\nname = \"probe\"\nentry = \"src/main.j3\"\n\n[link]\nlibs = [\"SDL2\"]\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("src/main.j3"),
            "module probe;\n\nuse j3.game.{ loadFont, tryLoadFont };\n\nfn main() {\n    let missing = tryLoadFont(\"nope.ttf\", 12);\n    let fatal = loadFont(\"also-nope.ttf\", 12);\n    if missing.raw == 0 {\n        return;\n    }\n    if fatal.raw == 0 {\n        return;\n    }\n}\n",
        )
        .unwrap();
        let manifest = Manifest::load(&dir.join("j3.toml")).expect("toml");
        let prog = compile_manifest(&manifest).expect("typecheck");
        let err = super::check(&prog, Some(&manifest)).expect_err("missing fonts");
        assert!(err.contains("nope.ttf"), "{err}");
        assert!(err.contains("also-nope.ttf"), "{err}");
        assert!(err.contains("empty handle"), "{err}");
        assert!(err.contains("panics"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

fn walk_stmt(stmt: &Stmt, visit: &mut dyn FnMut(&Rvalue)) {
    match &stmt.kind {
        StmtKind::Assign { rvalue, .. } | StmtKind::Expr(rvalue) => visit(rvalue),
        StmtKind::If {
            then_block,
            else_block,
            ..
        } => {
            walk_block(then_block, visit);
            if let Some(else_block) = else_block {
                walk_block(else_block, visit);
            }
        }
        StmtKind::While { body, .. } | StmtKind::ForRange { body, .. } | StmtKind::Defer { body } => {
            walk_block(body, visit);
        }
        StmtKind::Block(body) => walk_block(body, visit),
        StmtKind::Switch { cases, default, .. } => {
            for (_, b) in cases {
                walk_block(b, visit);
            }
            walk_block(default, visit);
        }
        _ => {}
    }
}
