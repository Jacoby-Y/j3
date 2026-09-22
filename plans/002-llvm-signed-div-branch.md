# Plan 002: Branch around signed MIN/-1 before sdiv and srem

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: Not a git repository. Confirm the excerpts
> below still match the files. On a mismatch, STOP.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: plans/001-dual-backend-acceptance.md
- **Category**: bug
- **Planned at**: no git commit, working tree 2026-09-22

## Why this matters

`j3c/src/ir/wrap.rs` says overflow wraps, and `INT_MIN / -1` wraps to `MIN`.
The C printer implements that with a ternary, so the `/` is not evaluated on
the overflow path. The LLVM printer always emits `sdiv`/`srem`, then a
`select`. In LLVM, signed division of `INT_MIN` by `-1` is undefined, and
`select` does not skip the division. The default backend can trap or delete
the check. This plan emits a real branch so `sdiv`/`srem` run only when the
divisor is not the overflow case.

## Current state

Contract (`j3c/src/ir/wrap.rs`):

```1:4:j3c/src/ir/wrap.rs
//! Integer wrap and `/0` — one rule for every printer.
//!
//! Overflow wraps (two's complement). Division or remainder by zero is
//! `Abort`, not a host panic and not C UB. `INT_MIN / -1` wraps to `MIN`.
```

`fold_bin` uses `wrapping_div` and `wrapping_rem`. Rust's `wrapping_div(MIN, -1)`
is `MIN`. `wrapping_rem(MIN, -1)` is `0`. The fold test locks div:

```54:59:j3c/src/ir/wrap.rs
    fn div_zero_does_not_panic_host() {
        assert_eq!(fold_bin(BinOp::Div, 1, 0), None);
        assert_eq!(fold_bin(BinOp::Rem, 1, 0), None);
        assert_eq!(fold_bin(BinOp::Div, i64::MIN, -1), Some(i64::MIN));
        assert_eq!(fold_neg(i64::MIN), i64::MIN);
    }
```

LLVM printer (`j3c/src/emit/llvm_expr.rs`), inside the signed `Div | Rem` arm.
`self.block`, `self.term`, `self.label`, and `self.fresh` already exist on this
type (see `StmtKind::If` around line 422). `self.inst` does not terminate the
block.

```1048:1075:j3c/src/emit/llvm_expr.rs
            BinOp::Div | BinOp::Rem => {
                let signed = matches!(self.cx.ty_kind(ty), TyKind::Int(i) if i.is_signed());
                if signed {
                    // ... computes ismin, ism1, ov ...
                    let norm = self.fresh();
                    if op == BinOp::Div {
                        self.inst(&format!("{norm} = sdiv {ll} {l}, {r}"));
                    } else {
                        self.inst(&format!("{norm} = srem {ll} {l}, {r}"));
                    }
                    let v = self.fresh();
                    self.inst(&format!("{v} = select i1 {ov}, {ll} {min}, {ll} {norm}"));
                    Ok(v)
```

C already short-circuits (`j3c/src/emit/c_expr.rs` `wrap_int`, the ternary
around line 675). Do not edit the C printer. J3 source has no `%` operator;
`BinOp::Rem` still exists in IR and must stay correct.

Unsigned `udiv`/`urem` stay as they are. Division by zero is a separate
`Abort` branch inserted by check (plan 004). Do not add a zero check here.

## Commands you will need

From `j3c/`. `cargo test` copies the binary to `/home/jacoby/.bin/j3c`. Leave
the wrapper alone.

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| New test | `cargo test signed_div_min_is_a_branch -- --test-threads=1` | exit 0 |
| Dual backend | `cargo test both_backends_run_01_through_08 -- --test-threads=1` | exit 0 |
| Full | `cargo test` | exit 0 |

## Scope

**In scope**:
- `j3c/src/emit/llvm_expr.rs` (the signed `Div | Rem` arm only)
- `j3c/src/compile.rs` (one new test in `mod tests`)

**Out of scope**:
- `j3c/src/emit/c_expr.rs`
- `j3c/src/ir/wrap.rs` (do not change fold results)
- `--fast` / `Abort` (plan 004)

## Git workflow

No git repository. Do not commit.

## Steps

### Step 1: Replace select with a branch and phi

In the signed arm, keep the three `icmp`/`and` instructions that produce `{ov}`.
Delete the unconditional `sdiv`/`srem` and the `select`.

Emit this shape (`{ll}` is the existing LLVM type string, `{min}` is the
existing decimal minimum, `{l}` and `{r}` are the existing operand names):

1. `self.term(br i1 {ov}, label %{ov_bb}, label %{norm_bb})` using `self.label()`
   for the three labels.
2. On `{ov_bb}`: branch to `{join}`. The phi incoming value is `{min}` when
   `op == BinOp::Div`, and `0` when `op == BinOp::Rem`.
3. On `{norm_bb}`: `sdiv` or `srem` into `{norm}` via `self.inst`, then branch
   to `{join}`.
4. On `{join}`: `{v} = phi {ll} [ {wrapped}, %{ov_bb} ], [ {norm}, %{norm_bb} ]`.
   Return `Ok(v)`.

`self.term` marks the block terminated. `self.block` opens the next label.
Follow the same order as `StmtKind::If` in this file.

### Step 2: Test the LLVM text, do not execute MIN/-1

Add `signed_div_min_is_a_branch` in `j3c/src/compile.rs` `mod tests`.

Write a temp file under `std::env::temp_dir()` (delete it at the end of the
test) with:

```j3
module t;
fn main() {
    let a = -9223372036854775808;
    let b = -1;
    let q = a / b;
    let _ = q;
}
```

`compile_path` that file. `emit::emit_llvm(..., &EmitOpts::default())`.
Assert:

- the text contains `sdiv `
- the text contains `phi `
- no line contains both `select i1` and `-9223372036854775808`

Do not `run_program` this file. Executing `MIN / -1` is the bug.

**Verify**: `cargo test signed_div_min_is_a_branch -- --test-threads=1` → exit 0.

### Step 3: Acceptance still agrees

**Verify**: `cargo test both_backends_run_01_through_08 -- --test-threads=1` → exit 0.
Then `cargo test` → exit 0.

## Test plan

- `signed_div_min_is_a_branch` as above.
- Pattern: `hello_j3_prints_hello_world` emits LLVM text and asserts substrings.
- Do not add a runtime test of `MIN / -1`.

## Done criteria

- [ ] `cargo test signed_div_min_is_a_branch` exits 0
- [ ] `cargo test both_backends_run_01_through_08` exits 0
- [ ] `cargo test` exits 0
- [ ] `rg -n "select i1" j3c/src/emit/llvm_expr.rs` still finds the bool-string select near the interpolate path, and does not find a select in the `BinOp::Div` arm
- [ ] Only `j3c/src/emit/llvm_expr.rs`, `j3c/src/compile.rs`, and `plans/README.md` change
- [ ] Plan 002 row in `plans/README.md` is DONE

## STOP conditions

- `self.label` / `self.block` / `self.term` are not available on the type that contains the `Div | Rem` arm.
- The phi fails to assemble (`cargo test` LLVM parse/run errors on `01`–`08`). Do not switch to `select` to silence that.
- You need to edit `c_expr.rs` to make a test pass.

## Maintenance notes

- If a `%` operator is added later, the Rem phi incoming must stay `0`, matching `wrapping_rem`.
- Plan 004 changes `/ 0`. This plan does not. A divisor of `0` must still be handled by the abort branch that check inserts before the div.
