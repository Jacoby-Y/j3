use super::*;

fn module(query: &str, item: Option<&str>) -> Result<String, String> {
    lookup(query, item, false)
}

#[test]
fn script_doc_lists_props_shared_state_and_calls() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../speculative/11_script/scripts/door.j3s");
    let text = script_doc(&path.display().to_string()).expect("script doc");
    assert!(text.contains("hinge: Node (required)"), "{text}");
    assert!(text.contains("openAngle: f32 = 90.0"), "{text}");
    assert!(text.contains("shared: one value for every instance"), "{text}");
    assert!(text.contains("doorsOpened: int = 0"), "{text}");
    assert!(text.contains("state: each instance's own; private"), "{text}");
    assert!(text.contains("open = false"), "{text}");
    assert!(text.contains("fn toggle()"), "{text}");
    assert!(!text.contains("fn smooth("), "{text}");
}

#[test]
fn index_lists_tasks_modules_and_provenance() {
    let text = index().expect("index");
    assert!(text.contains("stdlib embedded, docs "), "{text}");
    assert!(text.contains("recipes scene3d"), "{text}");
    assert!(text.contains("j3c doc search <word>"), "{text}");
    assert!(text.contains("    j3.app "), "{text}");
    assert!(text.contains("j3c doc lang"), "{text}");
    assert!(text.contains("Bevy-shaped runner"), "{text}");
    assert!(text.contains("    j3.gfx.mesh "), "{text}");
    assert!(text.contains("internal (not for games): "), "{text}");
    assert!(text.contains(" j3.scene3d.editor.paste "), "{text}");
    assert!(!text.contains("Main thread only"), "{text}");
    assert!(text.len() < 17_400, "index grew to {} bytes", text.len());
}

#[test]
fn guide_pages_split_into_sections_that_cover_them() {
    for names in GUIDES {
        let text = guide(names[0]).expect("page");
        let all = sections(&text);
        assert!(all.iter().all(|(n, r)| !n.is_empty() && !r.is_empty()), "{}", names[0]);
        let joined: String = all.iter().map(|(_, r)| &text[r.clone()]).collect();
        assert_eq!(joined, text, "{} lost text", names[0]);
    }
    assert!(sections(CSS_PAGE).len() >= 10);
    let layout = guide_section("css", "layout").expect("section");
    assert!(layout.starts_with("Layout") && layout.len() < CSS_PAGE.len() / 4, "{layout}");
    assert!(guide_section("css", "nosuch").is_err());
}

#[test]
fn app_create_is_documented_with_its_source() {
    let text = module("j3.app", Some("create")).expect("doc");
    assert!(text.contains("fn App<W>.create("), "{text}");
    assert!(text.contains("Boots"), "{text}");
    assert!(text.contains("at j3/app.j3:"), "{text}");
    assert!(text.contains("(embedded in j3c)"), "{text}");
    assert!(!text.contains("fn App<W>.run("), "{text}");
    assert_eq!(module("j3.app", Some("App.create")).expect("qualified"), text);
    assert_eq!(module("j3.app.App.create", None).expect("dotted"), text);
}

#[test]
fn ambiguous_names_list_qualified_choices() {
    let text = module("j3.math", Some("length")).expect("choices");
    assert!(text.contains("names more than one item"), "{text}");
    assert!(text.contains("j3c doc j3.math Vec3.length"), "{text}");
    assert!(text.contains("j3c doc j3.math Quat.length"), "{text}");
    let one = module("j3.math", Some("Vec3.length")).expect("one");
    assert!(one.contains("fn Vec3.length("), "{one}");
    assert!(!one.contains("Quat"), "{one}");
}

#[test]
fn a_bare_name_finds_its_module() {
    let text = module("textureFromCanvas", None).expect("found");
    assert!(text.contains("module j3.game"), "{text}");
    assert!(text.contains("fn textureFromCanvas("), "{text}");
    assert!(module("noSuchNameAnywhere", None).is_err());
}

#[test]
fn defaults_and_constant_values_show() {
    let text = module("j3.math", Some("PI")).expect("PI");
    assert!(text.contains("const PI: f32 = 3.14159"), "{text}");
}

#[test]
fn short_view_fits_a_screen() {
    let text = lookup("j3.game", None, true).expect("short");
    assert!(text.contains("fn: "), "{text}");
    assert!(text.contains("textureFromCanvas"), "{text}");
    assert!(text.contains("signatures and comments: `j3c doc j3.game`"), "{text}");
    assert!(text.lines().count() < 80, "{} lines\n{text}", text.lines().count());
}

#[test]
fn search_finds_names_then_comments() {
    let text = search("canvas").expect("search");
    assert!(text.contains("j3c doc j3.game textureFromCanvas"), "{text}");
    assert!(search("zzqqxx").is_err());
}

#[test]
fn scripts_module_comes_from_the_project_scenes() {
    let dir = std::env::temp_dir().join(format!("j3-doc-scripts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (path, text) in crate::editor::project_files("demo", false) {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let m = scripts_at(&dir).expect("module");
    let text = render_module(&m, None).expect("render");
    assert!(text.contains("module j3.scene3d.scripts"), "{text}");
    assert!(text.contains("fn start(scene: &Scene)"), "{text}");
    assert!(text.contains("fn update(scene: &Scene, dt: f32)"), "{text}");
    assert!(text.contains("scripts/spin.j3s\n        scenes/main.j3scene:"), "{text}");
    assert!(!text.contains("find__"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn testing_and_css_pages_are_found() {
    assert!(TESTING_PAGE.contains("--headless") && TESTING_PAGE.contains("imgdiff"));
    assert!(J3UI_PAGE.contains("j3c doc css") && J3UI_PAGE.contains("j3c doc testing"));
    for w in ["imgdiff", "golden", "headless", "bind"] {
        assert!(search(w).expect(w).contains("j3c doc "), "{w}");
    }
    assert!(search("imgdiff").unwrap().contains("j3c doc testing"));
    assert!(index().unwrap().contains("j3c doc testing"));
    assert!(search("deb").unwrap().contains("j3c doc ship"));
}

#[test]
fn j3ui_pages_link_each_other() {
    assert!(J3UI_PAGE.contains("j3c doc j3ui-immediate"));
    assert!(J3UI_PAGE.contains("recipes components"));
    assert!(j3ui_page().contains("j3c doc j3ui`"));
}

#[test]
fn j3ui_page_lists_every_element() {
    for name in crate::ui::gen::ELEMENTS {
        assert!(J3UI_PAGE.contains(&format!(" {name}")), "`j3c doc j3ui` is missing element {name}");
    }
}

#[test]
fn j3ui_page_lists_the_contract() {
    let text = j3ui_page();
    assert!(text.contains("@j3ui shopRow"), "{text}");
    assert!(text.contains("cannot see locals"), "{text}");
    for name in J3UI_END.iter().chain(J3UI_POP).chain(J3UI_IF).chain(J3UI_CALL) {
        assert!(
            crate::parse::syntax::is_j3ui_widget(name),
            "{name} is not a widget"
        );
        assert!(text.contains(&format!("`{name}`")), "page missing {name}\n{text}");
    }
}

#[test]
fn lang_page_lists_shipped_forms() {
    let text = lang_page();
    assert!(text.contains("0..n"), "{text}");
    assert!(text.contains("0..=n"), "{text}");
    assert!(text.contains("TryCatch"), "{text}");
    assert!(text.contains("`j3ui`"), "{text}");
    assert!(text.contains("j3c doc j3.app"), "{text}");
    assert!(text.contains("`match`"), "{text}");
    assert!(text.contains("@script"), "{text}");
    assert!(text.contains("props {"), "{text}");
    assert!(text.contains("f32(n)"), "{text}");
    assert!(text.contains("swapRemove") && text.contains("inside the struct body"), "{text}");
}

#[test]
fn missing_item_errors() {
    let err = module("j3.app", Some("nope")).unwrap_err();
    assert!(err.contains("nope"), "{err}");
    let err = module("j3.app", Some("App.nope")).unwrap_err();
    assert!(err.contains("App.nope"), "{err}");
}

#[test]
fn threads_guide_is_reachable_and_complete() {
    let page = THREADS_PAGE;
    for needle in ["Shared data that keeps changing", "Program globals", "visible to that job", "HAND WORK OVER", "trySubmit", "main-thread only", "Common mistakes", "emitFaceAo", "@atomicCas"] {
        assert!(page.contains(needle), "{needle}");
    }
    let idx = index().unwrap();
    assert!(idx.contains("j3c doc threads"), "{idx}");
    assert!(idx.contains("j3.thread\n"), "j3.thread is listed");
}

/// Every recipe's files make a program that `j3c check` accepts.
#[test]
fn recipes_check() {
    let mut failed: Vec<String> = Vec::new();
    for r in crate::helptext::recipes() {
        if r.files.is_empty() {
            continue;
        }
        let dir = std::env::temp_dir().join(format!("j3-recipe-{}-{}", r.name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (path, text) in &r.files {
            let path = dir.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let targets: Vec<PathBuf> = if dir.join("j3.toml").is_file() {
            vec![dir.clone()]
        } else {
            r.files.iter().filter(|(p, _)| p.ends_with(".j3")).map(|(p, _)| dir.join(p)).collect()
        };
        for t in targets {
            let code = crate::cli::run(["check".into(), t.display().to_string()]);
            if code != 0 {
                failed.push(r.name.to_string());
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
    assert!(failed.is_empty(), "recipes that do not check: {:?}", failed);
}
