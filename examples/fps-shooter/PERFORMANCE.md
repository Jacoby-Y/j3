# Performance

## Status

Automated validation passed on 2026-09-27 after rebuilding the compiler and shooter
(including the reflective/glass instance-tint fix and the benchmark sound listener).
Every run meets the target by a wide margin.

Still manual: playtest movement, shrine fights, distant enemies, gems, and mass
kills for feel and readability. Repeat the benchmark when a real boss ships.

## Results

Full runs: 10 s warm-up + 60 s capture, offscreen without `glFinish`, three runs
per scenario. Times are milliseconds.

| Scenario | Run | Frames | Mean | p95 | p99 | Worst | Worst kill (events) |
|----------|-----|--------|------|-----|-----|-------|---------------------|
| Opening   | 1 | 95,086 | 0.63 | 0.65 | 0.67 | 3.02 | — |
| Opening   | 2 | 95,564 | 0.63 | 0.65 | 0.67 | 3.09 | — |
| Opening   | 3 | 91,470 | 0.66 | 0.69 | 0.78 | 3.06 | — |
| Populated | 1 | 39,604 | 1.52 | 1.59 | 1.83 | 3.72 | — |
| Populated | 2 | 39,516 | 1.52 | 1.60 | 1.85 | 11.49 | — |
| Populated | 3 | 39,694 | 1.51 | 1.59 | 1.82 | 3.64 | — |
| Pack      | 1 | 41,677 | 1.44 | 1.56 | 1.72 | 3.42 | 3.17 (209) |
| Pack      | 2 | 41,538 | 1.44 | 1.56 | 1.73 | 3.57 | 3.17 (207) |
| Pack      | 3 | 41,600 | 1.44 | 1.56 | 1.72 | 3.47 | 1.69 (208) |

Window capture (populated, one run): 40,888 frames, mean 1.47, p95 2.13, p99 2.51,
worst 7.76.

Notes:

- Populated/pack peak at 192 living and 75 active enemies, about 108 draws. The
  opening scene is 9 living / 8 active, 119 draws.
- The one 11.5 ms populated frame was a single outlier; that run's p99 stayed at
  1.85 ms. It did not repeat in the other runs or the window capture.
- Offscreen runs report ~1.0 ms of UI GPU time under load versus 0.04 ms in the
  window capture, so treat offscreen UI GPU time as an artifact of that
  presentation path, not HUD cost.
- Every populated/pack capture had a fresh GPU sample on (nearly) every frame.
  Opening runs got fresh samples on 36–78% of frames, since they finish faster
  than queries resolve; this is the expected skip behavior, not stale data.
- The quick smoke test (`--quick --repeats 1`) also passed: 0.62 / 1.51 / 1.44 ms mean.
- The renderer fixture images are byte-identical (14 batched draws versus 395 scalar).

Test limitations (from the implementation pass, not rerun here): with
`RUST_MIN_STACK=16777216`, the compiler suite had 219 passes, 10 failures from
missing example fixtures, and one ignored test. The focused game stdlib test passed
after the profiler fix. Gallery checking is blocked by missing SDL2_mixer/font
assets; the optional C renderer-fixture build hits existing libc declaration
conflicts. LLVM renderer checks and the 3D editor scene smoke test passed.

## Target

The performance pass targets 100 FPS at 1280×720 on the development machine
(NVIDIA RTX 5060 Ti, driver 580.173.02). Acceptance is p95 ≤10 ms, p99 ≤16.7 ms,
and pack-kill frames ≤25 ms. These are frame-time targets, not an FPS guarantee
for other machines or future bosses.

## Changes

- Default LLVM optimization is now level 2, with debug information and bounds
  checks retained. Use `--opt 0` when prioritizing compiler turnaround.
- There are 192 stable enemy slots: 180 wilderness slots and 12 reserved for
  shrine enemies. Each unlocked tile targets `min(6 + tier * 3, 20)` inhabitants.
  Spawn attempts prefer the least populated eligible tile, stay 20 m from the
  player, and avoid adding ordinary enemies inside 60 m when 48 are already there.
  Existing enemies can still converge. Biome-themed spawns retain their 25% chance.
- Ordinary enemies wake inside 55 m, sleep beyond 65 m, and wake for three seconds
  when damaged. Distant grounded enemies idle; flyers hover. Airborne ground
  bodies settle before sleeping. Attack timers freeze while asleep. Distance
  never deletes an enemy. Shrine behavior and fall-out recovery remain active.
- Enemy parts are prepared once per rendered frame into reusable buffers. Each
  pass culls whole enemies independently against its camera/light volume, then
  draws colored instances. Within 45 m, enemies retain their detail; beyond 50 m,
  they omit facial details and outlines. The 5 m gap prevents LOD oscillation.
  Distant gold enemies retain a gold body tint. Wings, horns, and biome markers remain.
- Bodies and larger appendages cast shadows. Facial details, gems, debris, biome
  crystals, and projectile visuals do not. Camera-visible gems and effects are batched.
- Debris has 160 live slots, a shared 48-piece update budget, and a token bucket
  refilled at 480 pieces/second (48-token burst capacity). Kills request 10 pieces;
  nonlethal hits request two. Emission beyond 60 m is suppressed. Cosmetic random
  numbers have their own stream; effect budgets do not alter spawn decisions.
- Gems use simulation-clock expiry and stop physics when settled. Distant falling
  gems update at 10 Hz with staggered deadlines and ≤20 ms physics substeps.
  Nearby and magnetized gems update every frame. Magnet range, pickup, XP values,
  90-second lifetime, and the existing 256-slot replacement behavior are retained.

## Engine interfaces

`gfx.Instance { model, tint, emission }`, `gfx.drawInstances`, and
`gfx.drawHullInstances` add colored batching alongside the existing matrix-only
instancing API. Batches automatically split at 64 instances. Custom effects fall
back to individual draws using their existing uniforms. Material properties and
instance/culling state are restored afterward.

`game.lastFrameStats()` exposes the preceding completed frame without smoothing.
GPU query results arrive asynchronously: raw GPU fields are -1 when no fresh
sample is available. Pending queries are retained and their slots are skipped
until ready; the profiler never waits for them or reports stale values as fresh
samples. Averaged GPU counters use only completed samples.

## Repeatable benchmark

From the repository root:

```sh
cargo build --manifest-path j3c/Cargo.toml
j3c/target/debug/j3c build examples/fps-shooter
python3 examples/fps-shooter/tools/performance.py --quick --repeats 1
python3 examples/fps-shooter/tools/performance.py --repeats 3
```

Full runs warm up for 10 wall-clock seconds and record 60 seconds per scenario,
three times. Quick runs use 1 + 3 seconds and are smoke tests only. Each render
advances the simulation by a fixed 10 ms. The stress route visits all nine biome
centers, rotating the view and repeating every 6,000 frames. This is a deterministic
synthetic route with jumps between tiles, rather than a recorded player walk.

The three scenarios are:

1. **Opening:** nine normal hub enemies and normal scenery/HUD.
2. **Populated:** 192 enemies, approximately 48 deliberately placed nearby, 256
   gems, 64 enemy orbs, the real boss of whichever tile the route is in
   (`boss.benchmark`: Colossus where a tile has none, fighting the player with its
   hazards), and 64 extra moving projectiles with terrain collision. Other residents and shrine enemies can
   raise the active count above 48. Refill fixtures every 200 simulation frames
   so the workload persists throughout the route.
3. **Pack:** the same workload, killing up to 24 nearby enemies together halfway
   through each refill cycle via the real damage, XP, debris, and sound paths.

The harness keeps the player alive and prevents reward menus from interrupting
capture. Setup/refill work during capture is included in frame times. It buffers
raw frame samples and writes them after capture; results and metadata are saved
under `target/performance/`. The JSON includes percentiles, worst kill frames,
CPU subsystem means (the raw CSV's `*_us` columns, reported as `*_ms`), fresh GPU
sample counts, draw counts, and population counts.

The default uses SDL offscreen presentation **without** `J3_FRAMES` and its forced
`glFinish`. To measure a real window separately:

```sh
python3 examples/fps-shooter/tools/performance.py --window --scenario populated --repeats 1
```

The runner requests uncapped NVIDIA presentation with `__GL_SYNC_TO_VBLANK=0`.
Window-system/compositor scheduling can still affect the result. It uses dummy
audio output while exercising sound calls. Do not compare these runs directly
with synchronized `j3c run --frames` captures.

## Regression checks

The shooter runs focused enemy checks at startup before sound loading: unlocked
spawn limits, player exclusion, shrine reservation, sleep hysteresis, persistence,
damage wake-up, debris rate/distance/RNG isolation, settled gems, pickup/expiry,
and death cleanup. The pre-existing player/perk checks also remain enabled.

The renderer fixture compares 130 instances, colors/emission, outlines, custom
effect fallback, ordinary and matrix-only draws afterward, and independent
camera/shadow visibility:

```sh
j3c/target/debug/j3c run --file --opt 2 --headless --frames 2 --screenshot /tmp/instances-batch.png examples/fps-shooter/tools/instances.j3
J3_INSTANCE_SCALAR=1 j3c/target/debug/j3c run --file --opt 2 --headless --frames 2 --screenshot /tmp/instances-scalar.png examples/fps-shooter/tools/instances.j3
```

Both images should match pixel-for-pixel. The fixture with its custom-effect
fallback submits 14 draws batched versus 395 scalar.

## Measurement limits

Before implementation, synchronized 600-frame opening captures measured 3.7 ms
at optimization level 0 and 0.8 ms at level 2. This isolates a substantial build
configuration cost, but is not a before/after measurement of late-game combat.
The larger repeatable workload was added in this pass.

The results above predate the bosses and used a 24-part synthetic boss. With the real
bosses in the route, a quick run (`--quick --repeats 1`) measured 0.73 / 1.65 / 1.63 ms
mean and at most 2.1 ms p99, still far inside the target. The full three-run
acceptance hasn't been redone. A manual late-game playtest remains the final check
for combat feel and visual readability.
