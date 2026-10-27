# Stage 01: Frame capture, image diff, 2D perf baseline

## Done

**Shipped**
- `j3.game.captureFrame(path: string): bool` (GL and SDL_Renderer via the existing `readWindow`; stub returning false in `game_headless.j3`). Call after drawing, before `present`.
- `J3_CAPTURE=out.png:N` (handled in `game.present`, same path as `J3_FRAMES`/`J3_SCREENSHOT`): saves frame N and quits. `J3_FIXED_DT=0.016`: `game.poll` returns that dt, so `App` (and anything using `poll`) is deterministic. Both documented in the `j3.app` module doc.
- `j3.gfx.canvas.diff(a: &Canvas, b: &Canvas, tol: int, out: &Canvas): int` (-1 on size mismatch; differing pixels red, others 25% grey).
- `j3c -e imgdiff a.png b.png [--tol N] [--max N] [--out diff.png]` in `j3c/src/imgdiff.rs` (Rust, uses `png::decode`/`encode_rgba`). Exit 1 if count > max or sizes differ, 2 on usage/IO error.
- `tools/bench2d/` (`bench2d rects N | text N | mixed`), prints `scene=... cpu_ms=median/p95 gpu_ms=... draws=...`. Build: `j3c build [--release] tools/bench2d`; run from `tools/bench2d` (font path is relative). Run headless: `SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy timeout 10 target/bench2d mixed`.
- `hudDraw` now calls `profDraw(2)`, so the profiler's `draws`/F3 overlay count 2D quads (counter only, drawing unchanged).

**Numbers** (CPU: Intel Core Ultra 7 270K Plus; GPU: RTX 5060 Ti, driver 580.173.02; headless via `SDL_VIDEODRIVER=offscreen`, median of 3 runs; cpu_ms = draw system only, median/p95; gpu_ms = profiler UI zone of the last frame)

| scene | release cpu_ms | draws | dev (opt 0) cpu_ms |
|---|---|---|---|
| rects 1000 | 0.67 / 0.68 | 1000 | 0.68 / 0.69 |
| rects 10000 | 6.70 / 6.81 | 10000 | 6.70 / 6.80 |
| text 1000 (10 glyphs each) | 5.85 / 5.89 | 9000 | 5.87 / 5.93 |
| mixed (5k rects, 1k text, 200 tex) | 9.27 / 9.41 | 14200 | 9.24 / 9.39 |

Offscreen gpu_ms tracks cpu_ms (about 0.7 / 6.8 / 5.9 / 9.3), so it is not a clean GPU time there. On the visible window earlier, rects 10000 was cpu 5.5 ms, gpu about 0.5 ms; use that run mode only if you must. Dev and release match because the stdlib draw path dominates.
Proof: `imgdiff` of two `J3_CAPTURE=...:30 J3_FIXED_DT=0.016` mixed captures: `0 of 921600 pixels differ`, exit 0. Frame 30 vs 31: `853280 of 921600 differ (tol 2)`, exit 1. `canvas.diff` checked by a tiny `j3c run --file` (`diff ok`).

**Deviations**
- Verified on `bench2d`, not `examples/gallery`: gallery cannot run here (no `libSDL2_mixer`, and its font path `../dungeon/assets/font.ttf` is missing). Its `j3c check` shows only those two environment errors; `j3c check examples/fps-shooter` passes.
- `J3_CAPTURE` reuses the `J3_FRAMES`/`screenshot` machinery that already existed (`--screenshot`), instead of new code.
- `cpu_ms` is draw start to draw end (not through `present`: `present` blocks on swap).
- `cargo test`: 7 failures, all in `compile.rs` tests naming missing examples (bounce, defense, dungeon, island, minecraft, raytrace, project_emit_cwd). None new.

**Gaps**
- `J3_FIXED_DT` parses digits and one dot only. Games that call `game.ticks()` directly bypass it.
- Capture reads the window size; a non-offscreen window can differ by driver. Goldens should be made in the same headless mode.
- `imgdiff`/`decode` handle 8-bit non-interlaced PNGs only.

**Next stage needs to know**
- J3 gotchas hit: `string == string` fails in LLVM emit (compare bytes); singleton init must be zero (`let X: string;`); `canvas.rgb` and `game.rgb` are different `Color`s.
- Run all windowed tests headless (env above) and under `timeout 10`.
- Stage 02 should beat: `rects 10000` 6.7 ms, `mixed` 9.3 ms, 14200 draws.
