# Speed baseline: j3c-boot vs stage0 (plan 11 Part D item 0)

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
