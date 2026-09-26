# Plan 004: Faster scene authoring through MCP and the scene CLI

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, do NOT update `plans/README.md`
> (the reviewer does that).
>
> **Drift check (run first)**: the tree has uncommitted work on top of
> `b086a5b`, so check by comparing the "Current state" excerpts below to the
> live files, not by SHA. On a mismatch, STOP.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW
- **Depends on**: 002 (the stale-server fix; see step 1)
- **Category**: dx / agent tools
- **Planned at**: 2026-09-25

## Why this matters

The village sample (`examples/editor_village`) was built entirely through MCP. Each modular
house took about 30 `scene_add` calls: 4 walls × 2 floors × sides + roof pieces, each with a
position on a 1-unit grid and a rotation in 90° steps. The agent ended up generating call
lists in Python and piping them through a hand-rolled MCP client. There was also no way back
from a bad batch while the editor was closed. EDITOR.md "MCP Fixes" items #7, #8, and #11
cover this.

## Current state

**#7 "scene_add can't start a new scene" is probably already fixed in the code.** The CLI
mode already lets `add` run on a missing file, and `store` writes it:

`j3c/editor/src/tool.j3:1479-1497`
```j3
pub fn cli(request: string, path: string): int {
    let mem = Arena.create(4 * 1024 * 1024);
    let scene = scene3d.read(path, &mem);
    ...
    if !scene.found && !same(request, "check") && !(request.len > 3 && same(cut(request, 0, 4), "add\t")) {
        ... fail "scene file ... does not exist"
    }
    let result = apply(&scene, &catalog, path, request, &out);
    if result.ok && result.changed && !scene3d.store(&scene, path) {
```
The "No such file or directory (os error 2)" the agent saw came from the stale MCP server
(plan 002 #1: `current_exe()` pointing at a deleted binary), not from a missing scene.
Step 1 confirms this.

**One request per tool run.** `scene::run_tool` passes one tab-joined request in `J3_TOOL`
and rejects tabs and newlines inside arguments:

`j3c/src/scene.rs:273-295`
```rust
pub(crate) fn run_tool(root: &Path, rel: &str, args: &[String]) -> Result<Reply, String> {
    if let Some(bad) = args.iter().find(|a| a.contains(['\t', '\n', '\r'])) {
        return Err(format!("argument {bad:?} has a tab or line break"));
    }
    ...
        .env("J3_TOOL", args.join("\t"))
```
MCP `scene_add` builds one `add` request (`j3c/src/mcp.rs:229-238`) and sends it through
`scene_request` (`mcp.rs:323`). That goes to the live editor as a
`cmd <id> <scene> <request>` events line when the editor is open, otherwise to `run_tool`.

**Live edits are one undo step per request.** `j3c/editor/src/live.j3:99-124` (`edit`):
it snapshots the user's pending edit, runs `tool.apply` once, restores on failure, and then
records the change.

**Tool ops.** `j3c/editor/src/tool.j3:60-72`: `list get add set delete check ground ontop scatter`.

**No file-mode history.** Nothing keeps the previous text of a scene before a
CLI/MCP write, apart from the editor's own undo while it's open.

## Design

1. **Batch:** a request may hold several sub-requests separated by the ASCII record
   separator `\x1e` (never valid in names or paths). `tool.apply` gets a `batch` wrapper.
   `cli` and live `edit` split on `\x1e`, apply each in order, and stop at the first
   failure. On failure the whole batch rolls back: cli doesn't store it, and live
   `restore`s the candidate. Replies are concatenated. That gives one file write and one
   undo step.
   - MCP: a new tool `scene_add_many` with `adds: [ {kind, name, parent, fields}, ... ]`
     (the same shape as `scene_add`). Rust turns each item into an `add` request exactly as
     `scene_add` does and joins them with `\x1e`.
   - `run_tool` still rejects `\t\n\r` inside single arguments. `\x1e` is only added by the
     Rust side and is rejected if it shows up in user input.
2. **Grid placement is Rust-only.** A `place` op `grid` with `cell` (default 1),
   `height` (floor height, default = cell), `parent`, and
   `pieces: [ {what, x, z, floor, turn, name} ]`. Rust turns each piece into an `add`
   (`kind instance` for a `.j3scene`, `mesh` + `asset` for a `.j3mesh`/builtin) with
   `pos=[x*cell, floor*height, z*cell]` and `rot=[0, turn*90, 0]`, and sends them as one
   batch. No J3 changes. (The village's house generator in the session did exactly this in
   Python.)
3. **File-mode undo:** before `run_tool` runs a changing op (`add set delete ground ontop
   scatter`, or a batch), Rust copies the scene file to
   `target/j3-editor/history/<rel with / → __>.<unix millis>` and keeps the newest 20 per
   scene. A new MCP tool `undo {scene}` (file mode only; live mode answers "use Ctrl+Z in
   the editor") moves the newest backup back over the scene and reports its time. Put the
   backup code in `scene.rs` next to `run_tool` so the `j3c -e scene` CLI gets it too, plus
   `j3c -e scene undo <scene>`.

Simplification (ponytail): backups are whole-file copies (scenes are small text); no redo.

## Commands you will need

| Command | Purpose |
|---|---|
| `cd j3c && cargo build --release` | build (copies the binary to ~/.bin/j3c, expected) |
| `cd j3c && cargo test --release scene_` | the new Rust tests |
| `timeout 60 ./target/release/j3c -e scene add cube /tmp/x/scenes/new.j3scene --name A` | CLI check in a temp project |
| MCP by hand: `printf '<initialize>\n<tools/call>\n' \| timeout 60 ./target/release/j3c -e mcp --project <dir>` | end-to-end tool calls |

Make a temp project with `timeout 60 ./target/release/j3c new /tmp/j3plan004` (or copy
`examples/3d-editor-scene`), and never launch the editor.

## Scope

In: step 1 verification of #7, batch requests (cli and live), `scene_add_many`, `place` op
`grid`, file-mode backups plus the `undo` tool and CLI, docs (EDITOR.md "Agent use", MCP
tool descriptions, `mcp.rs` USAGE).

Out: redo; batching for `scene_set`/`scene_delete` (the same wrapper would work, so add
later if wanted); rotation other than yaw in `grid`.

## Git workflow

Branch `plan-004-scene-authoring` in the main checkout (the editor sources are untracked,
so a separate worktree won't have them). One commit per step group (verify, batch, grid,
undo, docs). Don't commit to main.

## Steps

1. **Verify #7.** In a temp project, with no `scenes/new.j3scene`:
   `timeout 60 j3c -e scene add cube scenes/new.j3scene --name A` (check `j3c -e scene --help`
   for the exact CLI syntax first).
   Expected: `added #1 A`, and the file exists with `scene.version=1`. Also call MCP
   `scene_add {"scene":"scenes/new2.j3scene","kind":"cube"}` through a fresh server.
   Expected: success. If both work, #7 is done; note it in EDITOR.md "MCP Fixes" #7 as
   "works; the error was the stale server (plan 002)". If either fails, fix `cli`'s guard
   or `scene_rel`, then continue.
2. **Batch in J3.** In `tool.j3`, add `pub fn applyBatch(scene, catalog, path, request, out): Outcome`
   that splits `request` on byte 30, calls `apply` for each part, ORs `changed`, keeps the
   last `focus`/`reload`, and returns `ok=false` at the first failure. Use it in `cli` and
   in `live.j3` `edit` (replace the `tool.apply` call at line 107).
   Verify: a J3 self-check `j3c/examples/scene_batch_check.j3` that reads a temp scene,
   runs a two-add batch and a batch whose second part fails, and asserts the counts
   (the first adds 2 nodes; the second leaves the scene as it was when it's stored the way
   `cli` does). `timeout 60 j3c run --file j3c/examples/scene_batch_check.j3` prints `ok`.
3. **`scene_add_many`.** In `mcp.rs`, factor the request building out of `scene_add` into
   `fn add_request(args: &Json) -> Result<Vec<String>, String>`. The new tool maps `adds`
   through it, joins each request with `\t`, joins requests with `\x1e`, and sends one
   `scene_request`. Put the `\x1e` join in a small helper that `run_tool` accepts: allow
   `\x1e` only when it's joined by Rust, i.e. pass the batch as one pre-joined argument and
   skip the tab check for it. Add the schema to `TOOLS` and the name to the tool list
   (`mcp.rs:770`).
   Verify: `cargo test --release scene_add_many` with a test that serves
   `tools/call scene_add_many` (3 cubes) against a temp copy of a project and then
   `scene_list` shows 3 new nodes (follow the existing `mcp.rs` test at the bottom of the
   file).
4. **`place` op `grid`.** In the `place` handler, if `op == "grid"`, build `add` requests
   from `pieces` as described in Design and send them as a batch. Validate `turn` is an
   integer, `what` is a `.j3scene`/`.j3mesh`/builtin/shape, and at most 2000 pieces.
   Verify: a test placing a 2×2 floor of `builtin:cube` at cell 2 checks positions
   `(0,0,0) (2,0,0) (0,0,2) (2,0,2)` and yaw `90` for `turn: 1`.
5. **Backups plus `undo`.** In `scene.rs`: `fn backup(root, rel)` before changing ops in
   `run_tool` (look at the first arg, or `\x1e`-split parts), plus
   `pub(crate) fn undo(root, rel) -> Result<String, String>`. Add the MCP tool `undo`, and
   `j3c -e scene undo <scene>` in `scene::cmd`. In live mode (`Server::live`), `undo`
   answers with an error telling the agent to ask the person to press Ctrl+Z.
   Verify: `cargo test --release scene_undo`: add a cube (file mode), undo, and the file is
   byte-equal to before. After 25 changes, `history/` holds 20 files for that scene.
6. **Docs.** Update the `scene_add`/`place` descriptions and the new tools in `TOOLS`, the
   server `instructions` text (tell agents to prefer `scene_add_many` and `grid`), EDITOR.md
   "Agent use" and "MCP Fixes" (#7, #8, #11 → done), and `j3c/AGENTS.md`'s `mcp.rs` row.
   Verify: `cargo test --release` all pass.

## Test plan

- Rust tests: `scene_add_many` (step 3), grid positions (step 4), undo round trip and the
  cap (step 5).
- J3 self-check: `scene_batch_check.j3` (step 2).
- Manual (for the user, with the editor open): call `scene_add_many` with 5 cubes. They
  appear at once, and one Ctrl+Z removes all 5.

## Done criteria

- A 30-piece house is one `place grid` call.
- A failed batch leaves the file and the editor unchanged.
- `undo` restores the previous file in file mode.
- All tests pass.

## STOP conditions

- Step 1 shows `add` on a missing file fails for a reason other than plan 002's stale
  server, and the fix touches `scene3d.store` or the file format.
- The live editor can't apply a batch as one undo step without changing `history.j3`
  beyond calling `snapshot` once.
- `\x1e` turns out to reach saved scene text (names) in any path.

## Maintenance notes

- If more tools get batching, reuse `applyBatch`. Don't add per-tool batch ops.
- `grid` is deliberately Rust-only sugar over `add`. Keep geometry logic out of it (no
  bounds or snapping), so it never disagrees with the J3 ops.
- Backups live in `target/`, so they're disposable and never committed.
