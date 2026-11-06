//! Dynamic components (`<ui.choice>` and selector expansion).

use super::*;

#[test]
fn dynamic_components_expand_before_shared_scans() {
    let app = "<script>let kind = 0; let theme: int = 1; provide theme;</script><div><component this={kind} of={TextMsg, ImageMsg} msg={3} class=\"wide\" style:width={42.0}><span slot=\"footer\">foot</span>body</component></div>";
    let child = "<script>props { msg: int; } inject theme: int;</script><section>{msg}<slot/><slot name=\"footer\"/></section>";
    let g = build(files(&[("Screen", app), ("TextMsg", child), ("ImageMsg", child)]), "t", None).unwrap();
    for want in ["pub enum Of_TextMsg_ImageMsg", "@uiChoice(Of_TextMsg_ImageMsg,", "Of_TextMsg_ImageMsg"] {
        assert!(g.text.contains(want), "missing {want}");
    }
    assert!(g.text.contains(".destroy()"));
    assert_eq!(g.text.matches("j3pass_cls:").count(), 4);
    assert!(g.text.contains("slot1_footer_update") && g.text.contains("slot2_footer_update"));
    let bad = child.replace("msg: int;", "other: int;");
    let e = build(files(&[("Screen", app), ("TextMsg", child), ("ImageMsg", &bad)]), "t", None).err().unwrap();
    assert!(e.contains("unknown prop `msg` for `<ImageMsg>`"), "{e}");
    let app = "<component this={0} of={TextMsg, ImageMsg}/>";
    let e = build(files(&[("Screen", app), ("TextMsg", "<div/>"), ("ImageMsg", "<script>props { msg: int; }</script><div/>" )]), "t", None).err().unwrap();
    assert!(e.contains("missing prop `msg` for `<ImageMsg>`"), "{e}");
}

#[test]
fn dynamic_components_reject_bad_selectors_and_candidates() {
    for (tag, want) in [
        ("<component of={Row}/>", "needs `this={...}`"),
        ("<component this={0}/>", "needs `of={...}`"),
        ("<component this=\"0\" of={Row}/>", "needs `this={...}`"),
        ("<component this={0} of=\"Row\"/>", "needs `of={...}`"),
        ("<component this={0} of={Row, Row}/>", "duplicate component `Row`"),
        ("<component this={0} of={Nope}/>", "unknown component `Nope`"),
        ("<component this={0} of={div}/>", "unknown component `div`"),
        ("<component this={0} of={}/>", "empty `{}`"),
        ("<component this={0} of={Row,}/>", "unknown component"),
        ("<component this={0} this={1} of={Row}/>", "duplicate attribute `this`"),
        ("<component this={0} of={Row} j3pass_scope={1}/>", "reserved attribute"),
        ("<component this={0} of={Card}/>", "contains itself"),
    ] { fails(tag, want); }
}

#[test]
fn speculative_24_dynamic_components_check() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../speculative/24_ui_dynamic");
    let m = crate::project::Manifest::load(&dir.join("j3.toml")).unwrap();
    crate::compile::compile_manifest(&m).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn dynamic_selector_types_are_checked_and_remapped() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../speculative/24_ui_dynamic").canonicalize().unwrap();
    let path = dir.join("ui/Screen.j3ui");
    let src = std::fs::read_to_string(&path).unwrap();
    for selector in ["true", "1.5", "\"wrong\"", "unrelated enum"] {
        let mut text = src.replace("this={msg.kind}", &format!("this={{{selector}}}"));
        if selector == "unrelated enum" {
            // Use another generated enum, so rejecting an unrelated enum requires no fixture module.
            text = src.replace("this={msg.kind}", "this={Of_Embed_TextMsg.Embed}");
            text.push_str("<component this={0} of={Embed, TextMsg} msg={state.single}/>");
        }
        let (result, _) = crate::compile::analyze_ui(&dir.join("src/main.j3"), &[(path.clone(), text)]);
        let e = result.err().expect("bad selector must fail");
        assert!(e.message.contains("component selector must be `int` or `Of_TextMsg_ImageMsg_Embed`"), "{}", e.message);
        assert!(e.path.ends_with("Screen.j3ui"), "{}", e.path);
    }
}

#[test]
fn dynamic_components_can_fill_a_named_slot() {
    let g = build(files(&[
        ("Screen", "<Wrap><component slot=\"footer\" this={0} of={A, B}/></Wrap>"),
        ("Wrap", "<section><slot name=\"footer\"/></section>"),
        ("A", "<p>a</p>"), ("B", "<p>b</p>"),
    ]), "t", None).unwrap();
    assert!(g.text.contains("slot1_footer_update"));
}

#[test]
fn ui_choice_operation_validates_its_type_and_arm() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../speculative/24_ui_dynamic").canonicalize().unwrap();
    let path = dir.join("ui/Screen.j3ui");
    let src = std::fs::read_to_string(&path).unwrap();
    for (operation, want) in [
        ("@uiChoice(int, 0, 0)", "payloadless candidate-set enum"),
        ("@uiChoice(Of_TextMsg_ImageMsg_Embed, 0, 3)", "constant within the candidate set"),
        ("@uiChoice(Of_TextMsg_ImageMsg_Embed, 0, -1)", "constant within the candidate set"),
        ("@uiChoice(Of_TextMsg_ImageMsg_Embed, 0, msg.kind)", "constant within the candidate set"),
    ] {
        let text = src.replace("this={msg.kind}", &format!("this={{{operation}}}"));
        let (result, _) = crate::compile::analyze_ui(&dir.join("src/main.j3"), &[(path.clone(), text)]);
        let e = result.err().expect("bad operation must fail");
        assert!(e.message.contains(want), "{}", e.message);
        assert!(e.path.ends_with("Screen.j3ui"), "{}", e.path);
    }
}
