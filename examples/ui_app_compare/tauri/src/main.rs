use tauri::{WebviewUrl, WebviewWindowBuilder};

/// Stress runs only: the page sends its `mark` / `done` lines here. ../stress.py reads stdout and stops the app.
#[tauri::command]
fn report(line: String) {
    println!("{line}");
}

fn main() {
    let stress = std::env::var_os("STRESS").is_some();
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![report])
        .setup(move |app| {
            let (page, w, h) = if stress { ("stress.html", 1280.0, 800.0) } else { ("index.html", 480.0, 520.0) };
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App(page.into()))
                .title("Todos")
                .inner_size(w, h)
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
