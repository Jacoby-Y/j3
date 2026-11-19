# j3c-bootstrap: the J3 compiler (`j3c`)

`j3c` is written in J3 here (`src/`); since plan 11 C6 it is the installed compiler (`./build.sh`). Layout
and stage order: `README.md`. The Rust compiler it was ported from is at the git tag `rust-final` and runs
as `stage0/j3c`: the seed `./build.sh` builds stage 1 with, and the `tests/diff.sh` oracle (same output,
byte for byte, unless a plan item or `PORT_DEBT.md` says otherwise). `stage0/` and `tests/diff.sh` are
for the compiler only; stdlib and editor work lives in `../j3c/` (`../j3c/AGENTS.md`).

## Language contract

The contract is this file, `../roadmap.md`'s non-goals, and `../speculative/`: `01`-`08` compile and run
(`08` via headless `j3.game`), `09` typechecks `j3.game`, `10_j3ui.j3` typechecks, `speculative/lang/*.j3`
(one per language feature) run on both backends (LLVM and the `.j3s` bytecode). The spec wins over
examples; examples win over compiler comments.

Emit is LLVM text plus the host `clang` (no C backend, no libLLVM). `--fast` strips out-of-bounds aborts.
`--release` ships small: opt `s`, no `-g`, stripped, and swaps `j3.build` (`DEV`) for `build_release.j3`
(`RELEASE`). Every IR expression, statement and binding has a concrete type (no "filled in later"); places
are part of the IR (field/index of `&T` name a slot, `+=` binds the place once); do not intern places or
switch to a CFG. One parser (`syntax/`), shared by `fmt`, `lsp` and `doc`: grow the grammar there.

## Do not build

Borrow checker, drop/RAII, trait solver, comptime interpreter, heap or refcounted closure environments
(escaping closures are arena-boxed: `fn[mem]`), GC, user-defined `#[attributes]`, operand swap, a second
parser, a C backend, libLLVM, Cranelift. No dependencies in `j3.toml` (the compiler links libc and libm only).
Unknown `#[foo]` / `@foo()` are parse errors: `@ident(...)` is only compiler calls (`@syscall`,
`@sizeof`, `@slice`, `@atomic*`, `@embed*`, `@shader`); attributes are `#[attr]` on the next item.

## Coding rules

1. **Stored strings are never frame strings.** Calls through fn values, closures, `dyn` tables and C
   functions are assumed to keep nothing. Any string that outlives the expression that made it (AST, IR,
   diagnostics, tables) is a `Sym` from `base/intern` or an arena copy. Reviewers check this every chunk.
2. **Codegen and long text use `Text` / `t.fmt(...)`**, never plain interpolation. The stack path caps at
   512 bytes and exits with code 102; one LLVM line passes that.
3. **No hash-order output.** Anything printed (IR dump, `.ll`, diagnostics, cache key list) iterates a
   `List`, a `SortedMap` or `Map.sortedKeys`, never raw `Map` slots. Stage 2 = stage 1 depends on it.
4. **Cross-stage references are `Id<T>`, not pointers.** AST nodes, types, fns and modules are handles
   into tables owned by the stage that made them. Pointers stay inside one stage's working memory.
5. **One arena per compile `Session`, plus scratch arenas per fn.** Never a global arena that only
   grows: the LSP and MCP servers live for hours.
6. **Exhaustive matches without `_`** in `syntax/walk`, `syntax/fmt`, the IR validator and every backend
   dispatch. A new variant must fail to compile everywhere that has to handle it.
7. **Files under 600 lines**, split at real seams. Every file keeps its header: what it does,
   `Port of:` (Rust paths relative to `j3c/src/` at the tag `rust-final`), `J3:` notes. A Rust file you drop goes in
   `DELETED.md` with its reason.
8. **Headless only.** Never open a window or play audio: `j3c run --headless`, built binaries with
   `SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy`, everything long in `timeout`.

Also: a file may `use j3c.X` only for X in its own stage or above (`tools/layers.txt`). Tools (`cmd`,
`lsp`, `mcp`, `scene`, `editor`, ...) never `use j3c.lower`; they go through `driver`.

## Checklists

Each list is every place that has to change. The exhaustive matches catch most of them; the rest are on
you.

**Adding syntax**
- `syntax/token.j3`, `syntax/lex.j3` (keywords from `text/syntax/keywords.txt`)
- `syntax/ast.j3`, `syntax/parse.j3` and its parts
- `syntax/walk.j3`, `syntax/fmt.j3` (`diff.sh fmt` must stay clean)
- `textmate.j3` (grammar; then regenerate `editors/vscode` with `dump-syntax`)
- `lsp/complete.j3` (keywords, completion context)
- `sema.j3`, `lower.j3`; `shader.j3` and `backend/bc.j3` support it or reject it with a clear error
- `text/doc/` (`j3c doc lang`), a `speculative/lang/` sample, a `tests/cases/` file

**Adding a type**
- `sema.j3` (resolve, typing, generics, traits), `lower.j3`
- `ir.j3` (type table), `ir/dump.j3`, `ir/validate.j3`
- `backend/llvm.j3` (layout, ABI), `backend/bc.j3`, `shader.j3` (support or reject)
- `lsp/query.j3` (hover), `doc.j3` (signatures), `text/doc/`, a `tests/cases/` file

**Adding an IR op**
- `ir.j3`, `ir/dump.j3` (byte-compatible with `j3c/src/ir/dump.rs` at `rust-final` while stage 0 is the oracle),
  `ir/validate.j3`
- `lower.j3` (makes it), `ir/passes.j3` (each pass that must see it), `devmode.j3`
- `backend/llvm.j3`, `backend/bc.j3` (or a clear VM rejection), `shader.j3` (or rejection)
- a `tests/cases/` file

## Gates

Run from the repo root. Every chunk ends with all of them clean, or with its leftovers listed in
`tests/known/<mode>.txt`.
- `j3c-bootstrap/tools/coverage.sh`: prints every Rust file with no `Port of:` home and no `DELETED.md`
  line; must print nothing.
- `j3c-bootstrap/tools/layers.sh`: fails on a `use` that points down `tools/layers.txt`.
- `j3c-bootstrap/tests/diff.sh MODE [--norm] [files...]`: stage 0 against `target/j3c`, one line per file
  (`ok` / `DIFF` / `CRASH`), then a summary line to paste in your report. Modes in port order: `fmt`,
  `err`, `ir`, `llvm`, `run`, `self`. Runs coverage and layers first.
- `timeout 300 j3c-bootstrap/stage0/j3c check j3c-bootstrap` and `... build j3c-bootstrap` (binary
  `j3c-bootstrap/target/j3c`; `./build.sh` installs a stage-2 build of it, never this file).
  `tests/diff.sh` builds it with `build --opt 2` (plan 13: about 3x faster than the `opt = 0` dev build and
  smaller stripped); `j3.toml` stays `opt = 0`, which is what `diff.sh self` and `tools/bench.sh` time.
- `j3c-bootstrap/tools/bench.sh [--only N,M] [--list]`: the plan 13 speed bench, stage0 against `target/j3c` on
  scratch copies; writes `target/bench/result.md`. Scoreboard in `SPEED.md`.
- `j3c-bootstrap/tools/stage0.sh rust-final`: the only way `stage0/j3c` changes. Scripts call
  `stage0/j3c` by path, never `j3c` from `PATH`.

## Stage 0 is frozen

- The Rust compiler is at the tag `rust-final` only (`j3c/src/` is gone from the branch); it never changes.
  `tools/stage0.sh rust-final` rebuilds `stage0/j3c` from it (needs cargo).
- Stdlib edits go in `j3c/stdlib/`: `target/j3c` embeds the tree on disk, stage 0 the tree at its commit,
  so a stdlib change can show up as a `diff.sh` DIFF; list it in `tests/known/` or `PORT_DEBT.md`.
- Behaviour changes in the J3 compiler (plan 11 Part D) go in `PORT_DEBT.md` with the `diff.sh` cases they move.
