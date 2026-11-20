# Compiler cases (plan 11 B9)

Each case is one `.j3` (or `.j3s`) file under `tests/cases/` (folders by area: `basics/`, `generics/`, ...)
with its expectations in header comments at the top. One runner runs them all:

    j3c-bootstrap/target/j3c test-compiler [--compiler PATH] [-j N] [DIR|FILE...]

Default compiler: `j3c` itself (`--compiler PATH` runs the cases on another one). Default DIR:
`tests/cases` (from the cwd, else next to the binary). `-j` (default 4) runs cases in parallel; results
print in case order (`ok NAME` / `FAIL NAME: reason`), then `test-compiler: N ok, N FAIL`; exit 1 on any
failure, 2 on bad usage. Each case runs on a copy in its own temp dir (removed after), headless, under
`timeout 120`. A case with `expect-err` runs `check FILE`; any other runs `run --headless FILE ARGS`. Porting a Rust test from `j3c/src/compile/tests/*.rs` (tag `rust-final`) means turning its inline source into a
file here; a new feature's tests are new files. These run beside `tests/diff.sh` (expected output) and
`tests/unit.sh`.

## Header lines

Read from the leading `//` comment block, before `module`. Any other comment line is prose (older cases keep a
`// Port of: compile/tests/x.rs test_name` line naming the Rust test they came from). An unknown
`expect-*` key fails the case, so a typo cannot pass silently. Line and column count the header too.

| Line | Meaning |
|---|---|
| `// expect-out: TEXT` | The built program prints TEXT as one stdout line. Repeat the line for more lines, in order. |
| `// expect-exit: N` | The program exits with N (default 0). |
| `// expect-err: [CODE] at L:C [MESSAGE]` | Compiling fails with this error at line L, column C (1-based, the first `-->` span). CODE (`E0042`) is checked when the compiler prints codes (`error[E0042]: ...`); MESSAGE must be part of the message. Until codes print (plan 11 D1) a CODE needs a MESSAGE too. Repeat for more errors, in order; the error count must match. A case with `expect-err` is never run. |
| `// expect-warn: [CODE] at L:C [MESSAGE]` | Compiling prints this warning (others may print too; listed ones in order); the build still succeeds. |
| `// backends: llvm` | Backends to run it on, space separated. Default: `llvm`. `bc` (the bytecode VM) fails for now: nothing selects it for one file yet (`j3c` builds a lone `.j3s` with `pub fn main` ahead of time, so `scripts/` cases are llvm). `c` fails: there is no C backend (B8a). |
| `// args: x y` | Command-line arguments for the program, split on spaces. |

A case with no `expect-*` line only has to compile and exit 0.

## Example

`basics/add.j3`:

```j3
// A sum, printed: the smallest program case.
// expect-out: 5
// backends: llvm
module add;

use j3.io.printLn;

fn main() {
    let a = 2;
    let b = 3;
    printLn(`{a + b}`);
}
```
