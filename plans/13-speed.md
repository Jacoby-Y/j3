# Plan 13: j3c in J3 beats Rust j3c on every metric

Goal (user, 2026-11-18): `j3c-boot` (j3c in J3, `j3c-bootstrap/`) is **faster, smaller and uses less memory** than the
Rust `j3c` (`j3c-bootstrap/stage0/j3c`) on **every** metric of a fixed 10-20 metric bench. Baseline: `j3c-bootstrap/SPEED.md`
(2026-11-18: 1.4-7x slower, up to 8x the memory, binary 30 MB vs 13.4 MB).

## Rules
- Output stays byte-identical to stage0: every gate in `.swarm-opus/11-port/context.md` (C5) stays green after each change.
  Speed never buys a diff.
- Measure before and after every change with the bench; keep a change only if its metric moves and none regresses.
- No crates.io, no libLLVM, never edit `j3c/` (Rust), headless, `timeout`, no network.
- Clang does the codegen for both compilers. Where clang is most of a metric, the win has to come from how and how
  often clang is run (fan-out, cache hits, less redundant work), never from a different `-O` or different IR.

## The bench (`j3c-bootstrap/tools/bench.sh`)
One script, both compilers, scratch copies, median of N (default 3), wall + peak RSS, writes `target/bench/result.md`
and a machine-readable line per metric. Metrics:

| # | Metric | Measures |
|---|---|---|
| 1 | self build cold | wall, RSS |
| 2 | self build warm (nothing changed) | wall, RSS |
| 3 | self build after a one-module edit | wall, RSS |
| 4 | editor program build cold (`j3c/editor/src`) | wall, RSS |
| 5 | desk-app build cold | wall, RSS |
| 6 | editor_village build cold | wall, RSS |
| 7 | desk-app build after a one-module edit | wall, RSS |
| 8 | `check` over the corpus (sum) | wall, max RSS |
| 9 | `check` worst UI input (`grid_test`) | wall, RSS |
| 10 | `check` j3c-bootstrap | wall, RSS |
| 11 | `fmt --check` on the largest stdlib files | wall, RSS |
| 12 | `dump-ir` on a mid-size program | wall, RSS |
| 13 | `.j3s` bytecode compile (`dump-bc`) | wall, RSS |
| 14 | `doc` startup / `doc j3.game --short` / `doc search mesh` | wall, RSS |
| 15 | `new` + first `check` of a fresh project | wall, RSS |
| 16 | compiler binary size (stripped, as installed) | bytes |

"Smaller" = the compiler binary (16). Built programs come from identical IR, so their size is equal by the
byte-identity rule, not a metric.

## Scoreboard (updated after every wave; details and phase splits in `j3c-bootstrap/SPEED.md`)
Goal: every row WIN in every column (wall, RSS, size). stage0 = Rust j3c; boot = j3c in J3.

### 2026-11-18, speed wave 2 (HASH: streamed cache key, emit buffers; NOSDL: scene tools via the editor `cli` subprocess as Rust, SDL2/GL dropped; FRONT: keyword table, pool hints, check-only stdlib body drop): wall 15/15 WIN, RSS 15/15 WIN, size WIN
| # | Metric | stage0 | j3c-boot opt | boot/stage0 wall / RSS | wall / RSS |
|---|---|---|---|---|---|
| 1 | self build cold | 3.64 s / 998 MB | 2.92 s / 461 MB | 0.80x / 0.46x | WIN / WIN |
| 2 | self build warm | 1.15 s / 447 MB | 0.450 s / 353 MB | 0.39x / 0.79x | WIN / WIN |
| 3 | self build leaf edit | 1.53 s / 470 MB | 0.710 s / 361 MB | 0.46x / 0.77x | WIN / WIN |
| 4 | editor program cold | 1.28 s / 262 MB | 1.15 s / 141 MB | 0.90x / 0.54x | WIN / WIN |
| 5 | desk-app cold | 1.17 s / 387 MB | 1.00 s / 214 MB | 0.85x / 0.55x | WIN / WIN |
| 6 | editor_village cold | 1.61 s / 165 MB | 1.52 s / 101 MB | 0.94x / 0.61x | WIN / WIN |
| 7 | desk-app leaf edit | 0.500 s / 172 MB | 0.300 s / 161 MB | 0.60x / 0.94x | WIN / WIN |
| 8 | check corpus (sum, max RSS) | 37.95 s / 218 MB | 19.63 s / 155 MB | 0.52x / 0.71x | WIN / WIN |
| 9 | check grid_test | 0.520 s / 150 MB | 0.170 s / 103 MB | 0.33x / 0.69x | WIN / WIN |
| 10 | check j3c-bootstrap | 1.11 s / 447 MB | 0.440 s / 328 MB | 0.40x / 0.73x | WIN / WIN |
| 11 | fmt --check j3c/stdlib/j3 | 0.090 s / 5 MB | 0.045 s / 3 MB | 0.50x / 0.60x | WIN / WIN |
| 12 | dump-ir examples/desk-app | 0.370 s / 177 MB | 0.200 s / 162 MB | 0.54x / 0.92x | WIN / WIN |
| 13 | dump-bc scripted/fighter.j3s | 0.016 s / 19 MB | 0.010 s / 17 MB | 0.63x / 0.89x | WIN / WIN |
| 14 | doc x3 | 0.195 s / 27 MB | 0.044 s / 21 MB | 0.23x / 0.78x | WIN / WIN |
| 15 | new + first check | 0.018 s / 19 MB | 0.010 s / 14 MB | 0.56x / 0.74x | WIN / WIN |
| 16 | binary size, stripped | 13.43 MB | 11.87 MB | 0.88x | WIN |

j3c-boot NEEDED: libc, ld-linux, libm only (was libSDL2 + ~40). Rows 1-3 build a smaller j3c-bootstrap than before (the
`j3_editor` dependency is gone), same program for both compilers. Thinnest margins: 7 RSS (0.94x), 12 RSS (0.92x),
6 wall (0.94x), 4 wall (0.90x).

### 2026-11-18, speed wave 1 (BUILD: posix_spawn clang fan-out, hash step; STARTUP: doc index, bench K-loop; MEM: arenas): wall 15/15 WIN, RSS 7/15 WIN, size WIN
| # | Metric | stage0 | j3c-boot opt | boot/stage0 wall / RSS | wall / RSS |
|---|---|---|---|---|---|
| 1 | self build cold | 4.00 s / 1097 MB | 3.46 s / 951 MB | 0.87x / 0.87x | WIN / WIN |
| 2 | self build warm | 1.32 s / 508 MB | 0.570 s / 492 MB | 0.43x / 0.97x | WIN / WIN |
| 3 | self build leaf edit | 1.72 s / 530 MB | 0.880 s / 542 MB | 0.51x / 1.02x | WIN / LOSE |
| 4 | editor program cold | 1.31 s / 267 MB | 1.20 s / 228 MB | 0.92x / 0.85x | WIN / WIN |
| 5 | desk-app cold | 1.15 s / 395 MB | 1.05 s / 383 MB | 0.91x / 0.97x | WIN / WIN |
| 6 | editor_village cold | 1.64 s / 162 MB | 1.55 s / 160 MB | 0.95x / 0.99x | WIN / WIN |
| 7 | desk-app leaf edit | 0.500 s / 172 MB | 0.310 s / 190 MB | 0.62x / 1.11x | WIN / LOSE |
| 8 | check corpus (sum, max RSS) | 38.04 s / 218 MB | 23.71 s / 222 MB | 0.62x / 1.02x | WIN / LOSE |
| 9 | check grid_test | 0.520 s / 150 MB | 0.190 s / 158 MB | 0.37x / 1.05x | WIN / LOSE |
| 10 | check j3c-bootstrap | 1.26 s / 508 MB | 0.570 s / 492 MB | 0.45x / 0.97x | WIN / WIN |
| 11 | fmt --check j3c/stdlib/j3 | 0.090 s / 5 MB | 0.065 s / 7 MB | 0.72x / 1.42x | WIN / LOSE |
| 12 | dump-ir examples/desk-app | 0.370 s / 178 MB | 0.220 s / 186 MB | 0.59x / 1.04x | WIN / LOSE |
| 13 | dump-bc scripted/fighter.j3s | 0.017 s / 19 MB | 0.014 s / 22 MB | 0.82x / 1.14x | WIN / LOSE |
| 14 | doc x3 | 0.195 s / 27 MB | 0.057 s / 27 MB | 0.29x / 1.00x | WIN / WIN |
| 15 | new + first check | 0.019 s / 19 MB | 0.017 s / 22 MB | 0.89x / 1.13x | WIN / LOSE |
| 16 | binary size, stripped | 13.43 MB | 12.23 MB | 0.91x | WIN |

Left: RSS on 3, 7 (module-hash step allocates), 8, 9, 12 (AST/typed sizes), 11, 13, 15 (libSDL2 + ~40 libs loaded at
startup via `use lib "SDL2"` in j3.game; needs a frozen-file/stdlib decision). Rows 11/13/14/15 now use a K-loop (3 decimals).

### 2026-11-18, phase 1 (j3c-boot at `--opt 2`): wall 8 WIN 2 TIE 6 LOSE, RSS 0/15 WIN, size WIN
| # | Metric | stage0 | j3c-boot dev | j3c-boot opt | opt/stage0 wall / RSS | opt wall / RSS |
|---|---|---|---|---|---|---|
| 1 | self build cold | 4.05 s / 1123 MB | 13.67 s / 1925 MB | 7.73 s / 1921 MB | 1.91x / 1.71x | LOSE / LOSE |
| 2 | self build warm | 1.32 s / 505 MB | 2.35 s / 913 MB | 0.72 s / 905 MB | 0.55x / 1.79x | WIN / LOSE |
| 3 | self build leaf edit | 1.72 s / 528 MB | 4.34 s / 1689 MB | 1.48 s / 1680 MB | 0.86x / 3.18x | WIN / LOSE |
| 4 | editor program cold | 1.34 s / 261 MB | 2.31 s / 331 MB | 1.39 s / 327 MB | 1.04x / 1.25x | LOSE / LOSE |
| 5 | desk-app cold | 1.16 s / 384 MB | 2.91 s / 511 MB | 1.45 s / 508 MB | 1.25x / 1.32x | LOSE / LOSE |
| 6 | editor_village cold | 1.63 s / 166 MB | 2.29 s / 226 MB | 1.64 s / 223 MB | 1.01x / 1.34x | LOSE / LOSE |
| 7 | desk-app leaf edit | 0.50 s / 171 MB | 1.18 s / 404 MB | 0.41 s / 394 MB | 0.82x / 2.29x | WIN / LOSE |
| 8 | check corpus (244 inputs: sum, max RSS) | 38.16 s / 218 MB | 106.13 s / 1204 MB | 33.14 s / 1196 MB | 0.87x / 5.48x | WIN / LOSE |
| 9 | check grid_test | 0.51 s / 151 MB | 1.58 s / 1204 MB | 0.61 s / 1196 MB | 1.20x / 7.92x | LOSE / LOSE |
| 10 | check j3c-bootstrap | 1.27 s / 505 MB | 2.32 s / 914 MB | 0.72 s / 905 MB | 0.57x / 1.79x | WIN / LOSE |
| 11 | fmt --check j3c/stdlib/j3 | 0.09 s / 4 MB | 0.29 s / 8 MB | 0.07 s / 6 MB | 0.78x / 1.40x | WIN / LOSE |
| 12 | dump-ir examples/desk-app | 0.37 s / 177 MB | 0.94 s / 400 MB | 0.29 s / 391 MB | 0.78x / 2.20x | WIN / LOSE |
| 13 | dump-bc scripted/fighter.j3s | 0.01 s / 19 MB | 0.05 s / 36 MB | 0.01 s / 30 MB | 1.00x / 1.59x | TIE / LOSE |
| 14 | doc + doc j3.game --short + doc search mesh | 0.19 s / 27 MB | 1.16 s / 56 MB | 0.24 s / 52 MB | 1.26x / 1.88x | LOSE / LOSE |
| 15 | new + first check | 0.01 s / 19 MB | 0.06 s / 36 MB | 0.01 s / 29 MB | 1.00x / 1.51x | TIE / LOSE |
| 16 | binary size, stripped | 13.43 MB | 24.24 MB | 12.20 MB | 0.91x | WIN |

## Phases
1. **Bench + free wins:** write `bench.sh`; build j3c-boot optimized (`opt = 2` or `--release`, stripped); rebench;
   write the scoreboard into `SPEED.md`.
2. **Profile:** per losing metric, find where the time and memory go (phase timings in `target/compilation.log`; `perf`
   if installed). Turn the findings into disjoint hotspot chunks.
3. **Hotspot waves** (parallel, disjoint files, each gated): candidates from the baseline are lexer/parser per-token cost,
   the module-hash step (prints IR text and then hashes it), clang fan-out (24 workers x 4 clangs on 24 cores), arena growth
   in check/lower on big UI modules, `doc` startup indexing, binary size (embedded text, debug info, dead code).
4. Repeat 2-3 until every metric is green; then the full gate and a final scoreboard.

## Status
- [x] Phase 1 (2026-11-18: `tools/bench.sh`; `diff.sh` builds j3c-boot `--opt 2`; scoreboard in `SPEED.md`: wall 8 WIN 2 TIE 6 LOSE, RSS 0 WIN, size WIN)
- [x] Phase 2 (profiled by wave 1 chunks; perf works)
- [x] Phase 3 (wave 1: wall 15/15, RSS 7/15; wave 2 2026-11-18: wall 15/15, RSS 15/15, size WIN)
- [ ] Phase 4: widen thin margins (7/12 RSS, 4/6 wall) if wanted; final review
