# mini-voxel performance

## How to measure
- F3 shows fps, per-stage CPU ms (gen / mesh / lod / draw), drawn meshes and vertices, and mesh memory.
- Headless numbers: set `BENCH_VIEW` in `src/config.j3` (0 = off), then
  `timeout 250 j3c run examples/mini-voxel --frames 300` (add `--screenshot out.png` to look at it).
  The player stands still at spawn, so this measures steady-state draw cost, not streaming.
- `BENCH_AWAY = N` digs a trench at the origin and starts the player N columns away looking back:
  the far-edit test (a dirt patch should show at the horizon).

## Design
- Cells exist only within `NEAR` columns (21x21 drawn, 25x25 meshed, ~11 MB), chunk-owned, spilled to
  `world/edits.j3` when a column leaves the ring (so holes survive walking away).
- Beyond that, LOD levels 1..4: a level-L cell is 2^L voxels; each tile (16x16 cells) is meshed
  from the generator, no cells stored. Rings are `K` tiles wide per level, so each ring reaches
  twice as far as the one inside it. Drawing is a quadtree (`render/world_draw.j3`): a tile is
  drawn at one level only, and falls back to the coarser one while finer meshes build.
- LOD tiles are greedy-meshed (`render/lod_mesher.j3`, `render/quad.j3`): tops merge into
  rectangles, walls into runs.
- Edited columns keep a 16x16 surface-height table (`edits.setTops`); LOD sampling uses it
  (`terrain.surfaceAt`), and `stream.syncEdits` marks the covering tiles stale so they rebuild.
- Tiles deep under a finer ring (`lod.covered`, `COVER_MARGIN`) are not meshed at all.
- Generate / mesh / LOD stages share a `FRAME_US` budget each frame, nearest first. They walk
  distance-sorted offset lists (`util/spiral.j3`) and skip entirely while nothing they depend on
  changed (`stream.Sched`).

## Measured (RTX 5060 Ti, stationary, vsync-bound ~100 fps)
Start vertices at view 160 (256 m) after each step:

| change | vertices | tris drawn | draw calls |
|---|---|---|---|
| before this round (all tiles built, one quad per column) | 5.83 M | 407 k | 506 |
| + greedy LOD meshing | 2.15 M | 157 k | 506 |
| + skip covered tiles | 2.03 M | 157 k | 506 |

Default view 10: 1.58 M vertices before, 111 draw calls. At view 160 the vertices are about 80 MB of
GPU memory (40 B each).

Idle frame at view 160, after loading: gen 0.0001 ms, mesh 0.00001 ms, lod 0.00001 ms. The
remaining CPU cost is the quadtree draw, ~2.1 ms with 506 draw calls (`J3STATS cpu=2.4`).

## Known limits / next
- LOD tiles show an edit only as a surface height: caves and overhangs are filled at distance.
- The draw traversal is now the biggest CPU cost; batching tiles of a level into fewer draws, or
  skipping subtrees that are fully behind the camera earlier, are the next steps.
- Vertex layout is 40 B/vertex (6 per face) from `gfx.emitFace`; packing it is a stdlib change.
