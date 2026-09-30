# J3

AOT game language (`j3c`, a Rust crate, no deps) plus a 3D scene editor and an MCP server for agents. Spec wins over examples; examples win over compiler comments. The contract is `j3c/AGENTS.md`; there is no `speculation.md`.

## Where to learn what

Ask the tools first. They ship with the compiler, so they match the binary you run (`j3c version --verbose` says which build and stdlib).

| I need to... | Go to |
|---|---|
| Start a project | `j3c new <name>` (`--app` window, `--template fps` first-person 3D), `j3c guide` |
| Find a stdlib name or module | `j3c doc`, `j3c doc search <word>`, `j3c doc <module> --short` |
| See a complete working program | `j3c doc recipes` (`app2d`, `scene3d`, `canvas`, `script`, `meshgen`, `material`, `urp`, `check`) |
| Learn the language | `j3c doc lang`, then `speculative/01`–`08` (they compile and run) |
| Write UI | `j3c doc j3ui`, `speculative/10_j3ui.j3`, `examples/gallery` |
| Write `.j3s` scripts | `j3c doc recipes script`, `speculative/11_script/`, `examples/scripted` |
| Build or edit 3D scenes | `j3c doc editor`, `j3c/editor/README.md` (controls, file format, import limits) |
| Drive the editor as an agent | see **Agent workflow** below |
| Know what is left to build | `roadmap.md`, `EDITOR.md` (**Still left**, **Agentic use help**), `plans/` |

## Agent workflow for scenes

Scenes are `scenes/*.j3scene` text files (one field per line, `node.<i>.<field>=`). Edit them through the tools, not by hand: the tools keep ids, parents, and undo correct, and `scene3d` is the only reader and writer of the format.

**MCP server.** `claude mcp add -s user j3 -- j3c -e mcp`. It follows the project the editor has open, else the one around the working directory (`--project DIR` pins one). Its startup instructions and every tool description say when to use the tool; replies carry hints when a better tool exists. A project can add its own notes with `notes = "AGENT_NOTES.md"` in `j3.editor.toml` (see `examples/fps-shooter`). After rebuilding `j3c`, run `./build.sh` (installs to `~/.bin/j3c`) and restart the server, since running servers keep the old binary.

**The loop:** change, look, check.

1. Learn the scene: `scene_list` (glob `filter`, `group_by=prefix`), `scene_get`.
2. Build with bulk tools, not many single calls: `place` (`ring`, `wall` with a door, `stairs`, `spiral`, `grid`, `scatter`, `ground`, `ontop`), `scene_add_many`, globs in `scene_set`/`scene_delete`.
3. Look from where the player stands: `render` with `at`/`yaw`/`pitch`, `views` for several angles in one image, `hide` (`Roof*`) to see inside, `view=top` + `ortho` for a plan. A render after every few edits beats one at the end.
4. Verify: `check` (scene rules, the project's `check` script, and code), `scene_stats` (draw cost; above 60 distinct looks is slow), `scene_diff` (review your own edits against git HEAD), `play` (runs the game headless, returns a frame).

The same operations exist on the command line: `j3c -e scene --help`, `j3c -e render --help`, `j3c -e gen`, and `j3c -e mcp --tools` (each MCP tool's arguments next to its CLI twin). Use the CLI when there is no MCP server; nothing else differs.

Facts that cost agents time:
- **Rotations are degrees in the tools and radians in `.j3scene` files.** Anything that writes scene text directly must convert. `check` warns on a stored rotation above two turns. A slab pointing along `(dx, dz)` needs `ry = -atan2(dz, dx)`; yaw 0 looks toward -z.
- With `j3c editor` open, tool edits are live undo steps in it and unsaved until the person saves. With it closed they write the file and `undo` restores it (20 kept). Every reply says which happened.
- Put project rules in a `check` script (`j3.editor.toml`: `check = "tools/check.j3s"`, prints `error: ` / `warning: ` lines), so a mistake becomes a named object instead of a bug found in play. `examples/fps-shooter/tools/check.j3s` is the pattern.
- Objects carry `tags` (`scene_set tags=nocollide,launch=30`; `scene3d.hasTag` / `tagValue`). Prefer them to name prefixes in new game code.

## Repo map

| Path | What |
|---|---|
| `j3c/` | Compiler crate. **Map and contract: `j3c/AGENTS.md`** |
| `j3c/src/scene.rs`, `j3c/src/mcp.rs` | `j3c -e scene`/`render` and the MCP server: thin wrappers that run the editor binary headless |
| `j3c/editor/src/` | The editor program, in J3, one module per file. `tool.j3` is every scene op (`check`, `stats`, `place` helpers, globs); `render.j3` is `-e render`; `live.j3` is commands sent to an open editor. `README.md` is the user guide |
| `j3c/stdlib/j3/` | Stdlib in J3, embedded in the binary (rebuild `j3c` after editing it; `J3_STDLIB` overrides). Top level: `sys`, `io`, `math`, `rand`, `memory`, `option`, `result`, `text`, `app`, `game`, `gfx`, `ui`, `scene3d`, `script` (the `.j3s` VM). Folders: `game/` (`ecs` `input` `anim` `fx` `collide` `space` `physics` `path` `tile`), `gfx/` (`mesh` `meshgen` `voxel` `bvh` `canvas` `noise` `urp/`), `data/` (`save` `assets` `res` `scene` `debug`), `scene3d/` (`fpwalk`, `fx`, `editor/` used only by the editor). A folder is part of the name: `use j3.game.ecs;`. `j3.sys` is `sys.j3` / `sys_windows.j3` / `sys_macos.j3` by `[emit] target` |
| `speculative/` | `01`–`08` compile and run, `09` typechecks `j3.game`, `10_j3ui.j3` is the `@j3ui` sample, `11_script/` is a `.j3s` script (AOT and on the VM) |
| `editors/vscode/` | Cursor/VS Code extension: highlighting (`j3c dump-syntax`), `j3c lsp`, `j3.toml` keys, Run/Build/Format |
| `roadmap.md`, `EDITOR.md` | Direction and status. Editor brainstorms go in `EDITOR.md`, not code |
| `plans/` | Numbered handoff plans (import pipeline, scene authoring, URP performance, animation, effects, ...). Read one before redoing that area |
| `tools/` | `j3mesh_import.py` |
| `build.sh` | Release-build `j3c` and install it to `~/.bin/j3c` |

## Examples

| Path | What it shows | Needs |
|---|---|---|
| `examples/fps-shooter/` | The large one: first-person shooter, biomes as scene files in `scenes/biomes/`, bosses, HUD, save, perks. `tools/arena.j3s` (scene generator), `tools/check.j3s` (project rules), `AGENT_NOTES.md`, `PLAN.md`, `PERFORMANCE.md` | SDL2 + image/mixer/ttf + libGL |
| `examples/3d-editor-scene/` | Smallest editor project: two scenes, a `.j3s` script, `scene3d` in a game | SDL2 + libGL |
| `examples/editor_village/` | Scenes placing scenes (houses in a village), scene props | SDL2 + libGL |
| `examples/gallery/` | UI gallery: widgets, controls, layout, style, inventory | SDL2 + image/mixer/ttf |
| `examples/scripted/` | Headless `.j3s` scripts (fighters, duels, `shared` state); `j3c run` (VM) and `j3c build` (AOT) print the same | nothing |

`j3c run examples/<name>`, `j3c check examples/<name>`, `j3c editor examples/<name>`. A project run uses cwd = package root.

## Working here

- Build and test from `j3c/`: `cargo build --release`, `cargo test`. The editor program is rebuilt per `j3c` version into `~/.cache/j3/editor`; `J3_EDITOR_SRC=$PWD/j3c/editor/src` builds it from disk, so set it while editing `j3c/editor/src/` and unset it for normal use.
- Wrap anything long or windowed (`j3c editor`, `j3c run` of a game, renders) in `timeout`, so nothing blocks the session.
- Driver shader crashes can hide behind NVIDIA's disk cache: test GLSL with `__GL_SHADER_DISK_CACHE=0`.
- Many `j3c/src/compile.rs` tests name examples that no longer exist (`bounce`, `dungeon`, `island`, `minecraft`, `defense`, `raytrace`, ...), so `cargo test` shows failures that are not regressions. Fix or delete them when you touch that file.
- Do not add crates.io dependencies or libLLVM, and do not start a second parser (`j3c/AGENTS.md` has the full list).
