# FPS optimization results

Production compiler settings are unchanged: LLVM, `opt = 0`, `debug = true`. Resolution, clip distance, LOD thresholds, lighting, and production simulation timing are unchanged.

Native RTX 5060 Ti medians from three samples per scene, seed 42, 1280×720:

| Scene | Before → after FPS | Before → after ms/frame | Frame-time reduction |
|---|---:|---:|---:|
| start | 1,755 → 1,928 | 0.570 → 0.519 | 9.0% |
| surface | 330 → 706 | 3.030 → 1.416 | 53.3% |
| cave | 331 → 743 | 3.021 → 1.346 | 55.5% |
| entrance | 338 → 903 | 2.959 → 1.107 | 62.6% |
| factory | 67 → 172 | 14.925 → 5.814 | 61.0% |

Times above are `1000 / median FPS`, avoiding the engine's coarser 0.1 ms frame-time rounding. Direct headless compiler invocations verified the NVIDIA renderer in every sample. All hardware acceptance targets pass, including the starting-scene regression limit. Hardware snapshots, logs, and screenshots are in `target/benchmarks/gpu-before/` and `gpu-final/`.

Hardware testing caught a CPU regression in the initial section implementation: repeating matrix multiplication and eight-corner clipping per section erased its GPU savings. Clip planes are now extracted once per pass and tested using each box's extreme point. This is the same conservative clip-plane rejection, with a small roundoff allowance. World model bounds use their exact cardinal rotation and uniform scale. Reference clipping checks cover both projections, all rotations, faces, edges, and corners.

Separate three-sample factory profiles retain production stage order and add timers only to benchmark snapshots:

| CPU stage | Before ms | After ms | Reduction |
|---|---:|---:|---:|
| furnace | 0.02494 | 0.02379 | 4.6% |
| transport | 0.05630 | 0.03427 | 39.1% |
| drill | 0.02677 | 0.02565 | 4.2% |
| complete update | 0.10919 | 0.08488 | 22.3% |

These small timings include timer overhead; the uninstrumented hardware acceptance runs measure ~0.110 → 0.085 ms for the complete update. Raw instrumented sources and logs are in `stages-before/` and `stages-after/`.

Final software-renderer medians from three samples per scene, seed 42, 1280×720, llvmpipe LLVM 20.1.2. World scenes run 600 frames and the expensive factory runs 180; the engine discards its warmup frames. The benchmark calls production update/draw functions with deterministic simulation steps. Compare frame times rather than rounded integer FPS.

| Scene | Before ms/frame | After ms/frame | Reduction | Before → after FPS |
|---|---:|---:|---:|---:|
| start | 11.3 | 8.3 | 26.5% | 89 → 121 |
| surface | 67.4 | 43.4 | 35.6% | 15 → 23 |
| cave | 59.5 | 39.7 | 33.3% | 17 → 25 |
| entrance | 43.9 | 32.0 | 27.1% | 23 → 31 |
| factory | 379.6 | 85.8 | 77.4% | 3 → 12 |

All five software cases meet the 20% frame-time target; the starting scene improves rather than regresses. The factory submits 1,535,460 triangles versus 7,827,285 (~80% fewer), and 784 draws versus 9,727 (~92% fewer). Factory CPU update time falls from ~0.146 ms to ~0.125 ms.

The same single-voxel edit at (216, 103, 216), which touches four sections, falls from roughly 119–125 ms to 12–13 ms. Full-world mesh/assets startup falls from ~10 seconds to ~8.9 seconds. Starting startup rises by about 21 ms.

Exact model merging, independent model/cargo culling, cached node transforms, and instancing provide the first improvement. Smaller terrain sections improve camera culling and edit locality. Invisible world borders are culled without changing their fade. A shadow cache combines already-merged section vertices and draws whole chunks only after all their sections pass visibility; partially visible chunks keep individual section draws. Shadow maps still update every frame.

Tradeoff: retained CPU section vertices and combined shadow GPU meshes increase memory in larger cases. Factory peak RSS rises from ~615 MiB to ~655 MiB; the surface case rises from ~606 MiB to ~616 MiB. RSS includes the launcher/compiler, so medians reduce first-build effects. Direct hardware samples did not measure RSS or separate VRAM usage.

Validation: compiler checks and the complete automation suite pass. Tests cover every model face/color/emission, rotations and drill pivots, conservative culling, AO and section seams, holes, machine footprints, cache replacement, and partially visible shadow batches. Static screenshot differences exceeding two RGB levels are 3 pixels at start, 29 on the surface, and zero in cave/entrance views, out of 921,600 pixels. Factory screenshots differ in 460 pixels (0.05%), including animated drill poses; silhouettes, cargo, lighting, and gameplay were reviewed.

Final hardware screenshot differences are 3 pixels at start, 41 on the surface, zero in the cave, 13 at the entrance (none above 16 RGB levels), and 468 in the animated factory. Draw and triangle counts are unchanged by the faster visibility tests.

Raw software logs, source snapshots, screenshots, and median metrics are under `target/benchmarks/baseline/` and `software-final/`. `model-stage/` has one exploratory sample per scene; `optimized/` has three samples before shadow batching; `final/` and `gpu-after/` preserve the stage before clip-plane caching. See [README.md](README.md) to reproduce the runs.

## After the machine-state and definition-table refactor

Same hardware, seed 42, 1280×720, project compiler settings, factory scene (180 frames, two samples): 168–170 FPS (172 before), draw submission 5.68–5.71 ms (5.60 before), complete update ~0.055 ms (0.085 before: per-kind tables and slot loops replace the per-machine hash lookups), startup 7.4 s (9.0 s before). Per-stage machine timing with `--profile-stages`: furnace 0.014 ms, belts 0.023 ms, drills 0.016 ms. Machines now step at a fixed 60 Hz; the benchmark's one step per frame is unchanged.

## Earlier notes (moved from tests/README.md)

Before terrain LOD, full-world review (seed 42, optimized build, all 81 chunks unlocked): generation ~28 ms, initial meshes/assets ~0.51 s, single edited chunk rebuild ~4 ms. A 1280 × 720 llvmpipe software-renderer stress run peaked at ~587 MiB RSS and ~9 FPS; Full-world hardware GPU performance was not measured. The normal starting-world headless run also passed on the available NVIDIA renderer. Surface, cave, and entrance previews were inspected under `target/world-review/`.

Terrain LOD checks compare merged rectangle coverage against detailed voxel faces, including tunnels, mined holes, mixed materials, finite ore, surface-node exclusions, shared chunk boundaries, winding, and rebuilt edits. A solid 8 × 4 × 6 box reduces from 208 voxel faces to six rectangles. Selection checks cover the 24 m / 32 m hysteresis thresholds, diagonal distance, and underground cameras. Headless full-world surface, cave, and entrance views verify cached rendering and the merged shadow pass.

After terrain LOD, the same seed-42 entrance view submits 805,906 triangles across the camera and shadow passes, versus 2,884,990 before (~72% fewer). All 81 merged meshes total 435,966 vertices versus 6,138,624 detailed vertices (~93% fewer). The 120-frame llvmpipe run measured ~22 FPS versus the previous ~9 FPS. Building both caches costs ~1.34 s initially and ~14 ms for a single edited chunk in the optimized review build, compared with ~0.51 s and ~4 ms before. LOD selects cached geometry during movement; models remain at full detail.

Further FPS optimization checks cover all 13 exactly merged model assets, independently culled cargo and shadow casters, node transform caches, section boundaries and original AO, complete machine edit invalidation, active section list reuse, and cached shadow batches after edits. Cached clip planes and cardinal model bounds match reference camera/shadow clipping across faces, edges, and corners. Repeatable before/after metrics and screenshots are saved under `target/benchmarks/`; see `benchmarks/README.md` for commands and measurement conditions.
