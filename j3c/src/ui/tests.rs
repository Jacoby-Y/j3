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

fn src(path: &str, name: &str, text: &str) -> SrcFile {
    SrcFile { path: path.into(), name: name.into(), text: text.into() }
}

const SHELL: &str = "<script>\n let n = 0;\n</script>\n<div>\n  <game.Bar hp={3} on:buy={n = 1} />\n  <game.hud.Cool hp={1} />\n</div>";
const BAR: &str = "<script>\n    props { hp: int; }\n    events { buy; }\n</script>\n<div>{hp}</div>\n";

const SLOT_CARD: &str = "<script>\n props { title: string; }\n</script>\n<section>{title}<slot name=\"actions\" /><slot /><div><slot name=\"footer\">Thanks</slot></div></section>";

fn fails_in(list: &[(&str, &str)], want: &str) {
    match build(files(list), "t", None) {
        Ok(_) => panic!("expected an error containing {want:?}"),
        Err(e) => assert!(e.contains(want), "wanted {want:?} in:\n{e}"),
    }
}

const CHIP: &str = "<script>\n props { label: string; }\n</script>\n<div class=\"chip\" style:height={20}>{label}</div>\n";

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

mod widgets;
mod components;
mod slots;
mod dynamic;
mod control;
mod styling;
