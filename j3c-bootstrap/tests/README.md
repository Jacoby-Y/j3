# Tests

Differential first: run the Rust `j3c` (`stage0/j3c`, the oracle) and `j3c-boot` on the same input and
compare the text. Unit checks live beside the code they cover, as `*_test.j3` files the way
`j3c/stdlib/tests/` does it.

## `diff.sh` (plan 11 A4)

`tests/diff.sh MODE [--norm] [--user] [--es] [--no-coverage] [--update-known] [--list] [-j N] [files...]`, from any folder.

1. Runs `tools/coverage.sh` (skip with `--no-coverage` or `DIFF_NO_COVERAGE=1`) and `tools/layers.sh`;
   either failing stops the run (exit 2).
2. Builds `target/j3c-boot` with `stage0/j3c build j3c-bootstrap` when it is missing or older than `src/`.
3. Runs every input through stage0 twice and j3c-boot once, each under `timeout` (`DIFF_TIMEOUT`). Two
   stage0 runs that disagree print `NONDET` (the oracle is not deterministic there). Inputs run in parallel:
   `-j N`, else `J3_TEST_JOBS`, else half the cores (a quarter for `llvm` and `self`'s llvm calls: a big
   project's dump holds ~1 GB and writes ~80 MB per side). Each input writes only its own scratch folder;
   `run` inputs that build in one project run one after another in one job (they share its `target/`);
   `bc` and `tool` inputs each copy their project, so every one is its own job.
   Output is printed in input order after all jobs end, the same as with `J3_TEST_JOBS=1`.
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
| `run` | `run --headless --frames 3 PATH`, offscreen SDL, dummy audio | stdout without the `J3STATS` / `J3UISTATS` timing lines and with the number before ` us` / ` ms` / ` ns` / ` s` masked (tests print their timings), exit code |
| `bc` | every compiler run on its own scratch copy (`mktemp -d`, `tar --exclude=./target`), never the tree. A project: `run --headless --frames 3 .` (dev mode: scripts on the VM). A `.j3s` file: `dump-bc REL` in a copy of its project (of its folder when that project is j3c-bootstrap). Inputs: the `[bc]` section of `corpus.txt` | a project: exit code, the sorted `sha256sum` of every `*.j3sb` it wrote (stdout has timings: `run` covers it) and, when it fails, stderr without the `[n/6]` lines (a refusal text); a file: stdout (the dump), stderr, exit code and the `.j3sb` hashes |
| `tool` | one case per line of `tool/*.txt` (C5; id `STEM/NAME`): `NAME<TAB>PROJECT\|-<TAB>ARGV[<TAB>OPTION]...`, run in a fresh scratch copy of `PROJECT` (`-`: an empty folder) per compiler run, never the tree. OPTIONs: `stdin=FILE` (under `tool/`), `out=GLOB` (`**` recurses), `unpack=GLOB` (`.zip`, `.deb`, `.tar.gz`), `mask=SED` (`sed -E`), `env=K=V` (`@ROOT@` = the repo), `timeout=N` | stdout, stderr, exit code, each `out=` file (text, or sha256 when binary, with its mode), each `unpack=` archive's members and unpacked files (mode, sha256; times left out); built-in masks: the scratch path (`$DIR`), the `docs` id, the MCP `serverInfo` version, timings |
| `self` | stage 1 (`j3c-boot`) builds stage 2 from a scratch copy of `j3c-bootstrap/{j3.toml,src,text}` under `<repo>/target/self.PID/` (never in place), then `DIFF_STAGE0=stage1 DIFF_BOOT=stage2` over `fmt`, `ast`, `err`, `ui`, `glsl`, `glsl --es`, `ir --norm` and `llvm` (one input per call) | one summary line per comparison, then `diff.sh self: N ok, N failed` |


`run` removes a project's `target/.j3c` (stamps and objects) and `target/j3s` (a dev run's bytecode) before each of
the three runs and after the last, so no compiler reuses another's build and the corpus keeps no build output. A
project with `.j3s` scripts runs in dev mode in both compilers (bytecode VM, `cmd/devrun.j3`); `bc` compares its
bytecode, `run` its stdout. A file input inside a project (a `j3.toml`
above it) builds that project, so its folder is cleared the same way; a file outside any project builds to a temp
file, writes no stamps and runs in a scratch cwd (what it writes by relative path is removed). `self` prints one summary line per clean comparison and the whole output (the DIFF /
CRASH detail) of a failed one. `--list` prints the expanded inputs and exits.
A known-list line with a `# skip: CAUSE` comment is never run (`tls_live_test.j3`: network).
A line with `# nondet: CAUSE` names an input whose stage0 output changes from run to run (timings): it runs, and
no outcome of it fails the run.

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
- `known/bc.txt`: the known list for `bc` (empty since the end of C4).
- `tool/`: the `tool` cases, one file per C5 chunk (`tool/STEM.txt`, its session files in `tool/STEM/`);
  `dispatch.txt` checks that every route of `cmd/args.j3` reaches its command. `known/tool.txt`: its known list.

## `unit.sh`

`tests/unit.sh [NAME...]` builds and runs `tests/unit/NAME_test.j3` (all of them by default) as its own
package in `target/unit/NAME/`, in parallel (`J3_TEST_JOBS`, default half the cores), and prints `ok NAME` /
`FAIL NAME` in name order, then `unit.sh: N ok, N FAIL`. A test writes only under its own name (scratch
folders are `target/unit/NAME`, `target/NAME`, or carry the pid), so tests never share a folder.

## Other files

- `cases/`: compiler cases as J3 files with expectations in their headers (plan B9), see
  `cases/README.md`.
- `SCALE.md`: build time and memory of the oracle, the bar for the port (plan A11).
