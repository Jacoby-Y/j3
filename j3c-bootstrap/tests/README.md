# Tests

Expected output first: `diff.sh` runs `target/j3c` on every corpus input and compares its text with a checked-in
file in `expect/`. Those files began as the Rust oracle's output (stage 0, the compiler at the git tag `rust-final`, until plan 15); since then
a change to them is made with `--bless` and reviewed in git, like `j3c golden`. Unit checks live beside the code
they cover, as `*_test.j3` files the way `j3c/stdlib/tests/` does it.

## `diff.sh` (plan 11 A4)

`tests/diff.sh MODE [--bless] [--norm] [--user] [--es] [--list] [-j N] [files...]`, from any folder.

1. Runs `tools/layers.sh`; a failure stops the run (exit 2).
2. Builds `target/j3c` with the seed (`tests/boot.sh`, shared with `unit.sh`; `J3_SEED`, else the first of
   `tools/seed.sh list`, reading this tree's stdlib) as `build --opt 2 j3c-bootstrap` (optimized; plan 13) when
   it is missing, older than `src/`, or a `./build.sh` binary with a git version (the expected files hold the
   plain `J3C_VERSION`). `self` still builds stage 2 the way `j3.toml` says (`opt = 0`).
3. Runs every input through `target/j3c` once, under `timeout` (`DIFF_TIMEOUT`), and compares the normalized
   text with `expect/KIND/INPUT.out` (KIND is the mode, `ir-user` for `ir --user`, `glsl-es` for `glsl --es`;
   INPUT is the repo-relative input path or the tool case id). `--bless` runs each input twice instead: two runs
   that disagree print `NONDET` and write nothing (add a mask), else the expected file is written. A
   whole-corpus run also names expected files no input produces (`stale`, a failure); a whole-corpus bless
   deletes them. `DIFF_REF=J3C` compares with another compiler's output instead (`self` uses it; it also shows
   the text behind a changed hash: `DIFF_REF=~/.bin/j3c.prev diff.sh llvm FILE`). Inputs run in parallel:
   `-j N`, else `J3_TEST_JOBS`, else half the cores (a quarter for `llvm` and `self`'s llvm calls: a big
   project's dump holds ~1 GB and writes ~80 MB per side). Each input writes only its own scratch folder;
   `run` inputs that build in one project run one after another in one job (they share its `target/`);
   `bc` and `tool` inputs each copy their project, so every one is its own job.
   Output is printed in input order after all jobs end, the same as with `J3_TEST_JOBS=1`.
   Half the cores at most (the rule for every repo script): each job's builds get `J3_JOBS` = (nproc/2) / jobs
   clang processes (at least 1); the `boot.sh` and `self` stage 2 builds, which run alone, get nproc/2.
4. Prints `ok FILE`, `blessed FILE` (written), `DIFF FILE` (the text differs from the expected file, or there is
   none) or `CRASH FILE` (the harness failed on the input), the first 40 diff lines of each failure, and a summary
   line `diff.sh MODE: N ok, N DIFF, N CRASH` to paste into a chunk report.

The compared text is stdout, `--- stderr`, stderr and `--- exit N`, so a changed exit code or diagnostic fails.
Masked everywhere: the repo folder (`$ROOT`) and the home folder (`$HOME`). A stdout or stderr over 64 KiB
becomes one `--- sha256 HASH, N lines, N bytes` line: whole-program `ir` and `llvm` dumps carry the stdlib and
are megabytes each (all of `llvm`, most of `ir`; `ir-user` keeps the entry module as text). `fmt` stores stdout as
a diff against the input (empty for a formatted file). Editing a corpus input, the stdlib or the compiler's
output changes expected files, so the same change re-blesses the affected modes (`diff.sh MODE --bless`, then
`git diff j3c-bootstrap/tests/expect`). Routinely edited code is kept out of the corpus for that reason:
`fmt`/`ast` take fixtures, `speculative/`, `examples/` and test files, never `j3c-bootstrap/src` or the stdlib
modules (a stdlib edit still moves the whole-program `ir`/`llvm` hashes).

When to bless: after a deliberate output change (new diagnostic wording, a lowering change, an edited corpus
file), run the modes it touches with `--bless` and read the diff before committing; `blessed FILE` lines name
every rewritten file.

| Mode | Command | Compared |
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
| `self` | stage 1 (`target/j3c`) builds stage 2 from a scratch copy of `j3c-bootstrap/{j3.toml,src,text}` under `<repo>/target/self.PID/` (never in place), then `DIFF_REF=stage1 DIFF_BOOT=stage2` over `fmt`, `ast`, `err`, `ui`, `glsl`, `glsl --es`, `ir --norm` and `llvm` (one input per call) | one summary line per comparison, then `diff.sh self: N ok, N failed` |


`run` removes a project's `target/.j3c` (stamps and objects) and `target/j3s` (a dev run's bytecode) before each
run and after the last, so no run reuses another's build and the corpus keeps no build output. A
project with `.j3s` scripts runs in dev mode in both compilers (bytecode VM, `cmd/devrun.j3`); `bc` compares its
bytecode, `run` its stdout. A file input inside a project (a `j3.toml`
above it) builds that project, so its folder is cleared the same way; a file outside any project builds to a temp
file, writes no stamps and runs in a scratch cwd (what it writes by relative path is removed). `self` prints one summary line per clean comparison and the whole output (the DIFF /
CRASH detail) of a failed one. `--list` prints the expanded inputs and exits.
A corpus line `!PATH  # CAUSE` drops that input from its section's modes and prints `skip PATH`
(`tls_live_test.j3` in `run`: network).

Exit: 0 when every input is `ok` (or `blessed`), 1 on a `DIFF`, `CRASH`, `NONDET` or stale file, 2 on a setup
error. `DIFF_BOOT` swaps the compiler under test, `DIFF_REF` the reference (see 3).

## Corpus and expected files

- `expect/KIND/`: the expected output per input (see `diff.sh` above), mirrored by input path.
- `corpus.txt`: inputs per mode, one path or glob per line under `[mode mode ...]` section headers. A
  folder is a project for `ir`/`llvm`/`run` (a folder without `j3.toml` means every project under it);
  `fmt`/`ast`/`err` take every `.j3` under it, skipping `target/` and `*.d/`.
- `bad/`: parse, name and type errors for `err`, beside the Rust goldens in
  `diag/` (moved from `j3c/src/compile/tests/diag/` at C6).
  A case that needs more files (a `.j3s` script) keeps them in `bad/NAME.d/`, copied beside `NAME.j3`
  for the check (`*.d` folders are never inputs themselves). A `NAME.d/` with a `j3.toml` makes the case a
  project: `check .` in the copy, entry `NAME.j3`, modules in `NAME.d/src/`.
- `bad-ui/`: one small UI project per `.j3ui` error (`j3.toml` with `[ui]`, `src/main.j3`, `ui/*.j3ui`; the
  `.j3ui` says in a comment which error it shows). `ui` runs `dump-ui NAME/ui` on each and compares the error
  text; the `span_*` ones are also `err` cases (`check .` in a scratch copy): their error is found in the
  generated J3 and must be shown at the `.j3ui` line (the span map).
- `proj/`: small multi-module projects for `fmt`/`ast`/`ir`/`llvm`/`run` (rules that need two modules, such
  as an instance body lowered with the names of the module that uses it: `merged_names`, `merged_types`).
- `tool/`: the `tool` cases, one file per C5 chunk (`tool/STEM.txt`, its session files in `tool/STEM/`);
  `dispatch.txt` checks that every route of `cmd/args.j3` reaches its command.

## `unit.sh`

`tests/unit.sh [NAME...]` builds and runs `tests/unit/NAME_test.j3` (all of them by default; one test:
`tests/unit.sh lower_plain`) as its own package in `target/unit/NAME/`, with `target/j3c` (`tests/boot.sh`
rebuilds it with the seed when it is missing or stale, as `diff.sh` does), in parallel (`J3_TEST_JOBS`,
default 2; each build gets `J3_JOBS` = (nproc/2) / `J3_TEST_JOBS`, so the suite stays within half the cores;
a `J3_JOBS` you set wins), and prints `ok NAME` / `FAIL NAME` (with the tail of its log) in name order, then
`unit.sh: N ok, N FAIL`. A test writes only under its own name (scratch folders are `target/unit/NAME`,
`target/NAME`, or carry the pid), so tests never share a folder. Cost: one test build is about 3.5 s and
400 MB; the whole suite runs in a few minutes (each package compiles the `j3c` modules it imports again; the
object cache is per package).

Expected output: a test that compares a text it made (an IR or AST dump, diagnostics, a tool table) reads
`tests/unit/expect/NAME...` through the `unitkit` package (`tests/unit/kit`, a dependency of every test
package): `unitkit.expect(got, FILE, WHAT)` panics on a difference and prints the first differing line;
`unitkit.mask` swaps a machine-specific path (temp folder, repo) for `$TMP` / `$ROOT` first; `unitkit.digest`
stores a sha256 for a text too big to keep. Most of these files began as the Rust compiler's output (plan 15).
After a deliberate output change, `J3_UNIT_BLESS=1 tests/unit.sh NAME` rewrites them (`blessed FILE` lines);
review `git diff j3c-bootstrap/tests/unit/expect` before committing. `toolchain_run_test` reads `diff.sh run`'s
`tests/expect/run/` instead.

## Other files

- `cases/`: compiler cases as J3 files with expectations in their headers (plan B9), see
  `cases/README.md`.
- `SCALE.md`: build time and memory of the Rust oracle, the bar for the port (plan A11).
