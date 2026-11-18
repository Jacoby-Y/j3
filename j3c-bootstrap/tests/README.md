# Tests

Differential first: run the Rust `j3c` (`stage0/j3c`, the oracle) and `j3c-boot` on the same input and
compare the text. Unit checks live beside the code they cover, as `*_test.j3` files the way
`j3c/stdlib/tests/` does it.

## `diff.sh` (plan 11 A4)

`tests/diff.sh MODE [--norm] [--user] [--es] [--no-coverage] [--update-known] [-j N] [files...]`, from any folder.

1. Runs `tools/coverage.sh` (skip with `--no-coverage` or `DIFF_NO_COVERAGE=1`) and `tools/layers.sh`;
   either failing stops the run (exit 2).
2. Builds `target/j3c-boot` with `stage0/j3c build j3c-bootstrap` when it is missing or older than `src/`.
3. Runs every input through stage0 twice and j3c-boot once, in parallel, each under `timeout`
   (`DIFF_TIMEOUT`). Two stage0 runs that disagree print `NONDET` (the oracle is not deterministic there).
4. Prints `ok FILE`, `DIFF FILE` (both ran, the text differs) or `CRASH FILE` (j3c-boot failed, timed out
   or died where stage0 did not), the first 40 diff lines of each new failure, and a summary line
   `diff.sh MODE: N ok, N DIFF, N CRASH` to paste into a chunk report.

| Mode | Command on both sides | Compared |
|---|---|---|
| `fmt` | `fmt FILE` | stdout, stderr, exit code |
| `ast` | `dump-ast FILE` | stdout, stderr, exit code |
| `err` | `check FILE` (on a copy in a scratch folder, so no `j3.toml` above it applies); `check .` in a copy of a project folder (`bad-ui/span_*`) | stdout, stderr, exit code |
| `ir` | `dump-ir PATH` (`--norm`: temps renumbered by `norm_ir.awk`, plan A6) | stdout, stderr, exit code |
| `llvm` | `dump-llvm PATH` | stdout, stderr, exit code |
| `glsl` | `dump-glsl PATH` (`--es`: `dump-glsl PATH --es`, the OpenGL ES text) | stdout, stderr, exit code |
| `ui` | `dump-ui PROJECT/ui` (the J3 source generated from a project's `.j3ui` files) | stdout, stderr, exit code |
| `run` | `run --headless --frames 3 PATH`, offscreen SDL, dummy audio | stdout without the `J3STATS` / `J3UISTATS` timing lines, exit code |
| `self` | not yet: needs a j3c-boot that builds j3c-bootstrap (stage 2 = stage 1) | |

Exit: 0 when every input is `ok` or listed as known, 1 on a new failure, a `NONDET`, or a known input
that now passes, 2 on a setup error. `DIFF_STAGE0` / `DIFF_BOOT` swap either compiler (testing the
harness, or comparing stage 2 later).

## Corpus and known failures

- `corpus.txt`: inputs per mode, one path or glob per line under `[mode mode ...]` section headers. A
  folder is a project for `ir`/`llvm`/`run` (a folder without `j3.toml` means every project under it);
  `fmt`/`ast`/`err` take every `.j3` under it, skipping `target/` and `*.d/`.
- `bad/`: parse, name and type errors for `err`, beside the Rust goldens in
  `j3c/src/compile/tests/diag/`.
  A case that needs more files (a `.j3s` script) keeps them in `bad/NAME.d/`, copied beside `NAME.j3`
  for the check (`*.d` folders are never inputs themselves). A `NAME.d/` with a `j3.toml` makes the case a
  project: `check .` in the copy, entry `NAME.j3`, modules in `NAME.d/src/`.
- `bad-ui/`: one small UI project per `.j3ui` error (`j3.toml` with `[ui]`, `src/main.j3`, `ui/*.j3ui`; the
  `.j3ui` says in a comment which error it shows). `ui` runs `dump-ui NAME/ui` on each and compares the error
  text; the `span_*` ones are also `err` cases (`check .` in a scratch copy): their error is found in the
  generated J3 and must be shown at the `.j3ui` line (the span map).
- `proj/`: small multi-module projects for `fmt`/`ast`/`ir`/`llvm`/`run` (rules that need two modules, such
  as an instance body lowered with the names of the module that uses it: `merged_names`, `merged_types`).
- `known/MODE.txt`: inputs that fail today, one per line. It only shrinks: a listed input that passes
  fails the run until its line is removed. `--update-known` rewrites the list from the current run
  (keeping entries for inputs this run did not cover).
- `known/ir-user.txt`: the known list for `ir --user` (entry module only), kept apart from the full-file `ir.txt`.
- `known/glsl-es.txt`: the known list for `glsl --es`, kept apart from `glsl.txt`.

## Other files

- `cases/`: compiler cases as J3 files with expectations in their headers (plan B9), see
  `cases/README.md`.
- `SCALE.md`: build time and memory of the oracle, the bar for the port (plan A11).
