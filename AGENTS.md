# J3

AOT game language (`j3c`, written in J3 in `j3c-bootstrap/`, no deps) plus a 3D scene editor and an MCP server for agents. Spec wins over examples; examples win over compiler comments. The contract is `j3c-bootstrap/AGENTS.md`; there is no `speculation.md`.

NOTE: Always run ./build.sh when finishing implementing something (only after confirming the build won't be broken)

## Where to learn what

Ask the tools first. They ship with the compiler, so they match the binary you run (`j3c version --verbose`).

| I need to... | Go to |
|---|---|
| **Anything stdlib** (a module, an OS feature, a dialog, hotkeys, clipboard, files, processes, http, TOML, threads...) | `j3c doc search <word>`, then `j3c doc <module> --short`. Big guide pages (`css`, `j3ui`, `editor`) read by section: `j3c doc css --short` |
| Start a project | `j3c new <name>` (`--app`, `--lib`, `--template fps`/`ui`), `j3c guide` |
| See a complete working program | `j3c doc recipes` |
| Learn the language | `j3c doc lang`, then `speculative/01`–`08`, then `speculative/lang/` (one file per feature) |
| Packages, shipping, Android, Steam | `j3c doc packages`, `j3c doc ship`, `j3c doc android`, `j3c doc steam` |
| Write UI | `j3c doc j3ui`, `j3c doc css`, `examples/desk-app`, `examples/big_ui_test` |
| Write a shader | `j3c doc shaders`, `j3c dump-glsl` |
| Scripts / mods | `.j3s`: `j3c doc recipes script`; runtime j2 mods: `j3c doc j2` |
| **Run, screenshot, or test anything windowed** | `j3c doc testing`. **Never open a window:** `j3c run --frames N --screenshot f.png --headless`; a built binary needs `SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy`. Goldens: `j3c golden <dir>`. Wrap in `timeout` |
| Build or edit 3D scenes | `j3c doc editor`, `j3c/editor/README.md`, and **Scenes** below |
| Know what is left to build | `roadmap.md`, `EDITOR.md`, `plans/` |

## Scenes

Scenes are `scenes/*.j3scene` text files, one object per line (`Name kind key=value #id`, indent for children; format in `j3c/editor/README.md`). Edit them through the tools, not by hand: `j3c -e scene --help`, `j3c -e render --help`, `j3c -e gen` (the MCP server, `j3c -e mcp`, is a stub until `plans/06-deferred-tools.md`).

Loop: change (bulk ops: `place`, `apply`, globs), look (`render` from where the player stands, often), check (`check`, `stats`, `diff`).

- Rotations are **degrees**. A slab along `(dx, dz)` needs `rot` y `= -atan2(dz, dx)`; yaw 0 looks toward -z.
- With `j3c editor` open, tool edits are unsaved undo steps in it; closed, they write the file (`undo` restores, 20 kept).
- Project rules go in a `check` script (`j3.editor.toml`: `check = "tools/check.j3s"`, prints `error: ` / `warning: `).
- Prefer `tags` (`scene3d.hasTag` / `tagValue`) over name prefixes. `tags=decal` fixes z-fighting the renderer's own scan misses.

## Repo map

| Path | What |
|---|---|
| `j3c-bootstrap/` | The compiler. **Map and contract: `j3c-bootstrap/AGENTS.md`**. Gates: `tests/diff.sh`, `tests/unit.sh`; embedded docs/help/templates in `text/` |
| `j3c/` | What the compiler embeds: `stdlib/`, `editor/`, fixtures. **Map: `j3c/AGENTS.md`**. Rust compiler is at tag `rust-final` |
| `j3c/editor/src/` | The editor, in J3 (`tool.j3` + `tool/` = every scene op) |
| `speculative/` | Language samples that compile and run |
| `examples/` | `3d-editor-scene`, `editor_village` (scenes); `desk-app`, `big_ui_test` (UI); `j2_script` (mods); `terminal`, `openrouter-chat`, `scripted`. `j3c run examples/<name>` |
| `tools/` | `voxel-editor/`, `composer/` (each has an `AGENTS.md`), `bench2d/`, import/migrate scripts |
| `editors/vscode/`, `docs/` | Editor extension; the J3 Book (humans; agents use `j3c doc`) |
| `plans/` | Open handoff plans only (`plans/README.md`) |

## Working here

- Build the compiler: `J3_STDLIB=$PWD/j3c/stdlib j3c build --opt 2 j3c-bootstrap` (writes `j3c-bootstrap/target/j3c`, installs nothing). Tests: `j3c-bootstrap/tests/unit.sh`, `tests/diff.sh MODE` (`j3c-bootstrap/tests/README.md`). No cargo.
- `./build.sh` builds stage 1 and 2 from a seed (installed `j3c`, its `j3c.prev`, or `seed/VERSION`) and installs to `$J3_BIN_DIR` / `~/.bin` / `~/.local/bin`. `J3C_NO_INSTALL=1` skips installing. Restart MCP/LSP after.
- Stdlib edits need a `j3c` rebuild (or `J3_STDLIB`). Editor edits: set `J3_EDITOR_SRC=$PWD/j3c/editor/src`, unset after.
- Repo scripts use half the cores via `J3_JOBS`.
- Wrap anything long or windowed in `timeout`.
- Test GLSL with `__GL_SHADER_DISK_CACHE=0` (NVIDIA cache hides driver crashes).
- Do not add dependencies to the compiler, use libLLVM, or start a second parser (`j3c-bootstrap/AGENTS.md` has the full list).
