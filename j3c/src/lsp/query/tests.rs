//! Tests for hover, definition, and doc-comment display.

use super::*;
use crate::compile::default_hello_path;
use std::path::PathBuf;

#[test]
fn script_type_hover_and_path_goto() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../speculative/11_script/main.j3");
    let prog = analyze(&path, &[]).expect("analyze script");
    let src = std::fs::read_to_string(&path).unwrap();
    let shown = path.display().to_string();
    let type_at = src.find("const Door").unwrap() + "const ".len();
    let type_hover = hover(&prog, &shown, type_at as u32).expect("hover script type");
    assert!(
        type_hover.text.contains("script Door"),
        "{}",
        type_hover.text
    );
    assert!(
        type_hover.text.contains("prop hinge"),
        "{}",
        type_hover.text
    );
    assert!(
        type_hover.text.contains("pub fn toggle"),
        "{}",
        type_hover.text
    );
    let path_at = src.find("scripts/door.j3s").unwrap() + 4;
    let target = definition(&prog, &shown, path_at as u32).expect("script path goto");
    assert!(target.path.ends_with("scripts/door.j3s"), "{}", target.path);
    let script = target.path.clone();
    let script_src = std::fs::read_to_string(&script).unwrap();
    let prop_at = script_src.find("if open").unwrap() + 3;
    let hover = hover(&prog, &script, prop_at as u32).expect("state hover");
    assert!(hover.text.contains("state open: bool"), "{}", hover.text);
}

#[test]
fn hover_print_ln_on_hello() {
    let path = default_hello_path();
    let prog = analyze(&path, &[]).expect("analyze hello");
    let src = std::fs::read_to_string(&path).unwrap();
    let off = src.find("printLn").expect("printLn") as u32;
    let h = hover(&prog, &path.display().to_string(), off).expect("hover");
    assert!(h.text.contains("fn") || h.text.contains("printLn"), "{}", h.text);
    let d = definition(&prog, &path.display().to_string(), off).expect("def");
    assert!(d.path.contains("io.j3"), "{}", d.path);
    let items = complete(&prog, &path.display().to_string(), off + 5);
    assert!(
        items.iter().any(|c| c.label == "printLn"),
        "{:?}",
        items.iter().map(|c| c.label.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn doc_comment_shows_on_hover_and_completion() {
    let path = default_hello_path();
    let src = "\
module hello;

/// Prints a line.
fn greet(name: string) {
let _ = name;
}

fn main() {
/// the player
let player = 1;
greet(\"a\");
let _ = player;
}
";
    let overlay = vec![(path.clone(), src.to_string())];
    let prog = analyze(&path, &overlay).expect("analyze");
    let shown = path.display().to_string();
    let greet = src.find("greet(\"a\")").unwrap() as u32;
    let h = hover(&prog, &shown, greet).expect("hover");
    assert!(h.text.contains("Prints a line."), "{}", h.text);
    let at = src.find("greet(\"a\")").unwrap();
    let items = complete(&prog, &shown, at as u32 + 2);
    let greet_item = items.iter().find(|c| c.label == "greet").expect("greet");
    assert!(greet_item.documentation.contains("Prints a line."), "{}", greet_item.documentation);
    let player = src.rfind("player").unwrap() as u32;
    let hp = hover(&prog, &shown, player).expect("player");
    assert!(hp.text.contains("the player"), "{}", hp.text);
}

#[test]
fn const_doc_shows_on_hover() {
    let path = default_hello_path();
    let src = "\
module hello;

use j3.math.PI;

fn main() {
let x = PI;
let _ = x;
}
";
    let overlay = vec![(path.clone(), src.to_string())];
    let prog = analyze(&path, &overlay).expect("analyze");
    let at = src.find("PI;").unwrap() as u32;
    let h = hover(&prog, &path.display().to_string(), at).expect("hover");
    assert!(h.text.contains("circumference"), "{}", h.text);
}

#[test]
fn array_const_analyzes_hovers_and_completes() {
    let path = default_hello_path();
    let src = "\
module hello;

/// Squares.
const SQ: [3]int = [1, 4, 9];

fn main() {
let x = SQ[1];
let _ = x;
}
";
    let overlay = vec![(path.clone(), src.to_string())];
    let prog = analyze(&path, &overlay).expect("analyze");
    let shown = path.display().to_string();
    let at = src.rfind("SQ").unwrap() as u32;
    let h = hover(&prog, &shown, at);
    eprintln!("array const hover: {:?}", h.as_ref().map(|h| h.text.clone()));
    let items = complete(&prog, &shown, at + 1);
    assert!(items.iter().any(|c| c.label == "SQ"), "completion lost SQ");
}

#[test]
fn else_if_expr_and_rvalue_field_analyze_and_hover() {
    let path = default_hello_path();
    let src = "\
module hello;

fn main() {
let xs: [4]int = [1, 2, 3, 4];
let i = 2;
let k = if i < 1 { 1 } else if i < 2 { 2 } else { 3 };
let n = xs[1..3].len;
let _ = k + n;
}
";
    let overlay = vec![(path.clone(), src.to_string())];
    let prog = analyze(&path, &overlay).expect("else-if and rvalue field analyze");
    let shown = path.display().to_string();
    let _ = hover(&prog, &shown, src.find("].len").unwrap() as u32 + 3);
    let _ = hover(&prog, &shown, src.find("else if").unwrap() as u32 + 1);
}

#[test]
fn complete_quat_rotate() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/programs/mesh.j3");
    let prog = analyze(&path, &[]).expect("analyze mesh");
    let src = std::fs::read_to_string(&path).unwrap();
    let at = src.find("q.rotate").expect("q.rotate") as u32;
    let items = complete(&prog, &path.display().to_string(), at + 2);
    assert!(
        items.iter().any(|c| c.label == "rotate"),
        "{:?}",
        items.iter().map(|c| c.label.clone()).collect::<Vec<_>>()
    );
}

/// Names bound by `if let` and `let .. else` are locals: hover gives their type, and
/// completion lists them.
#[test]
fn if_let_and_let_else_bindings_hover_and_complete() {
    let path = default_hello_path();
    let src = "\
module hello;

enum Tok { Num(int), Word(string), End }

fn f(t: Tok): int {
if let Tok.Word(word) = t {
let _ = word;
}
let Tok.Num(count) = t else { return 0; };
return count;
}

fn main() {
let _ = f(Tok.End);
}
";
    let overlay = vec![(path.clone(), src.to_string())];
    let prog = analyze(&path, &overlay).expect("analyze");
    let shown = path.display().to_string();
    let word = src.find("_ = word").unwrap() as u32 + 4;
    let h = hover(&prog, &shown, word).expect("hover word");
    assert!(h.text.contains("string"), "{}", h.text);
    let count = src.find("return count").unwrap() as u32 + 7;
    let h = hover(&prog, &shown, count).expect("hover count");
    assert!(h.text.contains("int"), "{}", h.text);
    let items = complete(&prog, &shown, count + 2);
    assert!(items.iter().any(|c| c.label == "count"), "{:?}", items.iter().map(|c| &c.label).collect::<Vec<_>>());
}

#[test]
fn hover_and_goto_on_dyn_and_char_literals() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../speculative/lang/dyn.j3");
    let prog = analyze(&path, &[]).expect("analyze dyn");
    let src = std::fs::read_to_string(&path).unwrap();
    let shown = path.display().to_string();
    let decl = src.find("pub trait Backend").unwrap() + "pub trait ".len();
    // `dyn` and the trait after it: the declaration, and goto lands on its name
    let at = src.find("out: dyn Backend").unwrap() + "out: ".len();
    let h = hover(&prog, &shown, at as u32).expect("hover dyn");
    assert!(h.text.contains("trait Backend: Named") && h.text.contains("fn emit"), "{}", h.text);
    let name_at = at + "dyn ".len();
    let h = hover(&prog, &shown, name_at as u32).expect("hover trait name");
    assert!(h.text.contains("trait Backend") && !h.text.contains("`dyn` value"), "{}", h.text);
    for off in [at, name_at] {
        let d = definition(&prog, &shown, off as u32).expect("goto trait");
        assert_eq!(d.span.start as usize, decl, "{}", d.path);
    }

    let chars = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../speculative/lang/chars.j3");
    let prog = analyze(&chars, &[]).expect("analyze chars");
    let src = std::fs::read_to_string(&chars).unwrap();
    let at = src.find("let a = 'a'").unwrap() + "let a = '".len();
    let h = hover(&prog, &chars.display().to_string(), at as u32).expect("hover char");
    assert!(h.text.contains("'a': u8") && h.text.contains("code point 97 (U+0061)"), "{}", h.text);
}
