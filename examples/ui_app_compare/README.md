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

Linux x86-64, Intel Core Ultra 7 270K Plus (24 threads), release builds. Run `./bench.sh` to measure on your machine (`./bench.sh j3 wails` for some), then `./stress.py` (needs `xvfb`) and `./envsize.sh`.

How to read it:

- **Clean build** starts with no build cache: no `target/`, a fresh Go cache, no sccache. Downloading dependencies (`npm install`, `cargo fetch`, `go mod download`) comes first and isn't timed.
- **Rebuild** changes the `Todos` heading and builds again.
- **Shipped size** is what you hand a user. J3's is the 1.1 MB binary plus the 0.7 MB font it loads at runtime. Electron's is the packaged app folder. The others are one binary each.
- Electron's "build" is really packaging: it copies the prebuilt Chromium runtime next to your JS, and nothing gets compiled.
- Not counted: the system libraries each app needs at runtime. J3 needs SDL2, SDL2_ttf and libGL. Tauri and Wails need WebKitGTK, about 120 MB of `libwebkit2gtk` and `libjavascriptcoregtk` on the machine. Dioxus native needs only libc, OpenSSL and a GPU driver.
- Every Rust build uses the same release profile (`opt-level = "s"`, LTO, strip, `panic = "abort"`). J3 uses `j3c build --release`, and Wails uses `-ldflags "-s -w"`.

## Heavy UI: FPS and memory

`STRESS=1` switches every app to the same stress scene: 1,200 cards, all on screen, each changing its number, its bar width
and (staggered, 1 in 8 per frame) its class every frame, so every stack restyles, lays out and paints the whole scene each frame.
`./stress.py` runs each one in its own Xvfb (1280x800), lets it warm up for 2 s, then counts frames for 8 s (median of 3 runs).

| App | FPS | CPU per frame | Peak memory |
|---|---:|---:|---:|
| **J3** | 61.8 | 58.5 ms | 331 MB |
| Electron | **93.9** | **22.3 ms** | 453 MB |
| Tauri | 35.8 | 28.6 ms | **175 MB** |
| Wails | 35.6 | 29.0 ms | 182 MB |
| Dioxus native | n/a | n/a | n/a |

- **Everything here is software rendering.** Xvfb has no GPU, so GL and Vulkan run on the CPU (Mesa's llvmpipe and lavapipe). That's even for every stack, but it's not how anyone runs these apps.
- **CPU per frame** is user plus system time of every process the app started (Chromium's GPU and renderer processes, WebKit's web process), divided by frames. Multi-threaded rasterizing can push it above the frame time.
- **Peak memory** is the summed PSS of those processes, so shared libraries are split between them, not counted once per process. The Xvfb server isn't counted.
- **J3** is a GPU painter: under llvmpipe its own thread takes about 15 ms a frame and llvmpipe's threads take the rest of the CPU. On a real GPU (RTX 5060 Ti, offscreen SDL, no Xvfb) the same scene runs at **104 fps with 9.6 ms of CPU per frame and 146 MB RSS**. None of the other stacks could be measured without a window, so that number has nothing to line up against yet.
- **Electron** runs with `--disable-frame-rate-limit --disable-gpu-vsync`. Chromium's Skia rasterizes in software well, and it shows.
- **Tauri and Wails** are WebKitGTK and land in the same place, about 28 ms of CPU per frame. That is their limit here, not a vsync cap.
- **Dioxus native** can't run here. Vello rasterizes in GPU compute shaders, and on Xvfb's software Vulkan (no DRI3, so it can't present) and its GL fallback (`WGPU_BACKEND=gl`) it didn't finish one frame in 20 s. It needs a real GPU.
- **J3 found a bug.** This scene first leaked about 4.6 MB a frame in J3 (8.5 GB in 10 s): the style cache was cleared when full and computed styles were never freed, so every clear rebuilt all styles into new memory. Fixed in `j3c/stdlib/j3/ui/cssmatch/cascade.j3`, with a regression check in `css_test.j3`.

## Dev environment size

What a developer needs on disk to build each app: the compiler toolchain, downloaded dependencies, the build cache left by a release build, and the system libraries the build links against. `./envsize.sh` measures it (run `./bench.sh` first).

| Stack | Toolchain | Downloaded deps | Build cache | System libs | Total |
|---|---:|---:|---:|---:|---:|
| **J3 + `.j3ui`** | 238 MB | 0 MB | 8 MB | 4 MB | **251 MB** |
| Electron | 198 MB | 146 MB | 0 MB | 0 MB | 344 MB |
| Tauri (Rust) | 561 MB | 567 MB | 990 MB | 143 MB | 2262 MB |
| Wails (Go) | 233 MB | 110 MB | 124 MB | 143 MB | 610 MB |
| Dioxus native (Rust) | 561 MB | 1007 MB | 1544 MB | 0 MB | 3111 MB |

- **Toolchain.** J3 counts `j3c` (12 MB, with the stdlib, docs and editor inside) plus clang and LLVM. Rust counts a minimal rustup install (rustc, cargo, std for the host only), without the docs, rust-src or clippy the default profile adds. Go counts its whole `GOROOT`, and Electron counts Node and npm.
- **Downloaded deps.** J3 downloads nothing. Electron's is `node_modules` plus the Chromium zip it caches. Rust counts the crate sources and `.crate` files in `~/.cargo/registry`, which include crates for other platforms, since `cargo fetch` downloads every one in the lockfile. Go counts the module cache entries the app uses.
- **System libs** are the Debian packages the build links against: SDL2 and SDL2_ttf for J3, and the WebKitGTK and GTK dev packages for Tauri and Wails.
- **Not counted:** the system linker and libc headers (gcc, binutils, libc6-dev) that all four AOT stacks use.

## Running them

```sh
j3c run j3                                   # J3
cd electron && npm install && npm start      # Electron
cd tauri && cargo run --release              # Tauri (needs libwebkit2gtk-4.1-dev)
cd wails && mkdir -p frontend/dist && cp ../web/* frontend/dist/ && go run -tags desktop,production,webkit2_41 .
cd dioxus && cargo run --release             # Dioxus native
```

Wails: `bench.sh` calls the `go build` that `wails build` runs, because the Wails 2.11 CLI fails on Go 1.27 while generating JS bindings (`package "embed" without types`). This app has no bindings, so the output is the same.
