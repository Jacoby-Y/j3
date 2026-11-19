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
- [ ] Phase 2
- [ ] Phase 3+
