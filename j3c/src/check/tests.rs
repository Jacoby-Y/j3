//! Typecheck tests: generic struct and fn instances across modules.

use super::*;

#[test]
fn generic_struct_instances_are_shared_across_modules() {
    let sources = [
        ("container", r#"
module container;
pub struct Box<T> {
    value: T;
    fn get(this: &Box<T>): T { return this.value; }
}
pub struct Item { value: int; }
"#),
        ("factory", r#"
module factory;
use container.{ Box, Item };
pub fn make(): Box<Item> { return Box<Item> { value: Item { value: 42 } }; }
"#),
        ("consumer", r#"
module consumer;
use container.{ Box, Item };
use factory.make;
struct World { item: Box<Item>; }
fn main() {
    let world = World { item: make() };
    let item = world.item.get();
}
"#),
    ];
    let files = sources.iter().enumerate().map(|(i, (name, src))| {
        let file_id = FileId(i as u32 + 1);
        ParsedFile {
            path: format!("{name}.j3"),
            src: src.to_string(),
            ast: parse::parse(src, file_id).expect("parse cross-module generic"),
            file_id,
        }
    }).collect();
    let prog = check_files(files).expect("share generic instances across modules");
    assert_eq!(prog.cx.structs().iter().filter(|s| !s.ty_args.is_empty()).count(), 1);
}

#[test]
fn generic_enum_instances_are_shared_across_modules() {
    let sources = [
        ("maybe", r#"
module maybe;
pub enum Maybe<T> { No, Yes(T), }
"#),
        ("factory", r#"
module factory;
use maybe.Maybe;
pub fn make(): Maybe<int> { return Maybe.Yes(42); }
"#),
        ("consumer", r#"
module consumer;
use maybe.Maybe;
use factory.make;
fn pass(m: Maybe<int>): Maybe<int> { return m; }
fn main() {
    let x = pass(make());
}
"#),
    ];
    let files = sources.iter().enumerate().map(|(i, (name, src))| {
        let file_id = FileId(i as u32 + 1);
        ParsedFile {
            path: format!("{name}.j3"),
            src: src.to_string(),
            ast: parse::parse(src, file_id).expect("parse cross-module generic enum"),
            file_id,
        }
    }).collect();
    let prog = check_files(files).expect("share generic enum instances across modules");
    assert_eq!(prog.cx.enums().iter().filter(|e| !e.ty_args.is_empty()).count(), 1);
}

#[test]
fn omitted_returns_use_instantiated_parameter_types() {
    let src = r#"
module test;

struct Box<T> { value: T; }

fn clear(box: &Box<int>) { box.value = 0; }
fn value(box: &Box<int>) { return box.value; }
fn identity(box: &Box<int>) { return box; }

struct Reader {
    fn read(this: &Reader, box: &Box<int>) { return box.value; }
}

fn main() {
    let box = Box<int> { value: 42 };
    let reader = Reader {};
    let n = value(&box);
    let m = reader.read(&box);
    let alias = identity(&box);
    clear(alias);
}
"#;
    let file_id = FileId(1);
    let ast = parse::parse(src, file_id).expect("parse generic parameters");
    let prog = check_files(vec![ParsedFile {
        path: "test.j3".to_string(),
        src: src.to_string(),
        ast,
        file_id,
    }])
    .expect("infer returns with generic parameters");
    for id in &prog.modules[0].funcs {
        let f = prog.cx.func(*id);
        let name = prog.cx.name_str(f.name);
        let (params, ret) = prog.cx.fn_sig(f.ty).expect("function signature");
        match name {
            "clear" => assert_eq!(ret, prog.cx.common.void),
            "value" | "read" => assert_eq!(ret, prog.cx.common.int),
            "identity" => assert_eq!(ret, params[0]),
            _ => {}
        }
    }
}
