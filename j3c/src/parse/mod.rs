//! Concrete syntax. Compiler and LSP both depend on this module.
//!
//! Live: modules, use, `use lib`, const, fn, struct, methods, `operator`,
//! `#[Debug]` / `#[singleton]` / `#[TryCatch]`, field defaults, let, assign/`+=`,
//! `&`/`*`, calls, `@syscall`, `as`, fields, arrays/slices, arithmetic,
//! comparisons, `if`/`while`/`for` (`..` and `..=`)/`defer`, interpolate, floats,
//! enum, match, `?`, generics, turbofish, `@sizeof` / `@slice` / `@j3ui`.

mod ast;
mod error;
pub(crate) mod lex;
mod parser;
pub mod syntax;
mod token;

pub use ast::{
    AssignOpStmt, AssignStmt, BinOp, Block, ConstItem, EnumItem, Expr, ExprKind, FieldItem, File,
    FnItem, ForEachStmt, Ident, InterpPart, Item, LetStmt, LibFnDecl, MatchArm, MatchStmt,
    ModLetItem, Param, Pat, ReturnStmt, Stmt, StructItem, Type, TypeKind, UnaryOp, UseItem,
    UseLibItem, UseLocalStmt, VariantItem,
};
pub use error::ParseError;
pub use token::{keyword, Token, TokenKind};

use crate::span::FileId;

pub fn parse(src: &str, file: FileId) -> Result<File, ParseError> {
    let tokens = lex::lex(src, file)?;
    parser::parse_tokens(&tokens)
}

pub fn format_error(path: &str, src: &str, err: &ParseError) -> String {
    crate::diag::Diagnostic::error(path, err.span, &err.message)
        .render(&[(path.to_string(), src.to_string())])
}

pub fn line_of(src: &str, start: u32) -> usize {
    let n = start as usize;
    if n > src.len() {
        return src.bytes().filter(|b| *b == b'\n').count() + 1;
    }
    src[..n].bytes().filter(|b| *b == b'\n').count() + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::FileId;

    fn parse_ok(src: &str) -> File {
        match parse(src, FileId(1)) {
            Ok(f) => f,
            Err(e) => panic!("{}", format_error("<test>", src, &e)),
        }
    }

    #[test]
    fn parse_hello_module() {
        let f = parse_ok(
            r#"
module hello;
use j3.io.printLn;
fn main() {
    printLn("Hello, world!");
}
"#,
        );
        assert_eq!(f.module_key(), "hello");
        assert_eq!(f.items.len(), 2);
        assert!(f.doc.is_empty(), "{}", f.doc);
    }

    #[test]
    fn module_doc_sits_on_the_module_line() {
        let f = parse_ok("/// Window and input.\nmodule j3.game;\n");
        assert_eq!(f.doc, "Window and input.");
        assert_eq!(f.module_key(), "j3.game");
    }

    #[test]
    fn parse_sys_file() {
        let src = include_str!("../../stdlib/j3/sys.j3");
        let f = parse_ok(src);
        assert_eq!(f.module_key(), "j3.sys");
        let fns: Vec<&str> = f
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Fn(f) => Some(f.name.name.as_str()),
                _ => None,
            })
            .collect();
        assert!(fns.contains(&"write"));
        assert!(fns.contains(&"start"));
        assert!(fns.contains(&"mmapAnon"));
    }

    #[test]
    fn parse_io_file() {
        let src = include_str!("../../stdlib/j3/io.j3");
        let f = parse_ok(src);
        assert_eq!(f.module_key(), "j3.io");
    }

    #[test]
    fn parse_control_file() {
        let src = include_str!("../../../speculative/04_control.j3");
        let f = parse_ok(src);
        assert_eq!(f.module_key(), "speculative.control");
    }

    #[test]
    fn parse_text_file() {
        let src = include_str!("../../stdlib/j3/text.j3");
        let f = parse_ok(src);
        assert_eq!(f.module_key(), "j3.text");
        assert!(f.items.iter().any(|i| matches!(i, Item::Let(_))));
    }

    #[test]
    fn parse_stdlib_game_file() {
        let src = include_str!("../../stdlib/j3/game.j3");
        let f = parse_ok(src);
        assert_eq!(f.module_key(), "j3.game");
        assert!(f.items.iter().any(|i| matches!(i, Item::Let(_))));
        assert!(f.items.iter().any(|i| matches!(i, Item::UseLib(_))));
    }

    #[test]
    fn parse_option_result_files() {
        let opt = parse_ok(include_str!("../../stdlib/j3/option.j3"));
        assert_eq!(opt.module_key(), "j3.option");
        assert!(opt.items.iter().any(|i| matches!(i, Item::Enum(_))));
        let res = parse_ok(include_str!("../../stdlib/j3/result.j3"));
        assert_eq!(res.module_key(), "j3.result");
        assert!(res.items.iter().any(|i| matches!(i, Item::Enum(_))));
    }

    #[test]
    fn parse_math_file() {
        let src = include_str!("../../stdlib/j3/math.j3");
        let f = parse_ok(src);
        assert_eq!(f.module_key(), "j3.math");
        assert!(f.items.iter().any(|i| matches!(i, Item::UseLib(_))));
        assert!(f.items.iter().any(|i| matches!(i, Item::Struct(_))));
    }

    #[test]
    fn parse_rand_file() {
        let src = include_str!("../../stdlib/j3/rand.j3");
        let f = parse_ok(src);
        assert_eq!(f.module_key(), "j3.rand");
        assert!(f.items.iter().any(|i| matches!(i, Item::Struct(_))));
    }

    #[test]
    fn parse_ecs_file() {
        let src = include_str!("../../stdlib/j3/game/ecs.j3");
        let f = parse_ok(src);
        assert_eq!(f.module_key(), "j3.game.ecs");
        let structs: Vec<&str> = f
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Struct(s) => Some(s.name.name.as_str()),
                _ => None,
            })
            .collect();
        assert!(structs.contains(&"Entity"));
        assert!(structs.contains(&"Ecs"));
        assert!(structs.contains(&"Column"));
        assert!(structs.contains(&"Commands"));
        assert!(structs.contains(&"Events"));
    }

    #[test]
    fn parse_collide_file() {
        let src = include_str!("../../stdlib/j3/game/collide.j3");
        let f = parse_ok(src);
        assert_eq!(f.module_key(), "j3.game.collide");
    }

    #[test]
    fn parse_framework_files() {
        for (src, key) in [
            (include_str!("../../stdlib/j3/data/res.j3"), "j3.data.res"),
            (include_str!("../../stdlib/j3/data/scene.j3"), "j3.data.scene"),
            (include_str!("../../stdlib/j3/data/assets.j3"), "j3.data.assets"),
            (include_str!("../../stdlib/j3/game/tile.j3"), "j3.game.tile"),
            (include_str!("../../stdlib/j3/ui.j3"), "j3.ui"),
            (include_str!("../../stdlib/j3/data/save.j3"), "j3.data.save"),
            (include_str!("../../stdlib/j3/game/fx.j3"), "j3.game.fx"),
            (include_str!("../../stdlib/j3/data/debug.j3"), "j3.data.debug"),
            (include_str!("../../stdlib/j3/gfx.j3"), "j3.gfx"),
            (include_str!("../../stdlib/j3/gfx/mesh.j3"), "j3.gfx.mesh"),
            (include_str!("../../stdlib/j3/gfx/voxel.j3"), "j3.gfx.voxel"),
        ] {
            let f = parse_ok(src);
            assert_eq!(f.module_key(), key);
        }
    }

    #[test]
    fn parse_app_file() {
        let src = include_str!("../../stdlib/j3/app.j3");
        let f = parse_ok(src);
        assert_eq!(f.module_key(), "j3.app");
        let structs: Vec<&str> = f
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Struct(s) => Some(s.name.name.as_str()),
                _ => None,
            })
            .collect();
        assert!(structs.contains(&"App"));
    }

    #[test]
    fn parse_data_file() {
        let src = include_str!("../../../speculative/02_data.j3");
        let f = parse_ok(src);
        assert_eq!(f.module_key(), "speculative.data");
        let structs: Vec<&str> = f
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Struct(s) => Some(s.name.name.as_str()),
                _ => None,
            })
            .collect();
        assert!(structs.contains(&"Vec2"));
        assert!(structs.contains(&"Entity"));
    }

    #[test]
    fn parse_enums_file() {
        let src = include_str!("../../../speculative/06_enums_errors.j3");
        let f = parse_ok(src);
        assert_eq!(f.module_key(), "speculative.enums_errors");
        assert!(f.items.iter().any(|i| matches!(i, Item::Enum(_))));
    }

    #[test]
    fn parse_embed_file() {
        let src = include_str!("../../../speculative/03_embedding.j3");
        let f = parse_ok(src);
        assert_eq!(f.module_key(), "speculative.embedding");
    }

    #[test]
    fn parse_arrays_file() {
        let src = include_str!("../../../speculative/05_arrays.j3");
        let f = parse_ok(src);
        assert_eq!(f.module_key(), "speculative.arrays");
    }

    #[test]
    fn parse_memory_file() {
        let src = include_str!("../../../speculative/07_memory.j3");
        let f = parse_ok(src);
        assert_eq!(f.module_key(), "speculative.memory");
    }

    #[test]
    fn unknown_intrinsic_is_error() {
        let src = "module t;\nfn main() { @foo(1); }\n";
        match parse(src, FileId(1)) {
            Ok(_) => panic!("expected error"),
            Err(e) => assert!(e.message.contains("@foo"), "{}", e.message),
        }
    }
}
