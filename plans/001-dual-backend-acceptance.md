# Plan 001: Acceptance programs run on both LLVM and C

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: This tree is not a git repository. There is no
> SHA. Open the files in "Current state" and confirm the excerpts still match.
> On a mismatch, STOP.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: LOW
- **Depends on**: none
- **Category**: tests
- **Planned at**: no git commit, working tree 2026-09-22

## Why this matters

`speculative/01`–`08` are the compiler's acceptance programs. Today they execute
through `exec()`, which uses LLVM when `clang` exists and falls back to C only
when clang is missing. The C backend is executed for hello and the hand-built
place samples. A printer bug that shows up only on one backend can pass
`cargo test`. This plan runs each acceptance program on both backends and
compares status and stdout. It does not change compiler behavior.

## Current state

- `j3c/src/compile.rs` — load, check, and the acceptance tests.
- `j3c/src/emit/opts.rs` — `EmitOpts` / `Backend`. Default backend is LLVM.
- `j3c/src/emit/mod.rs` — `run_program(cx, modules, opts)`.
- `speculative/01_basics.j3` through `08_game_loop.j3` — programs. `08` must be
  compiled with `compile_path_headless` (headless `j3.game`, no SDL window).
  `09` typechecks only; do not run it.

`exec` forces LLVM unless clang is absent:

```415:426:j3c/src/compile.rs
    fn llvm_opts() -> emit::EmitOpts {
        let mut o = emit::EmitOpts::default();
        o.backend = emit::Backend::Llvm;
        if !emit::clang_available(&o.clang) {
            o.backend = emit::Backend::C;
        }
        o
    }

    fn exec(prog: &Program) -> emit::RunResult {
        emit::run_program(&prog.cx, &prog.modules, &llvm_opts()).expect("emit/run")
    }
```

Hello is the only program with an explicit C run (`c_backend_still_runs_hello`,
around line 452). Expected stdout already asserted by the LLVM tests:

| Program | Compile | stdout (exact, from the existing `*_runs` tests) |
|---------|---------|--------|
| `01_basics.j3` | `compile_path` | `Hello, world!\nHello, Coby, how are you?\n` |
| `02_data.j3` | `compile_path` | `len=4.47214 player.pos=Vec2 { x: 12, y: 9 }\n` |
| `03_embedding.j3` | `compile_path` | `x=10, width=1\nx=10, width=1\n` |
| `04_control.j3` | `compile_path` | `hp=3\nn=3\nn=2\nn=1\nabs=4\ni=0\ni=1\ni=3\nhello player\nleaving main\n` |
| `05_arrays.j3` | `compile_path` | `sum=102 mid0=21\nxs[0]=11\nxs[1]=21\nxs[2]=30\nxs[3]=40\nhi Coby bytes=4\n` |
| `06_enums_errors.j3` | `compile_path` | `north\nhit 2\nmissing\n` |
| `07_memory.j3` | `compile_path` | `entities=1 hits=1\n` |
| `08_game_loop.j3` | `compile_path_headless` | `bye\n` |

Style: tests live in `#[cfg(test)]` modules next to the code. No crates.io.
Match `c_backend_still_runs_hello`.

## Commands you will need

Run from `j3c/`. A successful `cargo test` also copies `j3c` to
`/home/jacoby/.bin/j3c`. Leave the wrapper alone.

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| One test | `cargo test both_backends_run_01_through_08 -- --test-threads=1` | exit 0 |
| Full suite | `cargo test` | exit 0 |

## Scope

**In scope**:
- `j3c/src/compile.rs` (the `tests` module only)

**Out of scope**:
- Any change to emit, check, parse, or stdlib.
- `speculative/09_*.j3` and `speculative/10_*.j3` (do not execute them here).
- `j3c/src/emit/host.rs`.
- Goldens under `j3c/src/ir/goldens/`.

## Git workflow

No git repository. Do not run `git init` or commit.

## Steps

### Step 1: Add one test that runs both backends

Inside `mod tests` in `j3c/src/compile.rs`, add a test named
`both_backends_run_01_through_08`.

For each row in the table above:

1. Compile with the function in the table.
2. Run with `EmitOpts` whose `backend` is `Backend::Llvm`. Require
   `emit::clang_available`. If clang is missing, `panic` with
   `"clang required for dual-backend test"` — do not silently skip.
3. Run again with `backend = Backend::C` (same `EmitOpts` shape as
   `c_backend_still_runs_hello`: start from `EmitOpts::default()`, set
   `backend`).
4. Assert both runs have `status == 0` and identical stdout, equal to the
   string in the table. Include stderr in the assertion message, as the
   existing tests do.

Use the stdout strings in the table. They were copied from `compile.rs` tests `basics_j3_runs`, `data_j3_runs`, `embedding_j3_runs`, `control_j3_runs`, `arrays_j3_runs`, `enums_j3_runs`, `memory_j3_runs`, and `game_loop_j3_runs`.

**Verify**: `cargo test both_backends_run_01_through_08 -- --test-threads=1` → exit 0.

### Step 2: Full suite

**Verify**: `cargo test` → exit 0.

## Test plan

- The new test is the test. No new files.
- Pattern: `c_backend_still_runs_hello` and `game_loop_j3_runs` in `compile.rs`.

## Done criteria

- [ ] `cargo test both_backends_run_01_through_08 -- --test-threads=1` exits 0
- [ ] `cargo test` exits 0
- [ ] The test runs `01`–`08`, uses `compile_path_headless` for `08`, and asserts LLVM stdout equals C stdout
- [ ] `git status` is irrelevant (no repo). `diff` shows only `j3c/src/compile.rs` and `plans/README.md`
- [ ] `plans/README.md` status row for 001 is DONE

## STOP conditions

- An existing `*_j3_runs` test's expected stdout does not match the table. STOP. Do not pick a new expected string.
- LLVM and C disagree on stdout or status. That is a product bug, not a test bug. STOP and report the program name and both outputs. Do not weaken the assert.
- Clang is not on `PATH`.
- The fix seems to require editing a printer.

## Maintenance notes

- New acceptance programs that `printLn` should be added to this test in the same change that adds their `*_runs` test.
- `09` stays typecheck-only. `10_j3ui.j3` is not part of this plan.
- Reviewers should confirm `08` stays headless so the test does not open a window.
