# Scale (plan 11 A11)

Build cost of the Rust oracle, so the port can be held to it. Measured with
`/usr/bin/time -v stage0/j3c build DIR` on a fresh copy of each project in a scratch folder ("cold": no
`target/` yet) and then again ("warm": the build's own cache in `target/.j3c`). Peak RSS is
"Maximum resident set size".

## Now (stubs, before C1)

- Date: 2026-11-16. Machine: 24 cores (`nproc`), Linux x86-64.
- stage0: `commit b7fd5978c1fe6d7268334291b238a28eab89c25c`, `dirty yes (uncommitted edits under j3c/)`, j3c 0.1.0, docs 970ea767.

| Build | Source lines (own) | Wall cold | RSS cold | Wall warm | RSS warm |
|---|---|---|---|---|---|
| `j3c-bootstrap` (stubs, `[emit] opt = 0`, debug) | 1.3k (mostly headers) | 0.07 s | 77 MB | 0.03 s | 66 MB |
| editor (`j3c/editor/src` + the `j3.toml` `j3c editor` writes) | 13k + stdlib | 1.31 s | 262 MB | 0.26 s | 118 MB |

The stub build is near zero because `main.j3` reaches almost none of the stub modules. The editor is the
real yardstick: the biggest J3 program built today (about 160k lines with the stdlib).

`tests/diff.sh fmt` over the corpus (496 files, 3 stage0 runs + 1 j3c-boot run each, 24 jobs): about 2 s.
`ir`: 219 inputs, 11 s. `llvm`: 219 inputs, 60 s.

## After C1 (front end ported)

Stage 1 parses the whole corpus plus the stdlib. Stop and fix before C2 if parse time is above 3x the
Rust time or RSS above 4x the Rust RSS.

| Input | Rust wall | Rust RSS | j3c-boot wall | j3c-boot RSS | Ratio wall | Ratio RSS |
|---|---|---|---|---|---|---|
| `dump-ast` of 1005 files (the `[fmt ast]` corpus, `j3c/stdlib/j3`, `j3c/editor/src`; 167k lines, 6.0 MB), one dir, fmt gate included | 0.29 s | 8.5 MB | 0.32 s | 6.0 MB | 1.1x | 0.7x |
| same, before the fix below (one Session for every file) | 0.29 s | 8.5 MB | 0.37 s | 500 MB | 1.3x | 59x |
| `check` of the editor project (load + parse: 179 files, 1.6 MB with the stdlib) | 0.030 s | 60 MB | 0.07 s | 97 MB | 2.3x | 1.6x |
| `check` of `j3c-bootstrap` (load + parse: 100 files, 0.75 MB) | 0.020 s | 39 MB | 0.03 s | 45 MB | 1.5x | 1.2x |

- Date: 2026-11-16, same machine. stage0 as above (docs 51255b76). j3c-boot built with `j3c build --release`
  (a dev build, `opt = 0`, is about 3.5x slower on `dump-ast`: 1.25 s). Median of 2-3 runs, `/usr/bin/time -v`.
- Rust "load + parse" is its own `J3C_TIMINGS=1` line (no process start), with RSS read from `/proc` when that
  line is printed (Rust `check` goes on to sema; its whole-run peak is 120 MB for the editor, 94 MB for
  `j3c-bootstrap`). j3c-boot `check` stops after load (`sema not ported yet`), so its numbers are the whole
  process. Both load the same files (179 and 100).
- Fixed here: `dump-ast` kept every file's tokens, fmt output and AST in one Session arena (500 MB over the
  corpus); it now uses one Session per file, as Rust drops each file.
- Where j3c-boot's memory goes (for C2, not a blocker): the arena never frees, so each file's token list
  and every `List`/`Pool` growth step stay until the Session ends (about 60 bytes of RSS per source byte,
  vs Rust's 38). A scratch arena per parsed file for its tokens is the first lever if sema's numbers need it.

**Verdict: within budget (parse wall at most 2.3x, RSS at most 1.6x of Rust); C2 is not blocked.**

## After C3 (self build through the object cache)

Stage 1 (`target/j3c-boot`, built by stage0, dev: opt 0) builds stage 2 from a scratch copy of `j3c-bootstrap`
(`{j3.toml,src,text}` under `<repo>/target/`, what `diff.sh self` does): 97 modules, 271 clang jobs.

| Build of the scratch copy | Wall | Peak RSS | Notes |
|---|---|---|---|
| j3c-boot (stage 1), cache cold | 9.0 s | 1292 MB | every module emitted and compiled in forked workers, 24 cores |
| j3c-boot (stage 1), cache warm (nothing changed) | 1.6 s | 691 MB | parse + check, then `up to date`; no clang, no link |
| stage0 (Rust), cache cold | 2.8 s | 868 MB | same copy |
| stage0 (Rust), cache warm | 0.8 s | 365 MB | same copy |

- Date: 2026-11-18, same machine, `/usr/bin/time -f "%e %M"` (the RSS is the largest single process: the parent,
  whose workers are forks of it). One run each. Ratio to Rust: 3.2x wall and 1.5x RSS cold, 2.0x and 1.9x warm.
- A comment-only edit of one module recompiles nothing (the key is the module's IR text); adding a fn to one
  module recompiles that module only (`tests/unit/cache_build_test.j3`).
- `diff.sh self` (stage 2 = stage 1): fmt 586, ast 586, err 161, ui 221, glsl 4, glsl --es 4, ir --norm 222 and
  222 llvm inputs (one per call), 229 comparisons, 0 failed, about 14 minutes in all (run as `SELF_LLVM=1:90`,
  `91:160`, `161:222`). `dump-llvm j3c-bootstrap` is byte-equal between the stages (78.7 MB).
- Where the time goes: emission inside the workers (about 31 s of cpu at opt 0), key pass 1.1 s. Stage 1 is a dev
  build; a release-built stage 1 is the first lever.

