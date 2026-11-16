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
