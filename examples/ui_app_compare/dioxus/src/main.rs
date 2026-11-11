use dioxus::prelude::*;
use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use std::time::Instant;

#[derive(Clone, PartialEq)]
struct Item {
    id: u32,
    text: String,
    done: bool,
}

fn main() {
    use dioxus::native::{Config, LogicalSize, WindowAttributes};
    let stress = std::env::var_os("STRESS").is_some();
    let (w, h) = if stress { (1280.0, 800.0) } else { (480.0, 520.0) };
    let attrs = WindowAttributes::default().with_title("Todos").with_inner_size(LogicalSize::new(w, h));
    #[allow(deprecated)] // the "no renderer" warning misses the `native` feature; it launches native
    dioxus::LaunchBuilder::new()
        .with_cfg(Config::new().with_window_attributes(attrs))
        .launch(if stress { Stress } else { App });
}

/// Cards in the stress scene (the same in every stack).
const N: u32 = 1200;

/// Resolves on its second poll: lets the renderer run between frames.
struct YieldNow(bool);
impl Future for YieldNow {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            return Poll::Ready(());
        }
        self.0 = true;
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

/// The stress scene: N cards, every one changes text, bar width and (staggered) class each frame.
/// Warm up 2 s, count frames for 8 s, print `mark` and `done frames=N` (../stress.py reads them).
#[component]
fn Stress() -> Element {
    let mut frame = use_signal(|| 0u32);
    let rendered = use_hook(|| Rc::new(Cell::new(0u32)));
    let clock = use_hook(|| Rc::new(Cell::new((Instant::now(), 0u32, 0u8)))); // start, marked frame, phase
    let seen = rendered.clone();
    use_future(move || {
        let seen = seen.clone();
        async move {
            loop {
                YieldNow(false).await;
                // the next frame starts only after the last one rendered
                if seen.get() == frame() {
                    frame += 1;
                }
            }
        }
    });

    let f = frame();
    rendered.set(f);
    let (t0, marked, phase) = clock.get();
    let t = t0.elapsed().as_millis();
    if phase == 0 && t >= 2000 {
        clock.set((t0, f, 1));
        println!("mark");
    } else if phase == 1 && t >= 10000 {
        clock.set((t0, marked, 2));
        println!("done frames={}", f - marked); // keeps running: ../stress.py reads its CPU time, then stops it
    }

    rsx! {
        style { {include_str!("../assets/stress.css")} }
        div { class: "grid",
            for i in 0..N {
                div { key: "{i}", class: "card c{((i + f) / 8) % 4}",
                    span { "{(i * 7 + f) % 1000}" }
                    div { class: "bar", div { class: "fill", style: "width: {(i + f) % 100}%" } }
                }
            }
        }
    }
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
