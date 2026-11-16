# Port debt: Rust changes the port has to mirror

The Rust `j3c` is frozen for the port (`AGENTS.md`, plan 11 A8). Every Rust change made anyway gets one
line here, newest last:

`- <Rust commit>: <what changed> (<j3c/src/... files>) -> <j3c-bootstrap/src/... file to mirror it> | already mirrored`

Mark a line done by changing its target to `already mirrored` once the bootstrap file does the same
thing; delete done lines when stage 0 is bumped past them.

**Stage-0 bump rule:** `tools/stage0.sh` may move `stage0/j3c` to a newer commit only when every line
here says `already mirrored`, or each open line has a matching entry in `tests/known/<mode>.txt`.
Stdlib-only edits (`j3c/stdlib/`) need no line, but still need a bump for the oracle to see them.

## Ledger

- pre-freeze: `[package] bin` key, the program file name, default `name` (`j3c/src/project.rs`,
  `j3c/src/pkgtool.rs`, `j3c/src/devrun.rs`) -> mirrored by the port of `j3c-bootstrap/src/project/manifest.j3`
  (and `cmd/run.j3` for the `<bin>-dev` name)
- pre-freeze: `j3c dump-ast FILE`, S-expression AST on stdout, lex errors as `j3c fmt` prints them, parse errors as the rendered diagnostic, rc 1
  (`j3c/src/parse/dump.rs`, `j3c/src/cli/dump.rs`) -> mirrored by the port of `j3c-bootstrap/src/syntax/dump.j3`
  and `j3c-bootstrap/src/cmd/dump.j3`
