# Plan 007: Scene drawing performance and a frame profiler

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
- **Effort**: L (phased; each phase ships alone)
- **Risk**: MED (touches the draw path every game and the editor use)
- **Depends on**: none. Plan 006 (URP tracing) uses phase 1's profiler.
- **Category**: performance
- **Planned at**: 2026-09-25

## Why this matters

The village sample (`examples/editor_village`) has about 20 modular houses (about 30 pieces
each) and about 1000 scattered instances. Each placed piece is its own mesh node, so the
scene has thousands of mesh nodes, and each one is a separate draw call in the shadow pass
and again in the camera pass. There's no culling, world matrices are recomputed on every
`draw` call, and there's no way to see where frame time goes. EDITOR.md "Performance" lists
these. This plan covers the non-URP costs; plan 006 covers tracing.

## Current state

**Every `draw` recomputes all world matrices.** `j3c/stdlib/j3/scene3d.j3:2809-2813`:
```j3
pub fn draw(scene: &Scene) {
    let transforms = scratch(0).alloc::<Mat4>(scene.len);
    worlds(scene, transforms);
    drawWith(scene, transforms);
}
```
`worlds` (`scene3d.j3:605-633`) walks every node's parent chain. Games call `draw` twice a
frame (shadow and camera); the village's `src/main.j3` does exactly that.

**One draw per mesh node, no culling.** `scene3d.j3:3013-3027` (`drawWith`):
```j3
    traceScene(scene, transforms);
    ...
    while i < scene.len {
        let n = scene.nodes[i];
        if n.kind == MESH && n.gpu.vao != 0 && !isSky(n) {
            if alphaOf(n) == BLEND { blended += 1; }
            else { drawNode(n, transforms[i]); }
```
`drawNode` (`scene3d.j3:3105-3135`) sets the surface (`look(n)`, `gfx.surface`), shadow
tint, and calls `gfx.draw`/`drawDoubleSided`/`drawCutout`.

**`traceScene` runs its signature every draw.** With URP off it returns early
(`if !settings.urp { gfx.traceOff(); return; }`). With URP on, `traceSignature` (`scene3d.j3`
near 2873) loops over every mesh's 16 matrix floats on every `drawWith`, twice a frame, just
to detect changes.

**Instancing exists but is limited.** `j3c/stdlib/j3/gfx.j3:2132-2198`, `drawInstanced(mesh, models)`:
up to `INST_MAX` (64) copies. The shadow pass draws each copy separately. The camera pass is
one `glDrawArraysInstanced`, but the instanced program "never tints" (`gfx.j3:1678`: shared
fragment shader, no per-draw `Surface`), so materials don't work with it today.

**Editor costs (EDITOR.md Performance > Editor):**
- Hot reload: `j3c/editor/src/watch.j3:24-45` reads every watched file (open tabs plus every
  `.j3scene`/`.j3mat` in the asset list) once a second (`EVERY: int = 60`) and compares
  the text.
- Picking: `j3c/editor/src/gizmo.j3:198-213` (`pickPast`) calls `scene3d.hitWith` per node.
  That does a box test, then `hitTriangles` (`scene3d.j3:660`, "ponytail: linear scan of
  the mesh after its box").
- `place`: `j3c/editor/src/tool.j3:1187` calls `scene3d.hitTriangles(verts, ...)` for
  meshes with no box test first.
- `j3.gfx.bvh` exists (`j3c/stdlib/j3/gfx/bvh.j3:14` `Bvh`, `:22` `build(corners, n, arena)`) and
  is only used for URP.

## Design, cheapest first

1. **Profiler.**
   - `gfx.frameStats()` returns `{ draws, triangles, shadowMs, cameraMs, uiMs }`: counters
     bumped in `drawMesh`/`drawInstanced`, plus GL timer queries (`GL_TIME_ELAPSED`, loaded
     like the other entry points in the `P` table) around the shadow and camera passes.
     Results are read one frame late, so nothing stalls.
   - CPU time per `scene3d.draw` via `game.ticks` or a finer clock if `j3.sys` has one.
   - The editor status bar shows `draws · tris · ms`.
   - The village settings panel shows it.
   - MCP `play` prints the last frame's stats in its answer: the game prints a
     `J3STATS ...` line at exit when `J3_FRAMES` is set, and `play` already captures output.
2. **Cache world matrices.** `Scene` gets `worldCache: []Mat4`, `worldsDirty: bool`. Every
   scene3d function that changes a transform, parent, add, remove, or relink sets it
   (grep for writes to `position`, `rotation`, `scale`, `parent`, and `relink`). `draw`
   uses the cache.
   - Code outside scene3d writes `nodes[i].position` directly (behaviors do), so dirty
     flags can't see every change.
   - So do the simplest safe thing first (ponytail): recompute once per frame, not once per
     call. Keep a `frame` counter bumped by `gfx.shadowPass` (the first pass of a frame)
     and reuse the cache when it matches. That removes the second recompute without any
     dirty tracking. Add dirty flags (plus a `scene3d.moved(scene)` for code that writes
     directly) only if the profiler says `worlds` still matters.
3. **Culling.** In `drawWith`, skip a mesh node whose world AABB (from `boundLo/Hi` through
   the cached matrix, computed with the cache) is outside the view frustum (camera pass),
   outside the shadow box (shadow pass, which gfx knows from `shadowPass` bounds), or
   farther than `fogEnd` plus its radius. gfx exposes `gfx.visible(lo, hi): bool` using
   `G.proj*G.view` or the light matrix, depending on `G.pass`.
4. **Batching.**
   - Group opaque, non-cutout, same-`gpu.vao`, same-`mat`, and same-`doubleSided` nodes and
     draw each group with `drawInstanced` (chunks of 64).
   - The instanced path needs the material's `Surface` uniforms: make it call the same
     `surface upload` as `drawMesh` does, and remove "never tints".
   - Shadow pass: add a real instanced depth draw (the depth shader plus a per-instance
     matrix) instead of the loop at `gfx.j3:2142-2147`.
   - This is the big win for the village: about 50 unique meshes × about 3000 nodes.
5. **Trace signature.** With the cached worlds (step 2), compute `traceSignature` only when
   the worlds were recomputed this frame or a material changed (`refreshMaterial`/
   `previewMaterial` bump a counter).
6. **Editor:**
   - Hot reload stats mtimes first: add a `stat` or `mtime` helper to `editor/src/util.j3`
     (the libc boundary) and read a file only when its mtime or size changed.
   - Picking and `place`: build a per-mesh `Bvh` lazily over `traceVerts(n)` and cache it
     by `gpu.vao` in a small table in scene3d. `hitWith` uses it when the mesh has more
     than 256 triangles.
   - `place` at `tool.j3:1187` calls `hitWith` (which has the box test) instead of
     `hitTriangles` directly.

## Commands you will need

| Command | Purpose |
|---|---|
| `cd j3c && cargo build --release` | build j3c (copies to ~/.bin/j3c) |
| `timeout 120 ./target/release/j3c run ../examples/editor_village --frames 300 --headless` | headless benchmark (village) |
| `timeout 60 ./target/release/j3c run --file examples/scene3d_check.j3` | scene3d self-check |
| `J3_EDITOR_SRC=editor/src timeout 120 ./target/release/j3c -e render ../examples/editor_village` | editor-side build plus render in one go |

Never open a window. Record FPS as `300 / wall-clock seconds` minus startup: run it once
with `--frames 1` and subtract.

## Scope

In: the profiler, cached worlds, culling, batching, trace signature, and editor
watch/pick/place.
Out: URP tracing (plan 006), LOD, multithreading, GPU-driven culling.

## Git workflow

Branch `plan-007-scene-draw-perf` in the main checkout (the village and editor sources are
untracked, so a worktree won't have them). One commit per phase.

## Steps

1. **Baseline.** Build, then time the village at 300 frames headless, three runs, taking the
   median. Write the numbers into this plan's Maintenance notes.
   Expected: a number (reference: about 90 FPS headless on the author's machine, URP on,
   traced shadows off).
2. **Profiler (Design 1).** Add the counters, timer queries, `gfx.frameStats`, the
   `J3STATS` line when `J3_FRAMES` is set, the editor status text, and a line in the village
   settings panel.
   Verify: the headless village run prints `J3STATS draws=<n> tris=<n> shadow=<ms> camera=<ms>`.
   Expect draws in the thousands.
3. **Once-per-frame worlds (Design 2).**
   Verify: `scene3d_check.j3` passes, and the village median improves or stays the same.
   Also check that behaviors that move nodes still move: `examples/3d-editor-scene` has a
   `spin` behavior; the headless screenshot at frames 1 and 60 must differ.
4. **Culling (Design 3).**
   Verify: `J3STATS draws` drops when looking along a road (at least 40% fewer on the
   village spawn view), and a screenshot compare against step 3 at the same frame shows no
   missing objects on screen (`compare -metric AE` below a small threshold, or eyeball it).
5. **Batching (Design 4).**
   Verify: `draws` drops about 10× on the village, materials still show (the water pond
   keeps its reflection; tinted materials in `examples/3d-editor-scene` look the same), and
   shadows are unchanged in a screenshot compare.
6. **Trace signature (Design 5).**
   Verify: with URP on, the CPU time for `drawWith` from the profiler drops, and moving an
   object in a behavior still updates reflections (the 3d-editor-scene glass cube over a
   spinning object).
7. **Editor (Design 6).** mtime check in `watch.j3`, a cached per-mesh BVH for picking,
   and `place` through `hitWith`.
   Verify: the editor sources build (`J3_EDITOR_SRC=... j3c -e scene list` succeeds), and
   the `j3c -e scene ground` / MCP `place ground` answers match before and after on
   `examples/3d-editor-scene`.
8. **Record results.** Update EDITOR.md "Performance" (mark items done, with before/after
   numbers).

## Test plan

- `j3c/examples/scene3d_check.j3` (existing) after each phase.
- A new `j3c/examples/cull_check.j3`: a box in front of and behind a camera, and
  `gfx.visible`-style math on the CPU side (keep the frustum test a pure function in math
  or scene3d so it can be checked without GL).
- Headless screenshots before and after for each visual phase.

## Done criteria

- The village camera pass draws at least 5× fewer draw calls than baseline.
- The headless median FPS improves by at least 1.5× with traced shadows off.
- Stats are visible in the editor, the game, and MCP `play`.

## STOP conditions

- The instanced path can't take per-draw `Surface` uniforms without duplicating the lit
  shader (then batch only material-less nodes and report).
- Culling hides visible objects in any screenshot compare and the cause isn't the bounds
  math.
- Timer queries aren't available in the GL context (then ship CPU timing only).

## Maintenance notes

- Baseline numbers (2026-09-25, RTX 5060 Ti; headless still uses the real GPU): village 300 frames
  in 2.26 s minus 0.59 s startup, about 180 FPS; `J3STATS` draws=4629 tris=398288 scene=5.5 ms.
  After all steps: draws=113, about 770 FPS, scene=1.1 ms. Per-step numbers are in EDITOR.md.
- Deviations: the profiler lives in `j3.game` (present is the frame boundary), not `gfx`.
  `place` got its speedup through `hitTriangles` (BVH cache keyed by vertex pointer, length,
  and a float sample) instead of switching to `hitWith`, since the tool has no uploaded bounds.
  3d-editor-scene has no node using `spin`; the step 3 check used a scratch copy with one set.
- Keep the frustum test a pure function so it stays testable without GL.
- Batching keys must include everything `drawNode` varies on: mesh, material,
  double-sided, and alpha mode. Add new node visual fields to the key.
