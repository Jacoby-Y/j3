# Plan 004: --fast strips bounds aborts and keeps div-by-zero

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
- **Risk**: MED
- **Depends on**: plans/001-dual-backend-acceptance.md
- **Category**: bug
- **Planned at**: no git commit, working tree 2026-09-22

## Why this matters

`j3c help` says `--fast` strips bounds-check `Abort`. `strip_aborts` drops
every `Abort`, including the one `abort_if_zero` inserts before integer `/`.
After that pass, `/ 0` is a raw `sdiv` or C `/`, which is undefined.
`wrap.rs` says division by zero is `Abort`, not UB. This plan gives div-zero
its own statement kind so the fast pass cannot remove it.

## Current state

Help text (`j3c/src/cli.rs` around line 169):

```
--fast           with run/build: strip bounds-check Abort; no `-g`
```

`j3c/src/helptext.rs` lines 63 and 79 say the same ("strip bounds-check aborts").
Do not change those strings. They become true once the pass only strips bounds.

`abort_if_zero` (`j3c/src/check/lower.rs` around 1624–1656) pushes
`StmtKind::Abort` inside an `If` whose condition is `rhs == 0`. It runs only
for `parse::BinOp::Div` on an int (`lower.rs` around 1615). There is no `%`
in the grammar. Do not add a rem check.

`StmtKind::Abort` (`j3c/src/ir/stmt.rs` around line 59):

```
    /// Bounds-check / panic abort. `--fast` strips these. Not a Linux syscall.
    Abort,
```

Every match on `StmtKind::Abort` today (update each one):

- `j3c/src/ir/fast.rs` — `strip_block` skips `Abort`; `is_abort_only` matches `Abort`
- `j3c/src/ir/dump.rs` around line 193 — prints `abort;\n`
- `j3c/src/emit/c_expr.rs` around line 486 — `_exit(1);`
- `j3c/src/emit/llvm_expr.rs` around line 411 — `exit_abort()`
- `j3c/src/ir/validate.rs` around line 161 — empty arm

`fast_strips_abort` in `compile.rs` (around 1091) compiles `05_arrays.j3`,
calls `apply_fast`, and asserts the dump contains no `abort;`. That test
must keep passing. `05_arrays.j3` is bounds, not division.

`apply_fast` is `compile.rs` `pub fn apply_fast`.

## Commands you will need

From `j3c/`.

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Fast tests | `cargo test fast_strips_abort fast_keeps_div0 -- --test-threads=1` | exit 0 (see note) |
| Full | `cargo test` | exit 0 |

`cargo test` accepts one filter. Run the two names as two commands if a
single filter does not match both:

- `cargo test fast_strips_abort -- --test-threads=1`
- `cargo test fast_keeps_div0 -- --test-threads=1`

## Scope

**In scope**:
- `j3c/src/ir/stmt.rs`
- `j3c/src/ir/fast.rs`
- `j3c/src/ir/dump.rs`
- `j3c/src/ir/validate.rs`
- `j3c/src/emit/c_expr.rs` (the `Abort` match arm only)
- `j3c/src/emit/llvm_expr.rs` (the `Abort` match arm only)
- `j3c/src/check/lower.rs` (`abort_if_zero` only)
- `j3c/src/compile.rs` (one new test)

**Out of scope**:
- Help text and `helptext.rs` (wording already says bounds-only)
- `InterpFill` and its overflow `Abort`
- Slice bounds (plan 003). Those stay `Abort` and stay strippable
- The LLVM `sdiv` branch (plan 002)

## Git workflow

No git repository. Do not commit.

## Steps

### Step 1: Add `StmtKind::Div0`

Next to `Abort` in `stmt.rs`:

```rust
/// Integer division by zero. `--fast` does not strip this.
Div0,
```

Leave `Abort`'s doc as bounds-check only.

Rust compile will list every match that needs a new arm. Add `Div0` beside
`Abort` in dump, validate, C, and LLVM:

- dump prints `abort div0;\n` (not `abort;\n`)
- C prints the same `_exit(1);` as `Abort`
- LLVM calls the same `exit_abort()`
- validate is an empty arm, like `Abort`

`fast.rs`: do not skip `Div0`. `is_abort_only` stays true only for `Abort`.
An `If` whose then-block is only `Div0` must survive `strip_aborts`.

### Step 2: Lower `/ 0` as `Div0`

In `abort_if_zero`, push `StmtKind::Div0` instead of `StmtKind::Abort`.

### Step 3: Test

In `compile.rs` `mod tests`, add `fast_keeps_div0`. `parse_file` is already
defined in that module (around line 809).

```j3
module t;
fn main() {
    let n = 1;
    let d = 0;
    let q = n / d;
    let _ = q;
}
```

`check::check_files(vec![parse_file(1, "t.j3", src)])`. Dump with
`dump_program`. Assert the dump contains `abort div0;`. `apply_fast` on a
second check of the same source (or the same `Program` after the first dump).
Assert the fast dump still contains `abort div0;`.

Do not run the binary. `/ 0` aborts.

**Verify**: `cargo test fast_keeps_div0 -- --test-threads=1` → exit 0.
**Verify**: `cargo test fast_strips_abort -- --test-threads=1` → exit 0.
**Verify**: `cargo test` → exit 0.

## Test plan

- `fast_keeps_div0` as above.
- Existing `fast_strips_abort` must still see zero `abort;` after `apply_fast`
  on `05_arrays.j3`. The new dump line is `abort div0;`, which does not
  contain the substring `abort;`. Do not change `fast_strips_abort`.

## Done criteria

- [ ] `cargo test fast_keeps_div0` exits 0 and the test asserts `abort div0;` before and after `apply_fast`
- [ ] `cargo test fast_strips_abort` exits 0
- [ ] `cargo test` exits 0
- [ ] `rg "StmtKind::Abort =>" j3c/src` — each site also handles `Div0`, or uses a pattern that includes both. No compile error is the check.
- [ ] Plan 004 row is DONE

## STOP conditions

- `cargo test` fails in a match you did not know about after two attempts to add the arm. STOP and report the file.
- `05_arrays` fast dump still needs to contain `abort;` for some other reason. STOP. Do not weaken `fast_strips_abort`.
- Making this pass requires changing plan 002's phi or the C ternary.

## Maintenance notes

- A future `%` operator must call `abort_if_zero` and push `Div0` too.
- `--fast` may still delete bounds `Abort`. Reviewers should check `is_abort_only` does not return true for `Div0`.
