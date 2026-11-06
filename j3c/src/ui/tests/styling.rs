//! Dynamic class and style, drag and drop, animate, transition, render hooks.

use super::*;

#[test]
fn speculative_12_generates_and_checks() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../speculative/12_ui_runtime");
    let m = crate::project::Manifest::load(&dir.join("j3.toml")).unwrap();
    crate::compile::compile_manifest(&m).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn long_text_holes_have_no_buffer_limit() {
    // a lone hole stays `{x}` (a lone string hole lowers to the string itself); text around holes,
    // with backticks and newlines too, goes through a scratch `Text` with one template per hole
    let page = "<script>\n props { text: string; n: int; }\n</script>\n<p title=\"of `{text}`\">{text}</p>\n<p>a\nb {n} after {text}</p>";
    let g = build(files(&[("Page", page)]), "t", None).unwrap_or_else(|e| panic!("{e}"));
    for want in ["d.text(`{text}`)", "Text.scratch().add(\"of `\").add(`{text}`).add(\"`\").str()", "Text.scratch().add(\"a b \").add(`{n}`).add(\" after \").add(`{text}`).str()", "use j3.text.Text;"] {
        assert!(g.text.contains(want), "missing {want} in:\n{}", g.text);
    }
}

#[test]
fn dynamic_class_and_style_generate() {
    let page = "<script>\n props { r: int; extra: string; pct: f32; }\n</script>\n<div class=\"slot r{r} {extra} on\" style:width={pct * 100.0} style:background=\"{extra}\" style:padding=\"2px\" style:color={\"red\"}>x</div>";
    let g = build(files(&[("Page", page)]), "t", None).unwrap_or_else(|e| panic!("{e}"));
    for want in ["d.clsId(n", "d.clsDyn(n", "Text.scratch().add(\"r\").add(`{r}`).add(\" \").add(`{extra}`).str()", "d.styleNumAt(n", "0, \"width\", f32(", "d.styleStrAt(n", "1, \"background\"", "2, \"padding\"", "3, \"color\""] {
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
        "d.dragOver(", "d.dropAt(", "d.attrDragTarget(", ", 8);", "_dragstart = d.dragData();", "_drop = d.dragIdOf();", "_dragend = d.dragDropped();",
        "let accept_n2_dragover = true;", "if !accept_n2_dragover { d.dragReject(",
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
