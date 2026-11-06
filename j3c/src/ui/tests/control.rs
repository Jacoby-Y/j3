//! `{#each}`, `{#match}`, `{#let}`, virtual lists, emitting fns.

use super::*;

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

/// J3 refuses a name declared twice in one fn, even in sibling blocks: each handler's `mouse`, `size`, `button` is its own local.
#[test]
fn handlers_on_one_update_each_get_their_own_locals() {
    let card = "<script>\n fn f(m: Vec2, b: int) {}\n</script>\n<div><canvas on:draw={f(size, 0)} on:mousedown={f(mouse, button)} on:mouseup={f(size, button)} on:mousemove={f(mouse, 0)} on:wheel={f(mouse, int(wheel))}></canvas><canvas on:mousedown={f(mouse, button)}></canvas></div>";
    let g = build(files(&[("Card", card)]), "t", None).unwrap_or_else(|e| panic!("{e}"));
    for want in ["let button_n2_mousedown = d.downBtn;", "let button_n2_mouseup = d.upBtn;", "this.f(mouse_n3_mousedown, button_n3_mousedown);"] {
        assert!(g.text.contains(want), "missing {want:?} in:\n{}", g.text);
    }
}

#[test]
fn grid_attribute_binds_the_grid_and_edit_events() {
    let pre = "<script>\n use j3.ui.gridstate.Grid;\n props { xs: []Group; g: &Grid; }\n</script>\n";
    let g = build(files(&[("Card", &format!("{pre}<div virtual row-height={{24}} grid={{g.sized(xs.len)}} on:edit={{ if kind == 0 {{ g.draft = g.initial(\"\"); }} }}>{{#each xs as r (r.id)}}<Row label=\"a\" />{{/each}}</div>")), ("Row", ROW)]), "t", None).unwrap_or_else(|e| panic!("{e}"));
    for want in ["d.gridBind(n1, g.sized(", "if d.gridEdit(n1) {", "let kind_n1_edit = d.gridOf(n1).evKind;", "let row_n1_edit = d.gridOf(n1).evRow;", "let col_n1_edit = d.gridOf(n1).evCol;", "let text_n1_edit = d.gridOf(n1).evText;"] {
        assert!(g.text.contains(want), "missing {want:?} in:\n{}", g.text);
    }
    fails(&format!("{pre}<div grid={{g}}></div>"), "goes on the `virtual` scroll box");
    fails(&format!("{pre}<div on:edit={{ g.row = 0; }}></div>"), "needs `grid=");
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
