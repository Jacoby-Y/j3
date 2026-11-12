# Plan 06: Web target as a platform package (no Emscripten)

**Status (2026-11-11): TODO** (rewritten). Decided: no emsdk. `j3c` keeps writing LLVM text and the
system clang makes wasm objects (verified); everything web-specific lives in a **platform package**
`j3web` that a game adds under `[dependencies]`: its own `j3.sys`/`j3.threadsys`/`j3.netsys`, a
freestanding C runtime (crt, libc subset, an "SDL2 + SDL2_ttf subset" over WebGL2/DOM/canvas), the JS
glue, and a link script written in J3. `j3c` gets one general hook (platform packages) and keeps the
web dev server. The stdlib gets no web files. First milestone: a triangle screenshot from headless
Chrome; target: `../j3-projects/io_breakout` playable in Chrome.

## Goal

```toml
# a game's j3.toml
[dependencies]
j3web = { git = "...", tag = "v0.1.0" }     # or a path while developing
[emit]
target = "wasm64-unknown-unknown"
```

`j3c build` writes `target/wasm64-unknown-unknown/<name>/` (`index.html`, `j3web.js`, `<name>.wasm`,
asset files); `j3c run` builds and opens it (`--headless --frames N --screenshot F` uses headless
Chrome, like native). The same `j3.game`/`gfx`/`ui`/`app` code runs unchanged: the package implements
the slice of SDL2 and SDL2_ttf those modules call, so the stdlib's game layer has one shape on every
platform.

## Current state (verified 2026-11-11)

- **Toolchain here**: system `clang` 18.1.3 has `wasm32`/`wasm64` targets. No `wasm-ld` (apt
  `lld-18` 1:18.1.3 is available and ships it: **the developer installs it**, `sudo apt install
  lld-18`; the agent must not). No emsdk, no binaryen, no wasi-libc. `google-chrome` 151, `firefox`
  156, `node` 26.5 present.
- **IR → wasm objects works with system clang** (verified twice): `examples/desk-app`,
  `3d-editor-scene`, `j2_script` and `io_breakout` (37 MB of IR, 8.9 MB object at `-O1`), via `j3c
  dump-llvm` of a scratch copy with the target set, then `clang --target=wasm64-unknown-emscripten
  -c -O1 -Wno-override-module`. `-unknown-unknown` was not tried yet (chunk 1 does).
- **Undefined symbols of io_breakout's object** (verified, `llvm-nm`), i.e. what the package must
  provide: libc `malloc calloc realloc free memcpy memmove memset strlen snprintf getenv exit _exit
  write open read close lseek mmap munmap chdir mkdir rename unlink clock_gettime __errno_location`;
  libm `sinf cosf tanf asinf acosf atanf atan2f expf floorf ceilf fmodf sqrtf log10f` (j2_script adds
  `sin cos tan sqrt atan2 strtod time utimensat`); 98 `SDL_*` functions (window, GL context, events,
  keyboard/mouse state, clipboard, cursors, display modes, renderer/texture calls of the non-GL path,
  `SDL_LoadObject`/`SDL_LoadFunction` for the add-ons). The stdlib declares 138 SDL/add-on functions
  in total (`game/sdlcore.j3`, `addonfns.j3`), 34 add-on symbols are looked up by name
  (`game/addons.j3`), and 129 GL entry points by name through `SDL_GL_GetProcAddress` (`glProc`/`load`).
- **What the emitter itself calls** (`emit/llvm.rs:79-87`): `_exit`, `write(i32, ptr, i64)`,
  `snprintf` with `%lld`, `%g`, `%s` (string interpolation), `puts`. `data/json` also calls
  `snprintf("%.*g")` and `strtod`.
- **Layout**: J3 is LP64 throughout (`ir/abi.rs`); wasm64 is LP64, wasm32 is not (old plan's chunk 12
  has the details). So **wasm64 first**. Memory64 validates in Chrome 151 and node 26 (verified);
  Safari unknown. In JS, every i64 (J3 `int`, every pointer under memory64) arrives as a `BigInt`:
  the glue converts with `Number(p)`; the J3/C side passes `i32`/`f32`/`f64` where it can.
- **Frame loop**: `App.run` blocks in a `while true` (`app.j3:521`); also `splash.finish`,
  `game.idleWait` (`SDL_WaitEventTimeout`), the FPS cap `SDL_Delay`, `game.delayMs`. All of them reach
  the browser through **SDL calls the package implements**: `SDL_GL_SwapWindow`, `SDL_Delay`,
  `SDL_WaitEventTimeout`. So the yield lives in the package, not in the stdlib.
- **GL**: `game/window.j3:62` asks for ES 3.0 only on Android or `J3_GLES=1`; `game.prepGl` turns
  `#version 330 core` into `300 es` only then. All 18 J3-shader programs checked compile in headless
  Chrome WebGL2 (verified for the old plan). `requireGlVersion` parses `GL_VERSION`: the package's
  `glGetString` returns `OpenGL ES 3.0 (WebGL 2.0)` so it passes.
- **Packages** (landed today): `[dependencies]` path/git, `Loader::dep_module` maps `use name.x` to
  the dep's files; deps are code only, cannot replace `j3.*` modules, their `[link]` libs are not
  added, no build scripts run (`src/doc/pages/packages.md`).
- **Already in the tree from the Emscripten attempt** (this morning, uncommitted): `TargetOs::Web`
  for triples containing `emscripten`, `OsKind::Web`, loader rows to `stdlib/j3/{sys,threadsys,
  netsys}_web.j3`, `PLATFORM_WEB = 4` in every `sys*.j3`, `datalayout_line` (x86-64 datalayout only
  for `x86_64*` triples), test `web_hello_uses_libc`, `src/web/{serve.rs,mod.rs,shell.html}`
  (`j3c -e web serve|run DIR`, dev server with `/__j3/env|log|file|exit`, headless Chrome runner;
  verified end to end with a stand-in runtime). Chunk 1 moves the three `*_web.j3` files into the
  package and keeps the rest.
- **Headless Chrome** (verified): `--headless=new --no-sandbox --enable-unsafe-swiftshader
  --use-angle=swiftshader` gives WebGL2 and runs rAF at ~57 fps.

## Design decisions

1. **Platform package, not a compiler backend written in J3.** A J3 wasm code generator (IR export
   format + structured codegen) is months and still needs this same runtime underneath; clang already
   produces good wasm from our LLVM text. A J3 emitter stays a possible later swap behind the same
   package.
2. **The package fakes SDL, the stdlib stays as it is.** Implementing ~100 SDL2 functions (many are
   no-ops or one-liners on a canvas) plus ~15 SDL_ttf ones in C/JS is far less than a second
   `j3.game` (292 pub fns, 13k lines) and keeps one game layer for every platform. Stdlib changes are
   limited to platform branches (`wantEs` on `PLATFORM_WEB`, saves path, `Pool` inline when there are
   no threads). Because we write the shim, ABI corners disappear: `TTF_RenderUTF8_Blended` takes the
   colour as `uint32_t` exactly as J3 passes it.
3. **What `j3c` learns: `[platform]` in a package's `j3.toml`.**
   ```toml
   [platform]
   targets = ["wasm64-unknown-unknown", "wasm32-unknown-unknown"]   # exact triples it serves
   modules = { "j3.sys" = "platform/sys.j3", "j3.threadsys" = "platform/threadsys.j3",
               "j3.netsys" = "platform/netsys.j3", "j3.instance" = "platform/instance.j3" }
   link = "tools/link.j3"     # J3 program j3c runs (host build) after the objects are compiled
   run = "tools/run.j3"       # optional: what `j3c run` does with the linked folder
   ```
   A program whose target matches a dependency's `[platform] targets` takes those modules from the
   package (`Loader::module_path` asks the platform first; only the four OS-layer modules may be
   replaced: a list in the loader, so a package cannot swap `j3.game` behind the user's back). A wasm
   triple with no platform package is an error that says to add one. At most one platform package per
   target. The link and run scripts are the only package code `j3c` executes; they run only when that
   target is built or run, never on fetch (say so in `packages.md`).
4. **Link script contract.** `j3c build` compiles the per-module objects as for any cross target, then
   runs the link script as `j3c run --file <pkg>/<link> --` with: `--objs <file listing object paths>`,
   `--out <dir>`, `--name <package name>`, `--root <project dir>`, `--pkg <platform package dir>`,
   `--libs SDL2,SDL2_ttf,...` (from `[link] libs` and `use lib` markers, as the native link sees
   them), `--include assets,scripts,...` (`dist::include`), `--release` when set, `--clang <path>`.
   Its exit code is the build's. The run script gets `--out <dir>` plus the user's run flags
   (`--frames`, `--screenshot`, `--headless`, `--secs`); `j3web`'s run script calls `j3c -e web run`.
   The host build of each script is cached like any `j3c run --file`.
5. **Runtime split inside the package**: `rt/*.c` freestanding C (`-ffreestanding -nostdlib`, compiled
   by the link script with the system clang) for crt, `malloc` on `__builtin_wasm_memory_grow`,
   `mem*`/`str*`, `snprintf` (`%lld %d %s %g %.*g` only), fd table, the SDL and TTF entry points and the
   name tables for `SDL_GL_GetProcAddress`/`SDL_LoadFunction`; `web/j3web.js` for everything the browser
   owns: WebGL2 calls, DOM events, `requestAnimationFrame`, canvas-2D text, file fetches, `%g`
   formatting and `strtod` (`Number.prototype.toPrecision` / `parseFloat`: exact and tiny), libm
   (`Math.*`). C calls JS through `__attribute__((import_module("env"), import_name(...)))` imports.
6. **Yield per frame with JSPI** (`WebAssembly.Suspending` on the `j3_web_frame` / `j3_web_sleep`
   imports, `WebAssembly.promising` on the exported entry). No binaryen, no loop inversion; the whole
   stdlib loop shape keeps working. Chrome ships JSPI (believed 137+, **unverified here**: chunk 3's
   first step). Fallback if Firefox matters and lacks it: `wasm-opt --asyncify` (apt `binaryen`) in the
   link script, same imports. Rule for docs: no sleeping inside a system called through a function
   pointer if the Asyncify fallback is in use.
7. **Files**: the link script copies each `--include` folder next to `index.html` and writes
   `files.txt`; at start `j3web.js` fetches every listed file into a `Map<path, Uint8Array>` before
   `main` (no `.data` packing until sizes ask for it). `open`/`read`/`lseek`/`close` are fd-table
   imports over that map; writes go to an in-memory map flushed to IndexedDB on `close` under
   `/save/` (chunk 9). `statAt`/`dirents` answer from the map.
8. **Threads**: `threadsys.create` fails, locks are no-ops, `cores()` 1; `j3.thread.Pool` with one core
   runs jobs inline (stdlib change, generic: also right for any single-core platform).
9. **Dev server stays in `j3c`** (`src/web/`, already built and tested): it is a generic static
   server + log/exit endpoints, used by the package's run script. The Emscripten `shell.html` goes; the
   page template moves to the package (`web/index.html`), keeping the `/__j3/*` client code.

## Chunks

Each chunk: `cd j3c && J3C_NO_INSTALL=1 cargo test` green before and after; never open a window; wrap
browser runs in `timeout`; new stdlib files go in `src/stdlib/files.rs`. The package lives at
`packages/j3web/` in this repo while it is young (moves to its own git repo later; projects point at it
by path). Chunks 3+ need `wasm-ld` (`which wasm-ld wasm-ld-18`): if missing, stop and ask the
developer to `sudo apt install lld-18`.

1. **Platform packages in `j3c` (no wasm yet).** Files: `src/project.rs` + `project/deps.rs` (parse
   `[platform]` into the `Dep`: `targets`, `modules`, `link`, `run`; errors: unknown module key, two
   platform deps for one target, a missing file), `src/compile/loader/paths.rs` (`module_path` takes a
   platform module first; allowed keys `j3.sys`, `j3.threadsys`, `j3.netsys`, `j3.instance`),
   `src/emit/host/target.rs` (`TargetOs::Web` for any triple starting `wasm32`/`wasm64`, not only
   `emscripten`), `src/compile.rs` (`OsKind::Web` with no platform package is an error naming the fix).
   Delete `stdlib/j3/{sys,threadsys,netsys}_web.j3` and their `files.rs`/`doc/mod.rs` rows; they become
   `packages/j3web/platform/*.j3` (sys rewritten in chunk 3). Test (`compile/tests/deps.rs`): a fixture
   platform package in `tests/programs/platform_pkg/` serving `wasm64-unknown-unknown` whose `sys.j3`
   has a marker pub const; a fixture game depending on it compiles for that triple and its IR has the
   marker; the same game for the host does not; no platform dep → the named error. Update
   `web_hello_uses_libc` to use the fixture and `wasm64-unknown-unknown`.
2. **Link and run hooks.** Files: `src/cli/build.rs` (platform target: compile objects into
   `target/.j3c/<triple>/<name>/obj` as today's per-module path does, write the objects list, run the
   link script per decision 4, report its exit code), `src/cli/runner.rs` (platform target: build,
   then the run script or, without one, print the out dir), `src/doc/pages/packages.md` (`[platform]`,
   the trust note). Test: the fixture package's `tools/link.j3` writes `out/linked.txt` with its args
   and object count; `j3c build` of the fixture game produces it; a link script exiting 3 fails the
   build with 3.
3. **`j3web` hello (needs `wasm-ld`).** Spike first, in scratch, each answer written into "Current
   state": a C object with a `WebAssembly.Suspending` import and a `promising` export linked by
   `wasm-ld --no-entry --export-dynamic` runs in headless Chrome 151 and node 26 and actually suspends;
   memory64 + `-mbulk-memory`; taking the address of an imported function (for the GL table) links;
   `wasm64-unknown-unknown` IR from `j3c` compiles. Then files: `packages/j3web/j3.toml` (`[lib]`,
   `[platform]`), `platform/sys.j3` (libc calls as in today's `sys_web.j3`, `start` calls `main`),
   `platform/{threadsys,netsys,instance}.j3`, `rt/crt.c` (exported `j3_main(argc, argv)` calling the
   host `main`), `rt/libc.c`, `rt/fd.c`, `web/j3web.js` (instantiate, imports, stdout/stderr lines,
   exit code via an exception), `web/index.html` (from `src/web/shell.html`, minus Emscripten),
   `tools/link.j3` (clang the `rt/*.c` once per opt into `target/.j3c/j3web/`, `wasm-ld` with
   `--export=j3_main --import-memory=no --stack-size=8388608 -z stack-size=8388608 --gc-sections`,
   `--strip-all` in release, copy web files and includes), `tools/run.j3` (`j3c -e web run`), a
   `node` runner `web/node.mjs` for plain programs. Done: `j3c new hello` + the dep + target →
   `j3c run --headless` prints `hello` through the dev server and exits 0; a panicking program exits
   101 and a failed index check 102; `node out/node.mjs` prints the same.
4. **Single-thread `j3.thread`.** `thread.j3`: `Pool.create` keeps no workers and `submit`/
   `parallelFor` run inline when `threadsys.cores() == 1` or `create` fails; `spawn` panics with "no
   threads on this platform". Done: a scratch `Pool.parallelForEach` + `Queue` program gives the same
   output natively (with `J3`'s normal pool) and under node.
5. **SDL core shim and the first frame (milestone).** Files: `rt/sdl.c` + `web/sdl.js`: init/quit,
   one window = the canvas (size, title → `document.title`, resizable follows the page), WebGL2 context
   (`antialias: false`, `preserveDrawingBuffer: true` so screenshots read it), `SDL_GL_GetProcAddress`
   over a generated table of the 129 GL names (`tools/gen_gltable.j3` greps the stdlib, or a checked-in
   list with a test that compares), GL calls forwarding to WebGL2 with object-id tables, `glGetString`,
   `SDL_GL_SwapWindow` = suspend until the next rAF, `SDL_Delay`/`SDL_WaitEventTimeout` = suspend,
   ticks and performance counter, event queue (keys with SDL scancodes, mouse, wheel, text input,
   window resize/focus, quit), keyboard/mouse state, display-mode queries answering with the canvas.
   Every other SDL entry the stdlib declares returns a harmless value; list which in a table in the
   package README. Stdlib: `game/window.j3` `wantEs` true on `PLATFORM_WEB`; skip the EGL hint there.
   Done: a `@shader` triangle app (`j3c new tri --app`) screenshot from `j3c run --headless --frames 30
   --screenshot tri.png` matches the native one with `j3c -e imgdiff --tol 2` (`J3_FIXED_DT=0.016`);
   wasm size and F3 frame time written here.
6. **Fonts: SDL_ttf over canvas 2D.** `rt/ttf.c` + `web/ttf.js`: `TTF_Init`, `OpenFont` (bytes from the
   file map → `FontFace`, loaded before `main` for every font file in the includes), `SizeUTF8`,
   `RenderUTF8_Blended` → a malloc'd `SDL_Surface` (layout as J3 reads it, LP64), metrics (`FontHeight`,
   `FontAscent`, `GlyphMetrics`), style/outline/hinting as no-ops where canvas cannot. `SDL_LoadObject`/
   `SDL_LoadFunction` answer for `SDL2_ttf` from a name table. Done: `examples/desk-app` renders in
   headless Chrome; text differs from FreeType, so it gets its own web golden and a tolerance written
   here; layout widths within 1 px of native for the desk-app header text (log both).
7. **io_breakout.** Whatever it still needs (grep its `use` list and the undefined symbols left after
   chunk 6). Done: `cd ../j3-projects/io_breakout`, add the dep and target in a scratch copy, `j3c run
   --headless --frames 120 --screenshot iob.png` shows the game; keyboard and mouse driven by a
   `J3_TEST`-style fake input or `j3.ui.testkit` if it has UI; developer plays it in desktop Chrome.
8. **Images and audio.** `IMG_Load` via `createImageBitmap` at preload time (decode every image in the
   includes before `main`, hand out surfaces); SDL_mixer subset over WebAudio (chunks, music, channels,
   volume), started on first user gesture; the shell shows "click to start" when audio is wanted.
   Done: a PNG loads; a sound plays under `--autoplay-policy=no-user-gesture-required` (check the
   AudioContext state from the page).
9. **Saves, ship, docs.** Saves: `data/save.j3` `userDir` → `/save/<game>` on web; writes under `/save`
   persist to IndexedDB (sync on close, load before `main`). Ship: `j3c build --release` + `[dist]
   build_outputs = ["web"]` zips the out folder with `index.html` at the root (itch.io's rule). Docs:
   `j3c doc web` page (setup: `lld-18`, the dep, the target, what works, what does not), root
   `AGENTS.md` row, README matrix "web: experimental", `roadmap.md` link. Done: a save survives two
   headless runs with one `--user-data-dir`; the zip, served by `j3c -e web serve`, runs.

Conditional:

10. **Asyncify fallback.** Trigger: a browser that matters lacks JSPI. Link script runs `wasm-opt
    --asyncify` when `[platform]`-level option `yield = "asyncify"` is set (`binaryen` from apt).
11. **wasm32.** Trigger: Safari/iOS. Pointer width from the target in `ir/abi.rs`, per-target libc
    declarations in the emitter, the package's C built for wasm32 (it is pointer-width clean by then).
12. **J3 code generator.** Trigger: clang becomes the bottleneck or a dependency we want gone. A J3
    program in the package reads an IR export and writes wasm; the platform runtime above stays.

## Testing without the hardware

Headless Chrome (SwiftShader WebGL2) through `j3c -e web run`, `node` for programs without a window,
`j3c -e imgdiff` against native or web goldens with `J3_FIXED_DT=0.016`. Gate every `cargo test` that
needs `wasm-ld` or Chrome on finding it (like `mingw_clang()`).

## Verify on real hardware (developer)

`sudo apt install lld-18`; desktop Chrome and Firefox on a real GPU: io_breakout, desk-app, input,
resize, high DPI, audio after a click; Safari when a Mac is around (memory64? then chunk 11); itch.io
draft upload of the zip.

## Out of scope

The editor and MCP in a browser, `j3.proc`/`term`/`steam`/`hotkey`/`instance`/drag-out, raw sockets
(HTTP over `fetch` is a later chunk if a game asks), hot reload, WebGPU, WASI.
