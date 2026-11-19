# Plan 15: retire stage 0

Status: TODO. Written 2026-11-19, right after `j3.data.toml` landed (the first compiler change that needed a
stdlib module newer than the `rust-final` tag).

## Why

`j3c-bootstrap/stage0/j3c` (the Rust compiler at the tag `rust-final`) still does two jobs, both left over
from the port (plan 11):

1. **Seed.** `build.sh` has stage 0 build stage 1, then stage 1 build stage 2. Since C6 `j3c` builds itself, so
   any working `j3c` can be the seed.
2. **Oracle.** `tests/diff.sh` (19 references) and 36 of `tests/unit/*_test.j3` run `stage0/j3c` and compare
   byte for byte. Every improvement to `j3c` is now a "DIFF" to log in `PORT_DEBT.md` or `tests/known/`.

What it costs today:
- Stage 0 embeds its tag's stdlib. Since `base/toml.j3` uses `j3.data.toml`, every stage 0 build of
  `j3c-bootstrap` needs `J3_STDLIB=<repo>/j3c/stdlib` (`build.sh`, `tests/diff.sh`, `tests/unit.sh`,
  `tools/bench.sh`, `tools/release/package.sh`, and stage 0 runs inside `load_core_test`). A plain
  `stage0/j3c build j3c-bootstrap` fails.
- `tests/unit.sh` builds about 120 test packages with stage 0. Each depends on all of `j3c`, so the compiler
  is compiled about 120 times, half the cores at a time, about 1 GB each. That is too slow and heavy for a
  laptop, and too slow for CI (commit 6b08bce moved CI to `j3c test-compiler`).
- A fresh clone needs cargo to build stage 0 (`tools/stage0.sh`).

## Decision for the user

**Seed choice** (pick one before starting):
- (a) The installed `j3c` (`command -v j3c`), falling back to (b). Fast, but a broken install breaks the build
  (keep the last good binary as `j3c.prev`).
- (b) A pinned release binary: `j3c-bootstrap/seed/VERSION` names a GitHub release, and `tools/seed.sh`
  downloads and checks its sha256. Reproducible and needs no cargo. Needs plan 02's release pipeline.
- (c) Both: (a) when present and new enough (`j3c version` at least `seed/VERSION`), else (b).
  Recommended.

Stage 0 itself stays reachable at the tag `rust-final` (`tools/stage0.sh rust-final`) as a last resort; it
leaves `build.sh`.

## Chunks

1. **Seed swap.** `build.sh` and `tools/release/package.sh` take the seed from the chosen source; drop the
   `J3_STDLIB` lines added for stage 0. Stage 1 = seed builds `j3c-bootstrap`, stage 2 = stage 1 builds it, as
   now (the `version` check stays). Gate: `./build.sh` from a clean `target/`, and from a clone with no
   `stage0/`.
2. **Freeze the oracle.** For each `diff.sh` mode, write what stage 0 prints today into checked-in expected
   files (`tests/expect/<mode>/<case>.out`, one per corpus input; `tests/known/<mode>.txt` lines turn into the
   current `target/j3c` output, reviewed). `diff.sh MODE` compares `target/j3c` with those files;
   `diff.sh MODE --bless` rewrites them (the diff is reviewed in git, like `j3c golden`). `self` mode stays
   (stage 1 vs stage 2).
3. **Unit tests off stage 0.** The 36 tests that run `stage0/j3c` compare with expected strings or files
   (most already hold the expected text; `base_toml_test`'s Rust-compared inputs become a table like its
   `ours`). `unit.sh` builds with `target/j3c` (its object cache makes the shared `j3c` modules compile once),
   default `J3_TEST_JOBS` 2. Optional: one binary for all tests (`unit NAME`) if per-test builds are still slow.
4. **Clean up.** `PORT_DEBT.md` becomes history (or is deleted; git keeps it), `tests/known/` goes, the
   `Port of:` header rule and `tools/coverage.sh` go (the port is finished), `j3c-bootstrap/AGENTS.md` "Stage 0
   is frozen", the root `AGENTS.md` build line, `tests/README.md`, CI (`.github/workflows/ci.yml` stage 0
   cache) and `tools/ci/*.sh` hints are updated. `stage0/` is removed from the tree.

## Gates

- `./build.sh` passes with no `stage0/` directory and no cargo.
- `diff.sh fmt|err|ir|llvm|run|ui|tool|bc|self` pass against the expected files, and `--bless` on an
  unchanged tree changes nothing.
- `tests/unit.sh` finishes in under 10 minutes with `J3_TEST_JOBS=2` and never runs `stage0/j3c`
  (`grep -rn stage0 j3c-bootstrap/tests` is empty).
- `j3c test-compiler` and `tools/ci/stdlib-tests.sh` still pass.
