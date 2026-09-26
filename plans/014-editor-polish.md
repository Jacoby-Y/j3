# Plan 014: Editor polish (editing gaps, UI polish, editor speed)

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
- **Effort**: M (8 small independent phases)
- **Risk**: LOW per phase
- **Depends on**: none. Phase H overlaps plan 007 step 7; do it in whichever plan runs first and mark it in the other.
- **Category**: editor UX
- **Planned at**: 2026-09-25

## Why this matters

EDITOR.md "Still left > Editing gaps / UI polish" and "Performance > Editor" list small
things that each cost a little every session. They're independent, so each phase is one
commit that can ship alone.

## Current state (per phase)

**A. Undo keeps only one selected object.** `j3c/editor/src/types.j3:102-109`:
```j3
pub struct Snap {
    nodes: []Node;
    len: int;
    next: int;
    data: Save;
    /// Selected id when taken.
    selected: int;
}
```
Multi-selection lives in `World.picked`/`pickedCount`. `history.j3` copies only
`selected` (`:54`, `:92`, `:141`, `:170`), so after undo the other picked objects drop out
of the selection.

**B. Box select tests origins.** `j3c/editor/src/input.j3:332-350` (`boxSelect`):
```j3
let p = gfx.toScreen(matrices[i].transformPoint(Vec3.zero()));
if p.z > 0 && p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y && !isPicked(w, n.id) {
```

**C. Overrides only reach the placed file's own objects; duplicate drops overrides.**
`scene3d.j3` `graft` (`:1495`) applies overrides keyed by `source` (the id in the placed
file; `Node.source` is 0 for objects nested deeper). Duplicate is in
`j3c/editor/src/commands.j3`/`nodes.j3`: find the duplicate command and whether it copies
`over.<id>.*` lines. EDITOR.md says it doesn't.

**D. Prop rename.** `j3c/editor/src/ui_props.j3:30-84` has add (`declareProp`), a type
toggle, a default field, and remove (`dropProp`); there's no rename. Values live in outer
scenes as `over.<copyId>.prop.<name>` lines, and `scene3d.uses(file, target)`
(`scene3d.j3:1957`) finds scenes that place a file.

**E. No "Place on top of..." or scatter UI.** The ops exist: `tool.j3:70-72`
(`groundOp`, `ontopOp`, `scatterOp`). The right-click menu is built in `menu.j3:61`
`buildMenu`, and dialogs live in `ui_dialogs.j3`.

**F. Camera preview assumes 16:9.** `j3c/editor/src/cameras.j3:99-102`:
```j3
    // ponytail: a 16:9 game window; read j3.toml's window size if games differ.
    *size = Vec2 { x: wide, y: wide * 9 / 16 };
```
The frustum drawing also uses 16/9 (`viewport.j3:78` `let hx = hy * 16 / 9;`).

**G. Material tabs don't autosave.** `j3c/editor/src/files.j3:138-146` (`autosave`)
skips material tabs: `if w.tabs[i].dirty && w.tabs[i].mat as ptr == 0 { ... }`.

**H. Editor speed.** DONE in plan 007 step 7 (mtime check in `watch.j3`, cached per-mesh BVH in
`hitTriangles`, which `place` uses too). Skip this phase.
- `watch.j3:24-45` reads every watched file each second and compares text.
- Picking (`gizmo.j3:198-213`) and `place` (`tool.j3:1187`) use
  `scene3d.hitTriangles`, a linear scan (`scene3d.j3:660`, ponytail note).
- `place` skips the box test (it calls `hitTriangles` directly, not `hitWith`).

**I. Color picker square is a grid of cells.** `j3c/editor/src/picker.j3:215-216`:
```j3
    // Saturation across, value down, as a grid of cells.
    // ponytail: cell grid; switch to a generated texture if the banding shows.
```

## Design (smallest per phase)

- **A:** add `picked: []int` and `pickedCount: int` to `Snap` (copied in the 4 places
  that copy `selected`), restored with `addPick` for ids that still exist.
- **B:** project the 8 corners of the node's world AABB (`boundLo/Hi` through
  `matrices[i]`) and select if the screen rect of those overlaps the box. Groups, cameras,
  and lights keep origin testing (no bounds).
- **C (two parts):**
  - Duplicate copies the source node's `over.<id>.*` lines to the new id (a small helper
    over `scene.data` lines, like `watch.j3`'s line helpers).
  - Deeper overrides: key them by a path of sources (`over.<id>.<src1>/<src2>.<field>`).
    This is the one real design change. Keep it last in this plan, and STOP for review if
    `graft` needs more than passing the source chain down.
- **D:** a rename button beside each prop. `scene3d.renameProp(scene, old, new)` renames
  the declaration. Then, for every `.j3scene` in the asset list whose file `uses` this
  scene, rewrite its `over.*.prop.<old>` keys: open tabs through their scene, other files
  through read, change, `store`. Say how many files changed.
- **E:** right-click row "Place on top of..." enters a pick mode (next click on an object
  runs `ontop selected target` through the same path as live MCP edits, `live.edit`). "+
  Add > Scatter..." opens a small dialog (what, count, area from the current view's
  ground rect, seed) that runs `scatterOp`.
- **F:** the host passes the game window size from `j3.toml`/the App create call as env
  `J3_EDITOR_GAME_SIZE=WxH`. If it can't be found cheaply, read the `App<...>.create("...", W, H, ...)`
  numbers from `src/main.j3` with a line scan (ponytail) and fall back to 16:9. Use the
  aspect in `cameras.j3` and `viewport.j3:78`.
- **G:** material tabs autosave to `target/j3-editor/autosave.<path>` the same way, and
  are restored on open like scene tabs (`files.j3:153` pattern).
- **H:** mtime first in `watch.j3` (add `mtime(path): int` to `util.j3` via `stat`; plan
  013 later moves it behind the stdlib); `place` goes through `hitWith`; a cached per-mesh
  `Bvh` for meshes over 256 triangles in `hitWith`.
- **I:** bake the saturation/value square into a 64×64 texture when the hue changes
  (`gfx.tex` + `updateTex`), and draw it with one textured quad.

## Commands you will need

| Command | Purpose |
|---|---|
| `cd j3c && cargo build --release` | rebuild j3c (embeds the editor sources) |
| `J3_EDITOR_SRC=editor/src timeout 120 ./target/release/j3c -e scene list ../examples/3d-editor-scene` | typechecks and builds the editor from disk without a window |
| `timeout 60 ./target/release/j3c run --file examples/scene3d_check.j3` | scene3d self-check |
| `timeout 120 ./target/release/j3c -e render ../examples/3d-editor-scene --out /tmp/x.png` | render regression |

The executor can't open the editor window. Each phase lists a manual check for the user.

## Scope

In: phases A-I. Out: anything in plans 007 (draw performance) or 011 (extensions).

## Git workflow

Branch `plan-014-editor-polish` in the main checkout (editor sources are untracked). One
commit per phase, in the order A, B, F, G, H, I, E, D, C (smallest and safest first).

## Steps

1. **A: undo keeps the multi-selection.** Verify: the editor builds (the `J3_EDITOR_SRC`
   command succeeds). Manual: select 3 cubes, move them, Ctrl+Z; all 3 stay selected.
2. **B: box select by bounds.** Put the corner projection in a pure helper
   `boxHits(lo, hi, corners)`. Verify: a J3 self-check `j3c/examples/boxselect_check.j3`
   on that helper (a rect overlapping only a node's corner selects it; one fully outside
   doesn't). Manual: drag a box over part of a big plane, and it's selected.
3. **F: camera preview aspect.** Verify: `-e render --camera Camera` still works. Manual:
   a 4:3 game shows a 4:3 preview.
4. **G: material autosave.** Verify: the build succeeds. Manual: edit a material, kill the
   editor, reopen, and the edit is offered or restored like scenes.
5. **H: editor speed.** Verify: MCP/CLI `place ground` gives the same answer before and
   after on `examples/3d-editor-scene` (compare `j3c -e scene get` output). Hot reload
   still picks up an outside edit (manual).
6. **I: smooth picker.** Manual: the square is a smooth gradient and picking still gives
   the same color at the same spot.
7. **E: Place on top of / Scatter UI.** Verify: build. Manual: right-click cube → Place on
   top of... → click the plane, and the cube rests on it as one undo step. Scatter dialog
   makes N copies as one undo step.
8. **D: prop rename.** Add `scene3d.renameProp`. Verify: a J3 self-check with a scene that
   declares `speed`, an outer scene with `over.5.prop.speed=2`, rename to `rate`, and both
   files now say `rate` with the value kept.
9. **C: overrides.** First the duplicate-copies-overrides part (verify with a self-check
   on the line-copy helper). Then STOP and report the design for nested override keys
   before implementing it (see STOP conditions).
10. **Docs.** Remove the done items from EDITOR.md "Editing gaps"/"UI polish"/"Performance
    > Editor", and update `j3c/editor/README.md` for Place on top, Scatter, and prop rename.

## Test plan

Self-checks for pure helpers (B, D, C part 1), render/CLI regressions (F, H), and the
manual list per phase for the user.

## Done criteria

Phases A, B, D, E, F, G, H, and I are done, C part 1 is done, and C part 2 has a reviewed
design. EDITOR.md is updated.

## STOP conditions

- C part 2 (nested override keys) needs changes to the saved scene format beyond the key
  string: stop and write up the options.
- Any phase needs `World` or `Snap` layout changes that break reading old session files:
  keep old files readable or stop.

## Maintenance notes

- New Snap fields must be copied in all four places in `history.j3`. Grep for
  `selected:` when adding.
- Phase H's `mtime` helper is Linux libc; plan 013 moves it behind `j3.fs`.
