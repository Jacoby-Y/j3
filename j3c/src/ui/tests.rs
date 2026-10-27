use super::{build, SrcFile};

fn files(list: &[(&str, &str)]) -> Vec<SrcFile> {
    list.iter()
        .map(|(n, t)| SrcFile { path: format!("ui/{}.j3ui", n.replace('.', "/")), name: n.to_string(), text: t.to_string() })
        .collect()
}

const ROW: &str = "<script>\n    props { label: string; }\n    events { buy; }\n</script>\n<div>{label}</div>\n";

/// Build `Card` (the text under test) next to a `Row` child; the error text must hold `want`.
fn fails(card: &str, want: &str) {
    let r = build(files(&[("Card", card), ("Row", ROW)]), "t", None);
    match r {
        Ok(_) => panic!("expected an error containing {want:?}"),
        Err(e) => assert!(e.contains(want), "wanted {want:?} in:\n{e}"),
    }
}

#[test]
fn compile_time_checks_point_into_the_j3ui() {
    fails("<div>\n  <blink>x</blink>\n</div>", "ui/Card.j3ui:2:3");
    fails("<div>\n  <span hint=\"x\">x</span>\n</div>", "ui/Card.j3ui:2:9");
    fails("<div>\n  <Nope />\n</div>", "ui/Card.j3ui:2:3");
    fails("<div>\n  <Row label=\"a\" on:sell={x = 1} />\n</div>", "ui/Card.j3ui:2:18");
    fails("<div>\n  <Row />\n</div>", "missing prop `label`");
    fails("<div>\n  <Row label=\"a\" size={1} />\n</div>", "unknown prop `size`");
    fails("<script>\n let n: int = 0;\n</script>\n<input type=\"checkbox\" bind:checked={n + 1} />", "ui/Card.j3ui:4:38");
    fails("<script>\n events { a; }\n</script>\n<button on:click={emit b}>x</button>", "ui/Card.j3ui:4:24");
    fails("<div>\n  <span>x</div>", "ui/Card.j3ui:2:10");
    fails("<div>\n  <span>x</span>", "ui/Card.j3ui:1:1");
    fails("{#if a}\n<div></div>", "ui/Card.j3ui:1:1");
    fails("{#each xs}\n<div></div>\n{/each}", "ui/Card.j3ui:1:8");
    fails("<script>\n props { sim: int; }\n</script>\n{#each 0..3 as sim}\n<div></div>\n{/each}", "ui/Card.j3ui:4:16");
    fails("<script>\n let n = foo();\n</script>\n<div></div>", "ui/Card.j3ui:2:6");
}

#[test]
fn j3_errors_in_handlers_map_back() {
    // a syntax error in a handler is found by the real parser in the generated text
    let src = files(&[("Card", "<div>\n  <button on:click={x = = 1}>x</button>\n</div>")]);
    let g = build(src, "t", None).expect("markup is fine");
    let at = g.text.find("= = 1").expect("handler copied verbatim");
    let rendered = g.render_parse_error(crate::span::Span { file: crate::span::FileId(1), start: at as u32 + 2, end: at as u32 + 3 }, "boom");
    assert!(rendered.contains("ui/Card.j3ui:2:"), "{rendered}");
    assert!(rendered.contains("on:click={x = = 1}"), "{rendered}");
}

#[test]
fn speculative_12_generates_and_checks() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../speculative/12_ui_runtime");
    let m = crate::project::Manifest::load(&dir.join("j3.toml")).unwrap();
    crate::compile::compile_manifest(&m).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn dialog_default_flags() {
    let card = "<script>\n let s = \"\";\n</script>\n<dialog modal autoselect>\n  <input type=\"text\" bind:value={s} on:submit={s = \"\"} />\n  <button default>OK</button>\n</dialog>";
    let g = build(files(&[("Card", card)]), "t", None).expect("builds");
    for want in [", 1);", ", 2);", ", 3);"] {
        assert!(g.text.contains(&format!("d.attrFlag(n{}", "")) && g.text.contains(want), "missing {want} in:\n{}", g.text);
    }
    fails("<div><input type=\"text\" autoselect /></div>", "unknown attribute `autoselect`");
    fails("<dialog modal autoselect=\"1\"></dialog>", "takes no value");
}

#[test]
fn input_widgets_bind_by_type() {
    let card = "<script>\n let s = \"\";\n let v: f32 = 0.0;\n let hi: f32 = 1.0;\n</script>\n<div>\n  <input type=\"text\" placeholder=\"x\" bind:value={s} on:submit={s = \"\"} />\n  <input type=\"range\" bind:value={v} bind:high={hi} />\n  <select multiple bind:value={s}><option value=\"a\">A</option></select>\n</div>";
    let g = build(files(&[("Card", card)]), "t", None).expect("builds");
    for want in ["= d.strValue(", "d.attrText(", "= d.value(", "= d.highValue(", "d.attrHigh(", "d.attrMultiple(", "d.attrPlaceholder(", "if d.submitted("] {
        assert!(g.text.contains(want), "missing {want} in:\n{}", g.text);
    }
    assert!(g.text.contains("inputs.begin(d)") && g.text.contains("inputs.overlay(d)"));
    fails("<script>\n let s = \"\";\n let t = \"text\";\n</script>\n<input type={t} bind:value={s} />", "needs a literal `type`");
    fails("<div><input type=\"text\" bind:nope={x} /></div>", "unknown attribute `bind:nope`");
    fails("<div><select high={1}></select></div>", "unknown attribute `high`");
}

#[test]
fn glyph_prompts_take_action_key_and_pad() {
    let card = "<script>\n let act = \"back\";\n let k = \"Space\";\n</script>\n<p>Press <glyph action=\"confirm\" /> or <glyph action={act} /> or <glyph key={k} pad=\"x\" /> or <glyph pad=\"lb\" /></p>";
    let g = build(files(&[("Card", card)]), "t", None).expect("builds");
    for want in ["d.glyph(n2, \"confirm\", \"\", \"\")", "d.glyph(n3, this.act, \"\", \"\")", "\"\", this.k, \"x\")", ", \"\", \"\", \"lb\")"] {
        assert!(g.text.contains(want), "missing {want} in:\n{}", g.text);
    }
    // whitespace between elements is dropped, `&nbsp;` is kept (docs: Differences from a browser)
    let sp = |m: &str| build(files(&[("Card", m)]), "t", None).expect("builds").text.matches("d.text(").count();
    assert_eq!(sp("<p><glyph action=\"up\" /> <glyph action=\"down\" /></p>"), 0);
    assert_eq!(sp("<p><glyph action=\"up\" />&nbsp;<glyph action=\"down\" /></p>"), 1);
    fails("<glyph />", "needs `action");
    fails("<glyph action=\"fire\" />", "unknown glyph action `fire`");
    fails("<glyph action=\"confirm\">Enter</glyph>", "takes no children");
    fails("<glyph action=\"confirm\" size=\"2\" />", "unknown attribute `size`");
}

/// `GLYPH_ACTIONS` (compile-time check) and the runtime tables (`glyphKey`, `glyphPad` in dom/glyph.j3) are two copies of one list.
#[test]
fn glyph_actions_match_the_runtime_tables() {
    let src = include_str!("../../stdlib/j3/ui/dom/glyph.j3"); // the file `stdlib/mod.rs` embeds
    let names_in = |func: &str| -> Vec<String> {
        let body = src.split(&format!("fn {func}(")).nth(1).expect(func).split("\n}\n").next().unwrap();
        let mut v: Vec<String> = body.split("same(a, \"").skip(1).map(|t| t.split('"').next().unwrap().to_string()).collect();
        v.sort();
        v.dedup();
        v
    };
    let mut want: Vec<String> = super::gen::GLYPH_ACTIONS.iter().map(|s| s.to_string()).collect();
    want.sort();
    assert_eq!(names_in("glyphKey"), want, "glyphKey vs GLYPH_ACTIONS");
    assert_eq!(names_in("glyphPad"), want, "glyphPad vs GLYPH_ACTIONS");
}

#[test]
fn radio_groups_bind_by_value() {
    let card = "<script>\n let mode = \"a\";\n let n = 1;\n let on = false;\n</script>\n<div role=\"radiogroup\">\n  <input type=\"radio\" name=\"m\" value=\"a\" bind:group={mode} on:change={n = 5} />\n  <input type=\"radio\" name=\"m\" value=\"b\" bind:group={mode} />\n  <input type=\"radio\" name=\"k\" value={2} bind:group={n} />\n  <input type=\"radio\" name=\"z\" bind:checked={on} />\n  <input type=\"radio\" name=\"z\" checked />\n</div>";
    let g = build(files(&[("Card", card)]), "t", None).expect("builds");
    for want in ["d.attrName(", "d.attrRadio(", "d.attrGroup(", "d.textIs(", "= \"b\";", ") == (", "d.userToggled(", "= d.isChecked("] {
        assert!(g.text.contains(want), "missing {want} in:\n{}", g.text);
    }
    fails("<script>\n let s = \"\";\n</script>\n<input type=\"checkbox\" value=\"a\" bind:group={s} />", "`bind:group` is for `<input type=\"radio\">`");
    fails("<script>\n let s = \"\";\n</script>\n<input type=\"radio\" bind:group={s} />", "needs a `value`");
    fails("<script>\n let n = 0;\n</script>\n<input type=\"radio\" value=\"a\" bind:group={n} />", "`n` is int, but `value=\"..\"` is text");
    fails("<script>\n let s = \"\";\n</script>\n<input type=\"radio\" value=\"a{s}\" bind:group={s} />", "can't have `{}` holes");
    fails("<script>\n let s = \"\";\n</script>\n<input type=\"radio\" value=\"a\" bind:group={s + \"x\"} />", "ui/Card.j3ui:4:");
    fails("<script>\n let s = \"\";\n</script>\n<input type=\"text\" on:change={s = \"\"} />", "`on:change` is for");
}

#[test]
fn accessibility_attributes() {
    let card = "<div role=\"list\" aria-label={\"Files\"}><div tabindex=\"0\" role=\"option\">a</div><img alt=\"logo\" /></div>";
    let g = build(files(&[("Card", card)]), "t", None).expect("builds");
    for want in ["d.attrRole(", "d.attrAria(", "d.attrTabIndex("] {
        assert!(g.text.contains(want), "missing {want} in:\n{}", g.text);
    }
    let g = build(files(&[("Card", "<div tabindex=\"-1\">a</div>")]), "t", None).expect("builds");
    assert!(g.text.contains("d.attrTabIndexOff("), "missing attrTabIndexOff");
    let g = build(files(&[("Card", "<div tabindex=\"2\">a</div><div tabindex={n}>b</div>")]), "t", None).expect("builds");
    assert!(g.text.contains("d.attrTabIndexN(") && g.text.contains(", 2);"), "positive tabindex:\n{}", g.text);
    fails("<div tabindex=\"x\"></div>", "`tabindex` takes");
}

#[test]
fn aria_state_attributes() {
    let card = "<script>\n let on = false;\n</script>\n<div><div role=\"tab\" aria-selected={on}>a</div><div role=\"tab\" aria-selected=\"false\">b</div><div aria-live=\"polite\" aria-describedby=\"h\" id=\"s\">x</div><p id=\"h\">hint</p></div>";
    let g = build(files(&[("Card", card)]), "t", None).expect("builds");
    for want in ["d.attrSelected(", "d.attrLive(", "d.attrDescBy("] {
        assert!(g.text.contains(want), "missing {want} in:\n{}", g.text);
    }
    fails("<script>\n let t = \"a\";\n</script>\n<div aria-describedby={t}></div>", "static ids");
    fails("<div aria-selected=\"maybe\"></div>", "aria-selected");
}

#[test]
fn html_text_elements_compile() {
    let card = "<div><h3>a</h3><h4>b</h4><h5>c</h5><h6>d</h6><ul><li><a>x</a></li></ul><ol><li><strong>s</strong><em>e</em><b>b</b><i>i</i></li></ol><pre><code>k</code></pre><hr /></div>";
    let g = build(files(&[("Card", card)]), "t", None).expect("builds");
    for want in ["d.tag(\"h3\")", "d.tag(\"li\")", "d.tag(\"hr\")", "d.tag(\"code\")"] {
        assert!(g.text.contains(want), "missing {want} in:\n{}", g.text);
    }
    fails("<div><blink /></div>", "unknown element `<blink>`");
}

#[test]
fn label_for_is_static() {
    build(files(&[("Card", "<script>\n let on = false;\n</script>\n<div><label for=\"a\">A</label><input id=\"a\" type=\"checkbox\" bind:checked={on} /></div>"), ("Row", ROW)]), "t", None).unwrap_or_else(|e| panic!("{e}"));
    fails("<script>\n let t = \"a\";\n</script>\n<div><label for={t}>A</label></div>", "for= takes a static id");
    fails("<div><label for=\"a{1}\">A</label></div>", "for= takes a static id");
    fails("<div><span for=\"a\">A</span></div>", "for");
}

#[test]
fn focus_group_and_nav_are_static() {
    build(files(&[("Card", "<div focus-group=\"wrap trap\"><button id=\"a\" nav-down=\"b\">A</button><button id=\"b\">B</button></div>"), ("Row", ROW)]), "t", None).unwrap_or_else(|e| panic!("{e}"));
    fails("<script>\n let t = \"a\";\n</script>\n<div><button nav-up={t}>A</button></div>", "nav-up= takes a static string");
    fails("<div focus-group=\"a{1}\"></div>", "focus-group= takes a static string");
}

#[test]
fn br_progress_meter_details_compile() {
    let card = "<script>\n let v = 0.5;\n let o = false;\n let hits = 0;\n</script>\n<div>a<br>b<br/>c\n<progress value={v} max=\"2\"></progress><progress />\n<meter value={v} min=\"0\" max=\"1\" low=\"0.3\" high={0.8} optimum=\"1\"></meter>\n<details bind:open={o} on:toggle={hits += 1}><summary>S</summary><p>body</p></details><details open><summary>T</summary></details></div>";
    let g = build(files(&[("Card", card)]), "t", None).expect("builds");
    for want in ["d.tag(\"br\")", "d.tag(\"progress\")", "d.attrValue(n", "-1.0", "d.attrMax(", "d.attrMeter(", ", 0, f32(", "d.attrOpen(", "= d.isOpen(", "if d.toggled(", "if d.userToggled(", "d.attrTabIndex("] {
        assert!(g.text.contains(want), "missing {want} in:\n{}", g.text);
    }
    fails("<div><progress>50%</progress></div>", "takes no children");
    fails("<div><meter value=\"1\"><b>x</b></meter></div>", "takes no children");
    fails("<div><br class=\"x\" id=\"y\" value=\"1\" /></div>", "unknown attribute `value` on `<br>`");
    fails("<div><details bind:open=\"x\"><summary>a</summary></details></div>", "`bind:open` needs");
    fails("<div><details open=\"yes\"></details></div>", "`open` takes");
    fails("<div><summary on:toggle={x = 1}>s</summary></div>", "unknown attribute `on:toggle` on `<summary>`");
    fails("<div><progress bind:open={x} /></div>", "unknown attribute `bind:open` on `<progress>`");
    fails("<div><textarea><br /></textarea></div>", "`<br>` is not allowed inside `<textarea>`");
    fails("<div><input>\n <br/> </input></div>", "`<br>` is not allowed inside `<input>`");
    fails("<div><input><br>x</input></div>", "takes no content");
    // a br after a text box is fine
    build(files(&[("Card", "<div><input type=\"text\"><br><textarea /><br></div>")]), "t", None).expect("br beside a text box builds");
}

#[test]
fn textarea_is_a_typed_input() {
    let card = "<script>\n let s = \"\";\n let n = 4;\n</script>\n<div>\n  <textarea rows=\"3\" maxlength={n} placeholder=\"x\" bind:value={s} on:submit={s = \"\"}></textarea>\n  <input type=\"password\" maxlength=\"8\" bind:value={s} />\n</div>";
    let g = build(files(&[("Card", card)]), "t", None).expect("builds");
    for want in ["d.attrType(", "\"textarea\"", "d.attrRows(", "d.attrMaxLen(", "= d.strValue(", "if d.submitted("] {
        assert!(g.text.contains(want), "missing {want} in:\n{}", g.text);
    }
    fails("<div><textarea cols={1} /></div>", "unknown attribute `cols`");
}

#[test]
fn rich_textarea_binds_spans() {
    let card = "<script>\n use j3.ui.spans.Span;\n let s = \"\";\n let sp: []Span = [];\n</script>\n<textarea rich rows=\"3\" bind:value={s} bind:spans={sp}></textarea>";
    let g = build(files(&[("Card", card)]), "t", None).expect("builds");
    for want in ["d.attrRich(", "= d.spansValue(", "d.attrSpans(", "= d.strValue("] {
        assert!(g.text.contains(want), "missing {want} in:\n{}", g.text);
    }
    // the spans binding is emitted after the value binding whatever the attribute order
    let card = "<script>\n use j3.ui.spans.Span;\n let s = \"\";\n let sp: []Span = [];\n</script>\n<textarea rich bind:spans={sp} bind:value={s}></textarea>";
    let g = build(files(&[("Card", card)]), "t", None).expect("builds");
    let (v, sp) = (g.text.find("d.attrText(").expect("value"), g.text.find("d.attrSpans(").expect("spans"));
    assert!(v < sp, "spans after the text in:\n{}", g.text);
    fails("<script>\n let sp = 0;\n</script>\n<textarea bind:spans={sp}></textarea>", "needs `rich`");
    fails("<script>\n let sp = 0;\n</script>\n<input type=\"text\" bind:spans={sp} />", "is for `<textarea rich>`");
    fails("<input type=\"text\" rich />", "is for `<textarea rich>`");
}

#[test]
fn caret_popup_and_trigger() {
    let card = "<script>\n let s = \"\";\n let open = false;\n let lo = 0;\n</script>\n<div>\n  <textarea rich bind:value={s} triggers=\"@\" on:trigger={open = trigger.len > 0; lo = tlo; if thi < 0 { open = false; }}></textarea>\n  {#if open}<div popover=\"caret\"><button on:click={open = false}>x</button></div>{/if}\n</div>";
    let g = build(files(&[("Card", card)]), "t", None).expect("builds");
    for want in ["d.attrTriggers(", "if d.triggered(", "= d.triggerText(", "d.attrCaret("] {
        assert!(g.text.contains(want), "missing {want} in:\n{}", g.text);
    }
    fails("<div><textarea triggers=\"@\"></textarea></div>", "needs `on:trigger`");
    fails("<script>\n let t = \"@\";\n</script>\n<div><textarea triggers={t} on:trigger={t = trigger}></textarea></div>", "takes a literal");
    fails("<script>\n let n = 0;\n</script>\n<div on:trigger={n = 1}></div>", "unknown attribute `on:trigger`");
}

fn src(path: &str, name: &str, text: &str) -> SrcFile {
    SrcFile { path: path.into(), name: name.into(), text: text.into() }
}

const SHELL: &str = "<script>\n let n = 0;\n</script>\n<div>\n  <game.Bar hp={3} on:buy={n = 1} />\n  <game.hud.Cool hp={1} />\n</div>";
const BAR: &str = "<script>\n    props { hp: int; }\n    events { buy; }\n</script>\n<div>{hp}</div>\n";

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
    let got = super::scan::scan(&d, "ui", &[]).unwrap();
    let names: Vec<_> = got.iter().map(|f| (f.name.as_str(), f.shown.as_str())).collect();
    assert_eq!(names, [("Shell", "ui/Shell.j3ui"), ("game.Bar", "ui/game/Bar.j3ui"), ("game.hud.Cool", "ui/game/hud/Cool.j3ui")]);
    std::fs::write(d.join("a.b.j3ui"), "").unwrap();
    assert!(super::scan::scan(&d, "ui", &[]).unwrap_err().contains("cannot contain `.`"));
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

const SLOT_CARD: &str = "<script>\n props { title: string; }\n</script>\n<section>{title}<slot name=\"actions\" /><slot /><div><slot name=\"footer\">Thanks</slot></div></section>";

#[test]
fn slots_generate_methods_on_the_owner() {
    let page = "<script>\n let xs: []int = [1, 2];\n let n = 0;\n</script>\n<div>\n  {#each xs as x, i}\n    <Card title=\"a\"><button slot=\"actions\" on:click={n += x}>b</button><p>{i}</p></Card>\n  {/each}\n  <Card title=\"b\"><Card title=\"c\">deep</Card></Card>\n  <Card title=\"d\" />\n</div>";
    let g = build(files(&[("Card", SLOT_CARD), ("Page", page)]), "t", None).unwrap_or_else(|e| panic!("{e}"));
    for want in [
        "fn slot1_actions_update(this: &Page, d: &Dom, i: int)",
        "let x = x2[i];",
        "fn slot1_default_paint(",
        "fn slot3_default_update(",
        "slotSite: int;",
        "((this.slotOwn as ptr) as &Page).slot1_actions_update(d, this.slotI);",
        ".slotOwn = this as int;",
        // the child tells the owner when it stops rendering a slot it rendered last frame
        "fn slot1_actions_leave(this: &Page, d: &Dom, i: int)",
        "if this.slotWas",
        ".slot1_actions_leave(d, this.slotI);",
    ] {
        assert!(g.text.contains(want), "missing {want} in:\n{}", g.text);
    }
    // a use with no children gets no methods
    assert!(!g.text.contains("slot4_"), "{}", g.text);
}

#[test]
fn slot_errors_point_at_the_use() {
    let card = |inner: &str| build(files(&[("Card", SLOT_CARD), ("Page", inner)]), "t", None).err().expect("expected an error");
    let e = card("<div>\n  <Card title=\"a\">\n    <p slot=\"nope\">x</p>\n  </Card>\n</div>");
    assert!(e.contains("no `<slot name=\"nope\">`") && e.contains("ui/Page.j3ui:3:8"), "{e}");
    let e = card("<div><p slot=\"actions\">x</p></div>");
    assert!(e.contains("unknown attribute `slot`"), "{e}");
    let e = build(files(&[("Row", ROW), ("Page", "<div><Row label=\"a\">x</Row></div>")]), "t", None).err().expect("no slot");
    assert!(e.contains("takes no children"), "{e}");
    let e = build(files(&[("Dup", "<div><slot /><slot /></div>")]), "t", None).err().expect("dup");
    assert!(e.contains("one default `<slot />`") && e.contains("ui/Dup.j3ui:1:14"), "{e}");
    let e = build(files(&[("Dup", "<div><slot name=\"a\" /><slot name=\"a\" /></div>")]), "t", None).err().expect("dup named");
    assert!(e.contains("two `<slot name=\"a\">`"), "{e}");
    let e = build(files(&[("S", "<script>\n let n = \"x\";\n</script>\n<div><slot name={n} /></div>")]), "t", None).err().expect("dynamic name");
    assert!(e.contains("plain `name"), "{e}");
    // loose text goes to the default slot; a slot that is not declared in a named-only child is an error
    let named = "<div><slot name=\"a\" /></div>";
    let e = build(files(&[("N", named), ("Page", "<div><N>text</N></div>")]), "t", None).err().expect("no default");
    assert!(e.contains("no default `<slot />`"), "{e}");
    assert!(build(files(&[("Card", SLOT_CARD), ("Page", "<div><Card title=\"a\">text</Card></div>")]), "t", None).is_ok());
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
fn slots_constant_per_site() {
    let card = "<script>\n props { t: string; }\n</script>\n<section>{#if $slots.footer}<small>{t}</small>{/if}<slot /><slot name=\"footer\" /></section>";
    let page = "<div><Card t=\"a\"><p slot=\"footer\">f</p></Card><Card t=\"b\">body</Card><Card t=\"c\" /></div>";
    let g = build(files(&[("Card", card), ("Page", page)]), "t", None).expect("builds");
    // site 1 fills only footer; site 2 only the default; site 3 has no children (never sets slotSite)
    assert!(g.text.contains("fn slotHas_footer(this: &Card): bool {\n        return this.slotSite == 1;"), "{}", g.text);
    assert!(g.text.contains("fn slotHas_default(this: &Card): bool {\n        return this.slotSite == 2;"), "{}", g.text);
    assert!(g.text.contains("this.slotHas_footer()"), "{}", g.text);
    let bad = "<script>\n props { t: string; }\n</script>\n<div>{#if $slots.nope}x{/if}<slot /></div>";
    let e = build(files(&[("Bad", bad)]), "t", None).err().expect("unknown slot");
    assert!(e.contains("`$slots.nope`") && e.contains("ui/Bad.j3ui:4:"), "{e}");
    let e = build(files(&[("Bad", "<div>{#if $slots}x{/if}</div>")]), "t", None).err().expect("bare");
    assert!(e.contains("needs a slot name"), "{e}");
}

#[test]
fn slot_named_default_and_numeric_shorthand_style_are_errors() {
    let e = build(files(&[("D", "<div><slot name=\"default\" /></div>")]), "t", None).err().expect("default name");
    assert!(e.contains("use `<slot />`"), "{e}");
    let e = build(files(&[("G", "<script>\n props { g: int; }\n</script>\n<div style:gap={g}></div>")]), "t", None).err().expect("numeric gap");
    assert!(e.contains("`style:gap` is a shorthand") && e.contains("{v}px"), "{e}");
    assert!(build(files(&[("G", "<script>\n props { g: int; }\n</script>\n<div style:gap=\"{g}px\" style:width={g}></div>")]), "t", None).is_ok());
    // a keyword property is text: `font-variant-numeric` (tabular digits) is settable inline
    assert!(build(files(&[("G", "<script>\n props { on: bool; }\n</script>\n<div style:font-variant-numeric=\"tabular-nums\">1</div>")]), "t", None).is_ok());
}

#[test]
fn speculative_14_slots_check() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../speculative/14_ui_slots");
    let m = crate::project::Manifest::load(&dir.join("j3.toml")).unwrap();
    crate::compile::compile_manifest(&m).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn speculative_20_passthrough_check() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../speculative/20_ui_passthrough");
    let m = crate::project::Manifest::load(&dir.join("j3.toml")).unwrap();
    crate::compile::compile_manifest(&m).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn dynamic_class_and_style_generate() {
    let page = "<script>\n props { r: int; extra: string; pct: f32; }\n</script>\n<div class=\"slot r{r} {extra} on\" style:width={pct * 100.0} style:background=\"{extra}\" style:padding=\"2px\" style:color={\"red\"}>x</div>";
    let g = build(files(&[("Page", page)]), "t", None).unwrap_or_else(|e| panic!("{e}"));
    for want in ["d.clsId(n", "d.clsDyn(n", "`r{r} {extra}`", "d.styleNum(n", "0, \"width\", f32(", "d.styleStr(n", "1, \"background\"", "2, \"padding\"", "3, \"color\""] {
        assert!(g.text.contains(want), "missing {want} in:\n{}", g.text);
    }
    // only whole literal words stay static: `r{r}` is not the class `r`
    assert!(!g.text.contains("cls_r"), "{}", g.text);
}

#[test]
fn style_numeric_offsets_generate() {
    let g = build(files(&[("Page", "<script>\n props { x: f32; }\n</script>\n<div><span style:left={x} style:top={x} style:right={x} style:bottom={x}>x</span></div>")]), "t", None).unwrap_or_else(|e| panic!("{e}"));
    for p in ["left", "top", "right", "bottom"] {
        assert!(g.text.contains(&format!("\"{p}\", f32(")), "missing {p} in:\n{}", g.text);
    }
}

#[test]
fn style_attr_errors() {
    for (attr, want) in [
        ("style:animation=\"x 1s\"", "can be set inline"),
        ("style:nope=\"1\"", "not a CSS property"),
        ("style:width", "needs a value"),
        ("style:padding={4.0}", "shorthand"),
    ] {
        fails(&format!("<div {attr}>x</div>"), want);
    }
}

#[test]
fn lifecycle_fns_have_fixed_signatures() {
    fails("<script>\n fn onFrame() { }\n</script>\n<div></div>", "ui/Card.j3ui:2:5");
    fails("<script>\n fn onFrame(dt: int) { }\n</script>\n<div></div>", "`onFrame` has a fixed signature");
    fails("<script>\n fn onMount(x: int) { }\n</script>\n<div></div>", "`onMount` has a fixed signature");
    fails("<script>\n fn onDestroy(): bool { return true; }\n</script>\n<div></div>", "`onDestroy` has a fixed signature");
    let card = "<script>\n fn onMount() { }\n fn onFrame(dt: f32) { }\n fn onDestroy() { }\n</script>\n<div>{#if true}<Row label=\"a\" />{/if}</div>";
    let g = build(files(&[("Card", card), ("Row", ROW)]), "t", None).expect("builds");
    for want in ["this.onFrame(d.dt());", "this.onMount();", "this.onDestroy();", "this.c1.destroy();"] {
        assert!(g.text.contains(want), "missing {want}");
    }
}

#[test]
fn keyed_and_nested_each() {
    let ok = |card: &str| build(files(&[("Card", card), ("Row", ROW)]), "t", None).unwrap_or_else(|e| panic!("{e}"));
    // components in two nested lists, the inner one keyed: rows own the state, the key is formatted once per row
    let g = ok("<script>\n props { xs: []Group; }\n</script>\n{#each xs as g (g.id)}\n<div>{#each g.rows as r (r.id)}<Row label={r.name} on:buy={x = 1} />{/each}</div>\n{/each}");
    for want in ["struct Card_R1", "struct Card_R2", "kd1.add(`{", "d.keyBegin(r1.uid)", "d.groupNew(r1.uid)", ".kd2.finish(\"ui/Card.j3ui:5\")"] {
        assert!(g.text.contains(want), "missing {want:?} in:\n{}", g.text);
    }
    assert!(g.text.contains("use j3.ui.keyed.Keyed;"), "{}", g.text);
    // unkeyed nesting with components, an {#if} holding one, and a range key
    ok("<script>\n props { xs: []Group; show: bool; }\n</script>\n{#each xs as g}{#each g.rows as r}{#if show}<Row label={r.name} />{/if}{/each}{/each}");
    ok("{#each 0..3 as i (i * 2)}<Row label=\"a\" />{/each}");
    ok("<script>\n props { xs: []Group; }\n</script>\n{#each xs as g}{#each g.rows as r (r.id)}<Row label=\"a\" />{/each}{/each}");
    ok("<script>\n props { xs: []Group; }\n</script>\n{#each xs as g (g.id)}{#each g.rows as r}<Row label=\"a\" />{/each}{/each}");
    ok("<script>\n props { xs: []Group; }\n</script>\n{#each xs as g, n (n)}<Row label=\"a\" />{/each}");
    // the key is formatted from a renamed item, so a body that binds the same name doesn't clash
    let g = ok("<script>\n props { xs: []Group; }\n</script>\n{#each xs as g (g.id)}<Row label=\"a\" />{/each}");
    assert!(g.text.contains("let ki1 = x1[kv1];") && g.text.contains("`{ki1.id}`"), "{}", g.text);
    assert!(!g.text.contains("Branch"), "{}", g.text);
}

#[test]
fn each_key_errors() {
    fails("<script>\n props { xs: []Group; }\n</script>\n{#each xs as g ()}\n<div></div>\n{/each}", "the `(key)` is empty");
    fails("<script>\n props { xs: []Group; }\n</script>\n{#each xs as g (g.id) (g.n)}\n<div></div>\n{/each}", "unbalanced");
    fails("<script>\n props { xs: []Group; }\n</script>\n{#each xs as g (g.id}\n<div></div>\n{/each}", "at the end of `{#each}`");
    // a slot site inside two lists would need two indices
    let card = "<script>\n props { xs: []Group; }\n</script>\n{#each xs as g}{#each g.rows as r}<Wrap><Row label=\"a\" /></Wrap>{/each}{/each}";
    let wrap = "<div><slot /></div>";
    let e = build(files(&[("Card", card), ("Row", ROW), ("Wrap", wrap)]), "t", None).err().expect("error");
    assert!(e.contains("two nested `{#each}`"), "{e}");
}

#[test]
fn virtual_list_generates_a_window() {
    let pre = "<script>\n props { xs: []Group; }\n</script>\n";
    let g = build(files(&[("Card", &format!("{pre}<div virtual row-height={{26}}>{{#each xs as g, n (g.id)}}<Row label=\"a{{n}}\" />{{/each}}</div>")), ("Row", ROW)]), "t", None).unwrap_or_else(|e| panic!("{e}"));
    for want in ["vl1.cfg(f32(26), f32(40.0), false, false);", "if this.vl1.stale(x2.len) {", "this.vl1.all.add(`{kr2d.id}`);", "this.vl1.all.push(`{kr2c.id}`);", "this.vl1.pk.add(`{kr2a.id}`);", "let ap2 = this.vl1.grew(x2.len);", "let w2 = vlist.step(d, n1, &this.vl1, x2.len);", "let m2 = this.vl1.cnt;", "for i2 in 0..m2 {", "let ax2 = this.vl1.at(i2); let g = x2[ax2]; let n = ax2;", "let ki2 = x2[kj2];", "let kj2 = this.vl1.at(kv2);", "this.vl1.verify(&this.kd1);", "vlist.finish(d, n1, &this.vl1);"] {
        assert!(g.text.contains(want), "missing {want:?} in:\n{}", g.text);
    }
    // measured rows with every option
    let all = "virtual stick-to-bottom est-height={50} keep={ks} bind:at-end={atEnd}";
    let pre2 = "<script>\n props { xs: []Group; ks: []string; }\n let atEnd = false;\n</script>\n";
    let g = build(files(&[("Card", &format!("{pre2}<div {all}>{{#each xs as g (g.id)}}<Row label=\"a\" />{{/each}}</div>")), ("Row", ROW)]), "t", None).unwrap_or_else(|e| panic!("{e}"));
    for want in [".cfg(f32(0.0), f32(50), true, true);", ".keep = ", "&& !", ".atEnd { ", ".toEnd = true;", " = this.vl1.atEnd;"] {
        assert!(g.text.contains(want), "missing {want:?} in:\n{}", g.text);
    }
    let g = build(files(&[("Card", &format!("{pre}<div virtual row-height=\"auto\">{{#each xs as g (g.id)}}<Row label=\"a\" />{{/each}}</div>")), ("Row", ROW)]), "t", None).unwrap_or_else(|e| panic!("{e}"));
    assert!(g.text.contains(".cfg(f32(0.0), f32(40.0), false, false);"), "auto row height is measured:\n{}", g.text);
    let body = |inner: &str, attrs: &str| format!("{pre}<div {attrs}>{inner}</div>");
    let each = "{#each xs as g (g.id)}<Row label=\"a\" />{/each}";
    fails(&body(each, "row-height={26}"), "needs `virtual`");
    fails(&body(each, "stick-to-bottom"), "needs `virtual`");
    fails(&body(each, "virtual row-height={26} est-height={30}"), "this list has a `row-height`");
    fails(&body(each, "virtual stick-to-bottom={1}"), "takes no value");
    fails(&body(each, "virtual keep"), "`keep` needs");
    fails(&body(each, "virtual bind:at-end={1 + 2}"), "needs a place");
    fails(&body(each, "virtual est-height=\"big\""), "needs a number");
    fails(&body("<p>x</p>", "virtual row-height={26}"), "exactly one child");
    fails(&body(&format!("{each}{each}"), "virtual row-height={26}"), "exactly one child");
    fails(&body("{#each xs as g}<Row label=\"a\" />{/each}", "virtual row-height={26}"), "needs a key");
    fails(&body("{#each 0..9 as g (g)}<Row label=\"a\" />{/each}", "virtual row-height={26}"), "needs a key and a list");
    fails(&body(each, "virtual row-height=\"tall\""), "needs a number");
    let seven = (0..7).map(|i| format!("style:width={{{i}}}")).collect::<Vec<_>>().join(" ");
    fails(&body(each, &format!("virtual row-height={{26}} {seven}")), "at most 6 `style:`");
}

#[test]
fn grid_attribute_binds_the_grid_and_edit_events() {
    let pre = "<script>\n use j3.ui.gridstate.Grid;\n props { xs: []Group; g: &Grid; }\n</script>\n";
    let g = build(files(&[("Card", &format!("{pre}<div virtual row-height={{24}} grid={{g.sized(xs.len)}} on:edit={{ if kind == 0 {{ g.draft = g.initial(\"\"); }} }}>{{#each xs as r (r.id)}}<Row label=\"a\" />{{/each}}</div>")), ("Row", ROW)]), "t", None).unwrap_or_else(|e| panic!("{e}"));
    for want in ["d.gridBind(n1, g.sized(", "if d.gridEdit(n1) {", "let kind = d.gridOf(n1).evKind;", "let row = d.gridOf(n1).evRow;", "let col = d.gridOf(n1).evCol;", "let text = d.gridOf(n1).evText;"] {
        assert!(g.text.contains(want), "missing {want:?} in:\n{}", g.text);
    }
    fails(&format!("{pre}<div grid={{g}}></div>"), "goes on the `virtual` scroll box");
    fails(&format!("{pre}<div on:edit={{ g.row = 0; }}></div>"), "needs `grid=");
}

fn fails_in(list: &[(&str, &str)], want: &str) {
    match build(files(list), "t", None) {
        Ok(_) => panic!("expected an error containing {want:?}"),
        Err(e) => assert!(e.contains(want), "wanted {want:?} in:\n{e}"),
    }
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
fn match_blocks() {
    let card = "<script>\n let pg = 1;\n</script>\n{#match pg}\n  {:case 0}<p>a</p>\n  {:case 1, 2}<Row label=\"x\" />\n  {:else}<p>c</p>\n{/match}";
    let g = build(files(&[("Card", card), ("Row", ROW)]), "t", None).expect("builds");
    assert!(g.text.contains("== 1 ||") || g.text.contains("== 1 &&") || g.text.contains(" || "), "{}", g.text);
    // no `{:else}` is fine
    build(files(&[("Card", "<script>\n let pg = 1;\n</script>\n{#match pg}{:case 0}<p>a</p>{/match}")]), "t", None).expect("builds");
    fails("<script>\n let pg = 1;\n</script>\n{#match pg}\n{:case 0}<p></p>\n{:case 1}<p></p>\n{:case 0}<p></p>\n{/match}", "ui/Card.j3ui:7:8");
    fails("<script>\n let pg = 1;\n</script>\n{#match pg}\n{:case 0}<p></p>\n{:else}<p></p>\n{:case 1}<p></p>\n{/match}", "ui/Card.j3ui:7:1");
    fails("<script>\n let pg = 1;\n</script>\n{#match pg}\n{:case 0}<p></p>\n{:else if pg}<p></p>\n{/match}", "ui/Card.j3ui:6:1");
    fails("<script>\n let pg = 1;\n</script>\n{#match pg}\n<p></p>\n{:case 0}<p></p>\n{/match}", "ui/Card.j3ui:4:1");
    fails("<script>\n let pg = 1;\n</script>\n{#match pg}\n{/match}", "has no `{:case}`");
    fails("<script>\n let pg = 1;\n</script>\n{#match pg}\n{:case 0}<p></p>", "ui/Card.j3ui:4:1");
    fails("<script>\n let pg = 1;\n</script>\n{#if pg}{:case 0}{/if}", "outside `{#match}`");
}

#[test]
fn handler_blocks_span_lines() {
    let card = "<script>\n let a = 0;\n let s = \"}\";\n</script>\n<button on:click={\n  a = 1;\n  if a > 0 { s = \"}{\"; }\n  a = 2\n}>x</button>";
    build(files(&[("Card", card)]), "t", None).expect("builds");
}

const CHIP: &str = "<script>\n props { label: string; }\n</script>\n<div class=\"chip\" style:height={20}>{label}</div>\n";

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
    let own = g.text.find("d.styleNum(n1, 0, \"height\"").expect("own style");
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

#[test]
fn context_popover_and_custom_tooltip_binding() {
    let g = build(files(&[("Card", r#"<script>
let menu: bool = false;
let card: bool = false;
</script>
<div bind:tooltip={card} on:contextmenu={menu = true}>
{#if menu}<div popover="context" on:close={menu = false}><button>Open</button></div>{/if}
{#if card}<div popover><button>Details</button></div>{/if}
</div>"#)]), "t", None).expect("context and custom tooltip compile");
    assert!(g.text.contains("d.attrContext("));
    assert!(g.text.contains("d.hoverCard("));
    assert!(g.text.contains("d.contextAt("));
    assert!(g.text.contains("let mouse = d.mouseAt();"));
    fails("<div bind:tooltip=\"yes\" />", "boolean_place");
    fails("<div bind:tooltip={a + 1} />", "place");
    fails("<div popover=\"other\" />", "takes");
    fails("<div bind:tooltip={card}><span>no card</span></div>", "direct child");
    fails("<div bind:tooltip={card}><div><div popover /></div></div>", "owner wrapper");
    fails("<div bind:tooltip={card}><div popover=\"context\" /></div>", "bare `popover`");
}

#[test]
fn option_visual_markup_and_restrictions() {
    let g = build(files(&[("Page", r#"<select><option value="a"><svg viewBox="0 0 10 10"><path d="M0 0 L10 10" /></svg>Red <span class="name">Apple</span>{#if true}<small> ripe</small>{/if}{#each [1] as x}<span>{x}</span>{/each}</option></select>"#)]), "t", None).unwrap();
    assert!(g.text.contains("d.attrMedia("));
    for child in ["<button>Go</button>", "<input />", "<span on:click={x = true}>Go</span>", "<span tabindex=\"0\">Go</span>", "<span popover>Go</span>", "{#if true}<button>Go</button>{/if}"] {
        fails(&format!("<select><option>{child}</option></select>"), "option");
    }
}

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

#[test]
fn let_blocks_scope_to_their_block() {
    let card = "<script>\n props { xs: []int; }\n</script>\n<div>\n  {#let n = xs.len}\n  {#let m = n + 1}\n  {#each xs as x, i}\n    {#let y = x + m}\n    {#if y > 1}{#let z = y * 2}<span>{z}</span>{/if}\n    <span>{y}</span>\n  {/each}\n  <span>{m}</span>\n</div>";
    let g = build(files(&[("Card", card)]), "t", None).expect("builds");
    assert_eq!(g.text.matches("let lt").count(), 4, "{}", g.text);
    // a sibling block may reuse a name
    let two = "<script>\n props { xs: []int; }\n</script>\n<div>\n  {#each xs as x}{#let y = x}<span>{y}</span>{/each}\n  {#each xs as x}{#let y = x}<span>{y}</span>{/each}\n</div>";
    build(files(&[("Card", two)]), "t", None).expect("sibling blocks reuse a let name");
    // inside slot content, and with a passthrough root
    let slot = "<script>\n props { xs: []int; }\n</script>\n<div>{#let k = xs.len}<Row label=\"a\">{k}</Row></div>";
    let slot_row = "<script>\n props { label: string; }\n</script>\n<div>{label}<slot /></div>";
    build(files(&[("Card", slot), ("Row", slot_row)]), "t", None).expect("let reaches slot content");
}

#[test]
fn let_errors_point_at_the_markup() {
    let dup = "<script>\n props { a: int; }\n</script>\n<div>\n  {#let n = a}\n  {#let n = a + 1}\n</div>";
    fails(dup, "ui/Card.j3ui:6:9");
    fails("<script>\n props { a: int; }\n</script>\n<div>{#let a = 1}</div>", "shadows a prop");
    fails("<div>{#let q}</div>", "needs `name = expr`");
    fails("<script>\n props { xs: []int; }\n</script>\n<div>{#each 0..3 as &i}{i}{/each}</div>", "ui/Card.j3ui:4:21");
}

#[test]
fn each_by_reference_binds_a_pointer() {
    let card = "<script>\n props { xs: []int; }\n</script>\n<div>{#each xs as &x (x)}<button on:click={x += 1}>b</button>{/each}</div>";
    let g = build(files(&[("Card", card)]), "t", None).expect("builds");
    assert!(g.text.contains("= &x"), "{}", g.text);
}

#[test]
fn int_keys_skip_formatting() {
    let pre = "<script>\n props { xs: []Group; ks: []string; }\n</script>\n";
    let g = build(files(&[("Card", &format!("{pre}<div>{{#each xs as g (g.id : int)}}<Row label=\"a\" />{{/each}}</div>")), ("Row", ROW)]), "t", None).unwrap_or_else(|e| panic!("{e}"));
    assert!(g.text.contains("kd1.addInt(ki"), "{}", g.text);
    assert!(!g.text.contains("kd1.add(`"), "an int key is not formatted:\n{}", g.text);
    // a text key keeps the template
    let g = build(files(&[("Card", &format!("{pre}<div>{{#each xs as g (g.id)}}<Row label=\"a\" />{{/each}}</div>")), ("Row", ROW)]), "t", None).unwrap_or_else(|e| panic!("{e}"));
    assert!(g.text.contains("kd1.add(`{") && !g.text.contains("addInt"), "{}", g.text);
    // virtual lists use the int calls too
    let g = build(files(&[("Card", &format!("{pre}<div virtual row-height={{26}}>{{#each xs as g (g.id : int)}}<Row label=\"a\" />{{/each}}</div>")), ("Row", ROW)]), "t", None).unwrap_or_else(|e| panic!("{e}"));
    for want in ["all.addInt(kr", "all.pushInt(kr", "pk.addInt(kr"] {
        assert!(g.text.contains(want), "missing {want:?} in:\n{}", g.text);
    }
    // spacing around the hint, and a key ending in a name that merely starts with `int`
    for key in ["(g.id:int)", "( g.id : int )", "(g.id :int)"] {
        let g = build(files(&[("Card", &format!("{pre}<div>{{#each xs as g {key}}}<Row label=\"a\" />{{/each}}</div>")), ("Row", ROW)]), "t", None).unwrap_or_else(|e| panic!("{key}: {e}"));
        assert!(g.text.contains("kd1.addInt(ki2.id);"), "{key}:\n{}", g.text);
    }
    let pre3 = "<script>\n props { xs: []Group; integer: int; }\n</script>\n";
    let g = build(files(&[("Card", &format!("{pre3}<div>{{#each xs as g (g.id + integer)}}<Row label=\"a\" />{{/each}}</div>")), ("Row", ROW)]), "t", None).unwrap_or_else(|e| panic!("{e}"));
    assert!(g.text.contains("kd1.add(`{") && !g.text.contains("addInt"), "{}", g.text);
    // `: integer` is not the hint: the text is left in the key expression (J3 then rejects it)
    let g = build(files(&[("Card", &format!("{pre}<div>{{#each xs as g (g.id : integer)}}<Row label=\"a\" />{{/each}}</div>")), ("Row", ROW)]), "t", None).unwrap_or_else(|e| panic!("{e}"));
    assert!(g.text.contains("kd1.add(`{") && !g.text.contains("addInt"), "{}", g.text);
    fails(&format!("{pre}<div virtual keep={{ks}}>{{#each xs as g (g.id : int)}}<Row label=\"a\" />{{/each}}</div>"), "has an int key");
}

#[test]
fn speculative_27_let_and_ref_check() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../speculative/27_ui_let");
    let m = crate::project::Manifest::load(&dir.join("j3.toml")).unwrap();
    crate::compile::compile_manifest(&m).unwrap_or_else(|e| panic!("{e}"));
}

/// A one-screen project in a temp dir, through the whole compiler (the J3 checker included).
fn project(tag: &str, screen: &str, extra: &[(&str, &str)]) -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("j3ui_let_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::create_dir_all(dir.join("ui")).unwrap();
    std::fs::write(dir.join("j3.toml"), "[package]\nname = \"lt\"\nentry = \"src/main.j3\"\n[ui]\ndir = \"ui\"\n").unwrap();
    std::fs::write(dir.join("src/main.j3"), "module lt;\nuse lt.ui.{ Screen, ScreenProps };\nfn main() {}\n").unwrap();
    std::fs::write(dir.join("ui/Screen.j3ui"), screen).unwrap();
    for (n, t) in extra {
        std::fs::write(dir.join(format!("ui/{n}.j3ui")), t).unwrap();
    }
    let m = crate::project::Manifest::load(&dir.join("j3.toml")).unwrap();
    let r = crate::compile::compile_manifest(&m).map(|_| ()).map_err(|e| e.to_string());
    let _ = std::fs::remove_dir_all(&dir);
    r
}

#[test]
fn let_in_keys_and_nested_constructs() {
    // a let before the each is visible to its key
    let ok = "<script>\n props { xs: []int; }\n</script>\n<div>{#let base = 10}{#each xs as x (x + base)}<span>{x}</span>{/each}</div>";
    project("key_ok", ok, &[]).unwrap_or_else(|e| panic!("{e}"));
    // a let inside the body is not: the key is evaluated before the body
    let bad = "<script>\n props { xs: []int; }\n</script>\n<div>\n{#each xs as x (x + inner)}{#let inner = 1}<span>{x}</span>{/each}</div>";
    let e = project("key_bad", bad, &[]).expect_err("inner let must not reach the key");
    assert!(e.contains("Screen.j3ui:5:"), "{e}");
    // an outer let survives a keyed each, a match arm, component props and an event handler inside them
    let row = "<script>\n props { v: int; }\n events { hit(n: int); }\n</script>\n<button on:click={emit hit(v)}>{v}</button>";
    let nest = "<script>\n props { xs: []int; }\n let got = 0;\n</script>\n<div>\n{#let k = xs.len}\n{#each xs as x (x + k)}\n{#match x}{:case 1}<Row v={k + x} on:hit={got += k + event.n} />{:else}<span>{k}</span>{/match}\n{/each}\n<Row v={k} on:hit={got += k + event.n} />\n</div>";
    project("nest", nest, &[("Row", row)]).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn emit_in_script_fns() {
    let row = "<script>\n events { pick(n: int, s: string); done; }\n fn send(n: int) { emit pick(n, \"x\"); }\n fn both() { send(1); emit done; }\n fn onMount() { both(); }\n</script>\n<button on:click={send(1)}>a</button><button on:click={both()}>b</button>";
    let g = build(files(&[("Card", "<div><Row /></div>"), ("Row", row)]), "t", None).unwrap_or_else(|e| panic!("{e}"));
    for want in ["this.sev.pick_n = n;", "this.sev.pick = true", "this.sev.done = true", "sev: RowEvents;", "if this.sev.pick {"] {
        assert!(g.text.contains(want), "missing {want:?} in:\n{}", g.text);
    }
    assert!(!g.text.contains("sev: CardEvents"), "a component without emitting fns gets no `sev`");
    // the whole thing goes through the J3 checker too
    project("emit_fn", "<script>\n let got = 0;\n</script>\n<div><Row on:pick={got = event.n} on:done={got = 0} /></div>", &[("Row", row)]).unwrap_or_else(|e| panic!("{e}"));
    // no event scope: paint and onDestroy
    let draw = "<script>\n events { done; }\n fn f() { emit done; }\n fn g() { f(); }\n</script>\n<canvas on:draw={g()} />";
    fails(draw, "`g` emits an event, but there is no event scope here");
    fails("<script>\n events { done; }\n fn onDestroy() { emit done; }\n</script>\n<div></div>", "doesn't work in `onDestroy`");
    build(files(&[("Card", "<script>\n events { done; }\n fn f() { }\n fn onDestroy() { f(); }\n</script>\n<div></div>")]), "t", None).expect("a fn that doesn't emit is fine anywhere");
}

#[test]
fn speculative_28_emit_fn_check() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../speculative/28_ui_emit_fn");
    let m = crate::project::Manifest::load(&dir.join("j3.toml")).unwrap();
    crate::compile::compile_manifest(&m).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn emitting_fns_are_scoped_by_call() {
    let head = "<script>\n events { done; }\n fn send() { emit done; }\n";
    // a markup expression runs on every update: refused, direct or through another fn
    fails(&format!("{head} fn g(): int {{ send(); return 1; }}\n</script>\n<div>{{g()}}</div>"), "`g` emits an event, but there is no event scope here");
    fails(&format!("{head}</script>\n<div>{{#if send()}}x{{/if}}</div>"), "`send` emits an event");
    // a local named like the fn is already refused by the shadowing rule, so the scan never has to tell them apart
    fails(&format!("{head} fn h() {{ let send = 1; }}\n</script>\n<div></div>"), "shadows a prop, `let`, `fn`");
    // a field or method of the same name is not a call to the fn
    let draw = format!("{head} fn k(q: P) {{ let a = q.send(); let b = P {{ send: 1 }}; let c = m::send(); }}\n</script>\n<canvas on:draw={{k(q)}} />");
    build(files(&[("Card", &draw)]), "t", None).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn recursive_emitting_fns_terminate_and_mark_both() {
    let head = "<script>\n events { done; }\n fn a(n: int) { if n > 0 { b(n - 1); } }\n fn b(n: int) { if n == 0 { emit done; } else { a(n - 1); } }\n fn r(n: int) { r(n - 1); }\n";
    fails(&format!("{head}</script>\n<canvas on:draw={{a(3)}} />"), "`a` emits an event");
    fails(&format!("{head}</script>\n<canvas on:draw={{b(3)}} />"), "`b` emits an event");
    build(files(&[("Card", &format!("{head}</script>\n<canvas on:draw={{r(3)}} />"))]), "t", None).expect("a self-recursive fn that never emits is fine");
}

#[test]
fn ordered_flex_paint_and_canvas_locals() {
    let source = "<script>\n const label = \"size\";\n props { rect: &Rect; }\n let sizeable = 1;\n</script>\n<div style:order={-1} style:flex-basis=\"50%\" style:flex-shrink={0} style:align-self=\"center\"><canvas on:draw={let w = size.x; let own = rect.size; let named = sizeable; let text = \"size\";} /><canvas on:draw={let h = size.y;} /></div>";
    let generated = build(files(&[("Screen", source)]), "t", None).expect("flex item longhands exposed");
    assert!(generated.text.contains("paintChildren(pass"));
    assert!(!generated.text.contains("let size ="));
    assert!(generated.text.contains("size2.x"));
    assert!(generated.text.contains("size3.y"));
    assert!(generated.text.contains("rect.size"));
    assert!(generated.text.contains("sizeable"));
    assert!(generated.text.contains("let text = \"size\""));
}

#[test]
fn drag_and_drop_markup() {
    let card = r#"<script>
let held: string = "";
let over: bool = false;
let n: int = 0;
let on: bool = true;
</script>
<div draggable={on} drag-data="sword" drag-id={n} on:dragstart={held = data; n = dragId} on:dragend={held = ""; over = dropped}>x</div>
<div on:dragenter={over = true} on:dragleave={over = false} on:dragover={if dragId == 3 { accept = false; }} on:drop={held = data; n = dragId}>slot</div>
<span draggable drag-ghost="none">y</span>"#;
    let g = build(files(&[("Card", card)]), "t", None).expect("drag markup compiles");
    for want in [
        "d.attrDraggable(", "d.attrDragData(", "d.attrDragId(", "d.attrDragGhost(", "d.dragStarted(", "d.dragEnded(", "d.dragEntered(", "d.dragLeft(",
        "d.dragOver(", "d.dropAt(", "d.attrDragTarget(", ", 8);", "let data = d.dragData();", "let dragId = d.dragIdOf();", "let dropped = d.dragDropped();",
        "let accept = true;", "d.dragReject(",
    ] {
        assert!(g.text.contains(want), "missing {want}");
    }
    fails("<div drag-data=\"x\">x</div>", "needs `draggable`");
    fails("<div drag-id={1}>x</div>", "needs `draggable`");
    fails("<div on:dragstart={n = 1}>x</div>", "needs `draggable`");
    fails("<div on:dragend={n = 1}>x</div>", "needs `draggable`");
    fails("<div draggable drag-ghost=\"yes\">x</div>", "takes only");
    fails("<div draggable drag-id=\"3\">x</div>", "needs `={integer}`");
    fails("<div draggable=\"true\">x</div>", "takes `{condition}`");
    fails("<div on:drop>x</div>", "needs `={ statements }`");
}

#[test]
fn animate_flip_markup() {
    let head = "<script>\nprops { items: []Group; }\nlet ms: int = 300;\nlet on: bool = true;\n</script>\n";
    let ok = |body: &str| build(files(&[("Card", &format!("{head}{body}"))]), "t", None);
    let g = ok("<div>{#each items as it (it.id)}<p animate:flip>{it.id}</p>{/each}</div>").expect("bare flip compiles");
    assert!(g.text.contains("d.flip(") && g.text.contains(", 200.0, 3);"), "default 200ms ease-out");
    let g = ok("<div>{#each items as it (it.id)}<p animate:flip={ms} animate-easing=\"bounce\">{it.id}</p>{/each}</div>").expect("flip with easing compiles");
    assert!(g.text.contains("f32(") && g.text.contains(", 5);"), "bounce preset");
    ok("<div>{#each items as it (it.id)}{#if on}<p animate:flip>{it.id}</p>{/if}{/each}</div>").expect("a row that is an {#if} child is still the row");
    let bad = |body: &str, want: &str| match ok(body) {
        Ok(_) => panic!("expected {want:?}"),
        Err(e) => assert!(e.contains(want), "wanted {want:?} in:\n{e}"),
    };
    bad("<div animate:flip>x</div>", "keyed `{#each}`");
    bad("<div>{#each items as it}<p animate:flip>{it.id}</p>{/each}</div>", "keyed `{#each}`");
    bad("<div>{#each items as it (it.id)}<p><span animate:flip>{it.id}</span></p>{/each}</div>", "keyed `{#each}`");
    bad("<div virtual row-height={20}>{#each items as it (it.id)}<p animate:flip>{it.id}</p>{/each}</div>", "virtual");
    bad("<div>{#each items as it (it.id)}<p animate:flip=\"x\">{it.id}</p>{/each}</div>", "takes `={milliseconds}`");
    bad("<div>{#each items as it (it.id)}<p animate:flip animate-easing=\"wobble\">{it.id}</p>{/each}</div>", "animate-easing");
    bad("<div>{#each items as it (it.id)}<p animate-easing=\"ease\">{it.id}</p>{/each}</div>", "needs `animate:flip`");
    fails(&format!("{head}<div>{{#each items as it (it.id)}}<Row animate:flip label=\"a\"/>{{/each}}</div>"), "goes on the row element, not a component");
}

#[test]
fn transition_markup() {
    let head = "<script>\nlet ms: int = 300;\nlet on: bool = true;\n</script>\n";
    let ok = |body: &str| build(files(&[("Card", &format!("{head}{body}"))]), "t", None);
    let call = |body: &str| ok(body).expect(body).text;
    // (inK, inMs, outK, outMs, ease, fx, fy, sf, axis); fade keeps its 150 ms default
    assert!(call("{#if on}<p transition:fade>x</p>{/if}").contains(", 1, 150.0, 1, 150.0, -1, 0.0, 20.0, 0.5, 0);"));
    assert!(call("{#if on}<p transition:fly={ms}>x</p>{/if}").contains("d.trans("));
    assert!(call("{#if on}<p transition:slide={200} slide-axis=\"x\">x</p>{/if}").contains(", 3, f32(200), 3, f32(200), -1, 0.0, 20.0, 0.5, 1);"));
    assert!(call("{#if on}<p transition:scale scale-from={0.8} transition-easing=\"bounce\">x</p>{/if}").contains(", 4, 150.0, 4, 150.0, 5, 0.0, 20.0, f32(0.8), 0);"));
    assert!(call("{#if on}<p in:fly={100} out:fade={50} fly-x={-40} fly-y={0}>x</p>{/if}").contains(", 2, f32(100), 1, f32(50), -1, f32(-40), f32(0), 0.5, 0);"));
    assert!(call("{#if on}<p in:fly={100} out:scale={50} fly-x={7} scale-from={0.9}>x</p>{/if}").contains(", 2, f32(100), 4, f32(50), -1, f32(7), 20.0, f32(0.9), 0);"), "each param feeds its own kind");
    assert!(call("{#if on}<p in:scale>x</p>{/if}").contains(", 4, 150.0, 0, 0.0, "));
    assert!(call("{#if on}<p out:slide>x</p>{/if}").contains(", 0, 0.0, 3, 150.0, "));
    assert!(call("{#each [1, 2] as it (it)}<p transition:fade>{it}</p>{/each}").contains("d.trans("), "keyed rows too");
    let bad = |body: &str, want: &str| match ok(body) {
        Ok(_) => panic!("expected {want:?}"),
        Err(e) => assert!(e.contains(want), "wanted {want:?} in:\n{e}"),
    };
    bad("{#if on}<p transition:wipe>x</p>{/if}", "unknown transition `wipe`");
    bad("{#if on}<p in:spin>x</p>{/if}", "unknown transition `spin`");
    bad("{#if on}<p transition:fade=\"x\">x</p>{/if}", "takes `={milliseconds}`");
    bad("{#if on}<p transition:fade in:fly>x</p>{/if}", "repeats the enter transition");
    bad("{#if on}<p transition:fade out:fly>x</p>{/if}", "repeats the leave transition");
    bad("{#if on}<p in:fly in:fade>x</p>{/if}", "repeats the enter transition");
    bad("{#if on}<p transition:fade fly-x={3}>x</p>{/if}", "`fly-x` needs a `fly` transition");
    bad("{#if on}<p transition:fly fly-x=\"3\">x</p>{/if}", "`fly-x` takes `={number}`");
    bad("{#if on}<p in:scale out:fade fly-y={3}>x</p>{/if}", "`fly-y` needs a `fly` transition");
    bad("{#if on}<p in:fly out:scale slide-axis=\"x\">x</p>{/if}", "`slide-axis` needs a `slide` transition");
    bad("{#if on}<p transition:fade scale-from={1}>x</p>{/if}", "needs a `scale` transition");
    bad("{#if on}<p transition:slide slide-axis=\"z\">x</p>{/if}", "slide-axis");
    bad("{#if on}<p transition:fade transition-easing=\"wobble\">x</p>{/if}", "transition-easing");
    bad("{#if on}<p transition-easing=\"ease\">x</p>{/if}", "needs `transition:`");
}

#[test]
fn render_sets_the_paint_hook_of_its_own_dom_only() {
    let child = "<script>\n props { label: string; }\n</script>\n<div>{label}</div>\n";
    let screen = "<script>\n</script>\n<div><Row label=\"a\" /><canvas on:draw={let w = size.x;} /></div>\n";
    let g = build(files(&[("Screen", screen), ("Row", child)]), "t", None).expect("builds");
    // the root's render points the hook at its own paint walk, through its own dom
    assert_eq!(g.text.matches("setPaintHook(").count(), 1, "one root sets one hook");
    assert!(g.text.contains("d.setPaintHook(ScreenPaintHook, this as ptr);\n        d.paintAll();"));
    assert!(g.text.contains("fn ScreenPaintHook(p: ptr) {\n    let r = p as &Screen;\n    r.paint(&r.dom);\n}"));
    // a child component has neither a hook nor a render of its own: it paints inside the root's walk
    assert!(!g.text.contains("RowPaintHook"));
    // the hook lives on the Dom (no global), so a second root with its own Dom keeps its own
    assert!(!g.text.contains("PAINT_HOOK"));
}

#[test]
fn style_block_starts_at_its_file_line() {
    // the style block's first line is file line 5 (the leading blank lines are trimmed from the embedded text): rules and warnings report file lines
    let g = build(files(&[("Card", "<script>\n</script>\n<div></div>\n<style>\n\n.a { width: 1px; }\n</style>")]), "t", None).expect("builds");
    assert!(g.text.contains("const LINE0_Card: int = 5;"), "{}", g.text);
    assert!(g.text.contains("parseAt(css, CSS_Card, \"ui/Card.j3ui\", SCOPE_Card, LINE0_Card)"), "{}", g.text);
}

#[test]
fn bind_this_and_use_actions() {
    let fns = "<script>\n let el = -1;\n let rows: [3]int = [0, 0, 0];\n let n = 0;\n fn grow(node: int, w: int) { n += w; }\n fn grow_update(node: int, w: int) { n += 1; }\n fn grow_destroy(node: int) { n = 0; }\n fn plain(node: int) { n += 1; }\n fn name(node: int, s: string) { }\n fn name_update(node: int, s: string) { }\n fn focus() { d.focusNode(el); }\n</script>\n";
    let ok = |body: &str| build(files(&[("Card", &format!("{fns}{body}")), ("Row", ROW)]), "t", None).unwrap_or_else(|e| panic!("{e}"));
    let g = ok("<div bind:this={el}>x</div>");
    assert!(g.text.contains("this.el = d.handle(n1);"), "{}", g.text);
    assert!(g.text.contains("let d = (this.dm as ptr) as &Dom;"), "script fns reach the Dom:\n{}", g.text);
    let g = ok("{#each 0..3 as i}<span bind:this={rows[i]}>x</span>{/each}");
    assert!(g.text.contains("this.rows[i") && g.text.contains("= d.handle("), "{}", g.text);
    // an action: mount after the first layout, update on change, destroy when gone (also from a row that goes away)
    let g = ok("{#if n > 1}<div use:grow={n + 1}>x</div>{/if}<i use:plain>y</i>");
    for want in ["let pv", "this.grow(uh", "this.grow_update(uh", "this.grow_destroy(this.ua", "this.plain(uh", "!d.alive(this.ua"] {
        assert!(g.text.contains(want), "missing {want:?} in:\n{}", g.text);
    }
    let g = ok("{#each 0..3 as i}<div use:grow={i}>x</div>{/each}");
    assert!(g.text.contains("fn destroy(this: &t_Card_R") || g.text.contains("_destroy(this.ua"), "rows destroy their actions:\n{}", g.text);
    let g = ok("<input use:name={\"a\"} />");
    assert!(g.text.contains("d.strHas(this.up"), "strings compare by bytes:\n{}", g.text);
    // errors
    fails(&format!("{fns}<div use:nope>x</div>"), "needs a script fn `nope`");
    fails(&format!("{fns}<div use:tip.attach={{1}}>x</div>"), "dotted module function");
    fails(&format!("{fns}<div use:grow>x</div>"), "calls `fn grow(node: int)");
    fails(&format!("{fns}<div use:plain={{1}}>x</div>"), "calls `fn plain(node: int, param: T)");
    fails("<script>\n fn bad(node: string) { }\n</script>\n<div use:bad>x</div>", "calls `fn bad(node: int)");
    fails("<script>\n fn bad(node: int, v: []int) { }\n</script>\n<div use:bad={1}>x</div>", "plain value");
    fails("<script>\n fn bad(node: int, v: int) { }\n fn bad_update(node: int, v: f32) { }\n</script>\n<div use:bad={1}>x</div>", "same parameters");
    fails("<script>\n fn bad(node: int) { }\n fn bad_destroy() { }\n</script>\n<div use:bad>x</div>", "`bad_destroy` must be");
    fails("<script>\n let el = 0;\n</script>\n<div bind:this>x</div>", "`bind:this` needs");
    fails("<script>\n let el = 0;\n</script>\n<div bind:this={el + 1}>x</div>", "needs a place");
    fails("<script>\n let el = 0;\n</script>\n<Row label=\"a\" bind:this={el} />", "`bind:this` on a component isn't supported yet");
    fails("<script>\n fn go(node: int) { }\n</script>\n<Row label=\"a\" use:go />", "isn't supported on a component");
    // `d` in a script fn is the injected Dom, so a user's own `d` is an error, never a shadow
    fails("<script>\n fn f(d: int) { }\n</script>\n<div>x</div>", "`d` shadows");
    fails("<script>\n let d = 0;\n</script>\n<div>x</div>", "reserved");
    fails("<script>\n fn d() { }\n</script>\n<div>x</div>", "reserved");
    // a place reachable after the build is reset when its element goes
    let g = build(files(&[("Card", "<script>\n let el = -1;\n</script>\n{#if true}<div bind:this={el}>x</div>{/if}")]), "t", None).unwrap_or_else(|e| panic!("{e}"));
    assert!(g.text.contains("!d.alive(this.bh") && g.text.contains("= -1; }"), "{}", g.text);
}
