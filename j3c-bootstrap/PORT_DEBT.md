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

## Stage-0 bugs (miscompiles in the Rust `j3c` the port works around; not Rust changes)

- (a) enum ABI align: a payload enum lowers to LLVM `{ i32, [N x i8] }` (align 4) while `ir/abi.rs` gives it its
  payload's align 8, so a struct with such a field at offset 4 mod 8, or whose size ends 4 mod 8, is laid out
  smaller by LLVM than the `memcpy` sizes assume (garbage `this`, lost fields). Workaround: payload-enum fields
  8-aligned, bools last, a `pad: u32` where needed (`ir.Func`, `GlobalDef`, `BinaryRv`, `IndexProj`, `LlvmFn`,
  `DeferFrame`, `syntax.ast.LetPatStmt`)
- (b) SIGSEGV returning a large payload struct out of a `match` arm (`StmtKind.Assign(a) { return a; }`). Workaround:
  work on the payload inside the arm
- (c) destroying and recreating a scratch `Arena` in a loop crashes only in split per-module builds (`[emit] chunks = 0`
  hides it). Workaround: per-fn LLVM buffers stay in the Session arena (`backend/llvm.j3` `emitFuncs`)
- (d) a `u64` inside `{..}` interpolation prints signed (18054884314459144840 prints -391859759250406776).
  Workaround: `Text.pushU64` (`backend/llvm/rvalue.j3` `constBits`)
- (a) also applied to sema: `ConstVal` (+ `pad`), `ConstDecl`, `FieldDecl`, `FnDecl`, `TForEach`, `TLetPat`, `traits.Member` (+ `pad`);
  and (C2b wave 7) `ir.UnaryRv` (operand first + `pad`), `ir.AssignOpStmt` (operand before `op` + `pad`),
  `project.manifest.Emit` (`chunks` first). None open from the last layout scan.
- Rust bug (not copied): a pattern warning in a part file prints the module main file as its path (`check/lower/pat/check.rs:320` uses `self.path`; `check/mod.rs:251` fixes the path for errors only). stage0 `src/util.j3:4:51`, boot `src/util/more.j3:4:51`: the one known `check` stderr difference.
