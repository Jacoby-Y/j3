//! Element, widget, input and accessibility markup.

use super::*;

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

#[test]
fn icons_copy_only_named_data() {
    let card = "<script>\n let on = true;\n</script>\n<p><icon name=\"trash\" aria-label=\"Delete\" /> <icon name={if on { \"volume-up\" } else { \"volume-mute\" }} /><svg viewBox=\"0 0 4 4\"><path fill-rule=\"evenodd\" d=\"M0 0h4v4H0z\" /></svg></p>";
    let g = build(files(&[("Card", card)]), "t", None).expect("builds");
    for want in ["d.attrIcon(n2, \"trash\", \"trash\\t0 0 16 16|", "d.attrIcon(n3, if this.on {", "volume-up\\t0 0 16 16|", "\\nvolume-mute\\t", "d.attrMedia(n5, 6, \"evenodd\")"] {
        assert!(g.text.contains(want), "missing {want} in:\n{}", g.text);
    }
    assert!(!g.text.contains("trash-fill"), "only named icons are copied");
    fails("<icon name=\"trash-can\" />", "unknown icon `trash-can`");
    fails("<icon name=\"trash-can\" />", "did you mean `trash");
    fails("<icon />", "needs `name=");
    fails("<script>\n let k = \"x\";\n</script>\n<icon name={k} />", "no string literals");
    fails("<icon name=\"x\">a</icon>", "takes no children");
}

/// `GLYPH_ACTIONS` (compile-time check) and the runtime tables (`glyphKey`, `glyphPad` in dom/glyph.j3) are two copies of one list.
#[test]
fn glyph_actions_match_the_runtime_tables() {
    let src = include_str!("../../../stdlib/j3/ui/dom/glyph.j3"); // the file `stdlib/files.rs` embeds
    let names_in = |func: &str| -> Vec<String> {
        let body = src.split(&format!("fn {func}(")).nth(1).expect(func).split("\n}\n").next().unwrap();
        let mut v: Vec<String> = body.split("same(a, \"").skip(1).map(|t| t.split('"').next().unwrap().to_string()).collect();
        v.sort();
        v.dedup();
        v
    };
    let mut want: Vec<String> = super::super::gen::GLYPH_ACTIONS.iter().map(|s| s.to_string()).collect();
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
fn noprimary_on_text_boxes() {
    let card = "<script>\n let s = \"\";\n</script>\n<div>\n  <textarea rich noprimary bind:value={s} />\n  <input type=\"search\" noprimary bind:value={s} />\n</div>";
    let g = build(files(&[("Card", card)]), "t", None).expect("builds");
    assert_eq!(g.text.matches("d.attrNoPrimary(").count(), 2, "in:\n{}", g.text);
    fails("<input type=\"checkbox\" noprimary />", "is for text boxes");
    fails("<textarea noprimary=\"1\" />", "takes no value");
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
    assert!(g.text.contains("= d.mouseAt();"));
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
