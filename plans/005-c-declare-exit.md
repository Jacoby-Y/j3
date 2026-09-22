# Plan 005: Every C program declares _exit

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

`Abort` prints `_exit(1);`. The C printer includes `<unistd.h>` only when the
IR contains `@syscall`. Windows `j3.sys` has no `@syscall`
(`compile.rs` test `target_list_picks_a_sys_file_per_triple` asserts that).
A Windows C build that still has bounds checks calls `_exit` with no
declaration. C11 rejects implicit declarations. LLVM always declares
`@_exit`. This plan declares `_exit` in every C file.

## Current state

`j3c/src/emit/c.rs` `emit_program`, the header block:

```21:38:j3c/src/emit/c.rs
    if syscall {
        out.push_str("/* Host shim: libc syscall on Linux. Pointer args are cast at the call. */\n");
        out.push_str("#define _GNU_SOURCE\n");
    }
    out.push_str("#include <stdint.h>\n");
    out.push_str("#include <limits.h>\n");
    out.push_str("#include <stdbool.h>\n");
    out.push_str("#include <stddef.h>\n");
    out.push_str("#include <stdio.h>\n");
    out.push_str("#include <string.h>\n");
    if syscall {
        out.push_str("#include <unistd.h>\n\n");
        out.push_str("static int64_t j3_syscall(...)\n");
        // ...
    } else {
        out.push('\n');
    }
```

`j3c/src/emit/c_expr.rs` around line 486: `StmtKind::Abort => Ok(format!("{pad}_exit(1);\n"))`.
If plan 004 has landed, `StmtKind::Div0` prints the same call. This plan does
not care which kind it is.

`j3c/src/emit/llvm.rs` line 36 always has `declare void @_exit(i32) noreturn`.

`compile_path_target(entry, target)` is `compile.rs` line 42. Windows triple
used by the existing test: `x86_64-pc-windows-gnu`.

## Commands you will need

From `j3c/`.

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| New test | `cargo test c_declares_exit_without_syscall -- --test-threads=1` | exit 0 |
| Both backends | `cargo test both_backends_run_01_through_08 -- --test-threads=1` | exit 0 |
| Full | `cargo test` | exit 0 |

## Scope

**In scope**:
- `j3c/src/emit/c.rs` (the header block only)
- `j3c/src/compile.rs` (one new test)

**Out of scope**:
- LLVM printer
- Replacing `_exit` with `exit` from stdlib.h
- Including `<unistd.h>` on Windows (mingw has it; a prototype does not depend on that)

## Git workflow

No git repository. Do not commit.

## Steps

### Step 1: Prototype

In `emit_program`, after the `#include <string.h>` line and in **both** the
syscall and non-syscall branches, emit:

```c
void _exit(int);
```

Put it once, before the `if syscall` include, so it is unconditional:

```rust
out.push_str("void _exit(int);\n");
```

A second compatible declaration from `<unistd.h>` is valid C. Do not wrap it
in `#ifndef`.

### Step 2: Test a Windows compile's C text

Add `c_declares_exit_without_syscall` in `compile.rs` `mod tests`.

```rust
let prog = compile_path_target(
    &speculative("01_basics.j3"),
    "x86_64-pc-windows-gnu",
).expect("windows 01");
let c = emit::emit_program(&prog.cx, &prog.modules).expect("c");
assert!(!c.contains("j3_syscall"), "{c}");
assert!(c.contains("void _exit(int);"), "{c}");
```

Do not link or run. This is a text assert. Windows link needs mingw libs
this plan does not require.

**Verify**: `cargo test c_declares_exit_without_syscall -- --test-threads=1` → exit 0.
**Verify**: `cargo test both_backends_run_01_through_08 -- --test-threads=1` → exit 0.
**Verify**: `cargo test` → exit 0.

## Test plan

- `c_declares_exit_without_syscall` as above.
- Pattern: `target_list_picks_a_sys_file_per_triple` for the triple;
  `hello_j3_prints_hello_world` for `emit_program` text asserts.
- The dual-backend test from plan 001 is the "C still links on Linux" check.

## Done criteria

- [ ] `rg "void _exit\\(int\\);" j3c/src/emit/c.rs` finds the prototype outside the `if syscall` block
- [ ] `cargo test c_declares_exit_without_syscall` exits 0
- [ ] `cargo test both_backends_run_01_through_08` exits 0
- [ ] `cargo test` exits 0
- [ ] Plan 005 row is DONE

## STOP conditions

- Windows `01_basics` IR contains `j3_syscall` / the C text contains `j3_syscall`. The "no syscall" premise is false. STOP.
- Linux C runs of `01`–`08` fail to compile after the extra prototype (duplicate with a conflicting signature). STOP and report clang's error. Do not delete the prototype to go back to an implicit declaration.

## Maintenance notes

- If `Abort` stops calling `_exit`, delete the prototype in the same change.
- Do not switch the printer to `exit(1)` here. LLVM and C both use `_exit`.
