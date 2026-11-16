# Compiler cases (plan 11 B9)

Each case is one `.j3` file under `tests/cases/` (folders by area: `basics/`, `generics/`, ...) with its
expectations in header comments at the top. One runner, `j3c-boot test-compiler` (not built yet), runs
them all. Porting a Rust test from `j3c/src/compile/tests/*.rs` means turning its inline source into a
file here; a new feature's tests are new files. Until the Rust compiler is retired, `tests/diff.sh` is the
main gate and these are extra; after that, `cases/` is the suite.

## Header lines

Read from the leading `//` comment block, before `module`. Any other comment line is prose.

| Line | Meaning |
|---|---|
| `// expect-out: TEXT` | The built program prints TEXT as one stdout line. Repeat the line for more lines, in order. |
| `// expect-exit: N` | The program exits with N (default 0). |
| `// expect-err: E0042 at 4:5` | Compiling fails with this error code at line 4, column 5 (1-based, the first `-->` span). Repeat for more errors, in order. A case with `expect-err` is never run. |
| `// expect-warn: W0007 at 9:1` | Compiling prints this warning; the build still succeeds. |
| `// backends: llvm` | Backends to run it on, space separated. Default: `llvm`. A `.j3s` case uses `vm` (the bytecode VM). `c` is not a backend (B8a): there is no C backend to name. |
| `// args: --flag x` | Command-line arguments for the program. |

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
