use dioxus::prelude::*;

#[derive(Clone, PartialEq)]
struct Item {
    id: u32,
    text: String,
    done: bool,
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut next_id = use_signal(|| 3u32);
    let mut draft = use_signal(String::new);
    let mut items = use_signal(|| {
        vec![
            Item { id: 1, text: "Write the app".into(), done: false },
            Item { id: 2, text: "Measure the build".into(), done: false },
        ]
    });
    let mut add = move || {
        if !draft.read().is_empty() {
            let id = next_id();
            items.write().push(Item { id, text: draft(), done: false });
            next_id += 1;
            draft.set(String::new());
        }
    };
    let left = items.read().iter().filter(|it| !it.done).count();

    rsx! {
        style { {include_str!("../assets/style.css")} }
        div { class: "app",
            h1 { "Todos" }
            form { class: "add", onsubmit: move |e| { e.prevent_default(); add(); },
                input { r#type: "text", placeholder: "What needs doing?", value: "{draft}",
                    oninput: move |e| draft.set(e.value()) }
                button { "Add" }
            }
            div { id: "list",
                for (i, it) in items.read().iter().enumerate() {
                    div { key: "{it.id}", class: if it.done { "row done" } else { "row" },
                        input { r#type: "checkbox", checked: it.done,
                            onchange: move |_| { let d = items.read()[i].done; items.write()[i].done = !d; } }
                        span { class: "text", "{it.text}" }
                        button { onclick: move |_| { items.write().remove(i); }, "×" }
                    }
                }
            }
            p { class: "foot", "{left} left" }
        }
    }
}
