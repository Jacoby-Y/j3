# Speed: j3c-boot vs stage0

## Scoreboard

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

### 2026-11-18, plan 07 phase 1

`tools/bench.sh` (median of 3, wall and peak RSS from `/usr/bin/time -f '%e %M'`, scratch copies under
`<repo>/target/bench/`; metric definitions in `plans/07-speed.md` and the script). **dev** = j3c-boot as `j3.toml`
builds it (`opt = 0`, `debug = true`); **opt** = `stage0/j3c build --opt 2 j3c-bootstrap` (ThinLTO per module, mold),
which `tests/diff.sh` now builds. The self-build metrics (1-3) build j3c-bootstrap's own `j3.toml` (opt 0) with
either compiler. The C5 gate stays green with the opt binary (unit 120 ok, test-compiler 25/25, fmt/ast/err/ui/glsl,
ir --norm 246 + j3c-bootstrap, llvm per file, run 244 ok, bc 19 ok, tool 937 ok + 5 known, self 67 + 186 ok, object cache).
`--opt 3` measured the same as 2; `--release` (opt s, stripped) is 10.8 MB but ~7% slower. A 0.01 s row is at the
timer's resolution.

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

Wall: 8 WIN, 2 TIE, 6 LOSE. RSS: 0 of 15 WIN (opt changed RSS by under 2%: the memory is data, not code).
Size: WIN.

Where the losing metrics go (cold phases from `target/compilation.log`, stage0 / opt):
- **1 self cold, wall:** parse 0.10 / 0.20 s, check 1.16 / 0.46 s, hash modules 0.32 / 0.58 s, compile 2.28 / 6.29 s
  wall. Emit CPU is now lower (17.3 / 9.4 s) and the slowest single module is the same (~2.3 s), but "clang cpu" is
  36.9 / 63.6 s: clang fan-out oversubscribes (24 workers x up to 4 clangs per split module; boot's "clang cpu" is the
  wall time of each clang, so contention shows there). *Guess:* one shared clang pool of 24 closes most of the 4 s.
- **4, 5, 6 cold game builds, wall:** desk-app parse 0.06 / 0.09 s, check 0.31 / 0.16 s, hash 0.05 / 0.06 s, compile
  0.65 / 1.04 s ("clang cpu" 11.0 / 14.3 s, emit cpu 3.4 / 2.3 s). Same fan-out cause as 1; editor_village is
  link-bound in both, so it is a tie.
- **9 grid_test, wall and RSS:** 1.2 GB against 151 MB. *Guess:* arena growth through lower/check of the big
  generated UI module, never reset; time follows the page faults (the same cause as the 5.5x RSS of metric 8).
- **14 doc, wall and RSS:** *guess:* every call parses/indexes the embedded stdlib and doc text before answering
  (Rust looks up lazily); 52 MB is that index.
- **RSS everywhere (1-3, 10, 12: 1.8-3.2x):** *guess:* per-session arenas sized for the whole program plus IR text
  kept for the module-hash step (the hash prints each module's IR and hashes the text); the leaf self build peaks at
  1.7 GB against 0.5 GB. 13 and 15 (30 MB against 19 MB) are startup: the binary's data and the embedded text.
- **13, 15 wall:** ties at the 10 ms timer resolution.

# Baseline (before plan 07)
Plan 11 Part D item 0.

Measured 2026-11-18, after C5 and before C6. This is a baseline only: no code was tuned.

- **Machine:** Intel Core Ultra 7 270K Plus (24 cores: 8 P + 16 E, no SMT), Linux 7.1.5, otherwise idle.
  Clang 18.1.3 (Ubuntu) does the codegen for **both** compilers, so clang time is a shared cost, not a
  compiler difference.
- **Compilers:** `stage0/j3c` (Rust oracle, commit ac4fa2c) and `target/j3c-boot` (built by
  `stage0/j3c build j3c-bootstrap` at 3b3a625, dev build: `opt = 0`, `debug = true`).
- **Method:** every number is the median of 3 runs of `/usr/bin/time -f '%e s %M KB'` (wall time and
  peak RSS; `%M` is the largest single process in the tree, which for builds is the compiler and not
  clang). Each compiler builds its own scratch copy under `<repo>/target/speed/` (`tar --exclude=./target`).
  Cold means a fresh copy with no `target/`. The object cache is `target/.j3c/host/<bin>` inside the
  project (`toolchain.cacheDir` / Rust `emit::host::cache_dir`), so a fresh copy is a cold cache.
  Warm means building again with nothing changed. Leaf means appending a comment line to one module
  before each build (`src/cmd/guide.j3`, `j3c/editor/src/console.j3`, desk-app `src/prefs.j3`,
  editor_village `src/settings.j3`).
  Env: `J3C_NO_INSTALL=1 SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy`. The scratch copy of the
  bootstrap carries `../j3c` too (minus `target/`), for its `@embed`s and the `j3_editor` dependency.
  Phase splits come from each run's `target/compilation.log` block. Both compilers write the same
  breakdown, but j3c-boot leaves out the `check:` sub-steps.

## 1. Self build of j3c-bootstrap (168 modules)

`cd <copy>/j3c-bootstrap && J3C build .`

| Case | stage0 wall | stage0 RSS | j3c-boot wall | j3c-boot RSS | ratio wall | ratio RSS |
|---|---|---|---|---|---|---|
| cold | 4.08 s | 1089 MB | 13.57 s | 1925 MB | **3.3x** | 1.8x |
| warm (nothing changed) | 1.30 s | 505 MB | 2.35 s | 913 MB | 1.8x | 1.8x |
| leaf edited | 1.73 s | 528 MB | 4.35 s | 1689 MB | 2.5x | 3.2x |

Neither compiler takes the up-to-date fast path on the self build. Both parse and check everything,
then report `up to date`.

Phases (cold):

| Phase | stage0 | j3c-boot | ratio |
|---|---|---|---|
| parse | 0.10 s | 0.77 s | 7.7x |
| check | 1.15 s | 1.49 s | 1.3x |
| emit: hash modules | 0.32 s | 1.74 s | 5.4x |
| compile (wall) | 2.42 s | 9.33 s | 3.9x |
| compile: emit cpu | 17.9 s | 53.6 s | 3.0x |
| compile: "clang cpu" | 37.1 s | 119.2 s | 3.2x |
| link | 0.13 s | 0.19 s | 1.5x |

Leaf edit: j3c-boot spends parse 0.78 + check 1.53 + hash 1.76 + link 0.19 s. Stage0 spends
0.10 + 1.17 + 0.32 + 0.11 s. The table shows `168 of 168 modules cached`, even though the edited
module was built again.

## 2. Editor program (`j3c/editor/src`, 76 modules)

The editor is built the way `editor/frontend.rs` builds it: a folder with `src/`, `font.ttf` and the
generated `j3.toml`, then `J3C build .`.

| Case | stage0 wall | stage0 RSS | j3c-boot wall | j3c-boot RSS | ratio wall | ratio RSS |
|---|---|---|---|---|---|---|
| cold | 1.35 s | 258 MB | 2.33 s | 331 MB | 1.7x | 1.3x |
| warm, first rebuild (re-check, all cached) | 0.27 s | 117 MB | 0.63 s | 277 MB | 2.3x | 2.4x |
| warm, no-op (stamp hit, runs 2-3) | 0.01 s | 64 MB | 0.00 s | 10 MB | n/a | n/a |
| leaf edited | 0.35 s | 118 MB | 0.84 s | 284 MB | 2.4x | 2.4x |

Cold phases: parse 0.04 / 0.27 s, check 0.21 / 0.35 s, compile 1.00 / 1.42 s (emit cpu 2.1 / 5.5 s,
"clang cpu" 15.8 / 22.0 s).

## 3. Big games

| Project | Case | stage0 wall | stage0 RSS | j3c-boot wall | j3c-boot RSS | ratio wall | ratio RSS |
|---|---|---|---|---|---|---|---|
| desk-app (108 modules, opt 0) | cold | 1.19 s | 391 MB | 3.00 s | 512 MB | **2.5x** | 1.3x |
| | warm, first rebuild | 0.39 s | 171 MB | 0.88 s | 387 MB | 2.3x | 2.3x |
| | warm, no-op | 0.01 s | 64 MB | 0.00 s | 10 MB | n/a | n/a |
| | leaf edited | 0.51 s | 172 MB | 1.18 s | 405 MB | 2.3x | 2.4x |
| editor_village (37 modules, opt 2) | cold | 1.65 s | 162 MB | 2.31 s | 227 MB | 1.4x | 1.4x |
| | warm, no-op (all 3 runs) | 0.01 s | 64 MB | 0.00 s | 10 MB | n/a | n/a |
| | leaf edited | 0.31 s | 82 MB | 0.62 s | 201 MB | 2.0x | 2.4x |

desk-app cold phases: parse 0.06 / 0.35 s, check 0.31 / 0.49 s, hash 0.05 / 0.22 s, compile 0.66 / 1.76 s
(emit cpu 3.3 / 9.8 s, "clang cpu" 11.1 / 22.9 s). The two builds write nearly the same objects:
282 `.o` each, 14.77 MB vs 14.88 MB of cache, and binaries of 6.09 MB vs 6.15 MB. editor_village is
link-bound in both (0.82 / 0.84 s link at opt 2).

## 4. `check` over the corpus

The inputs are the `[fmt ast ir llvm run]` section of `tests/corpus.txt`, expanded: 245 inputs.
A folder without a `j3.toml` counts as every project under it. A project is checked with `check .`
inside it, and a file with `check FILE`. The runs are sequential, on a scratch mirror of
`speculative/`, `examples/`, `j3c/stdlib/tests/` and `tests/proj/`.

| | stage0 | j3c-boot | ratio |
|---|---|---|---|
| total wall | 39.78 s | 107.71 s | **2.7x** |
| peak RSS (worst input) | 218 MB | 1204 MB | **5.5x** |
| median per input | 0.08 s | 0.26 s | 3.3x |

The exit codes match on all 245 inputs. Both compilers fail `speculative/09_stdlib_game.j3` and
`j3c/stdlib/tests/win_os_test.j3`.

These inputs are the slowest on j3c-boot. All except one are UI tests, and their memory grows the most:

| Input | stage0 | j3c-boot | ratio | RSS stage0 / boot |
|---|---|---|---|---|
| `j3c/stdlib/tests/grid_test.j3` | 0.52 s | 1.58 s | 3.0x | 151 / 1204 MB |
| `j3c/stdlib/tests/text_css_test.j3` | 0.34 s | 1.56 s | 4.6x | 149 / 1184 MB |
| `j3c/stdlib/tests/dom_test.j3` | 0.32 s | 1.19 s | 3.7x | 139 / 814 MB |
| `examples/big_ui_test` | 0.46 s | 1.19 s | 2.6x | 218 / 495 MB |
| `j3c/stdlib/tests/input_test.j3` | 0.32 s | 1.14 s | 3.6x | 155 / 718 MB |

## 5. `doc` startup

| Command | stage0 | j3c-boot | ratio | RSS stage0 / boot |
|---|---|---|---|---|
| `doc` | 0.09 s | 0.53 s | **5.9x** | 28 / 57 MB |
| `doc j3.game --short` | 0.01 s | 0.07 s | ~7x | 21 / 31 MB |
| `doc search mesh` | 0.08 s | 0.54 s | **6.8x** | 28 / 56 MB |
| `version` | <0.01 s | <0.01 s | n/a | 3 / 6 MB |

## 6. Front end

The stdlib files are in a scratch copy. `fmt --check` exits 1 on both compilers, because the same
files need formatting.

| Command | stage0 | j3c-boot | ratio | RSS stage0 / boot |
|---|---|---|---|---|
| `dump-ast j3/gfx/shader.j3` (44 KB, largest stdlib file) | <0.01 s | 0.01 s | n/a | 5 / 12 MB |
| `dump-ast j3/text/pow10.j3`, `j3/ui/dom.j3` | <0.01 s | <0.01 s | n/a | 5 / 10-11 MB |
| `fmt --check j3c/stdlib/j3` (whole tree) | 0.09 s | 0.29 s | 3.2x | 5 / 8 MB |
| `fmt --check j3c-bootstrap/src` (whole tree) | 0.10 s | 0.27 s | 2.7x | 5 / 9 MB |
| `dump-ir speculative/08_game_loop.j3` | 0.07 s | 0.21 s | 3.0x | 47 / 102 MB |
| `dump-ir examples/desk-app` | 0.37 s | 0.93 s | 2.5x | 179 / 401 MB |

## 7. Commands

The script that ran all of this was a throwaway, but the calls were these (C = either compiler):

```
C build .                         # cold: fresh copy; warm: again; leaf: echo '// x' >> LEAF; C build .
C check . | C check FILE          # every corpus input, sequential
C doc ; C doc j3.game --short ; C doc search mesh ; C version
C dump-ast F ; C fmt --check DIR ; C dump-ir F|.
```

## Where the time goes (biggest gaps)

1. **Parse is 7.7x slower** (self build: 0.10 s to 0.77 s; desk-app 0.06 s to 0.35 s). The
   `doc` and `fmt --check` numbers (3-7x) point the same way. *Guess:* the lexer and parser of a
   dev build (`opt = 0`, debug) do a lot of per-token work through arena `Text`/`List` and bounds
   checks. The doc startup (`doc` 0.53 s) probably pays to parse or index the embedded doc and stdlib
   text on every call. The first thing to try: build j3c-boot with `opt = 2` and measure again before
   tuning anything.
2. **`emit: hash modules` is 5.4x slower** (0.32 s to 1.74 s), and every warm or leaf build pays it.
   It is 40% of the leaf rebuild. *Guess:* the key pass prints each module's IR with `dumpModule` and
   hashes that text (`cache.j3`), where Rust feeds `DefaultHasher` directly. The print and its
   allocations cost more than the hash.
3. **The compile phase is 3.9x slower on wall time, and clang CPU counts 3.2x more for the same
   objects** (37 s to 119 s, for objects of nearly the same total size). *Guess:* there is
   oversubscription. `jobCap()` = 24 forked workers, and each split module runs up to 4 clangs side
   by side, so up to ~96 processes compete for 24 cores. Rust keeps one pool of 24. j3c-boot's
   "clang cpu" is also the wall time of each clang (`monotonicMs`), not CPU time, so contention
   inflates it. Emit CPU is 3x slower too (53.6 s against 17.9 s), which is the opt-0 codegen of the
   emitter itself (j3c.load and j3c.sema each need ~3-5 s of emit).
4. **`check` memory: 5-8x peak RSS on the UI tests** (grid_test 151 MB to 1.2 GB), and the leaf
   self build uses 1.7 GB against 0.5 GB. *Guess:* arenas grow without bound across the
   lower/check passes of big generated UI modules, and the per-session arenas are never reset
   (rule B6 removed the globals, but not the growth). Time follows memory here: these inputs are the
   3-4.6x outliers.
5. **Check itself is close** (1.3x on the self build: 1.15 s against 1.49 s). The "redesigned"
   `check` (B11) is not the bottleneck. Parse, hashing and process fan-out are.

Scratch output under `target/speed/` was deleted after the run.
