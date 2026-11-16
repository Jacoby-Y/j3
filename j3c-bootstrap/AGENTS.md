# j3c-bootstrap: rules for port agents

You are porting `j3c` from Rust (`j3c/src/`) to J3 (`j3c-bootstrap/src/`). Layout and stage order:
`README.md`. The Rust tree is the oracle: same output, byte for byte, unless a plan item says otherwise.

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
   `Port of:` (Rust paths relative to `j3c/src/`), `J3:` notes. A Rust file you drop goes in
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
- `ir.j3`, `ir/dump.j3` (byte-compatible with `j3c/src/ir/dump.rs` while Rust is the oracle),
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
- `j3c-bootstrap/tests/diff.sh MODE [--norm] [files...]`: stage 0 against `j3c-boot`, one line per file
  (`ok` / `DIFF` / `CRASH`), then a summary line to paste in your report. Modes in port order: `fmt`,
  `err`, `ir`, `llvm`, `run`, `self`. Runs coverage and layers first.
- `timeout 300 j3c-bootstrap/stage0/j3c check j3c-bootstrap` and `... build j3c-bootstrap` (binary
  `j3c-bootstrap/target/j3c-boot`; never install it, never name it `j3c`).
- `j3c-bootstrap/tools/stage0.sh COMMIT | --worktree`: the only way `stage0/j3c` changes. Scripts call
  `stage0/j3c` by path, never `j3c` from `PATH`.

## The Rust compiler is frozen

- Rust `j3c` changes only for bugs the port finds, oracle tooling, or what the person asks for; no new
  language features. Stdlib edits go in `j3c/stdlib/` (both compilers embed it; no ledger line).
- Every Rust change gets a line in `PORT_DEBT.md`. Bumping stage 0 needs that ledger empty, or each open
  line matched by a `tests/known/` entry.
