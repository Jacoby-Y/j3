# j3c-bootstrap

`j3c`, the J3 compiler, written in J3. Since plan 11 C6 this is the installed `j3c`: `../build.sh` builds
stage 1 with `stage0/j3c` (the Rust compiler from the git tag `rust-final`), stage 2 with stage 1, checks
they agree and installs stage 2. Every file's header says what it does and which Rust files it replaced
(`Port of:`, paths in `j3c/src/` at `rust-final`). A plain `build` writes `target/j3c`.

Agents read `AGENTS.md` first (coding rules, checklists, gates). Rust changes made during the port
are in `PORT_DEBT.md`; Rust files that are not ported are in `DELETED.md`. The gate is
`tests/diff.sh MODE` (`tests/README.md`).

The language features the port needs come first: `plans/boostrap_features.md` (plan 08, done). Setup, rules and gates for the port itself: `plans/11-pre-bootstrap.md`. Lines starting
`J3:` in a file header say which plan item that file waits on, or what changes from the Rust version.

Find where a Rust file went: `grep -rn "check/lower/enums.rs" j3c-bootstrap/src`.

## Layout

The package is named `j3c`, so `src/lower/stmt.j3` belongs to module `j3c.lower`. Big stages are **one
module split over part files** (`lower.j3` plus `lower/*.j3`, all saying `module j3c.lower;`). The
parts share names and `use`s with no `pub` plumbing between them, which matches how the Rust crate's
submodules were used. Put a stage's `use`s in its main file.

Stages, in order. A stage may use only the stages above it (`tools/layers.txt`, checked by `tools/layers.sh`):

| Stage | Files | What |
|---|---|---|
| base | `base/*` | span, diag, intern, `Id<T>`, toml, zip, deflate, hash, fs helpers, phase timings |
| project | `project/*` | `j3.toml`, deps, user config, appconfig |
| syntax | `syntax/*` | tokens, lexer, AST, parser (`syntax/parse` parts), extend folding, flow, walk, fmt |
| ui | `ui.j3`, `ui/markup` + parts, `ui/gen` + parts | the `.j3ui` compiler |
| load | `load.j3` + parts | find and parse every module, embedded stdlib, cycles, scripts, scene scripts |
| shader | `shader.j3` + parts | the shader language: check and write GLSL in one walk; an AST pass sema runs before it builds modules |
| sema | `sema.j3` + parts | resolve, uses, types, traits, generics, and every body typed into a typed tree (plan 11 B11) |
| ir | `ir.j3` + parts, `ir/validate`, `ir/dump`, `ir/fast` | the typed IR and IR-to-IR passes |
| lower | `lower.j3` + parts | sema's typed tree to IR, no type decisions (plan 11 B11) |
| devmode | `devmode.j3` | the script dev-mode rewrite (an IR pass) |
| backend | `backend.j3`, `backend/{llvm,bc}` + parts | IR to LLVM text, script bytecode (no C backend: plan 11 B8a) |
| toolchain, cache | `toolchain.j3` + parts, `cache.j3` + parts | run clang and link, incremental objects |
| driver | `driver.j3` | `Options` + `Session`, one `driver.run`; every command and tool compiles through it |
| tools | `cmd/*`, `pkgtool.j3`, `doc*`, `lsp*`, `mcp*`, `scene*`, `editor*`, `assets/*`, `dist*`, `textmate*` | everything a command does on top of the compiler, through `driver` (never `use j3c.lower`) |

`main.j3` only hands argv to `cmd.args`.

## What changed from the Rust layout (post-mortem)

- **Stages are named after what they produce.** `check/` held both declaration checking and body lowering
  (`check/lower/`). Now those are `sema` and `lower`. `emit/` held printers, the clang driver and the
  build cache. Now those are `backend`, `toolchain` and `cache`.
- **The shader language is one module.** `check/gpu` already wrote GLSL while it checked, so
  `emit/glsl` was a split with no real seam. Both are in `shader` now.
- **Merged files that were split by accident:** `parse/extend.rs` + `check/extend.rs`,
  `check/traits.rs` + `check/lower/bounds.rs`, generics from three files, three error types into
  `base/diag`, zip writer + reader into `base/zip`, the `attr_*` slices kept but `bind_group` folded in.
- **Dev mode and `--fast` are IR passes.** They were in `compile/dev*` and `ir/fast.rs` with a separate
  body builder. They are `devmode` and `ir.fast` now.
- **Deleted, because the stdlib or the language covers them:**
  - `scenefile.rs`: scenes are read through `j3.scene3d`. The repo rule is that it's the only reader.
  - `lsp/json.rs`: replaced by `j3.data.json`.
  - `csslint_main.j3` and the build-and-run-a-helper dance: `cmd/csslint` calls `j3.ui.css` in-process.
  - `stdlib/files.rs` (the hand-kept list of 461 files): replaced by `@embedDir` (plan 08 A7).
  - `ir/samples*.rs`, `ir/build.rs`: hand-built IR from before the parser existed. Tests are J3 source.
- **Data moves out of code.** Doc pages, recipes, `j3c new` templates and help text go to `text/` and are
  embedded, instead of living in Rust string constants.
- **`check` is redesigned, not transliterated** (plan 11 B11): typing (`sema`) and lowering (`lower`) are
  separate passes. **Stays in Rust's shape on purpose:** `syntax/walk` stays
  exhaustive with no `_` arm. The `.j3ui` generator keeps its file split.

## Order to port

1. Plan 08 features in the Rust j3c (this tree compiles with that j3c).
2. `base`, `syntax`, `load`: `j3c-bootstrap` parses every stdlib file and prints the same AST errors.
3. `sema`, `ir`, `lower`, then `backend/llvm`, `toolchain`, `cache`, `cmd/build`, `cmd/run`: `diff.sh err` and
   `diff.sh run` match Rust exactly; `diff.sh ir --norm` and `llvm` are exact only for plain code (C2a) and
   advisory after, because `check` is redesigned (plan 11 B11, Part C).
4. Self-host: stage 2 (j3c-bootstrap built by stage 1) gives the same output as stage 1 (`diff.sh self`).
5. `backend/bc`, `shader`, `devmode`, and the rest of `ui` and `project`. After this it can replace the Rust
   compiler for building games.
6. The tools (`lsp`, `doc`, `mcp`, `scene`, `editor`, `dist`, `assets`, `textmate`). The LSP and MCP
   servers, `j3c editor`, `-e android`, `-e web` and asset import are still stubs: `plans/12-deferred-tools.md`.
7. Switch (C6, done): this tree is `j3c`; `j3c/src/` is deleted (tag `rust-final` keeps it).

How to test it: `tests/README.md`.

## Seed

A fresh clone with no `stage0/j3c` builds the Rust compiler at the tag `rust-final` with cargo
(`tools/stage0.sh rust-final`, run by `../build.sh`), uses it to build stage 1, then stage 1 builds itself. There is no C seed (no C backend). If a clang-only seed
is ever wanted, commit `dump-llvm` output of `j3c` for x86-64 Linux.
