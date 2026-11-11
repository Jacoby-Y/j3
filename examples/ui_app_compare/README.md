# UI app compare

The same small todo app (add, check off, remove, "N left") with HTML/CSS-style UI, built five ways, measured for build time and shipped size.

| Folder | Stack | Language | How the UI is drawn |
|---|---|---|---|
| [`j3/`](j3) | J3 + `.j3ui` | J3 (AOT) | Own CSS layout and GL painter, no browser |
| [`electron/`](electron) | Electron 45 | JavaScript | Bundled Chromium |
| [`tauri/`](tauri) | Tauri 2.12 | Rust (AOT) + JS | System webview (WebKitGTK) |
| [`wails/`](wails) | Wails 2.11 | Go (AOT) + JS | System webview (WebKitGTK) |
| [`dioxus/`](dioxus) | Dioxus 0.7 native | Rust (AOT), RSX markup | Own HTML/CSS engine (Blitz: Stylo, Taffy, Vello), no browser |

Electron, Tauri and Wails share one frontend, [`web/`](web) (plain HTML, CSS and JS). J3 and Dioxus write the same markup and CSS in their own syntax.

## Results

| App | Clean build | Rebuild after a 1-line UI edit | Shipped size | .tar.gz |
|---|---:|---:|---:|---:|
| **J3** | **1.7 s** | 0.8 s | **1.8 MB** | **0.8 MB** |
| Electron | 1.6 s | 1.6 s | 273.2 MB | 118.1 MB |
| Tauri | 51.1 s | 25.1 s | 3.4 MB | 1.3 MB |
| Wails | 8.7 s | **0.6 s** | 8.6 MB | 3.6 MB |
| Dioxus native | 102.4 s | 65.7 s | 16.3 MB | 6.8 MB |

Linux x86-64, Intel Core Ultra 7 270K Plus (24 threads), release builds. Run `./bench.sh` to measure on your machine (`./bench.sh j3 wails` for some).

How to read it:

- **Clean build** starts with no build cache: no `target/`, a fresh Go cache, no sccache. Downloading dependencies (`npm install`, `cargo fetch`, `go mod download`) comes first and isn't timed.
- **Rebuild** changes the `Todos` heading and builds again.
- **Shipped size** is what you hand a user. J3's is the 1.1 MB binary plus the 0.7 MB font it loads at runtime. Electron's is the packaged app folder. The others are one binary each.
- Electron's "build" is really packaging: it copies the prebuilt Chromium runtime next to your JS, and nothing gets compiled.
- Not counted: the system libraries each app needs at runtime. J3 needs SDL2, SDL2_ttf and libGL. Tauri and Wails need WebKitGTK, about 120 MB of `libwebkit2gtk` and `libjavascriptcoregtk` on the machine. Dioxus native needs only libc, OpenSSL and a GPU driver.
- Every Rust build uses the same release profile (`opt-level = "s"`, LTO, strip, `panic = "abort"`). J3 uses `j3c build --release`, and Wails uses `-ldflags "-s -w"`.

## Running them

```sh
j3c run j3                                   # J3
cd electron && npm install && npm start      # Electron
cd tauri && cargo run --release              # Tauri (needs libwebkit2gtk-4.1-dev)
cd wails && mkdir -p frontend/dist && cp ../web/* frontend/dist/ && go run -tags desktop,production,webkit2_41 .
cd dioxus && cargo run --release             # Dioxus native
```

Wails: `bench.sh` calls the `go build` that `wails build` runs, because the Wails 2.11 CLI fails on Go 1.27 while generating JS bindings (`package "embed" without types`). This app has no bindings, so the output is the same.
