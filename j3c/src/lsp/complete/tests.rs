use super::*;
use crate::compile::default_hello_path;

fn hello() -> (Program, String, String) {
    let path = default_hello_path();
    let prog = query::analyze(&path, &[]).expect("analyze hello");
    let src = std::fs::read_to_string(&path).unwrap();
    (prog, path.display().to_string(), src)
}

fn labels(items: &[Completion]) -> Vec<&str> {
    items.iter().map(|c| c.label.as_str()).collect()
}

fn script_sample() -> (Program, String, String, String, String) {
    let root =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../speculative/11_script");
    let main = root.join("main.j3");
    let script = root.join("scripts/door.j3s");
    let prog = query::analyze(&main, &[]).expect("analyze script sample");
    let main_src = std::fs::read_to_string(&main).unwrap();
    let script_src = std::fs::read_to_string(&script).unwrap();
    (
        prog,
        main.display().to_string(),
        main_src,
        script.display().to_string(),
        script_src,
    )
}

#[test]
fn script_path_and_props_complete() {
    let (prog, main, src, _, _) = script_sample();
    let partial = "module demo; const Door = @script(\"scripts/do";
    let items = complete_buffer(
        std::path::Path::new(&main),
        &[],
        partial,
        partial.len() as u32,
    );
    assert!(
        items.iter().any(|c| c.label == "scripts/door.j3s"),
        "{:?}",
        labels(&items)
    );
    let at = src.find("Door { hinge").unwrap() + "Door { ".len();
    let items = complete_at(&prog, &main, &src, at as u32);
    assert_eq!(items.first().map(|c| c.label.as_str()), Some("hinge"));
    assert!(
        items[0].detail.contains("required prop: Node"),
        "{}",
        items[0].detail
    );
    assert!(items
        .iter()
        .any(|c| c.label == "openAngle" && c.detail.contains("= 90")));
    assert!(!items.iter().any(|c| c.label == "open"));
}

#[test]
fn script_scope_completion_and_outline() {
    let (prog, main, src, script, script_src) = script_sample();
    let at = script_src.find("if open").unwrap() + "if op".len();
    let items = complete_at(&prog, &script, &script_src, at as u32);
    assert!(items
        .iter()
        .any(|c| c.label == "open" && c.detail.contains("state")));
    let at = src.find("front.toggle").unwrap() + "front.".len();
    let members = complete_at(&prog, &main, &src, at as u32);
    assert!(
        members.iter().any(|c| c.label == "toggle"),
        "{:?}",
        labels(&members)
    );
    assert!(!members
        .iter()
        .any(|c| c.label == "open" || c.label == "smooth"));
    let call_at = src.find("front.update(").unwrap() + "front.update(".len();
    let sig = signature(&prog, &main, &src, call_at as u32).expect("script signature");
    assert!(
        sig.signatures[0].label.contains("update("),
        "{}",
        sig.signatures[0].label
    );
    assert!(
        !sig.signatures[0].label.contains("this:"),
        "{}",
        sig.signatures[0].label
    );
    let outline = symbols(&prog, &script);
    for wanted in ["props", "shared", "open", "t", "toggle"] {
        assert!(outline.iter().any(|s| s.name == wanted), "missing {wanted}");
    }
}

#[test]
fn use_completes_module_then_export() {
    let (prog, path, src) = hello();
    let at = src.find("use j3.").unwrap() + "use j3.".len();
    let items = complete(&prog, &path, at as u32);
    let names = labels(&items);
    assert!(names.contains(&"io"), "{names:?}");
    assert!(names.contains(&"sys"), "{names:?}");
    assert!(!names.contains(&"printLn"), "{names:?}");

    let at = src.find("printLn").unwrap() + 5;
    let items = complete(&prog, &path, at as u32);
    assert!(items.iter().any(|c| c.label == "printLn"), "{:?}", labels(&items));
}

#[test]
fn bare_completion_is_in_scope() {
    let (prog, path, src) = hello();
    let at = src.find("printLn(\"").unwrap();
    let items = complete(&prog, &path, (at + 2) as u32);
    assert!(items.iter().any(|c| c.label == "printLn"));
    assert!(items.iter().all(|c| c.label != "exit"));
    assert!(items.iter().all(|c| c.label != "readFile"));

    let items = complete(&prog, &path, at as u32);
    assert!(items.iter().any(|c| c.label == "let"));
    assert!(items.iter().any(|c| c.label == "printLn"));
    assert!(items.iter().all(|c| c.label != "exit"));
}

#[test]
fn signature_of_print_ln() {
    let (prog, path, src) = hello();
    let at = src.find("printLn(\"").unwrap() + "printLn(".len();
    let sig = signature(&prog, &path, &src, at as u32).expect("sig");
    assert!(sig.signatures[0].label.contains("string"), "{}", sig.signatures[0].label);
    assert_eq!(sig.signatures[0].params.len(), 1);
    assert!(sig.signatures[0].params[0].contains('s'), "{:?}", sig.signatures[0].params);
    assert_eq!(sig.active, 0);
}

#[test]
fn import_edit_for_unimported_export() {
    let (prog, _, src) = hello();
    let edits = import_edits(&prog, &src, &["unknown function `readFile`".to_string()]);
    assert_eq!(edits.len(), 1, "{:?}", edits.iter().map(|e| e.title.clone()).collect::<Vec<_>>());
    assert!(edits[0].new_text.contains("use j3.io.readFile;"), "{}", edits[0].new_text);
    let at = edits[0].offset as usize;
    assert!(src[..at].contains("use j3.io.printLn"), "{}", &src[..at]);
    let none = import_edits(&prog, &src, &["unknown name `noSuchName`".to_string()]);
    assert!(none.is_empty());
}

#[test]
fn signature_of_method_skips_this() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/mesh.j3");
    let prog = query::analyze(&path, &[]).expect("mesh");
    let src = std::fs::read_to_string(&path).unwrap();
    let at = src.find("q.rotate(").unwrap() + "q.rotate(".len();
    let sig = signature(&prog, &path.display().to_string(), &src, at as u32).expect("sig");
    assert!(
        sig.signatures.iter().any(|s| s.label.contains("Vec3") && s.params.iter().all(|p| !p.starts_with("this:"))),
        "{:?}",
        sig.signatures.iter().map(|s| s.label.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn outline_lists_main() {
    let (prog, path, _) = hello();
    let syms = symbols(&prog, &path);
    assert!(syms.iter().any(|s| s.name == "main" && s.kind == 12), "{:?}", syms.iter().map(|s| &s.name).collect::<Vec<_>>());
}

#[test]
fn module_dot_lists_exported_type() {
    let path = default_hello_path();
    let src = "\
module hello;

use j3.math;
use j3.io.printLn;

fn main() {
    math.sqrt(f32(4));
    printLn(\"ok\");
}
";
    let overlay = vec![(path.clone(), src.to_string())];
    let prog = query::analyze(&path, &overlay).expect("analyze");
    let shown = path.display().to_string();
    let at = src.find("math.sqrt").unwrap() + "math.".len();
    let items = complete_at(&prog, &shown, src, at as u32);
    let names = labels(&items);
    assert!(names.contains(&"sqrt"), "{names:?}");
    assert!(names.contains(&"Quat"), "{names:?}");
    assert!(names.contains(&"Vec2"), "{names:?}");
}

#[test]
fn incomplete_app_dot_lists_methods() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/fps-shooter/src/main.j3");
    let mut src = std::fs::read_to_string(&path).unwrap();
    let at_run = src.find("    app.run();\n").expect("run");
    src.insert_str(at_run, "    app.\n");
    let at = at_run + "    app.".len();
    let items = complete_buffer(&path, &[(path.clone(), src.clone())], &src, at as u32);
    let names = labels(&items);
    assert!(names.contains(&"useGl"), "{names:?}");
    assert!(names.contains(&"run"), "{names:?}");
    assert!(names.contains(&"startup"), "{names:?}");
    assert!(!names.contains(&"App"), "{names:?}");
}
