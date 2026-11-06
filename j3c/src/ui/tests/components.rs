//! Component names, props, events, provide/inject, class and style passthrough.

use super::*;

#[test]
fn nested_names_and_dotted_tags() {
    let list = files(&[("Shell", SHELL), ("game.Bar", BAR), ("game.hud.Cool", BAR)]);
    // from the root, and from a sibling folder (full path both times)
    let sib = "<div><game.Bar hp={1} /></div>";
    let mut list2 = files(&[("Shell", SHELL), ("game.Bar", BAR), ("game.hud.Cool", BAR)]);
    list2.push(src("ui/game/Side.j3ui", "game.Side", sib));
    for l in [list, list2] {
        let g = build(l, "t", None).unwrap_or_else(|e| panic!("{e}"));
        for want in ["pub struct game_Bar {", "pub struct game_BarProps {", "pub struct game_hud_CoolProps {", "game_Bar.new(mem)", "game_BarProps { "] {
            assert!(g.text.contains(want), "missing {want} in:\n{}", g.text);
        }
        assert!(g.sections.iter().any(|s| s.0 == "game.hud.Cool"));
    }
}

#[test]
fn dotted_tag_errors() {
    let r = build(files(&[("Shell", "<div><Bar hp={1} /></div>"), ("game.Bar", BAR)]), "t", None);
    assert!(r.err().is_some_and(|e| e.contains("unknown component `<Bar>`") && e.contains("game.Bar")));
    let r = build(files(&[("Shell", "<div><game.bar /></div>")]), "t", None);
    assert!(r.is_err());
}

#[test]
fn underscore_collision_names_both_paths() {
    let e = build(vec![src("ui/Game_Bar.j3ui", "Game_Bar", BAR), src("ui/Game/Bar.j3ui", "Game.Bar", BAR)], "t", None).err().expect("collision");
    assert!(e.contains("ui/Game_Bar.j3ui") && e.contains("ui/Game/Bar.j3ui") && e.contains("Game_Bar"), "{e}");
    let e = build(vec![src("ui/Foo.j3ui", "Foo", BAR), src("ui/FooProps.j3ui", "FooProps", BAR)], "t", None).err().expect("helper collision");
    assert!(e.contains("FooProps"), "{e}");
    let e = build(vec![src("ui/List.j3ui", "List", BAR)], "t", None).err().expect("reserved");
    assert!(e.contains("List"), "{e}");
}

#[test]
fn invalid_folder_name_is_an_error() {
    let e = build(vec![src("ui/9x/Bar.j3ui", "9x.Bar", BAR)], "t", None).err().expect("bad folder");
    assert!(e.contains("folder `9x`") && e.contains("ui/9x/Bar.j3ui"), "{e}");
}

#[test]
fn error_spans_point_at_the_nested_file() {
    let bad = "<div>\n  <blink>x</blink>\n</div>";
    let e = build(files(&[("game.Bar", BAR), ("game.hud.Cool", bad)]), "t", None).err().expect("error");
    assert!(e.contains("ui/game/hud/Cool.j3ui:2:3"), "{e}");
    // a type error in copied J3 maps back through the span map
    let g = build(files(&[("game.Bar", "<div>\n  <button on:click={x = = 1}>x</button>\n</div>")]), "t", None).unwrap();
    let at = g.text.find("= = 1").unwrap();
    let r = g.render_parse_error(crate::span::Span { file: crate::span::FileId(1), start: at as u32 + 2, end: at as u32 + 3 }, "boom");
    assert!(r.contains("ui/game/Bar.j3ui:2:"), "{r}");
}

#[test]
fn scan_is_recursive_sorted_and_skips_hidden() {
    let d = std::env::temp_dir().join(format!("j3ui_scan_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    for f in ["Shell.j3ui", "game/Bar.j3ui", "game/hud/Cool.j3ui", ".hid/X.j3ui", "game/notes.txt", ".Hidden.j3ui"] {
        let p = d.join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "<div></div>").unwrap();
    }
    let got = super::super::scan::scan(&d, "ui", &[]).unwrap();
    let names: Vec<_> = got.iter().map(|f| (f.name.as_str(), f.shown.as_str())).collect();
    assert_eq!(names, [("Shell", "ui/Shell.j3ui"), ("game.Bar", "ui/game/Bar.j3ui"), ("game.hud.Cool", "ui/game/hud/Cool.j3ui")]);
    std::fs::write(d.join("a.b.j3ui"), "").unwrap();
    assert!(super::super::scan::scan(&d, "ui", &[]).unwrap_err().contains("cannot contain `.`"));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn locator_maps_generated_bytes_to_j3ui_lines() {
    let g = build(files(&[("Card", "<script>\n let n: int = 0;\n</script>\n<div>\n  {n + zz}\n</div>")]), "t", None);
    // zz is undefined so this fails to check, not to generate; the generated text exists.
    let g = g.expect("builds");
    let loc = g.locator();
    let at = g.text.find("n + zz").expect("expr copied");
    assert_eq!(loc(at as u32), Some(("ui/Card.j3ui".to_string(), 5)));
    assert_eq!(loc(g.text.len() as u32 + 10), None);
}

#[test]
fn prop_defaults() {
    let kid = "<script>\n const BASE: int = 4;\n props { gap: int = 8; note: string = \"hi\"; far: int = BASE + 1; need: int; }\n</script>\n<div>{gap}{note}{far}{need}</div>";
    let page = |inner: &str| build(files(&[("Kid", kid), ("Page", inner)]), "t", None);
    let g = page("<div><Kid need={1} /><Kid need={2} gap={3} note=\"x\" /></div>").expect("builds");
    // omitted props send the default (consts renamed), given ones their value
    for want in ["gap: 8, note: \"hi\", far: Kid_BASE + 1, need: 1", "gap: 3, note: \"x\", far: Kid_BASE + 1, need: 2"] {
        assert!(g.text.contains(want), "missing {want} in:\n{}", g.text);
    }
    let e = page("<div><Kid gap={1} /></div>").err().expect("required");
    assert!(e.contains("missing prop `need`") && !e.contains("missing prop `gap`"), "{e}");
    let bad = "<script>\n props { a: int; b: int = a + 1; }\n</script>\n<div></div>";
    let e = build(files(&[("Bad", bad)]), "t", None).err().expect("default reads a prop");
    assert!(e.contains("default of prop `b` can't use `a`") && e.contains("ui/Bad.j3ui:2:"), "{e}");
}

#[test]
fn speculative_20_passthrough_check() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../speculative/20_ui_passthrough");
    let m = crate::project::Manifest::load(&dir.join("j3.toml")).unwrap();
    crate::compile::compile_manifest(&m).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn event_payloads() {
    let pad = "<script>\n events { pick(slot: int); rename(name: string, n: int); close; }\n</script>\n<button on:click={emit pick(1 + 2); emit rename(\"a\", f(1, 2)); emit close}>x</button>";
    let card = "<div><Pad on:pick={x = event.slot} on:close={x = 0} /></div>";
    let g = build(files(&[("Card", card), ("Pad", pad)]), "t", None).unwrap_or_else(|e| panic!("{e}"));
    for want in ["pick_slot: int;", "pub struct PadEvent_pick", "ev.pick_slot = 1 + 2;", "ev.rename_n = f(1, 2);", "let evt2_pick = PadEvent_pick { slot: e2.pick_slot };", "x = evt2_pick.slot"] {
        assert!(g.text.contains(want), "missing {want:?} in:\n{}", g.text);
    }
    assert!(!g.text.contains("PadEvent_close"), "plain events have no payload struct");
    let pad_with = |ev: &str, body: &str| format!("<script>\n events {{ {ev} }}\n</script>\n<button on:click={{{body}}}>x</button>");
    fails_in(&[("Card", "<div></div>"), ("Pad", &pad_with("pick(s: int);", "emit pick"))], "has 0 argument(s), expected 1");
    fails_in(&[("Card", "<div></div>"), ("Pad", &pad_with("pick(s: int);", "emit pick(1, 2)"))], "has 2 argument(s), expected 1");
    fails_in(&[("Card", "<div></div>"), ("Pad", &pad_with("pick;", "emit pick(1)"))], "has no payload");
    fails_in(&[("Card", "<div></div>"), ("Pad", &pad_with("pick(s: int);", "emit pick(1,)"))], "argument(s)");
    fails_in(&[("Card", "<div></div>"), ("Pad", &pad_with("pick(s: &Foo);", "emit pick"))], "payload type `&Foo` isn't supported");
    fails_in(&[("Card", "<div></div>"), ("Pad", &pad_with("pick(s: int); pick_s;", "emit pick(1)"))], "collides");
}

#[test]
fn provide_and_inject() {
    let deep = "<script>\n inject demo: &Demo;\n</script>\n<div>{demo.n}</div>";
    let mid = "<div><Deep /></div>";
    let top = "<script>\n props { demo: &Demo; }\n provide demo;\n</script>\n<div><Mid /><Holder><Deep /></Holder></div>";
    let holder = "<div><slot /></div>";
    let all = |top: &str, deep: &str| vec![("Top".to_string(), top.to_string()), ("Mid".into(), mid.into()), ("Deep".into(), deep.to_string()), ("Holder".into(), holder.into())];
    let run = |v: Vec<(String, String)>| {
        let l: Vec<(&str, &str)> = v.iter().map(|x| (x.0.as_str(), x.1.as_str())).collect();
        build(files(&l), "t", None)
    };
    let g = run(all(top, deep)).unwrap_or_else(|e| panic!("{e}"));
    for want in ["demo: &Demo;", "demo: demo", "pub struct MidProps {\n    demo: &Demo;"] {
        assert!(g.text.contains(want), "missing {want:?} in:\n{}", g.text);
    }
    let e = run(all("<div><Mid /></div>", deep)).err().expect("no provider");
    assert!(e.contains("nothing provides `demo: &Demo`") && e.contains("Top -> Mid -> Deep"), "{e}");
    let e = run(all(&top.replace("props { demo: &Demo; }", "props { demo: &Other; }"), deep)).err().expect("type mismatch");
    assert!(e.contains("injects `demo: &Demo`") && e.contains("&Other"), "{e}");
    let e = run(all(&top.replace("provide demo;", "provide nope;"), deep)).err().expect("bad provide");
    assert!(e.contains("`provide nope`"), "{e}");
    let e = run(all(&top.replace("provide demo;", ""), deep)).err().expect("own prop of the same name");
    assert!(e.contains("already has as its own prop"), "{e}");
    let e = run(all(top, "<script>\n inject demo: &Demo;\n inject demo: &Demo;\n</script>\n<div></div>")).err().expect("twice");
    assert!(e.contains("declared twice"), "{e}");
}

#[test]
fn script_consts() {
    let card = "<script>\n const NAMES: [3]string = [\"a\", \"b\", \"c\"];\n const MAX = 4;\n let i = 1;\n</script>\n<div>{NAMES[i]} {MAX}</div>";
    let g = build(files(&[("Card", card)]), "t", None).expect("builds");
    for want in ["const Card_MAX: int = 4;", "#[singleton]\nlet Card_NAMES: [3]string;", "Card_NAMES = [\"a\"", "Card_NAMES[this.i]"] {
        assert!(g.text.contains(want), "wanted {want:?} in:\n{}", g.text);
    }
    fails("<script>\n const N = 1;\n let N = 2;\n</script>\n<div></div>", "ui/Card.j3ui:2:8");
    fails("<script>\n const N = 1;\n fn f() { N = 2; }\n</script>\n<div></div>", "ui/Card.j3ui:3:11");
    fails("<script>\n const N = foo();\n</script>\n<div></div>", "ui/Card.j3ui:2:8");
    fails("<script>\n const N = 1;\n</script>\n{#each 0..2 as N}\n<div></div>\n{/each}", "ui/Card.j3ui:4:16");
}

#[test]
fn class_and_style_pass_to_the_root() {
    let page = "<script>\n props { on: bool; w: f32; }\n</script>\n<div><Chip label=\"a\" class=\"wide x{w}\" class:on={on} style:width={w} /><Chip label=\"b\" /></div>";
    let g = build(files(&[("Page", page), ("Chip", CHIP)]), "t", None).unwrap_or_else(|e| panic!("{e}"));
    // the root applies them after its own attributes, passed styles after the root's own slot 0
    for want in [
        "d.scopePass(n1, p.j3pass_scope);",
        "d.clsPass(n1, p.j3pass_cls);",
        "d.stylePass(n1, 1, p.j3pass_k0, p.j3pass_n0, p.j3pass_f0, p.j3pass_s0);",
        "j3pass_scope: SCOPE_Page, j3pass_cls: `wide x{w} {if on { \"on\" } else { \"\" }}`",
        "j3pass_n0: \"width\", j3pass_k0: 1, j3pass_f0: f32(w), j3pass_s0: \"\"",
        // the use that passes nothing still fills the fields
        "j3pass_scope: 0, j3pass_cls: \"\", j3pass_k0: 0, j3pass_n0: \"\"",
    ] {
        assert!(g.text.contains(want), "missing {want} in:\n{}", g.text);
    }
    let own = g.text.find("d.styleNumAt(n1, 0, \"height\"").expect("own style");
    assert!(own < g.text.find("d.stylePass(").unwrap(), "the passed style must come after the root's own");
    // a component nobody passes to has no hidden fields
    let g = build(files(&[("Page", "<div><Chip label=\"a\" /></div>"), ("Chip", CHIP)]), "t", None).unwrap();
    assert!(!g.text.contains("j3pass"), "{}", g.text);
}

#[test]
fn passthrough_needs_one_element_root_only_when_used() {
    let pair = "<span>a</span>\n<span>b</span>\n";
    build(files(&[("Page", "<div><Pair /></div>"), ("Pair", pair)]), "t", None).unwrap_or_else(|e| panic!("several roots are fine unless a use passes: {e}"));
    for (root, why) in [
        (pair, "it has 2 root nodes"),
        ("{#if true}<span>a</span>{/if}", "its root is an `{#if}`"),
        ("{#each [1] as x}<span>a</span>{/each}", "its root is an `{#each}`"),
        ("<Chip label=\"x\" />", "its root is the component `<Chip>`"),
        ("<slot />", "its root is a `<slot />`"),
        ("text", "its root is text"),
    ] {
        fails_in(&[("Page", "<div><Pair class=\"wide\" /></div>"), ("Pair", root), ("Chip", CHIP)], why);
    }
    fails_in(&[("Page", "<div><Pair style:width={4.0} /></div>"), ("Pair", pair)], "`style:width` on `<Pair>` has no element to go to");
}

#[test]
fn passthrough_errors() {
    let many = "style:left={1} style:top={1} style:width={1} style:height={1} style:opacity={1} style:flex={1} style:order={1} style:z-index={1}";
    let root = format!("<div style:margin-top={{1}}>x</div>");
    fails_in(&[("Page", &format!("<div><Chip {many} /></div>")), ("Chip", &root)], "at most 8 together");
    fails_in(&[("Page", "<div><Chip style:padding={4.0} /></div>"), ("Chip", "<div>x</div>")], "shorthand");
    fails_in(&[("Page", "<div><Chip class:on /></div>"), ("Chip", "<div>x</div>")], "needs a condition");
    fails("<script>\n props { j3pass_cls: string; }\n</script>\n<div></div>", "reserved");
}
