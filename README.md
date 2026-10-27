# J3

Ahead-of-time game language. `j3c` parses J3, typechecks it, and prints LLVM IR for the host `clang` to link. C is an optional backend (`--emit c`). The compiler is a Rust crate with no crates.io dependencies and no libLLVM.

## Requirements

- Rust (edition 2021) and Cargo
- `clang` on `PATH`
- SDL2 for the example games. Image, mixer, and ttf are vendored under `examples/dungeon/lib` and picked up by rpath. The 3D examples also need OpenGL 3.3 (`libGL`).

## Compiler

From `j3c/`:

```
cargo build --release
cargo test
```

The binary is `j3c/target/release/j3c`. Put it on `PATH`, or call it by that path. The standard library is compiled into that binary. `J3_STDLIB` (the parent of the `j3/` modules) replaces the embedded copy.

## Examples

```
j3c run examples/bounce
j3c check examples/dungeon
j3c build examples/island
```

`run` and `build` with no path use `./j3.toml`. `build` writes `target/<name>` in the package directory and sets the process working directory to the package root (`J3_ROOT`), so assets load from there.

| Example | What it is |
|---------|------------|
| `examples/bounce` | Paddle and ball |
| `examples/dungeon` | Tile crawler: scenes, UI, save, particles, looping music |
| `examples/gallery` | UI gallery: widgets, controls, layout, style, inventory |
| `examples/defense` | Tower defense. Drag the map, scroll to zoom |
| `examples/island` | Voxel island: GL mesh, shadow map, orbit camera |
| `examples/minecraft` | First person. Break, place, jump, corner shadows |
| `examples/leaves` | First person. Pick up leaves, spend them on reach and grab speed |
| `examples/canvas` | Headless procedural sprite and tile PNG generation |
| `examples/canvas_preview` | Preview generated assets and in-memory texture uploads |
| `examples/raytrace` | Interactive glass, metal, and plastic raytracing in a room |
| `examples/ocean` | Procedural ocean waves with glass and metal reflections |
| `examples/forest` | Autumn forest with CC0 tree assets, godrays, glass and metal |

## New project

```
j3c new mygame
cd mygame
j3c check
j3c run
```

`j3c new mygame --app` scaffolds an SDL window. `j3c guide` starts a project. `j3c doc lang` is the language, `j3c doc` is the standard library, and `j3c doc j3ui` is `.j3ui` UI components (`j3c new mygame --template ui` starts one; `j3c doc j3ui-immediate` is the older `@j3ui`). `j3c help manifest` lists `j3.toml` keys.

## Programmatic graphics

`j3.gfx.canvas` draws into an RGBA image without opening a window. Create a `Canvas` in `Mode.Pixel` for crisp sprites and tiles or `Mode.Smooth` for antialiased shapes. Draw shapes, gradients, noise, and other canvases; then call `savePng` to export or `j3.game.textureFromCanvas` to display it. Upload static art once and use `updateTextureFromCanvas` for a changed rectangle. Run `j3c doc j3.gfx.canvas` for the API and `j3c run examples/canvas` for a headless sample that writes `lantern.png` and `stone.png` into that example directory.

Build art from a readable silhouette, a restrained palette, consistent lighting, and a few deliberate details. The canvas sample uses separate light and dark layers and seeded variation. Check generated assets at their intended display size before adding texture noise.

## OpenGL rendering

`j3.gfx` provides the mesh renderer. Optional `j3.gfx.urp.raytrace` adds a fullscreen room and sphere path tracer, with `j3.gfx.urp.forest` for compositing `j3.gfx` meshes with volumetric light and glass/metal spheres. All need `App.useGl()` and OpenGL 3.3. Use `j3c doc j3.gfx.urp.raytrace` or `j3c doc j3.gfx.urp.forest` for the module API; the full pass order and GLSL requirements are in [`j3c/stdlib/j3/gfx/urp/README.md`](j3c/stdlib/j3/gfx/urp/README.md). Samples are `examples/raytrace`, `examples/ocean`, and `examples/forest`.

## Editor

`editors/vscode` is the Cursor / VS Code extension: highlighting, `j3c lsp` (completion, imports, signature help, outline), `j3.toml` keys, and run / build / format. Set `j3.j3cPath` when `j3c` is not on `PATH`.

## Layout

| Path | What |
|------|------|
| `j3c/` | Compiler. Map in `j3c/AGENTS.md` |
| `j3c/stdlib/j3/` | Standard library, written in J3 |
| `speculative/` | Acceptance programs (`01`–`08` compile and run; `09` typechecks `j3.game`; `10_j3ui.j3` typechecks `@j3ui`) |
| `examples/` | Playable games |
| `editors/vscode/` | Editor extension |
| `tools/j3mesh_import.py` | Mesh importer |
| `roadmap.md` | What ships and what is still open |
| `AGENTS.md` | Working notes for this repo |

## Untested features

Built and checked by tests, goldens, or headless runs, but not yet played or used in a real game. Remove an entry when it has been.

- `.j3ui` components (`j3c doc j3ui`): markup and scoped CSS compiled by `j3c`; only `examples/idle-breakout` uses them.
- Cursor / VS Code support for `.j3ui` (`editors/vscode`, v0.3.0 `.vsix`): the LSP side is tested over stdio and the grammar by script; not installed or used in an editor yet.
- `z-index` in `.j3ui` as nested stacking contexts (opacity, transform, filter, `isolation` and fixed boxes make one (a scroll box does not: its positioned content joins the outer context, clipped and scrolled); sorted lists per context, hit test in paint order): tested by `stdlib/tests/stacking_test.j3`, `dom_test.j3`, the 54 golden and the big_ui_test dropdown headless, not by hand with a mouse.
- `j3.gfx.paint` (batched 2D shapes, text, clip, transforms) and the `j3.ui.css` / `j3.ui.dom` runtime under it: no hands-on session with mouse, scrolling, or sliders in a real window yet.
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
- Networking and processes (`j3.net.tcp`, `j3.net.dns`, `j3.net.tls` on the system OpenSSL, `j3.net.http`, `j3.data.json`, `j3.proc`): checked by `stdlib/tests/net_test.j3` (loopback server: chunked, redirects, timeouts, SSE lines), `json_test.j3`, `proc_test.j3`, and `tls_live_test.j3` (real hosts, bad certificates refused); `examples/openrouter-chat` was run against a local mock and against OpenRouter with a fake key (401 in all three modes), never with a real key or inside a game on a worker thread. Linux only: macOS and Windows compile and return errors.
