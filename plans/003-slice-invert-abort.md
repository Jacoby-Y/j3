# Plan 003: Inverted slice bounds abort instead of syscall 60

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: Not a git repository. Confirm the excerpts.
> On a mismatch, STOP.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: plans/001-dual-backend-acceptance.md
- **Category**: bug
- **Planned at**: no git commit, working tree 2026-09-22

## Why this matters

Index and endpoint bounds checks lower to `StmtKind::Abort`. An inverted
slice (`end < start`) lowers to `@syscall(60, 1)`, which is the Linux exit
syscall. `--fast` strips `Abort` and does not strip that syscall. Windows IR
is required to contain no `@syscall` (`target_list_picks_a_sys_file_per_triple`
in `compile.rs`). A slice on a Windows target injects a Linux syscall into
user code.

## Current state

`slice_bounds` in `j3c/src/check/lower.rs` (the `end < start` then-block):

```2198:2216:j3c/src/check/lower.rs
        let saved = self.b.begin_block();
        self.b.expr(
            span,
            Rvalue::syscall(
                span,
                self.cx.common.int,
                Operand::int(span, self.cx.common.int, 60),
                vec![Operand::int(span, self.cx.common.int, 1)],
            ),
        );
        let then_block = self.b.end_block(saved);
        self.b.push(crate::ir::stmt::Stmt {
            ty: self.cx.common.void,
            span,
            kind: crate::ir::stmt::StmtKind::If {
                cond: Operand::Copy(p),
                then_block,
                else_block: None,
            },
        });
```

The other bounds helper pushes `StmtKind::Abort` (around line 2268). Match that.

Goldens that contain this pattern (a compare, then a one-statement then-block
that is only `@syscall(60, 1);`), and must change to `abort;`:

- `j3c/src/ir/goldens/05_arrays.ir` lines 86–88 and 105–107
- `j3c/src/ir/goldens/06_enums_errors.ir` lines 153–155
- `j3c/src/ir/goldens/08_game_loop.ir` lines 149–151

`j3c/src/ir/goldens/samples.ir` also contains `@syscall(60, 1)`, inside the
hand-built place samples (`if copy g.running`, `if copy bad_x`). Those are
not slice checks. Do not edit `samples.ir`.

Golden compare helper (`compile.rs` `assert_golden`): if `J3C_WRITE_GOLDENS`
is set it overwrites the file. Do not use that env var. Edit the four
then-blocks by hand so the rest of each golden stays byte-identical.

`ir_goldens_01_through_08_and_samples` calls `assert_golden`.

## Commands you will need

From `j3c/`. Wrapper may copy `~/.bin/j3c`. Leave it.

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Goldens | `cargo test ir_goldens_01_through_08_and_samples -- --test-threads=1` | exit 0 |
| Windows IR | `cargo test target_list_picks_a_sys_file_per_triple -- --test-threads=1` | exit 0 |
| Both backends | `cargo test both_backends_run_01_through_08 -- --test-threads=1` | exit 0 |
| Full | `cargo test` | exit 0 |

## Scope

**In scope**:
- `j3c/src/check/lower.rs` (`slice_bounds` only)
- `j3c/src/ir/goldens/05_arrays.ir`
- `j3c/src/ir/goldens/06_enums_errors.ir`
- `j3c/src/ir/goldens/08_game_loop.ir`

**Out of scope**:
- `j3c/src/ir/goldens/samples.ir`
- `j3c/src/ir/fast.rs`
- Printers
- Any other `@syscall` in stdlib (`j3.sys` exit stays a syscall)

## Git workflow

No git repository. Do not commit.

## Steps

### Step 1: Push Abort

In `slice_bounds`, replace the `self.b.expr(..., Rvalue::syscall(...))` with the
same `self.b.push(Stmt { kind: StmtKind::Abort, ... })` used in
`bounds_check_inclusive` just below it. Keep the `If` that wraps that block.

### Step 2: Edit the three goldens

In each of the four then-blocks listed above, replace the line

```
        @syscall(60, 1);
```

with

```
        abort;
```

Leave every other line unchanged. `samples.ir` still contains `@syscall(60, 1)`.

**Verify**: `cargo test ir_goldens_01_through_08_and_samples -- --test-threads=1` → exit 0.

### Step 3: Suites

**Verify**:
- `cargo test target_list_picks_a_sys_file_per_triple -- --test-threads=1` → exit 0
- `cargo test both_backends_run_01_through_08 -- --test-threads=1` → exit 0
- `cargo test` → exit 0

## Test plan

- No new test. The golden diff is the regression lock.
- `rg -n "@syscall\\(60, 1\\)" j3c/src/ir/goldens` after the edit prints hits
  only in `samples.ir`.

## Done criteria

- [ ] `slice_bounds` pushes `StmtKind::Abort` and does not call `Rvalue::syscall`
- [ ] `rg "@syscall\\(60, 1\\)" j3c/src/ir/goldens` matches only `samples.ir`
- [ ] `cargo test ir_goldens_01_through_08_and_samples` exits 0
- [ ] `cargo test` exits 0
- [ ] `J3C_WRITE_GOLDENS` was not used
- [ ] Plan 003 row is DONE

## STOP conditions

- A golden diff is larger than those four `abort;` replacements. STOP. Do not
  regenerate goldens.
- `samples.ir` would have to change for `cargo test` to pass.
- `slice_bounds` is no longer the only `Rvalue::syscall(..., 60, ...)` in
  `lower.rs`. If you find a second site, STOP and report it. Do not convert
  stdlib exit calls.

## Maintenance notes

- New slice forms must use `Abort`, not a host syscall.
- `--fast` will strip this check once it is `Abort`. That is the same as
  other bounds checks. Plan 004 keeps `/ 0` aborts; this one should still
  be stripped.
