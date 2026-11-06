# J3

AOT game language (`j3c`, a Rust crate, no deps) plus a 3D scene editor and an MCP server for agents. Spec wins over examples; examples win over compiler comments. The contract is `j3c/AGENTS.md`; there is no `speculation.md`.

NOTE: Always run ./build.sh when finishing implementign something (only after confirming the build won't be broken)

## Where to learn what

Ask the tools first. They ship with the compiler, so they match the binary you run (`j3c version --verbose` says which build and stdlib).

| I need to... | Go to |
|---|---|
| Start a project | `j3c new <name>` (`--app` window, `--template fps` first-person 3D, `--template ui` `.j3ui` components), `j3c guide` |
| Find a stdlib name or module | `j3c doc`, `j3c doc search <word>`, `j3c doc <module> --short`. Big guide pages (`css`, `j3ui`, `editor`) read by section: `j3c doc css --short` lists them, `j3c doc css layout` prints one |
| See a complete working program | `j3c doc recipes` (`app2d`, `scene3d`, `canvas`, `script`, `paint`, `components`, `meshgen`, `material`, `urp`, `threads`, `audio`, `check`) |
| Learn the language | `j3c doc lang`, then `speculative/01`–`08` (they compile and run), then `speculative/lang/` (one file per feature: `ufcs`, `closures`, `generic_fns`, `traits`, `extend`, `simd`, `shader`, `f16`, `small_gaps`) |
| Ship a game (release build, zip, `.deb` with menu entry and icon) | `j3c doc ship`; `[dist] build_outputs = ["zip", "deb"]` in `j3.toml`. Dev-only code: `use j3.build;` + `if build.DEV { ... }` (`RELEASE` under `--release`); `build.VERSION` / `build.NAME` come from `[package]` |
| Ship on Steam (achievements, stats, leaderboards, cloud, overlay, Deck keyboard, Steam Input, lobbies, depot upload) | `j3c doc steam`, `j3c doc recipes steam`, `j3c doc j3.steam --short`; `j3c -e steam-check --sdk DIR` verifies the bindings against your SDK (run it first). `use j3.steam;` is a no-op without Steam, `J3_STEAM_FAKE=1` fakes it in dev, `use j3.steam.game.attach;` pumps it in an `App` |
| Build for Android (apk, touch) | `j3c doc android`, `tools/android/build-sdl.sh` |
| Video settings menu (windowed, borderless, fullscreen, resolution, monitor, vsync, frame cap, saved) | `j3c doc recipes display`, `j3c doc j3.game.display`; primitives are `j3c doc j3.game window` |
| Global hotkey (fires while another app has focus, e.g. F8 to pause a recording) | `j3c doc recipes hotkey`, `j3c doc j3.game.hotkey`. Linux X11 only (`XGrabKey`); `Err` on Wayland, headless, or a taken combo |
| Quick-terminal toggle: show/hide/raise the window, one running copy | `game.showWindow` / `hideWindow` / `raiseWindow` / `windowShown`; `j3c doc j3.instance` (`instance.claim(name)` forwards argv to the running copy, `App.onMessage`; a control CLI that needs an answer uses `instance.request(name, args, ms)` and `App.onRequest` / `req.reply`, deferrable; Linux) |
| Branch on the OS (Linux, Windows, macOS, Android) or CPU (x86-64, arm64) | `if sys.PLATFORM == sys.PLATFORM_WINDOWS { ... }` with `use j3.sys;`; compile-time, the other branch is dropped. `sys.PLATFORM_NAME` is `"linux"` etc. The CPU is `sys.ARCH` (`ARCH_X86_64`, `ARCH_AARCH64`, ...; `ARCH_NAME`), set by the loader from the target triple (`Loader::with_arch`), also compile-time. Dev vs release is `j3.build` |
| Paste images, GIFs, or file lists (non-text clipboard) | `j3c doc recipes paste`, `j3c doc j3.game clipboardTypes` / `clipboardData`. X11 own connection with INCR; Wayland without `DISPLAY` uses `wl-paste`; 1 s owner timeout; empty headless |
| Drag a file out of the window into another app (Discord, a browser, a file manager) | `j3c doc recipes dragout`, `j3c doc j3.game dragFiles` / `dragState`. XDND on its own X11 connection, `text/uri-list` of absolute files; works under XWayland; `false` headless, SDL on native Wayland, Windows, macOS |
| Write UI | `j3c doc j3ui` (`.j3ui` components: markup, props, events, bind, `<canvas>`), `j3c doc css` (supported CSS and browser differences), `<icon name="trash">` takes Bootstrap Icons ids (`j3c doc icons <word>` lists them; own ones in `icons/*.svg`), `j3c doc recipes components`, `j3c dump-ui ui`, `examples/idle-breakout`, `examples/desk-app` (app shell, data table), `examples/big_ui_test` (every feature on a page); older immediate-mode `@j3ui`: `j3c doc j3ui-immediate`, `speculative/10_j3ui.j3`, `examples/gallery` |
| Draw 2D from code (shapes, text, HUD) | `j3c doc recipes paint`, `j3c doc j3.gfx.paint --short` (batched, GL only) |
| **Run, screenshot, or test anything windowed** | `j3c doc testing`. **Never open a window:** `j3c run --frames N --screenshot f.png --headless`; a built binary needs `SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy` on the same command. Goldens: `j3c golden <dir>` (states in `golden.toml`), or by hand `J3_CAPTURE=out.png:N J3_FIXED_DT=0.016` + `j3c -e imgdiff a.png b.png`; layout as text: `j3.ui.domhtml.dumpHtml`. `J3_STEADY=60:120` fails (rc 1) if idle frames lay out, restyle, or allocate. Wrap in `timeout` |
| Write a shader (GPU code in J3) | `j3c doc shaders`: `#[shader(vertex)]`/`#[shader(fragment)]` fns, `#[vertex]`/`#[varying]` structs, `#[uniform] let`, `const PROG: Source = @shader(vs, fs);` (`PROG.vert`/`PROG.frag` are GLSL, `PROG.attrib(i)` the vertex layout); `j3c dump-glsl [--es]` prints the text; `speculative/lang/shader.j3`, `j3c/stdlib/tests/gpu_glsl_test.j3` |
| Write a TUI, a terminal emulator, or show colored (ANSI) logs | `j3c doc term` (modules, restore hook, limits), `j3c doc recipes tui` / `pty` / `ansi`, `j3c doc j3.term.grid --short`, `examples/terminal` (windowed emulator) |
| Write `.j3s` scripts | `j3c doc recipes script`, `speculative/11_script/`, `examples/scripted` |
| Let players mod a shipped game (j2 scripts read at runtime) | `j3c doc j2` (host API, keep/reload/error rules), `j3c doc j2 lang` (the language page to ship to modders), `j3c doc recipes j2`, `examples/j2_script` |
| Call a web API (https, JSON) or run another program (curl) | `j3c doc recipes http`, `j3c doc j3.net.http`, `j3c doc j3.data.json`, `j3c doc j3.proc`, `examples/openrouter-chat`. Blocking: run it on a `j3.thread` worker. Linux and Android (`j3c doc android`; on Android `j3.proc` runs only programs shipped with `[android] native_libs`) |
| Move heavy work off the frame thread | `j3c doc recipes threads`, then `j3c doc threads` (rules, which modules are safe, mistakes). GL, SDL, and `gfx`/`game`/`scene3d` state are main-thread only |
| Build or edit 3D scenes | `j3c doc editor`, `j3c/editor/README.md` (controls, file format, import limits) |
| Drive the editor as an agent | see **Agent workflow** below |
| Know what is left to build | `roadmap.md`, `EDITOR.md` (**Still left**, **Agentic use help**), `plans/` |

## Agent workflow for scenes

Scenes are `scenes/*.j3scene` text files (one object per line). Edit them through the tools, not by hand: the tools keep ids, parents, and undo correct, and `scene3d` is the only reader and writer of the format.

**MCP server.** `claude mcp add -s user j3 -- j3c -e mcp`. It follows the project the editor has open, else the one around the working directory (`--project DIR` pins one). Its startup instructions and every tool description say when to use the tool; replies carry hints when a better tool exists. A project can add its own notes with `notes = "AGENT_NOTES.md"` in `j3.editor.toml` (see `examples/fps-shooter`). After rebuilding `j3c`, run `./build.sh` (installs to `~/.bin/j3c`) and restart the server, since running servers keep the old binary.

**The loop:** change, look, check.

1. Learn the scene: `scene_list` (glob `filter`, `group_by=prefix`, `tree` for the file's own lines), `scene_get` (an object and its children as file lines).
2. Build with bulk tools, not many single calls: `place` (`ring`, `wall` with a door, `stairs`, `spiral`, `grid`, `scatter`, `ground`, `ontop`), `scene_add_many`, `scene_apply` (objects written as scene lines, or a subtree copied from `scene_get`), globs in `scene_set`/`scene_delete`. `scene_split`/`scene_inline` move objects into their own scene file and back.
3. Look from where the player stands: `render` with `at`/`yaw`/`pitch`, `views` for several angles in one image, `hide` (`Roof*`) to see inside, `view=top` + `ortho` for a plan. A render after every few edits beats one at the end.
4. Verify: `check` (scene rules, the project's `check` script, and code), `scene_stats` (draw cost; above 60 distinct looks is slow), `scene_diff` (review your own edits against git HEAD), `play` (runs the game headless, returns a frame).

The same operations exist on the command line: `j3c -e scene --help`, `j3c -e render --help`, `j3c -e gen`, and `j3c -e mcp --tools` (each MCP tool's arguments next to its CLI twin). Use the CLI when there is no MCP server; nothing else differs.

Facts that cost agents time:
- **A `.j3scene` file is one line per object** (`Name kind key=value #id`, indent for children, defaults left out; format in `j3c/editor/README.md`), and rotations are **degrees** there and in the tools. `list tree=1` and `get` print the lines, `scene_apply` takes lines back, `j3c -e scene fmt` normalises a hand edit, and `tools/j3scene_migrate.py` converts old files (`node.<i>.*`, radians). `check` warns on a rotation above two turns. A slab pointing along `(dx, dz)` needs `rot` y `= -atan2(dz, dx)`; yaw 0 looks toward -z.
- With `j3c editor` open, tool edits are live undo steps in it and unsaved until the person saves. With it closed they write the file and `undo` restores it (20 kept). Every reply says which happened.
- Put project rules in a `check` script (`j3.editor.toml`: `check = "tools/check.j3s"`, prints `error: ` / `warning: ` lines), so a mistake becomes a named object instead of a bug found in play. `examples/fps-shooter/tools/check.j3s` is the pattern.
- Objects carry `tags` (`scene_set tags=nocollide,launch=30`; `scene3d.hasTag` / `tagValue`). Prefer them to name prefixes in new game code.
- **Z-fighting:** the renderer pulls the smaller of two objects with overlapping faces on one plane toward the camera in depth by itself (`scene3d.flushUpdate`, rescanned once objects stop moving); `check` still names the pairs so you can fix the scene at the source. `tags=decal` (`decal=2` to stack) forces it for an object the scan misses (faces more than 2 mm apart, meshes over 2000 triangles).

## Repo map

| Path | What |
|---|---|
| `j3c/` | Compiler crate. **Map and contract: `j3c/AGENTS.md`** |
| `j3c/src/ui/`, `j3c/stdlib/j3/ui/`, `j3c/stdlib/j3/gfx/paint.j3` | `.j3ui` compiler (`ui/`: markup, gen, span map), CSS parser (`ui/css.j3` plus parts in `ui/css/`: types and `Css`; `cssparse`, `cssvalue`, `cssprop`, `cssmatch`, `cssimage`, `textedit`, `inputs`, `a11y`, `media`, `dropdown` have parts in folders beside them (`ui/cssparse/` etc.); `csslex` `cssmotion`, one stage each) and layout/runtime (`ui/dom.j3` plus its parts in `ui/dom/`; `domhot` style reload, `domhtml` snapshot), and the batched 2D painter. Tests: `j3c/stdlib/tests/`, `speculative/12_ui_runtime/` |
| `j3c/src/steamcheck/`, `j3c/src/dist/steam.rs`, `j3c/src/project/steam.rs` | Steam: `j3c -e steam-check` (binding checker), `[dist] build_outputs = ["steam"]` (depot tree, vdf, sniper script), `[steam]` keys |
| `j3c/src/imgdiff.rs`, `j3c/src/phase.rs`, `j3c/src/dist.rs` | `j3c -e imgdiff` (golden compare), build phase timings, `[dist] build_outputs` (zip, `.deb` in `dist/deb.rs`) |
| `j3c/src/scene/`, `j3c/src/mcp/` | `j3c -e scene`/`render` and the MCP server: thin wrappers that run the editor binary headless |
| `j3c/editor/src/` | The editor program, in J3, one module per file. `tool.j3` is every scene op (parts in `tool/`: `check`, `stats`, `place`, `edit`, `read`, `refs` globs, `split`; subfolders of `src/` must be listed in `FRONTEND`, `src/editor/mod.rs`); `render.j3` is `-e render`; `live.j3` is commands sent to an open editor. `README.md` is the user guide |
| `j3c/stdlib/j3/` | Stdlib in J3, embedded in the binary (rebuild `j3c` after editing it; `J3_STDLIB` overrides). Top level: `sys`, `io`, `math`, `rand`, `memory`, `thread` (Linux pthreads: `Pool`, `Queue`, `Mutex`), `option`, `result`, `text`, `app`, `game`, `gfx`, `ui`, `scene3d`, `script` (the `.j3s` VM), `net` (UDP), `proc` (spawn programs), `steam` (Steamworks, parts in `steam/`, plus `steam.game` and `steam.ui`). Folders: `game/` (`ecs` `input` `anim` `fx` `collide` `space` `physics` `path` `tile`), `gfx/` (`mesh` `meshgen` `voxel` `bvh` `canvas` `noise` `urp/`), `data/` (`save` `assets` `res` `scene` `debug` `json`), `net/` (`tcp` `dns` `tls` `http`; Linux and Android, OS calls in `netsys.j3` / `netsys_android.j3`), `scene3d/` (`fpwalk`, `fx`, `editor/` used only by the editor). A folder is part of the name: `use j3.game.ecs;`, except part files that say `module j3.game;` (big modules spread over files by concern: `ui/dom`, `game`, `gfx`, `scene3d`; map in `j3c/AGENTS.md`). `j3.sys` is `sys.j3` / `sys_windows.j3` / `sys_macos.j3` by `[emit] target` |
| `speculative/` | `01`–`08` compile and run, `09` typechecks `j3.game`, `10_j3ui.j3` is the `@j3ui` sample, `11_script/` is a `.j3s` script (AOT and on the VM), `lang/` is one sample per language feature (both backends) |
| `editors/vscode/` | Cursor/VS Code extension: highlighting (`j3c dump-syntax`), `j3c lsp`, `j3.toml` keys, Run/Build/Format |
| `docs/` | The J3 Book: human-readable language and compiler guide (mdBook, `mdbook serve docs`). Agents still use `j3c doc` |
| `roadmap.md`, `EDITOR.md` | Direction and status. Editor brainstorms go in `EDITOR.md`, not code |
| `plans/` | Open handoff plans only (`plans/README.md` is the index); finished ones are deleted, git history keeps them. Code comments citing "plan 023" etc. point at those |
| `tools/` | `bench2d/` (2D paint/UI perf project), `j3mesh_import.py`, `j3scene_migrate.py` (v1 `.j3scene` to the one-line-per-object format); `voxel-editor/` is a voxel model editor (window) plus `vox run` for agents: models, rigs with joints, poses and clips, pixel overlays (armor), `.j3voxel` files, `runtime/j3rig.j3` for games (`tools/voxel-editor/AGENTS.md`, `README.md`) |
| `tools/composer/` | Composer: a DAW with a built-in synth. `j3c-composer FILE` (window), `j3c-composer run FILE "cmds" --wav --render` for agents: chords, tracks, fills, beats, `analyze` (`tools/composer/AGENTS.md`) |
| `build.sh` | Release-build `j3c` and install it to `~/.bin/j3c`, then `tools/composer` to `~/.bin/j3c-composer` |

## Examples

| Path | What it shows | Needs |
|---|---|---|
| `examples/fps-shooter/` | The large one: first-person shooter, biomes as scene files in `scenes/biomes/`, bosses, HUD, save, perks. `tools/arena.j3s` (scene generator), `tools/check.j3s` (project rules), `AGENT_NOTES.md`, `PLAN.md`, `PERFORMANCE.md` | SDL2 + image/mixer/ttf + libGL |
| `examples/3d-editor-scene/` | Smallest editor project: two scenes, a `.j3s` script, `scene3d` in a game | SDL2 + libGL |
| `examples/editor_village/` | Scenes placing scenes (houses in a village), scene props | SDL2 + libGL |
| `examples/desk-app/` | The `.j3ui` proof app: sidebar, keyed sortable table with filter, details pane, rename modal, toast, saved settings; slot-based `ui/kit/`; goldens (`j3c golden`, incl. a `J3_STEADY` idle-cost check), `DESK_TEST=1` fake-input run (`j3.ui.testkit`) | SDL2 + ttf + libGL |
| `examples/big_ui_test/` | Every `.j3ui`/CSS feature on its own page (`UI_PAGE=N`); page 17 "New in 023" shows `{#let}`, selectors and pseudo-elements, inline boxes, `line-clamp`, flex order/basis, static selection; `FEATURE_CHECK.md` lists what was seen | SDL2 + ttf + libGL |
| `examples/idle-breakout/` | `.j3ui` components (Svelte-like markup + CSS) on `j3.gfx.paint`: a canvas game beside a tabbed sidebar. Goldens in `golden/` (`j3c golden examples/idle-breakout`, states in `golden.toml`), perf bench (`IDLE_BENCH=1`), style hot reload, HTML snapshot (`IDLE_SNAPSHOT=N`) | SDL2 + ttf + libGL |
| `examples/terminal/` | A simple terminal emulator: `$SHELL` on a pty (`j3.term.pty`), cells drawn by `j3.term.view`, keys, paste, wheel scrollback, resize, title from OSC. `TERM_CMD="..."` runs one command and keeps the screen (headless screenshots). Linux | SDL2 + ttf + libGL |
| `examples/gallery/` | UI gallery: widgets, controls, layout, style, inventory | SDL2 + image/mixer/ttf |
| `examples/openrouter-chat/` | Console LLM chat over OpenRouter: `j3.net.http` POST with a `j3.data.json` body, `--stream` reads server-sent events line by line, `--curl` does the same through `j3.proc`. `OPENROUTER_API_KEY`; `OPENROUTER_URL` points it at a mock | libssl3 (curl for `--curl`) |
| `examples/scripted/` | Headless `.j3s` scripts (fighters, duels, `shared` state); `j3c run` (VM) and `j3c build` (AOT) print the same | nothing |
| `examples/j2_script/` | J2 Arena: a top-down shooter whose content is j2 scripts (`j2-scripts/*/main.j2`) loaded at runtime, reload on save, per-script errors; `ARENA_TEST=1` self-test, goldens, `SCRIPTING.md` for modders | SDL2 + ttf + libGL |

`j3c run examples/<name>`, `j3c check examples/<name>`, `j3c editor examples/<name>`. A project run uses cwd = package root.

## Working here

- Build and test from `j3c/`: `cargo build --release`, `cargo test`. The editor program is rebuilt per `j3c` version into `~/.cache/j3/editor`; `J3_EDITOR_SRC=$PWD/j3c/editor/src` builds it from disk, so set it while editing `j3c/editor/src/` and unset it for normal use.
- Ship small with `j3c build --release` (opt s, no `-g`, stripped; fps-shooter 4.5 MB to 1.3 MB). `j3c size` shows sections, code per module and the biggest functions of a dev build. Dev builds (`opt = 0`, `debug`) are for the loop, not for sending to people.
- Wrap anything long or windowed (`j3c editor`, `j3c run` of a game, renders) in `timeout`, so nothing blocks the session.
- Driver shader crashes can hide behind NVIDIA's disk cache: test GLSL with `__GL_SHADER_DISK_CACHE=0`.
- Do not add crates.io dependencies or libLLVM, and do not start a second parser (`j3c/AGENTS.md` has the full list).
