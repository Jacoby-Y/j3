# Plan 011: Delete the hand-built hello IR

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: Not a git repository. Confirm `hello_world`
> is only called from `j3c/src/stdlib/mod.rs`. On a mismatch, STOP.

## Status

- **Priority**: P3
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: tech-debt
- **Planned at**: no git commit, working tree 2026-09-22

## Why this matters

`j3c/src/stdlib/mod.rs` starts with "IR bootstrap until the parser exists.
Delete it then." The parser exists. `j3c run` and `hello_j3_prints_hello_world`
compile `j3c/examples/hello.j3`. The hand-built `hello_world()` is only called
by its own test, and it is still linked into every `j3c` build. Deleting it
removes a second hello that can drift from the real one.

## Current state

`j3c/src/stdlib/mod.rs`:

- Module doc lines 1–2 tell you to delete the bootstrap.
- `HelloProgram` and `pub fn hello_world()` start at line 20.
- `pub fn stdlib_dir()` starts at line 449 and returns the `stdlib/` directory
  (`J3_STDLIB`, then the path next to the crate, then next to the exe).
- Tests: `hello_prints_hello_world` (line 475) calls `hello_world()` and
  `emit::compile_and_run`. `stdlib_j3_files_exist` (line 500) checks
  `stdlib_dir()`. Keep that second test.

Callers of `stdlib_dir`: `j3c/src/compile.rs` line 12, `j3c/src/doc.rs` line 7.
Callers of `hello_world` / `HelloProgram`: only `stdlib/mod.rs`.

`j3c/AGENTS.md` still says this file is "Old IR bootstrap of sys/io/hello. Do
not grow" (crate-root table) and "leftover hand-built IR — do not add to it"
(stdlib section).

`j3c/src/lib.rs` has `pub mod stdlib;`. Keep the module.

Do not delete `j3c/src/ir/build.rs`. Check still uses `BodyBuilder`.

Real hello test to keep green: `hello_j3_prints_hello_world` in `compile.rs`,
which compiles `examples/hello.j3`.

## Commands you will need

From `j3c/`.

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| No bootstrap symbol | `rg -n "hello_world|HelloProgram" j3c/src` | no matches |
| Hello via parser | `cargo test hello_j3_prints_hello_world -- --test-threads=1` | exit 0 |
| Stdlib dir | `cargo test stdlib_j3_files_exist -- --test-threads=1` | exit 0 |
| Full | `cargo test` | exit 0 |

## Scope

**In scope**:
- `j3c/src/stdlib/mod.rs`
- The two sentences in `j3c/AGENTS.md` that describe this file as an IR bootstrap

**Out of scope**:
- `j3c/examples/hello.j3`
- `j3c/src/ir/build.rs`
- `j3c/stdlib/j3/*.j3`
- `compile.rs` except you must not need to edit it. If a test outside `stdlib/mod.rs` calls `hello_world`, STOP.

## Git workflow

No git repository. Do not commit.

## Steps

### Step 1: Leave only `stdlib_dir`

Replace the body of `j3c/src/stdlib/mod.rs` with:

- A module doc of one or two lines: the J3 stdlib lives in `stdlib/j3/`; this module only finds that directory.
- `pub fn stdlib_dir()` copied unchanged from the current function (lines 449–466).
- The existing `stdlib_j3_files_exist` test, unchanged, including the file-name list.
- Delete `hello_prints_hello_world` and every helper it needed (`hello_world`, `HelloProgram`, `hello_main_body`, the `SYS_WRITE` / `SYS_EXIT` constants, the IR imports).

Drop imports the file no longer uses. `cargo test` will fail on unused imports; delete those, do not `allow` them.

### Step 2: Point AGENTS at the smaller file

In `j3c/AGENTS.md`, change the crate-root table cell and the stdlib-section sentence so they say this module is `stdlib_dir()` only, and the source of truth is `stdlib/j3/*.j3`. Do not leave "do not grow" on a file that no longer contains IR.

### Step 3: Tests

**Verify**: `rg -n "hello_world|HelloProgram" j3c/src` → no matches.
**Verify**: `cargo test hello_j3_prints_hello_world -- --test-threads=1` → exit 0.
**Verify**: `cargo test stdlib_j3_files_exist -- --test-threads=1` → exit 0.
**Verify**: `cargo test` → exit 0.

## Test plan

- No new test. `stdlib_j3_files_exist` stays. `hello_j3_prints_hello_world` is the hello that matters.

## Done criteria

- [ ] `j3c/src/stdlib/mod.rs` contains `stdlib_dir` and does not contain `hello_world`
- [ ] `rg "hello_world|HelloProgram" j3c/src` is empty
- [ ] `cargo test hello_j3_prints_hello_world` exits 0
- [ ] `cargo test stdlib_j3_files_exist` exits 0
- [ ] `cargo test` exits 0
- [ ] Plan 011 row is DONE

## STOP conditions

- `rg "hello_world" j3c` finds a caller outside `stdlib/mod.rs`. STOP. Do not rewrite that caller onto the J3 hello in this plan.
- `stdlib_dir`'s search order (`J3_STDLIB`, crate dir, exe dir) would have to change for a test to pass. STOP. Copy the function as it is.
- `cargo test` fails in `ir/build.rs` or `ir/samples.rs` because those still construct IR by hand. Those are not the bootstrap. STOP and report. Do not delete `samples.rs`.

## Maintenance notes

- New stdlib modules are new `stdlib/j3/*.j3` files, plus a name in `stdlib_j3_files_exist` if that test lists files explicitly. Read the rest of the list before adding one; the list may already be incomplete relative to the directory. Do not expand the list in this plan unless the test fails because you deleted a name.
- Do not put IR builders back in this file.
