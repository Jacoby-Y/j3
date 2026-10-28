# Plan 011: Editor extension API

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

- **Priority**: P3
- **Effort**: L (4 phases, each shippable)
- **Risk**: MED (per-project editor builds, compile failure fallback)
- **Depends on**: 002 (`host_exe()` helper for rebuilt binaries) is nice to have, not required
- **Category**: feature
- **Planned at**: 2026-09-25

## Why this matters

Projects can't add tools to the editor. A game that wants a "Scatter trees on this island"
menu entry, a custom inspector for its `Door` behavior, or a gizmo that shows a trigger's
radius has to edit j3c's embedded editor sources. EDITOR.md "Still left > Editor extension
API" asks for per-project extensions. A design was agreed with the user (see Design; two
open decisions are marked there).

## Current state

**One shared editor build per j3c, stdlib, and frontend source.**
`j3c/src/editor.rs:910-960` (`frontend`):
```rust
pub(crate) fn frontend(host: &Path) -> Result<(PathBuf, fs::File), String> {
    let sources: Vec<(String, String)> = match std::env::var("J3_EDITOR_SRC") {
        Ok(dir) if !dir.is_empty() => { /* read *.j3 from dir */ }
        _ => FRONTEND.iter().map(|(n, s)| (n.to_string(), s.to_string())).collect(),
    };
    let mut hasher = DefaultHasher::new();
    sources.hash(&mut hasher);
    ... (host exe len and mtime, stdlib dir when overridden)
    let key = format!("{:016x}", hasher.finish());
    let dir = cache.join(&key);
    ... writes sources into <tmp>/src/, font.ttf, a j3.toml (package j3_editor), runs `j3c build`
```
`FRONTEND` is the embedded source list (`editor.rs:16`). Callers: `launch` (the editor
window), `scene::run_tool` (CLI/MCP), and render. None pass a project.

**Generated code pattern to copy.** `editor.rs:767-800` (`generate_dispatch`) writes a
project's `src/behaviors.j3` with a `same(name, "...")` chain per behavior. Extension hooks
use the same approach inside the editor build.

**Where hooks would plug in:**
- Right-click menu: `j3c/editor/src/menu.j3:61` `buildMenu(w)`, rows added with
  `item(w, label, keys, cmd, on)` (`menu.j3:20`); commands are ints run by
  `commands.j3` `run(w, cmd)`.
- Inspector: `j3c/editor/src/ui_inspector.j3:89` `behaviorFields(ui, w, n)` draws a node's
  behavior fields from the catalog; `inspector` at `:198`.
- Viewport overlay: `j3c/editor/src/viewport.j3:146` `drawViewport(w)`, with helpers
  `drawBox` (`:56`) and `drawFrustum` (`:74`).
- Scene edits with undo: `j3c/editor/src/live.j3:99-124` (`edit`) runs one `tool.apply`
  request as one undo step. Extension edits can reuse it.
- Console: `j3_editor.console.say(w, text)`.

## Design

**Files.** A project's `editor/*.j3`. The host copies each into the editor build as
`src/ext_<stem>.j3`. Module-path rule: check in step 1 whether a file in package
`j3_editor` must declare `module j3_editor.<x>`. If so, the host rewrites the file's first
`module ...;` line to `module j3_editor.ext_<stem>;`, so authors write
`module editor.<stem>;` and never think about it. Extension files may `use j3.*` and the
editor API module only, not the game's own modules (out of scope: that would pull game code
into the editor build).

**Hooks** (open decision 1: naming convention vs attribute; **recommended: naming
convention**, which needs no compiler change). Public functions with these shapes:
- `pub fn menu_<Name>(ed: &Editor)`: a right-click menu row "<Name with _ as space>"
  under an "Extensions" separator.
- `pub fn inspect_<Behavior>(ui: &Ui, ed: &Editor, id: int)`: drawn under that behavior's
  fields in the inspector.
- `pub fn draw_<Behavior>(ed: &Editor, id: int)`: called in `drawViewport` for each node
  with that behavior, after the scene and before gizmos.

The host finds them with a line scan for `pub fn menu_`/`inspect_`/`draw_` (like
`behavior_names` does for toml; a missing or odd signature is a compile error the author
sees in the console) and generates `src/ext_hooks.j3` with:
- `pub fn menuCount(): int`, `menuName(i): string`, and `runMenu(ed, i)`
- `pub fn inspect(ui, ed, behavior: string, id)`
- `pub fn draw(ed, behavior, id)`

each a `same(...)` chain. The stock build (no `editor/` folder) gets the same file with
empty bodies, so the editor always calls `ext_hooks` unconditionally.

**API** (`src/ext_api.j3`, module `j3_editor.api`, embedded in FRONTEND). A small stable
surface over `World`:
- `struct Editor { w: &World }`
- `ed.scene(): &Scene`, `ed.selected(): int`, `ed.select(id)`
- `ed.run(request: string): bool`: a `tool.apply` request string like the CLI's
  (`add cube name=A pos=1,0,2`), applied through `live.edit` as one undo step
- `ed.say(text)`
- `ed.box(model, lo, hi, color)` / `ed.line(a, b, color)` for gizmos
- `ed.number(id, field, fallback)` and friends, forwarding to scene3d

Extensions never touch `World` directly.

**Per-project build.** `frontend(host, project: Option<&Path>)`. With a project that has
`editor/*.j3`, the extension texts (with their names) and the generated hooks file go into
`sources` before hashing, so each project gets its own cache key; without one, the key is
unchanged from today. `launch` passes the project; `run_tool` and render pass `None`
(headless tools don't need extensions, which keeps them fast and safe).

**Failure fallback.** If the build with extensions fails, `launch` builds or reuses the
stock editor and then appends the compiler errors to `target/j3-editor/events` as `build`
lines, which the editor shows in its console (see `host.j3` "build" kind). The editor
always opens.

**Reload** (open decision 2: restart vs hot rebuild; **recommended: restart for v1**).
RESCAN (`editor.rs`, where `J3EDITOR RESCAN` is handled) already runs on file changes. When
it sees `editor/*.j3` changed, it emits an event and the editor shows "Extensions changed:
save and reopen the editor to load them". Hot rebuild with tab restore is later.

## Commands you will need

| Command | Purpose |
|---|---|
| `cd j3c && cargo build --release && cargo test --release` | build and tests |
| `J3_EDITOR_SRC=editor/src timeout 120 ./target/release/j3c -e scene list <proj>` | builds the editor from disk sources without a window |
| `timeout 120 ./target/release/j3c check <tmp editor build dir>` | typecheck a generated editor build |

Never launch the editor window; the user does the GUI checks at the end.

## Scope

In: phases 1-4 below. Out: game modules inside extensions, hot reload of extensions, new
compiler attributes, and extension-defined MCP tools (a good follow-up: `menu_` hooks could
become MCP tools).

## Git workflow

Branch `plan-011-editor-extensions` in the main checkout (the editor sources are
untracked). One commit per phase.

## Steps

1. **Module rule and hook scan.** Read how `compile.rs`/`project.rs` map files to module
   paths and whether a package prefix is enforced. Write `fn scan_hooks(text) -> Vec<(kind, name)>`
   and `fn hooks_source(exts) -> String` in `editor.rs`.
   Verify: a Rust unit test on a sample extension text gives the expected hook list and
   generated source, and the stock (empty) generated source matches a golden string.
2. **Phase 1: per-project build plus fallback.** Add the `project` parameter to
   `frontend`, the source injection, and the module-line rewrite. Add the empty
   `ext_hooks.j3` and `ext_api.j3` to FRONTEND and call `ext_hooks` from `buildMenu`,
   `inspector`, and `drawViewport`. Fall back to stock on failure.
   Verify:
   - `cargo test --release editor_ext` builds the frontend for a temp project with one
     `editor/tools.j3` containing a `menu_Hello` that calls `ed.say("hi")`. The build
     succeeds and the cache key differs from the stock key.
   - The same test with a syntax error in `tools.j3` returns the stock build plus the error
     text.
   - Keep these tests `#[ignore]` if they need clang, and run them explicitly with
     `timeout`.
3. **Phase 2: API plus menu hooks.** Implement `j3_editor.api` (Design) and the menu rows
   (an "Extensions" separator, then a row per `menuName(i)`, commands after the built-in
   ones).
   Verify: a J3 check. A test-only `J3_EDITOR_MODE=cli` request `ext menu <i>` (add it in
   `tool.j3`'s cli, or a new `J3_EDITOR_MODE=ext` path) runs a menu hook headlessly on a
   scene. A hook doing `ed.run("add cube name=Ext")` leaves a scene with node `Ext`.
4. **Phase 3: inspector and gizmo hooks.**
   Verify: builds with an extension defining `inspect_Door` and `draw_Door`. Add a headless
   check that `ext_hooks.inspect`/`draw` dispatch by behavior name (a J3 self-check over
   the generated chain with a stub).
5. **Phase 4: reload message.** RESCAN detects `editor/*.j3` changes (hash the files) and
   sends the event; the editor shows the message.
   Verify: a unit test on the hash/compare helper.
6. **Docs.** `j3c/editor/README.md` gets an "Extensions" section with a full example file.
   Update EDITOR.md (move the item to Implemented) and `j3c/AGENTS.md`'s editor row.
7. **Manual checks for the user** (list them in the final report; don't run them): open a
   project with `editor/tools.j3`, right-click, run the Extensions > Hello row, and see
   "hi" in the console. A Door inspector panel shows. Break the file, reopen, and the
   editor opens with the error in the console.

## Test plan

Rust unit tests for scan/generate/key (steps 1-2), an ignored-by-default build test, J3
headless checks for menu and dispatch (steps 3-4), and the manual GUI list (step 7).

## Done criteria

- A project's `editor/*.j3` hooks show up in the editor.
- A broken extension never blocks the editor from opening.
- Projects without `editor/` build exactly as before (same cache key).

## STOP conditions

- The module path rule can't be satisfied by rewriting the `module` line (for example, the
  compiler ties module paths to directory layout in a way that rejects `src/ext_*.j3`).
- `@j3ui` or `Ui` can't be passed into hook functions from generated code.
- Either open decision is overridden by the user: follow their choice. If the attribute
  route is chosen, STOP before step 1 and re-plan the compiler part.

## Maintenance notes

- The API module is the compatibility promise. Add to it; don't change signatures.
- Every new hook kind needs: the scan prefix, the generated dispatch, the call site in the
  editor, and a README example.
