# J3 roadmap

Non-goals: no GC, borrow checker, trait objects (`dyn`), drop, escaping closures, or comptime interpreter in v1. Do not CFG-ify check, start a second parser, grow `j3c/src/stdlib/mod.rs`, or add libLLVM / crates.io.

`[emit] target` is one triple or an array (`sys.j3` / `sys_windows.j3` / `sys_macos.j3`). `build` writes one binary per triple; `run` uses the host triple.

## Shipped

- **Language:** typed IR, LLVM (default) and C printers, `--fast`, `--release`. Short-circuit, if-expr arms, `defer`, `+=`, interpolation, wrapping ints, inclusive `..=`, default fields, `@j3ui`, scripts (`.j3s`, bytecode VM plus AOT). Small gaps (labels, `/=` `%=`, int `match`, block-scoped `let`, `0b` and bitwise ops), UFCS, closures (`closure(..)`, see `j3c doc lang`), generic fns, traits (`#[impl]`, `j3.cmp`, `j3.data.map`), atomics on more types with memory orders, `f16` storage, SIMD `vec4f`/`vec4i` (LLVM and C), the shader language (`#[shader]`, `@shader`, `j3c doc shaders`), and path deps (`[deps]`).
- **Tooling:** `j3c lsp` (hover, goto, completion, signature help, outline, import quick-fix, format, cross-buffer diagnostics), `j3c fmt`, `j3c doc`, `j3c size`, the scene editor and MCP server.
- **Runtime checks:** every check names itself as `file:line: reason` and exits 102; `panic` exits 101. Debug builds on x86-64 Linux install a crash handler, symbolized by `j3c run`.
- **Threads (Linux):** `j3.thread` (pool, queue, mutex), `#[thread]` globals, `@atomic*`. See Multithreading.
- **Network and processes (Linux):** `j3.net.tcp`, `j3.net.dns` (`getaddrinfo`), `j3.net.tls` (system OpenSSL 3 through `dlopen`, always verified), `j3.net.http` (HTTP/1.1 client, chunked, redirects, streaming lines for SSE), `j3.data.json`, `j3.proc` (spawn with pipes, PATH lookup) on fork/exec/pipe in `j3.sys`. `j3c doc recipes http`, `examples/openrouter-chat`.
- **UI components:** `.j3ui` (markup, J3 script, scoped CSS) compiled by `j3c` onto `j3.ui.css`/`dom` and the batched `j3.gfx.paint`; `examples/desk-app` and `examples/big_ui_test` are the references (plan 022; editor support is stage 08); plan 023 added `{#let}`, data grid, rich textarea, caret popups, flex/selectors/inline boxes, `margin: auto`, `aspect-ratio`, `outline`, `cursor`, `text-decoration`, `font-style`, `:not()` / `:nth-child()` / `:last-child`, `j3c golden`, `J3_STEADY` and `j3.ui.testkit`; plan 026 added grid, transitions and `animate:flip`, drag-and-drop, more selectors and elements, transforms, `@property`, i18n, the F12 inspector, `run --hot`, CSS lint, gamepad focus and the a11y gaps; plan 027 added rotated overflow clips, RTL number neutrals, spacing transitions, `em` in transforms, `url()` custom properties, scroll boxes that are not stacking contexts, positive `tabindex`, radio groups and `<glyph>` input prompts; the screen reader bridge (plan 024) is left).
- **Stdlib:** `App` stages, ECS, collide, space, physics (light: 2D AABB and a 3D voxel-stopping body, no contact solver), path, anim, input, assets, save, particles, camera rig, UI, `j3.gfx` (GL 3.3, one shadow map, 4 point lights, instancing), voxel grid, math, mesh formats, SDL audio with groups, fades, and positional channels, `scene3d`.

## Left

- **Scale:** one translation unit per `cc`, and mono re-walks the AST. Measure check vs emit vs clang before touching it.
- **Package deps.** Path and git deps, `j3.lock`, `[lib]` packages, `j3c new --lib` / `add` / `fetch` / `update` shipped (`j3c doc packages`); still open: version ranges over tags, a registry (index repo) and `publish`, a dep's assets and `[link]` libs, hot reload watching dep files, compile caching per dep commit.
- **Ship a Windows binary:** mingw `clang` and SDL import libs. A missing lib stays a link error.
- **Pack / install:** rpath and asset roots matter once there is a build to hand someone.
- **Network follow-ups:** macOS and Windows for `j3.net.tcp`/`dns`/`tls` and `j3.proc` (stubs today), keep-alive and pooling, gzip, a non-blocking poll-style HTTP request for the frame thread, an HTTP server, WebSockets, `SSL_CTX` shared across requests, IP-literal certificate checks.
- **Later:** Wasm, consoles.
- **Idle frame skipping (plan 029 chunk 26):** `App` sleeps while a `.j3ui` UI is still (`App.idleSkip`, `J3_IDLE_SKIP`); left: skip the draw callback on safety ticks (they repaint unchanged frames), name more timers instead of the 250 ms tick, a per-`Dom` opt-out for apps that mix 3D and UI, check by hand in a real window (caret blink, hover, resize, drag).
- **J3 shader language follow-ups:** the language, checker, GLSL emitter, `@shader(vs, fs)` and `j3c dump-glsl` shipped (`j3c doc shaders`), and the p2 shader is J3 (`game/p2shader.j3`); still open: port `post.j3`'s blur, bright and fx effects, which are GLSL strings, uniform blocks / std140, MRT, `textureGrad`, integer textures, `gl_FragDepth`, geometry/compute, LSP hover for shader types, `j3c fmt` of `#[..]` attributes (it prints `# [..]`), a clearer error when CPU code names a `#[uniform]` (it says unknown name).
- **SIMD follow-ups:** `vec4f` / `vec4i` run on LLVM and C (the VM rejects them with a span); no lane masks or compares, no vector consts or global initializers, no wider vectors.

## Later (UI rendering)

From plan 029 phase C (measurements, plan since deleted). Idle skip shipped (chunk 26); the rest was skipped by user decision on 2026-11-01, since the non-DOM scenes are already sub-ms. Hotspot: `bench2d ui 1000` is 2001 draws, gpu ~5.9 ms, cpu ~5.9 ms (two draw calls per card; the batch breaks per card). Others: `paint-mixed` 0.44 ms, `big_ui_test` page 14 0.7 ms per frame.

- **Retained instance ranges per stacking context** (`ui/dom/retain.j3`): rebuild a context's `p2` instances only when its style, layout, text or motion changed. The main CPU cost in the `ui 1000` hotspot.
- **Partial GPU upload:** keep `p2`'s VBO and upload changed ranges with `glBufferSubData`; orphan when it grows. GLES 3.0 compatible. The GPU half of the `ui 1000` hotspot.
- **Layer texture cache:** `beginLayer`, backdrop blur and big shadows render to a cached texture with an LRU cap. `big_ui_test` page 13 spends 0.2 ms in blur and 0.2 ms in layers.
- **Damage regions:** persistent offscreen buffer, scissor to the changed rects, full redraw on resize or many rects.
- **Specialized paint shaders:** split `p2` into rect, round rect, text, gradient and shadow variants in the J3 shader language, batching by variant without breaking paint order.
- **SIMD in hot CPU loops:** instance transform and clip math, `p2TextRun` glyph positions, flex and grid sizing. Profile first.
- **SDF text atlas:** one atlas per font for all sizes; check small-size text in the goldens before switching.

## Build speed

Measured on a 54-module shooter (~45k lines with stdlib); `J3C_TIMINGS=1` shows the stages. Shipped: check 0.80s to 0.17s, cold build 1.45s to 0.75s (emit inside the parallel clang workers, `mold` when present), a struct edit recompiles 3 of 54 modules, and a rebuilt `j3c` no longer invalidates objects.

Open: `lower_module` clones per-module maps (0.06s), the LSP rechecks everything per edit, and there is no function-level pruning. Further ideas (unmeasured guesses from a 2026-10 code read): cache checked/lowered modules across runs for small edits (dependency-aware invalidation, generic bodies count); hash structured IR for object keys instead of printing it, and memoize symbols/layouts; a line-only debug mode (`DebugOut` without locals/types); emit non-escaping single-assignment temporaries as SSA values instead of stack slots.

## Binary size

Shipped: `--release` (opt `s`, stripped, no unwind tables on Linux), one `@j3_abort` for all checks in whole-program builds. A large game went 4.5 MB to 1.3 MB.

Open:
- A small 3D game still links ~117 KB of `j3.scene3d` and ~66 KB of `j3.gfx` it never runs (URP tracing, PNG decode). Find the roots and split flag-only code into its own import.
- `--icf=safe` is wired for `ld.lld` but unmeasured; drop it if it gains under 1%.
- macOS release flags are pinned by a unit test but untested against a real ld64.

## Panics

Open:
1. The `.j3s` VM's `abort` op and the C backend do not print `file:line`.
2. Traces stop at 32 frames and need `nm` + `addr2line`.
3. The crash handler is x86-64 Linux only.

Out of scope: exceptions, unwinding, `defer` on panic.

## Language friction

Not blockers; the games compile without any of this.

- **Stdlib only:** `Vec2.xy` (mirror of `Vec3.xyz`).
- **`f16` follow-ups:** storage only; no arithmetic (convert with `f32(h)`), no interpolation, no `const` of type `f16`, not a `j3.gpu` vertex attribute (half attributes), not in `.j3s`.
- **Generic fn follow-ups:** the return type is never inferred; methods of non-generic structs cannot infer their own type params (still `::<T>`); instance symbol names embed the type text, so two same-named types from different modules as `T` could collide when `T` is not in the signature.
- **Trait follow-ups:** bodies are checked per instance, not once against the bounds (needs a placeholder type through lowering); `#[impl]` fns are found by `x.m()` in every module, not only where a trait with `m` is imported; a bound is a bare name (`T: Eq`, not `T: cmp.Eq`); `j3c fmt` spaces generics and attributes badly (`fn f < T >`, `# [impl]`); no `where`, default methods, supertraits, generic traits or `Default`/`Debug` traits. `Map` has no `free` (arena memory) and no closure `forEach`; a `Set` and `List.indexOf` (needs lazy generic-struct methods) are not written.
- **UFCS follow-ups:** `j3c lsp` completion after `x.` does not list matching free fns yet (it needs the fns visible at the cursor and a first-parameter type test).

## Multithreading

Goal: let a game move CPU-heavy work (world meshing, generation, pathfinding, asset decoding, baking) off the frame thread without the language growing a memory model. Threads, locks, a queue, a pool. No async, no scheduler, no shared GC.

### Shipped (Linux)

`j3.thread` (`j3c doc threads` is the guide, `j3c doc recipes threads` a program, `speculative/13_threads.j3`, run on both backends): `spawn`/`join`, `Mutex`, `Cond`, `Queue<T>` (blocking and non-blocking), `Pool` (`submit`, non-blocking `trySubmit`, `pendingJobs`, `wait`, `parallelFor`, `shutdown`), `cores()`. A frame thread feeds the pool with `trySubmit` and drains a results `Queue` with `tryPop`, dropping results whose version stamp went stale; `j3c doc recipes threads` is that program. It is pthreads through `use lib "c"`; libc is always linked, so there is no raw `clone` path.

Language: `#[thread]` on a module `let` (one copy per thread; `thread_local` in LLVM, `_Thread_local` in C) and `@atomicLoad` / `@atomicStore` / `@atomicAdd` / `@atomicSub` / `@atomicExchange` / `@atomicCas` on `int`, `i32`, `u32`, `u64`, `bool`, `ptr` through a `&T`, with an optional memory order (`relaxed`, `acquire`, `release`, `acqrel`, `seqcst` default). The VM rejects atomics; `#[thread]` is ignored there since scripts run on the owner thread only.

Stdlib: `memory.SCRATCH`, `text.cstrScratch`, and `sys.THREAD_NAME` are `#[thread]`. A panic on a named thread prints `[name] ` first. Every thread gets a crash stack, so a stack overflow in a worker still reaches the crash handler (its trace has addresses but not the thread name).

### Rules of the model

1. **Share by handing off.** A worker owns the buffers it was given; the owner reads results only after a completion signal. Everything written before `unlock`, `signal`, `push`, or an atomic store is visible after the matching `lock`, `wait`, `pop`, or atomic load, and nothing else is ordered. Do not spin on a plain flag.
2. **Static functions only.** The thread entry is a plain `fn(ptr)`; its state arrives as one `ptr` to a struct the spawner owns. A capturing closure cannot be a job (`closure(..)` never converts to `fn(ptr)`); only helpers that finish before returning take one (`speculative/lang/closures.j3`).
3. **One arena per thread.** `Arena` is not shared. `memory.scratch` is per thread; pool workers rewind it after each job.
4. **Platform calls stay on the owner thread.** GL, SDL, window, audio, and file dialogs are main-thread only, as is all `j3.game`, `j3.gfx`, `j3.scene3d`, `j3.ui`, and `j3.app` state. Workers compute; the main thread uploads and draws.
5. **Races are the user's to avoid.** No compiler enforcement. Byte-wise reads of data another thread is editing are tolerated only where a version stamp makes the result discardable.
6. **Determinism.** Pool scheduling is nondeterministic. Anything that feeds saves, replays, or netcode reduces results in a fixed order (`parallelFor` writing `out[i]` does).

### Left

- **Stdlib audit.** Done by reading module globals and imports: the safe-on-a-worker list, the main-thread-only modules, and the pure `gfx` functions (`emitFace`, `emitFaceAo`, `boxInClip`, `rayAabb`, `plainSurface`, `fxWidth`) are in `j3c doc threads` and each module's header. Not done: `printLn` is two `write`s, so lines from two threads can interleave; `gfx` methods on structs were not audited.
- **Windows and macOS:** `CreateThread` and `SRWLOCK` / pthreads variants selected like `sys` (`thread_windows.j3`).
- **Scripts:** the `.j3s` VM stays single-threaded. A script requesting work through a native job queue is not built.
- **Hot reload:** `j3c run` swaps script code into a running game. Safe today because scripts never run on workers; keep it that way.
- **Crash traces** do not name the thread (panics do).
- **Older libc:** before glibc 2.34 pthreads live in `libpthread`; link it when the target is older.
- **Job priority and cancel.** There is neither; `j3c doc threads` documents the workaround (few jobs in flight, urgent ones first; a per-job `@atomicStore` flag for cancel). Add a real cancel only if a game needs one.

### Out of scope

`async`/`await`, green threads, work stealing, data-parallel kernels, a race detector in the compiler. Revisit only after a game needs one.

## Untested features
J3 is alpha. These are built and checked by tests, goldens, or headless runs, but have not yet been played or used in a real game. An entry is removed once it has been.

- `.j3ui` components (`j3c doc j3ui`): markup and scoped CSS compiled by `j3c`; used by `examples/desk-app` and `examples/big_ui_test`, not yet by a shipped game.
- Cursor / VS Code support for `.j3ui` (`editors/vscode`, v0.4.0 `.vsix`): the LSP side is tested over stdio and the grammar by script; not installed or used in an editor yet.
- `z-index` in `.j3ui` as nested stacking contexts (opacity, transform, filter, `isolation` and fixed boxes make one (a scroll box does not: its positioned content joins the outer context, clipped and scrolled); sorted lists per context, hit test in paint order): tested by `stdlib/tests/stacking_test.j3`, `dom_test.j3`, the 54 golden and the big_ui_test dropdown headless, not by hand with a mouse.
- `j3.gfx.paint` (batched 2D shapes, text, clip, transforms) and the `j3.ui.css` / `j3.ui.dom` runtime under it: no hands-on session with mouse, scrolling, or sliders in a real window yet.
- Terminal stack (`j3c doc term`: `j3.term.*`, `j3.ui.ansi`, `examples/terminal`): checked by `stdlib/tests/term_*_test.j3` (pipes and a pty) and a headless screenshot of `examples/terminal`; not tried: typing feel and key repeat, paste, wheel, resize and title in a real window, real-tty raw mode and the TUI restore (only pipes tested), Wayland, GNOME Terminal and kitty quirks, non-Linux (pty and tty are Linux-only).
- `.j3ui` gradients (`linear`/`radial`/`conic-gradient`, layers, gradient `border-image`), `box-shadow` (outer, inset, glow) and `border-style: dashed|dotted` (`j3.gfx.paint.gradient`/`shadow`/`insetShadow`/`roundRectDashed`, shader kinds 6-9 in `j3.game`): checked by `stdlib/tests/css_test.j3` and a headless screenshot of `examples/big_ui_test` Colors & Borders, not seen in a real window or on other GPUs/drivers.
- `.j3ui` input widgets (`input` text/search/range/color, `select`, `multiple`; `j3.ui.inputs`, `j3.ui.dropdown`): typing, caret and selection, clipboard, key repeat, Tab, popups, two-thumb range: tested by `stdlib/tests/input_test.j3` with synthetic keys and mouse and by headless screenshots of `examples/big_ui_test` Buttons & Inputs; never typed on a real keyboard (SDL text input, key repeat and the system clipboard are untried).
- Style hot reload in `j3c run` for `.j3ui` `<style>` blocks and `ui/theme.css` (about 250 ms poll): tested only by capturing after a file edit.
- `j3.ui.domhtml.dumpHtml` and F12 `target/ui-snapshot.html` (browser parity): compared once against headless Chrome, not by eye in a browser.
- `game.setVirtualSize` letterboxing (`j3.game.letterbox`): checked by `stdlib/tests/letterbox_test.j3`, headless screenshots at 1400x700 and 1000x900 of `examples/big_ui_test` (incl. Post Effects) and a 3D scene with a camera effect, and a warped pointer; the SDL non-GL renderer path (`SDL_RenderSetLogicalSize`) and a real window are untried.
- `.j3ui` `flex-wrap`, `calc()`/`min()`/`max()`/`clamp()`/`vw`/`vh` and `@media` `not`/`or` (`stdlib/tests/layout_test.j3`), and high-DPI (`game.setHiDpi`, `app.hiDpi()`, twin fonts for sharp text, `J3_UI_SCALE`; `stdlib/tests/hidpi_test.j3`): the drawable-size path has only run under the offscreen driver with `J3_UI_SCALE`, never on a real 2x display or a fractional-scale desktop.
- CSS-styled popups (`.j3-select-list`, `.j3-option`, `.j3-color-panel`, `.j3-tooltip`, `--j3-tooltip-delay`), `menuitem` keyboard menus with submenus and the 300 ms hover-intent hold, and select type-ahead (`stdlib/tests/overlay_test.j3`, `input_test.j3`, headless screenshots of `examples/big_ui_test`): never driven with a real keyboard or mouse.
- Window modes and video settings (`game.setWindowMode`, `setResolution`, `moveToDisplay`, `setVsync`, `setFpsLimit`, `j3.game.display`, `app.displaySettings`, `j3c doc recipes display`): logic checked by `stdlib/tests/display_test.j3` and a headless run; borderless/fullscreen switching, multi-monitor, vsync and the frame cap need a real display and were never run on one.
- `j3c new --template ui` scaffold: checks and runs headless; not opened in a window.
- Shared glyph sheet across fonts in `j3.game` text (one texture for all sizes and bold): pixel-identical in goldens, not used in other games yet.
- `.j3ui` CSS grid (`grid-template-rows`/`columns`, `span`, `minmax()`, `repeat(auto-fill|auto-fit)`, `grid-template-areas`/`grid-area`, `justify-self`/`align-self`, `justify-items`): checked by `stdlib/tests/gridlayout_test.j3` and `grid_test.j3`, and goldens of `speculative/26_ui_grid`, `40_ui_grid`, `41_ui_gridareas`; no `[line-names]`, and `normal` / `self-start` are not parsed. Not used for a real game screen.
- `.j3ui` text properties (`font-variant-numeric: tabular-nums`, `letter-spacing`, `word-spacing`, `text-transform`, `text-indent`): checked by `stdlib/tests/text_css_test.j3`, `text_num_test.j3`, `text_spacing_test.j3`, and goldens of `speculative/42_ui_numeric`, `43_ui_textspacing`; spacing transitions and animates (checked in `text_spacing_test.j3` and the `spacing-mid` golden), and the HarfBuzz path is not compared with a browser or a real display.
- `.j3ui` colors (`hsl()`/`hsla()`, named colors, `color-mix()`, `var()` in gradient stops): checked by `stdlib/tests/css_test.j3`, `cssvar_test.j3`, and the golden of `speculative/44_ui_themevars`; only the `srgb` space of `color-mix()`, and not seen in a real window.
- `.j3ui` transforms and motion (`transform-origin`, `scale(x, y)`, `skew`, function order, several `animation`s, `var()` and `%` in `@keyframes`, `@property`): checked by `stdlib/tests/transform_test.j3`, `motion_test.j3`, `cssvar_test.j3`, and goldens of `speculative/45_ui_transform`, `46_ui_multianim`, `47_ui_property`; rotated and skewed `overflow` clips (real shape, nested, one axis): `clipshape_test.j3` and the 63 golden, at most four turned clips nest (the fifth uses its bounding box), `border-radius` does not round an `overflow` clip; no hand check with a mouse.
- `.j3ui` selectors, nesting and `:has()` (`:checked`, `:empty`, `:nth-last-child()`, `*-of-type`, `:placeholder-shown`, `:enabled`, `&` nesting, limited `:has()`): checked by `stdlib/tests/selector_test.j3`, `has_test.j3`, and goldens of `speculative/48_ui_nesting`, `49_ui_selectors2`, `50_ui_has`; restyle cost of `:has()` is only measured headless.
- `.j3ui` elements `ul ol li a hr strong em b i pre code h3`-`h6 br progress meter details summary`, and `<label for>` clicks: checked by `stdlib/tests/elements_test.j3`, `label_test.j3`, and goldens of `speculative/51_ui_elements`, `52_ui_labels`; `<details>` and the meter colors are not compared with a browser.
- `.j3ui` drag and drop (`draggable`, `on:dragstart`/`dragover`/`drop`/`dragend`, `drag-data`), `animate:flip`, `transition:` / `in:` / `out:` with fly, slide and scale, and `@media (prefers-reduced-motion | prefers-color-scheme)`: checked by `stdlib/tests/drag_test.j3`, `flip_test.j3`, `transition_test.j3`, goldens of `speculative/35_ui_drag`, `36_ui_flip`, `37_ui_transitions`, `39_ui_prefs`; drags are synthetic mouse events from `j3.ui.testkit`, never a real pointer, and the OS color scheme and reduced-motion settings are never read (the program sets them).
- `.j3ui` scroll snap, `d.scrollIntoView`, and independent `overflow-x` / `overflow-y` (`clip`): checked by `stdlib/tests/scroll_test.j3`, `overflow_test.j3`, and goldens of `speculative/53_ui_scroll`, `62_ui_overflow`; no wheel or trackpad feel in a real window.
- `.j3ui` F12 UI inspector (`J3_INSPECT`), `j3c run --hot` for markup and script, and CSS lint in `j3c check`: inspector checked by `stdlib/tests/inspect_test.j3` and `speculative/56_ui_inspect`, lint by `stdlib/tests/csslint_test.j3`; `--hot` has no automated test and is untested on Windows and macOS.
- `j3.i18n` (`t()`, plurals, `j3c i18n extract`): checked by `stdlib/tests/i18n_test.j3`, the `speculative/57_ui_i18n` golden and its `locales/`; no real translation or translator's file in a project yet.
- `.j3ui` `bind:this` and `use:action`: checked by the golden of `speculative/58_ui_actions` only; no unit test.
- Gamepad spatial focus (`ui/padin.j3`, `ui/padnav.j3`) and `:focus-visible` / `:focus-within`: checked by `stdlib/tests/padnav_test.j3`, `focus_test.j3`, and the golden of `speculative/59_ui_padnav`; never used with a real controller, and hotplug is untried.
- `.j3ui` accessibility gaps (bounds, text, live regions, events, the Inbox): checked by `stdlib/tests/a11y_test.j3`, `a11y_bounds_test.j3`, `a11y_gaps_test.j3`, `a11y_inbox_test.j3`; no screen reader can read it yet, because the AT-SPI bridge (plan 024) is not built.
- `vertical-align` on inline boxes and `box-decoration-break: clone`: checked by `stdlib/tests/valign_test.j3`, `boxbreak_test.j3`, and goldens of `speculative/60_ui_valign`, `61_ui_boxbreak`; clone with bottom alignment is untested.
- `<input type="radio">` with `name` groups, `bind:group`, arrow keys and one Tab stop per group: checked by `stdlib/tests/radio_test.j3` and goldens of `speculative/68_ui_radio`; the gamepad A press on a radio, a screen reader's real radio-group announcements and radios inside a `<select>`-style popup are untested.
- `.j3ui` `<glyph>` input prompts (keycap or pad button for an action, live device switch, xbox / playstation / nintendo / generic labels, `d.setGlyphBinding`): checked by `stdlib/tests/glyph_test.j3` and the golden of `speculative/69_ui_glyph`; a real controller driving the switch, a font without the PlayStation symbols and screen-reader announcements are untested.
- `.j3ui` right-to-left text (`game/bidi.j3`: UAX #9 W1-W7, N1/N2, I1/I2 (not the X rules, N0 bracket pairs or LRM/RLM); `:` `,` `.` `/` and number separators next to digits follow the rules; at most 1024 runs per text, then one warning): checked by `stdlib/tests/bidi_test.j3` and the 64 golden, not read by a native speaker or compared with a browser on long mixed texts.
- `.j3ui` positive `tabindex` (positives first in ascending order, then 0 in tree order, wrapping; modal and focus-group traps honoured): checked by `speculative/67_ui_tabindex` (a run, no golden); no screen reader or real keyboard.
- `url()` in custom properties (`--bg: url(a.png)` in `background-image` / `border-image-source`, `@property` `<url>`; gradients still cannot be held) and `em`/`rem` in `transform` translate lengths (not in `calc()`): checked by the 65 golden and `transform_test.j3`; the PlayStation symbol fallback (`game.fontHas`) is tested by golden only and a real controller was never used.
- `j3c build --target x86_64-pc-windows-gnu`: `cargo check` passes and a release build links, but the exe was never run on Windows.
- Networking and processes (`j3.net.tcp`, `j3.net.dns`, `j3.net.tls` on the system OpenSSL, `j3.net.http`, `j3.data.json`, `j3.proc`): checked by `stdlib/tests/net_test.j3` (loopback server: chunked, redirects, timeouts, SSE lines), `json_test.j3`, `proc_test.j3`, and `tls_live_test.j3` (real hosts, bad certificates refused); `examples/openrouter-chat` was run against a local mock and against OpenRouter with a fake key (401 in all three modes), never with a real key or inside a game on a worker thread. Linux and Android: macOS and Windows compile and return errors. On Android (arm64 phone, Android 16) a probe APK did DNS (v4+v6), http and https GETs and refused expired and wrong-host certificates; x86_64 Android (emulator) and `examples/openrouter-chat` on a phone were not run.
- Android APK on a real device (`j3c build --release` with `build_outputs = ["apk"]`, `j3c -e android run`): the build, `aapt2`/`apksigner` checks and `stdlib/tests/apkassets_test.j3` run on Linux only; no APK has been installed on a phone or emulator, and GL context loss on resume is not handled (`j3c doc android`).
- Touch and fingers (`game.fingerCount`, `game.finger(i)`, touch as mouse): checked by `stdlib/tests/touch_test.j3` with synthetic SDL events; no real touchscreen yet.
- Background, resume and the Android Back key (`game.backgrounded`, `game.waitBackground`, `game.closeRequested` under `holdClose`): checked by `stdlib/tests/touch_test.j3`; no real home button, resume or Back press yet.
- The asset copy on first launch (`j3.apkassets`, `j3assets.txt`, the stamp): checked by `stdlib/tests/apkassets_test.j3` against a directory; never run on a device that reads the APK's own assets.
- JZero engine asks: `game.wakeAt` (idle loop, `stdlib/tests/idle_test.j3` with a fake clock), terminal reflow, combining marks, `?2026` sync and DECRQM (`stdlib/tests/term_grid_test.j3`), `View` italic font, line height and marks (headless screenshot of `examples/terminal` with `TERM_ITALIC`, `TERM_LINE_HEIGHT`), `[dist.actions]` (Rust test of the `.desktop` text): `View.ligatures` was never seen with a ligature font, the held `?2026` frame never on screen, and `App.transparent` / `setBackgroundAlpha` (ARGB visual on X11) and `App.windowClass` (`xprop WM_CLASS`, Wayland `app_id`) never in a real window; the dock menu of a `.desktop` action was never opened.
- `on:mousedown` / `on:mouseup` / `on:contextmenu` bubbling with `|stop`: checked by `stdlib/tests/bubble_test.j3`; not used in a real app (a canvas with a HUD over it) yet.
- `game.simKey` / `simText` / `simPreedit` / `simDrop` (fake `j3.game` input) and `j3.term.view` cursor blink (`View.blinkMs`): checked by `stdlib/tests/sim_input_test.j3` and `idle_key_test.j3`; the blink has not been watched in `examples/terminal`.
- Dragging files out of the window (`game.dragFiles` / `dragState`, XDND on X11, `j3c doc recipes dragout`): only the headless false path is checked (`stdlib/tests/dragout_test.j3`); a real drop onto Discord, a browser or a file manager, XWayland, and targets behind `XdndProxy` are untested.
- j2 runtime scripts (`j3.j2`, `j3c doc j2`, `examples/j2_script`): checked by `stdlib/tests/j2_*_test.j3` (also under `J3_J2_GC_STRESS=1`), the `ARENA_TEST=1` self-test and goldens of `examples/j2_script`; play feel in a real window, reload on save from a real editor (mtime is second-granular), long play sessions and real modders' scripts are untested.
