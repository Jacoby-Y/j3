//! Tests for `.j3ui` hover, definition, diagnostics, and completion.

use super::*;

/// A small `.j3ui` project (`tests/fixtures/ui_lsp`); some tests assert its line numbers.
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui_lsp").canonicalize().unwrap()
}

fn file(name: &str) -> (PathBuf, String) {
    let p = fixture().join("ui").join(name);
    let t = std::fs::read_to_string(&p).unwrap();
    (p, t)
}

fn labels(v: &[Completion]) -> Vec<&str> {
    v.iter().map(|c| c.label.as_str()).collect()
}

#[test]
fn span_map_round_trips_copied_text() {
    let (p, _) = file("BallCard.j3ui");
    let ui = analyze(&p, &[]).unwrap();
    let g = ui.gen.unwrap();
    let mut n = 0;
    for s in g.map.segs.iter().filter(|s| s.copy) {
        for at in [s.lo, (s.lo + s.hi) / 2, s.hi - 1] {
            let src = g.map.to_src(at, at + 1).unwrap();
            // A source range can be emitted twice (a condition used in two places), so the
            // way back may land on the other copy: same source byte, same text.
            let back = g.map.to_gen(src.file, src.lo).unwrap();
            assert_eq!(g.map.to_src(back, back + 1).unwrap(), src);
            assert_eq!(g.text[back..back + 1], g.text[at..at + 1]);
            n += 1;
        }
    }
    assert!(n > 20);
}

#[test]
fn int_key_hint_is_not_a_diagnostic() {
    let (p, t) = file("Upgrades.j3ui");
    for key in ["(k : int)", "(k:int)", "( k : int )"] {
        let ok = t.replace("{#each 0..KINDS as k}", &format!("{{#each 0..KINDS as k {key}}}"));
        assert_ne!(ok, t);
        let ui = analyze(&p, &[(p.clone(), ok)]).unwrap();
        assert!(ui.err.is_none(), "{key}: {}", ui.err.map(|e| e.message).unwrap_or_default());
    }
}

#[test]
fn diagnostic_lands_on_the_j3ui_line() {
    let (p, t) = file("BallCard.j3ui");
    let bad = t.replace("sim.buyBall(kind)", "sim.buyBal(kind)");
    let ui = analyze(&p, &[(p.clone(), bad.clone())]).unwrap();
    let e = ui.err.expect("error");
    assert_eq!(e.path, "ui/BallCard.j3ui");
    let ((l, c), _) = query::range_of(&bad, e.span);
    assert_eq!((l + 1, c + 1), (29, 48), "{}", e.message);
}

#[test]
fn markup_error_is_structured() {
    let (p, t) = file("BallCard.j3ui");
    let bad = t.replace("<small>", "<blink>");
    let ui = analyze(&p, &[(p.clone(), bad.clone())]).unwrap();
    let e = ui.err.expect("error");
    assert_eq!(e.path, "ui/BallCard.j3ui");
    let ((l, _), _) = query::range_of(&bad, e.span);
    assert_eq!(bad.lines().nth(l as usize).unwrap().trim_start().get(..6), Some("<blink"));
}

#[test]
fn hover_in_a_hole_gives_the_field_type() {
    let (p, t) = file("Upgrades.j3ui");
    let off = t.find("gold={sim.gold}").unwrap() + "gold={sim.g".len();
    let ui = analyze(&p, &[]).unwrap();
    let h = hover(&ui, &p, &t, off, &[]);
    let v = h.get("contents").and_then(|c| c.get("value")).and_then(Json::as_str).unwrap().to_string();
    assert!(v.contains("gold") && v.contains("int"), "{v}");
}

#[test]
fn definition_in_a_hole_lands_in_the_j3ui() {
    let (p, t) = file("BallCard.j3ui");
    let off = t.find("{shop().name}").unwrap() + "{sh".len();
    let ui = analyze(&p, &[]).unwrap();
    let d = definition(&ui, &p, &t, off, &[]);
    let uri = d.get("uri").and_then(Json::as_str).unwrap().to_string();
    assert!(uri.ends_with("BallCard.j3ui"), "{uri}");
    let line = d.get("range").and_then(|r| r.get("start")).and_then(|s| s.get("line")).and_then(Json::as_i64).unwrap();
    assert!(t.lines().nth(line as usize).unwrap().contains("fn shop"), "line {line}");
}

#[test]
fn member_completion_after_sim_dot() {
    let (p, t) = file("BallCard.j3ui");
    let at = t.find("<small>").unwrap();
    let ins = "<span>{sim.}</span>\n";
    let buf = format!("{}{ins}{}", &t[..at], &t[at..]);
    let off = at + "<span>{sim.".len();
    let items = complete_at(&p, &[(p.clone(), buf.clone())], &buf, off);
    let l = labels(&items);
    assert!(l.contains(&"gold") && l.contains(&"ballCount"), "{l:?}");
}

#[test]
fn tag_name_and_prop_completion() {
    let (p, t) = file("Game.j3ui");
    let buf = format!("{t}\n<Ba");
    let items = complete_at(&p, &[(p.clone(), buf.clone())], &buf, buf.len());
    let l = labels(&items);
    assert!(l.contains(&"BallCard") && l.contains(&"BallShop") && !l.contains(&"div"), "{l:?}");
    let buf = format!("{t}\n<UpgradeRow ");
    let items = complete_at(&p, &[(p.clone(), buf.clone())], &buf, buf.len());
    let l = labels(&items);
    for want in ["label", "value", "cost", "gold", "on:buy"] {
        assert!(l.contains(&want), "{want} in {l:?}");
    }
    // BallCard has a non-ASCII `·` before the cursor: offsets are bytes everywhere.
    let (p2, t2) = file("BallCard.j3ui");
    let buf = format!("{t2}\n<UpgradeRow ");
    let items = complete_at(&p2, &[(p2.clone(), buf.clone())], &buf, buf.len());
    assert!(labels(&items).contains(&"on:buy"));
    let buf = format!("{t}\n<button ");
    let l: Vec<String> = complete_at(&p, &[(p.clone(), buf.clone())], &buf, buf.len()).into_iter().map(|c| c.label).collect();
    assert!(l.contains(&"on:click".to_string()) && l.contains(&"disabled".to_string()), "{l:?}");
}

fn big(name: &str) -> (PathBuf, String) {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/big_ui_test/ui").join(name);
    let t = std::fs::read_to_string(&p).unwrap();
    (p, t)
}

#[test]
fn nested_components_complete_and_jump() {
    let (p, t) = big("game/Hud.j3ui");
    let off = t.find("<game.Bar").unwrap() + "<game.B".len();
    let ui = analyze(&p, &[]).unwrap();
    assert!(ui.err.is_none() && ui.prog.is_some());
    let d = definition(&ui, &p, &t, off, &[]);
    let uri = d.get("uri").and_then(Json::as_str).unwrap_or("").to_string();
    assert!(uri.ends_with("ui/game/Bar.j3ui"), "{uri}");
    let buf = format!("{t}\n<game.Ba");
    let items = complete_at(&p, &[(p.clone(), buf.clone())], &buf, buf.len());
    let bar = items.iter().find(|c| c.label == "game.Bar").expect("game.Bar offered");
    assert_eq!(bar.insert, "Bar");
    assert!(labels(&items).contains(&"game.Cooldown") == false);
    let buf = format!("{t}\n<game.Bar ");
    let l: Vec<String> = complete_at(&p, &[(p.clone(), buf.clone())], &buf, buf.len()).into_iter().map(|c| c.label).collect();
    assert!(l.contains(&"value".to_string()) && l.contains(&"max".to_string()), "{l:?}");
    // a root file offers the root and nested names alike
    let (sp, st) = big("Shell.j3ui");
    for (pre, want) in [("<pa", "pages.Text"), ("<po", "post.Glass")] {
        let buf = format!("{st}\n{pre}");
        let l: Vec<String> = complete_at(&sp, &[(sp.clone(), buf.clone())], &buf, buf.len()).into_iter().map(|c| c.label).collect();
        assert!(l.contains(&want.to_string()), "{l:?}");
    }
}

#[test]
fn css_completion_from_the_tables() {
    let (p, t) = file("BallCard.j3ui");
    let at = t.find(".card:hover").unwrap();
    let with = |ins: &str| format!("{}{ins}\n{}", &t[..at], &t[at..]);
    let buf = with(".x { flex-dir");
    let items = complete_at(&p, &[(p.clone(), buf.clone())], &buf, at + ".x { flex-dir".len());
    assert_eq!(labels(&items), ["flex-direction"]);
    let buf = with(".x { justify-content: space-b");
    let items = complete_at(&p, &[(p.clone(), buf.clone())], &buf, at + ".x { justify-content: space-b".len());
    assert_eq!(labels(&items), ["space-between"]);
    let buf = with(".x { color: var(--ac");
    let items = complete_at(&p, &[(p.clone(), buf.clone())], &buf, at + ".x { color: var(--ac".len());
    assert!(labels(&items).contains(&"--accent"), "{:?}", labels(&items));
}

#[test]
fn j3ui_feature_completion() {
    let (p, t) = file("BallCard.j3ui");
    let run = |buf: String, off: usize| -> Vec<String> { complete_at(&p, &[(p.clone(), buf.clone())], &buf, off).into_iter().map(|c| c.label).collect() };
    let b = format!("{t}\n<div ");
    let l = run(b.clone(), b.len());
    assert!(l.contains(&"slot".to_string()) && l.contains(&"style:name".to_string()), "{l:?}");
    let b = format!("{t}\n<sl");
    assert!(run(b.clone(), b.len()).contains(&"slot".to_string()));
    let b = format!("{t}\n<comp");
    assert!(run(b.clone(), b.len()).contains(&"component".to_string()));
    let b = format!("{t}\n<component ");
    let l = run(b.clone(), b.len());
    assert!(l.contains(&"this".to_string()) && l.contains(&"of".to_string()), "{l:?}");
    let b = format!("{t}\n<div>{{#ma");
    assert_eq!(run(b.clone(), b.len())[..1], ["match".to_string()]);
    let b = format!("{t}\n<div>{{:ca");
    assert_eq!(run(b.clone(), b.len())[..1], ["case".to_string()]);
    let at = t.find("<script>").unwrap() + "<script>\n".len();
    let b = format!("{}onF\n{}", &t[..at], &t[at..]);
    assert!(run(b.clone(), at + 3).contains(&"onFrame".to_string()));
    let at = t.find(".card:hover").unwrap();
    let with = |ins: &str| format!("{}{ins}\n{}", &t[..at], &t[at..]);
    let b = with(".x { width: cl");
    assert!(run(b.clone(), at + ".x { width: cl".len()).contains(&"clamp(".to_string()));
    let b = with(".x { width: 50vm");
    assert_eq!(run(b.clone(), at + ".x { width: 50vm".len()), ["50vmin", "50vmax"]);
    let b = with(".x { flex-wr");
    assert_eq!(run(b.clone(), at + ".x { flex-wr".len()), ["flex-wrap"]);
    let b = with(".x { grid-auto-fl");
    assert_eq!(run(b.clone(), at + ".x { grid-auto-fl".len()), ["grid-auto-flow"]);
    let b = with(".x { user-select: n");
    assert_eq!(run(b.clone(), at + ".x { user-select: n".len()), ["none"]);
}
