//! `<slot>` generation and errors.

use super::*;

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
