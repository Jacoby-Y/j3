<p align="center"><img src="logo.svg" width="112" alt="J3 logo"></p>

# J3

[![CI](https://github.com/Jacoby-Y/j3/actions/workflows/ci.yml/badge.svg)](https://github.com/Jacoby-Y/j3/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Jacoby-Y/j3?include_prereleases)](https://github.com/Jacoby-Y/j3/releases)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

J3 is a programming language for games. It compiles ahead of time to native code: `j3c` checks your program and hands it to `clang`, and you get a small native binary. The compiler is written in Rust with no third-party crates. One `j3c` binary holds everything else too: the standard library (windows, input, audio, 2D and 3D drawing, UI, physics, networking), a 3D scene editor, a language server, and all the docs.

**Status:** 0.1, alpha. Linux x86-64 is supported; Linux arm64 has prebuilt binaries but is less tested. Windows, macOS and Android builds are experimental. Expect breaking changes between 0.x releases ([CHANGELOG](CHANGELOG.md)).

<p align="center"><img src="docs/images/village.png" width="640" alt="examples/editor_village: a first-person walk through a low-poly village built in the J3 scene editor"></p>

## A small game

A window with a bar you move with the arrow keys:

```j3
module hello;

use j3.app.App;
use j3.game.{ Key, clear, drawRect, keyDown, rgb, setColor };
use j3.math.Vec2;
use j3.memory.Arena;

struct World { x: f32; }

fn boot(_mem: &Arena): World { return World { x: 280 }; }

fn update(w: &World, dt: f32) {
    if keyDown(Key.Left) { w.x = w.x - 200.0 * dt; }
    if keyDown(Key.Right) { w.x = w.x + 200.0 * dt; }
}

fn draw(w: &World) {
    setColor(rgb(32, 48, 72));
    clear();
    setColor(rgb(240, 200, 64));
    drawRect(Vec2 { x: w.x, y: 160 }, Vec2 { x: 80, y: 24 });
}

fn main() {
    let app = App<World>.create("hello", 640, 360, boot);
    app.always(update);
    app.draw(draw);
    app.run();
}
```

## Install

Linux (x86-64 or arm64), into `~/.local/bin`:

```sh
curl -fsSL https://raw.githubusercontent.com/Jacoby-Y/j3/main/install.sh | sh
```

Or download a `.deb` from [Releases](https://github.com/Jacoby-Y/j3/releases) and run `sudo apt install ./j3c_0.1.0_amd64.deb`. On Arch, `yay -S j3c-bin`.

J3 needs `clang` to build programs, and SDL2, OpenGL and a few fonts for anything with a window. Check your system with:

```sh
j3c doctor
```

It prints each requirement as `ok` or missing. On Ubuntu or Debian:

```sh
sudo apt install clang libsdl2-2.0-0 libsdl2-ttf-2.0-0 libsdl2-image-2.0-0 libsdl2-mixer-2.0-0 \
  libgl1 fonts-dejavu-core fonts-noto-color-emoji
```

To uninstall, delete `~/.local/bin/j3c` (and the cache in `~/.cache/j3`). [`packaging/README.md`](packaging/README.md) covers upgrades and the other channels.

### Build from source

You need Rust (stable) and `clang`.

```sh
git clone https://github.com/Jacoby-Y/j3
cd j3
./build.sh
```

`build.sh` builds a release `j3c` and installs it to `~/.bin` if that exists, else `~/.local/bin` (`J3_BIN_DIR` picks another place). To build without installing, run `cargo build --release` in `j3c/`; the binary is `j3c/target/release/j3c`.

## Quick start

```sh
j3c new hello --app     # a project with a window: j3.toml and src/main.j3
cd hello
j3c run                 # build and run it
j3c build --release     # a small, stripped binary in target/hello
```

`j3c new hello` without `--app` makes a console program. `--template ui` starts a `.j3ui` UI project and `--template fps` a first-person 3D project. `j3c guide` walks through the rest.

## Features

- **2D drawing**: batched shapes, text, images and transforms (`j3.gfx.paint`), plus procedural images you can save as PNG (`j3.gfx.canvas`).
- **3D**: meshes, lights, shadows and materials (`j3.gfx`, `j3.scene3d`), and a 3D scene editor (`j3c editor`) that saves scenes as readable text files.
- **UI components**: `.j3ui` files with Svelte-like markup, J3 script and a large subset of CSS (flex, grid, animations, inputs).
- **Shaders in J3**: write vertex and fragment shaders as J3 functions; `j3c` turns them into GLSL.
- **Mods**: j2, a small scripting language a shipped game can load at runtime for player mods.
- **Shipping**: `j3c build --release` makes zips and `.deb` packages with a menu entry and icon, Steam depots with Steamworks bindings, and Android APKs.
- **Tooling**: a language server (`j3c lsp`) and a [VS Code / Cursor extension](editors/vscode/), a formatter, headless screenshots and golden-image tests.
- **AI agents**: an MCP server that lets coding agents edit and render scenes (see [For AI agents](#for-ai-agents)).

## Examples

Run any of these from the repository root:

| Example | What it shows | Run |
|---|---|---|
| [`3d-editor-scene`](examples/3d-editor-scene) | The smallest editor project: two scenes and a `.j3s` script | `j3c run examples/3d-editor-scene` |
| [`editor_village`](examples/editor_village) | A walkable village where scenes place other scenes (houses) | `j3c run examples/editor_village` |
| [`desk-app`](examples/desk-app) | A desktop-style `.j3ui` app: sidebar, sortable table, rename dialog, saved settings | `j3c run examples/desk-app` |
| [`big_ui_test`](examples/big_ui_test) | Every `.j3ui` and CSS feature, one page each | `j3c run examples/big_ui_test` |
| [`j2_script`](examples/j2_script) | A top-down shooter whose enemies, weapons and HUD are j2 mod scripts | `j3c run examples/j2_script` |
| [`terminal`](examples/terminal) | A terminal emulator: your shell on a pty, drawn with J3 | `j3c run examples/terminal` |
| [`gallery`](examples/gallery) | Older immediate-mode UI widgets, layout and styles | `j3c run examples/gallery` |
| [`scripted`](examples/scripted) | `.j3s` scripts in the console, compiled or on the VM | `j3c run examples/scripted` |
| [`openrouter-chat`](examples/openrouter-chat) | A console LLM chat over HTTPS and JSON | `OPENROUTER_API_KEY=... j3c run examples/openrouter-chat` |

`j3c check examples/<name>` typechecks one without running it. To run one without a window, add `--headless --frames 60 --screenshot shot.png`.

<p align="center"><img src="docs/images/desk-app.png" width="560" alt="examples/desk-app: a file list with a sidebar and a details pane"></p>

## Docs

- `j3c guide`: start here; how a project is laid out and what to read next.
- `j3c doc lang`: the language in one page. `j3c doc`: the standard library (`j3c doc search <word>`, `j3c doc <module> --short`).
- `j3c doc recipes`: complete small programs (2D app, 3D scene, UI components, shaders, threads, audio, HTTP, Steam, ...).
- [The J3 Book](docs/) (`mdbook serve docs`): the language, the compiler and shipping, written for people.
- [`speculative/`](speculative/): sample programs that double as the compiler's acceptance tests. `01`–`08` are a language tour that compiles and runs (`j3c run speculative/01_basics.j3 --headless --frames 5`), `lang/` has one file per feature, and the numbered `*_ui_*` folders each test one `.j3ui` feature.
- [`j3c/editor/README.md`](j3c/editor/README.md): the scene editor. [`roadmap.md`](roadmap.md): what is done and what is next.

The docs ship inside `j3c`, so `j3c doc` always matches the compiler you run.

## Platforms

| Platform | Status |
|---|---|
| Linux x86-64 | Supported: compiler, games, editor, `.deb`/zip shipping, CI |
| Linux arm64 | Prebuilt `j3c` from the release workflow; untested until the first release runs |
| Windows | Experimental: `j3c build --target x86_64-pc-windows-gnu` cross-compiles and links; the exe has not been run on Windows |
| macOS | Experimental: a target exists; never built or run on a Mac |
| Android | Experimental: builds APKs (`j3c doc android`); a network probe ran on a phone, no game has yet |
| Steam (Linux, Steam Deck) | Bindings and depot output (`j3c doc steam`); no game shipped yet |

## For AI agents

J3 is built to be driven by coding agents as well as people. [`AGENTS.md`](AGENTS.md) is the map for them, and `j3c -e mcp` is an MCP server that edits, renders and checks 3D scenes (`claude mcp add -s user j3 -- j3c -e mcp`).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for building, testing and pull requests, and the [Code of Conduct](CODE_OF_CONDUCT.md). Security issues: [SECURITY.md](SECURITY.md).

## License

J3 is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option. Bundled fonts, icons and assets keep their own licenses, listed in [THIRD_PARTY.md](THIRD_PARTY.md). It also notes what a game built with J3 has to ship with.

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
