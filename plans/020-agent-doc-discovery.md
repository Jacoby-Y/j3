# Plan 020: Agent-facing documentation and discovery

## Status

- **Status**: DONE (2026-09-26)
- **Priority**: P1
- **Effort**: L
- **Risk**: LOW to MEDIUM (CLI output and lookup behavior; no language change)
- **Category**: documentation / developer experience / agent tools
- **Depends on**: none
- **Planned at**: 2026-09-26

## Outcome

Every row of the inventory, as shipped:

| # | Resolution |
|---|---|
| 1 | `j3c doc j3.scene3d.scripts` in a project builds the module through `scene_scripts::registry` (the compiler's own path), hides generated `find__` helpers, and lists each attached script with its scene lines and object ids. Outside a project it says it needs one. |
| 2 | `j3c doc j3.app App.create`, `j3c doc j3.app.App.create`, and `j3c doc j3.app create` print the same thing. A bare name that matches several items (`j3c doc j3.math length`) lists the qualified choices. |
| 3 | `--short` packs a module's names by type (`j3.game`: 34 lines, was 499); `j3c doc search <word>` lists modules, public names, then comments; `j3c doc <name>` finds a name's module when only one has it. |
| 4 | The index opens with a task table (recipe, then modules), and marks `j3.scene3d.editor.*`, `j3.script`, and `j3.gfx.urp.shaders` as internals. |
| 5 | `j3c doc recipes` ships complete programs in the binary: app2d, scene3d, canvas, script, meshgen (built from the same template code `j3c new`/`j3c editor --new` write), plus material, urp, and check. A test writes each out and runs `j3c check`. `guide` labels repo examples as optional; the scaffolded AGENTS.md points at recipes and search. |
| 6 | `doc --help` advertises `.j3s` queries; the script view shows props with defaults, `shared`, per-instance state, and public calls, each labeled with who can see it. |
| 7 | `j3c -e mcp --tools` prints every tool's arguments from the same JSON `tools/list` sends, with its CLI twin or "MCP only"; a test fails if a tool lacks an entry. Top-level help lists `-e script-props` and `-e mcp --tools`. |
| 8 | `-e scene --help` and `-e mcp --help` explain file versus live editing; every MCP edit reply now ends with which one happened. |
| 9 | `j3c doc recipes check` says what `check`, `-e scene check --all`, `-e render`, and headless `run` each catch, and where outputs land; `guide` and the scene help point at it. |
| 10 | Recipes cover scene load/draw order and lifetimes, canvas upload, meshgen, `.j3mat`/`.j3fx`, and URP; `j3c doc urp` embeds the URP README. |
| 11 | Top-level help: `run --file`, screenshot path resolution, `.j3s` formatting, `J3C_TIMINGS`; `help manifest` lists `J3C_TIMINGS`, `J3_FRAMES`, `J3_SCREENSHOT`; `-e gen --help` explains the import blind spot and `--force`. |
| 12 | Field defaults and const values print as written (`const PI: f32 = 3.14159...`); single-item queries end with `at j3/app.j3:82 (embedded in j3c)` or the real path. The literal pitfall no longer reproduces (`2 * far` is `f32` now; `roadmap.md` item 1 is stale), so `doc lang` documents the case that still fails: an `int` variable needs `f32(n)`. |
| 13 | `j3c version --verbose` prints a docs id (a hash of the embedded stdlib and the Rust files holding the pages), where the stdlib and editor sources come from, and the binary path. The `j3c doc` header shows the id; `guide` says what a mismatch means. |

## Goal

An agent with an installed `j3c` binary and a J3 project should be able to discover the
right API or tool, learn its required call sequence, and verify the result without
searching this compiler checkout. Keep `j3c doc` tied to the parser and the actual
stdlib; avoid a second, manually maintained API catalog.

This is a planning artifact; no implementation was done while writing it. The existing
`j3c guide`, `j3c doc lang`, `j3c doc j3ui`, `j3c doc editor`, module docs,
`j3c help manifest`, and MCP `tools/list` are useful starting points. Prefer
connecting and improving them over duplicating their content.

## Current state and gap inventory

The observations below come from the current source and a read-only run of
`j3c/target/debug/j3c` on 2026-09-26. That binary is behind the working tree (see #13),
so recheck behavior with the executable being changed before implementing a fix.

| # | Gap for an agent | Evidence / current location | Desired result |
|---|---|---|---|
| 1 | The generated `j3.scene3d.scripts` module has no `j3c doc` page. `j3c doc j3.scene3d.scripts` fails, even though games call its `scene`, `start`, and `update` functions and `scene.script::<T>`. | `j3c/src/compile/scene_scripts.rs`; `j3c/editor/README.md` Scripts section; `j3c/src/doc.rs` only resolves stdlib files | From a project root, `doc` shows the generated module's actual signatures and links each generated entry to the script or scene that caused it. |
| 2 | Item lookup rejects the qualified name it prints: `j3c doc j3.app App.create` fails while `j3c doc j3.app create` works. A bare name can also match members on several types. | `j3c/src/doc.rs` `split_query`, `render_struct`, `keep` | Accept `Type.member` and explain ambiguous bare names with qualified choices. Preserve working bare-name queries. |
| 3 | Large API pages have no short view or search. The observed binary prints about 499 lines for `j3.game`, 344 for `j3.scene3d`, and 350 for `j3.gfx`. | `j3c/src/doc.rs` `index` and `render` | Add a concise module overview and searchable public symbol index, with a path to full signatures and comments. |
| 4 | The module index is flat and includes implementation-oriented modules alongside game APIs. It does not guide tasks such as building a 2D game, drawing a scene, making a texture, or adding a script. | `j3c/src/doc.rs` `index`; `j3c/src/stdlib/mod.rs` | Give task-oriented routes to `app`, `game`, `ui`, `scene3d`, `gfx`, `canvas`, `meshgen`, data modules, and optional URP modules. Label editor/runtime internals without hiding valid public APIs. |
| 5 | `j3c guide` sends readers to repo examples, while generated project `AGENTS.md` says to learn J3 from the installed `j3c` and not search other projects. An installed binary need not have this repo's examples. | `j3c/src/helptext.rs` examples list; `j3c/src/scaffold.rs` generated `AGENTS` | Put minimal, runnable examples or complete recipes in binary-accessible docs; treat repo examples as optional deeper references. |
| 6 | `j3c doc path/to/file.j3s` exists in source but is absent from `doc --help`. Its output lists props and public functions, without orienting an author to `shared` and per-instance state. | `j3c/src/doc.rs` `cmd` and `script_doc` | Advertise script-file queries and show a useful script overview, clearly marking state as internal rather than externally callable. |
| 7 | The agent tooling surface is split across `j3c help`, `-e <tool> --help`, MCP `tools/list`, `j3c doc editor`, and `EDITOR.md`. Top-level help omits `-e script-props`; CLI readers do not get the arguments for `scene_add_many`, `place grid`, asset search/fetch, and other MCP tools. | `j3c/src/cli.rs`, `j3c/src/mcp.rs`, `j3c/src/scene.rs`, `EDITOR.md` Agent use | Give a clear tool inventory and point to exact argument schemas/examples from the tool's source of truth. Avoid copying MCP schemas into a second hand-maintained list. |
| 8 | The save/undo meaning changes with editor state. Live scene edits are unsaved undo steps; closed scenes are file edits with separate file undo. This is easy to miss when entering through scene command help. | `EDITOR.md` Agent use; `j3c/src/mcp.rs` `INSTRUCTIONS`; `j3c/src/scene.rs` `USAGE` | Put this rule next to scene/MCP command help and give agents a way to tell which mode a call used. |
| 9 | There is no compact validation path saying when to use `check`, `-e scene check --all`, `-e render`, and headless `run`. Each catches a different class of problem. | `j3c/src/helptext.rs`, `j3c/src/doc.rs` editor page, `j3c/src/scene.rs` | Add a short edit/check/render/play loop, including what each check covers and where output goes. |
| 10 | Multi-call graphics and asset workflows are scattered. URP pass order is in `j3c/stdlib/j3/gfx/urp/README.md`; scene integration and `.j3s` attachment are in the editor README; canvas color types and texture lifetime are clearest in generated `AGENTS.md`. | The cited README files and `j3c/src/scaffold.rs` | Make short recipes reachable from `doc` for scene loading/drawing, canvas upload, mesh generation, material/effect files, and URP pass ordering. Keep long guides where they are. |
| 11 | Operational CLI behavior is hard to find. `run --file`, `.j3s` formatting, screenshot path resolution, and `J3C_TIMINGS` are not all visible from top-level help. `-e gen` does not track changes in other project modules imported by a generator; `--force` is the recovery path. | `j3c/src/cli.rs` `take_run_flags`/`fmt_cmd`; `j3c/src/gen.rs` `run_all`; `j3c/editor/README.md` | Add the flags, environment variables, output locations, and generator cache limitation to the relevant command help. |
| 12 | Some language and API facts require reading the roadmap or implementation. Example: for `f32`, `far * 2` can typecheck while `2 * far` may need an explicit `f32(2)` because the left literal is typed first. Item docs print `= ...` for defaults, omit public constant values, and have no source location. | `roadmap.md` Language features; `j3c/src/doc.rs` `lang_page` and `render_item` | Add concrete language pitfalls and fixes. Show useful defaults/constants and source provenance where available, without promising a filesystem path for embedded-only sources. |
| 13 | The binary's embedded docs can lag the working tree. In this checkout, `target/debug/j3c doc --help` says “behaviors” while current `j3c/src/doc.rs` says “scripts.” `j3c version` is only `0.0.1`. | `j3c/src/stdlib/mod.rs` embeds stdlib; `j3c/src/doc.rs` embeds editor guide; `j3c/src/cli.rs` version | Show a build/docs identifier and whether `J3_STDLIB` overrides embedded sources, so an agent can recognize and report version drift. |

## Implementation phases

### A. Find the right topic quickly (gaps 3–5)

- Make `j3c doc` a task-oriented index as well as a module index. Keep module names
  searchable and preserve existing `j3c doc <module>` behavior.
- Provide a short view of a module's public symbols, organized by type or role, and a
  way to request the full output. A query should find a public symbol without the agent
  first guessing its module. Exclude private declarations from results.
- Put at least one complete, binary-accessible recipe each for a windowed 2D app, a
  scene3d game, a headless canvas asset generator, and a `.j3s` script. Examples in
  this repo may remain as extended references.

**Verify:** Starting only from `j3c help` and `j3c doc`, find the right command and API
for each of those four tasks. The short `j3.game` view should fit on a typical terminal
screen or give a clearly labeled continuation path; full output remains available.

### B. Make API lookup match J3 usage (gaps 1, 2, 6, 12)

- Accept qualified method queries such as `j3c doc j3.app App.create`; retain
  `j3c doc j3.app create`. If several types have `create`, return the choices rather
  than a misleading undifferentiated excerpt.
- When run in a project, resolve project and generated modules through the existing
  compiler analysis path. In particular, document `j3.scene3d.scripts` from the
  actual project scenes/scripts. Report clearly when a query needs a project context.
- Advertise `j3c doc <path.j3s>`. Show props, public calls, `shared` values, and
  per-instance state with visibility labels. Do not imply that outside code can read
  script fields.
- Display literal field defaults and public constant values where the parser can
  represent them safely. Show source module and line when available. If the source is
  embedded in the binary, identify it as embedded instead of printing a nonexistent
  local path.

**Verify:** `App.create` resolves to the same method as `create`; an ambiguous bare
name lists qualified alternatives; `j3c doc j3.scene3d.scripts` works in an editor
project and explains missing project context elsewhere; `doc scripts/spin.j3s` is
advertised and distinguishes public calls from internal state.

### C. Expose the agent tool contract (gaps 7–9, 11)

- Update top-level and subcommand help so an agent discovers `-e script-props`,
  `run --file`, `fmt` on `.j3s`, headless screenshots, and `J3C_TIMINGS`.
- Offer an inspectable reference for MCP tool names and arguments using the same
  definitions as `tools/list`. Include `scene_add_many`, `place grid`, undo,
  `asset_online_search`, and `asset_fetch`. Put CLI equivalents beside them where
  they exist; identify MCP-only operations.
- Explain live-editor versus file-mode save/undo behavior in `-e scene --help` and
  `-e mcp --help`, and report the mode in a tool response if it is not already clear.
- Add a short validation sequence: `j3c check`, `j3c -e scene check --all`,
  `j3c -e render`, then `j3c run --frames ... --screenshot ... --headless`.
  State which files and failures each step can inspect. Document where screenshot
  paths resolve and where editor build logs land.
- Explain `-e gen` cache inputs and `--force` when a generator's imported project
  module changes.

**Verify:** A reader of CLI help can discover every named tool and flag without
opening Rust source. Compare the documented MCP arguments with `tools/list` so the
two cannot quietly drift apart.

### D. Add short workflow guidance (gaps 5, 9, 10, 12)

- Link concise recipes from the relevant `doc` pages. Cover the call order and
  ownership/lifetime constraints for scene loading and drawing, canvas upload,
  mesh generation, `.j3mat`/`.j3fx` use, and URP rendering. Point to longer
  editor/URP guides for format details instead of pasting them into every API page.
- Add the left-literal `f32` example and fix to `doc lang`. Check other recurring
  errors against the language contract and examples before adding more advice.
- Review generated project `AGENTS.md`, `guide`, and the new recipes together so
  an installed-binary user is not told to find repo-only examples.

**Verify:** For each recipe, follow only its commands and snippets in a fresh
project (or a minimal fixture) and run the relevant `check`/render command.
Examples must use APIs and syntax from the current compiler.

### E. Prove the docs belong to this binary (gap 13)

- Show a stable build/docs identifier through `j3c version` or a dedicated verbose
  form. Show whether stdlib docs come from embedded files or `J3_STDLIB`.
- Make the guide's version/provenance advice explicit for repo contributors who
  have a stale `target/debug/j3c` or a different executable on `PATH`.
- Keep normal command output quiet and scriptable; provenance should be requested
  explicitly or appear only in an index header where it will not break parsers.

**Verify:** With and without `J3_STDLIB`, the reported documentation source is
correct. A stale binary is identifiable without inspecting its Rust source.

## Completion criteria

- Every inventory row has either a verified fix or a recorded reason it was
  deferred. A generic paragraph added to one README does not close a CLI
  discoverability gap.
- Existing documented invocations still work. New `doc` queries use the same parser,
  module resolver, and visibility rules as compilation; do not create a second
  frontend or hand-copy public signatures.
- Add focused tests for query resolution, ambiguous names, generated module docs,
  and help/discovery text. Test real command output where the gap is about CLI use.
- Review docs with two perspectives: an agent in this checkout, and an agent with
  only an installed `j3c` plus a freshly scaffolded project.

## Drift and execution notes

- This tree has existing uncommitted changes, including script and editor work.
  Recheck the current command output and source before implementing each phase;
  do not reset or overwrite those changes.
- `cargo build`/`cargo test` in `j3c/` copy the binary to `~/.bin/j3c` through
  `.cargo/copy-j3c.sh`. Account for that when validating version/provenance and
  any running MCP server.
- The stdlib module list in `j3c/src/stdlib/mod.rs` embeds source files in the
  binary. `J3_STDLIB` overrides those files; specialized `doc lang`/`doc editor`
  pages are Rust-side text. Check both paths when adding doc output.
