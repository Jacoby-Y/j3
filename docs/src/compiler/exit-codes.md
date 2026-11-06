# Runtime Checks and Exit Codes

J3 checks for a few mistakes at runtime and stops the program with a clear message instead of corrupting memory.

## What is checked

| Check | Example |
|---|---|
| Array and slice bounds | `xs[10]` on a 4-element array |
| Division by zero | `n / 0`, `n % 0` |
| Interpolated string too long | A `` `{...}` `` string past 512 bytes |

A failed check prints `file:line: reason`. `--fast` (or `[emit] fast = true`) removes the bounds-check aborts.

Integer overflow is *not* an error: it wraps.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Normal exit |
| `101` | `panic(...)` was called |
| `102` | A runtime check failed |
| `128 + n` | Killed by signal `n` |

Dev builds on x86-64 Linux install a crash handler that prints a symbolized stack trace.

## Compile-time checks

These are caught before the program ever runs:

- Type mismatches, including mixing number types without a conversion
- A `match` that misses a variant
- Import cycles between modules
- Naming an item that isn't `pub`
- A `#[singleton]` with a non-constant initializer
- Writing to a `const` table
