# Plan 11: before the bootstrap (setup, footguns, and the shape of the new j3c)

Status: TODO

Goal: get everything in place so porting `j3c` (Rust, about 84k lines in `j3c/src/`) into `j3c-bootstrap/`
(J3) is a long, boring run of chunks with a mechanical "done" check for each one. This plan also fixes the
layout the new compiler will grow into after the port, because the stubs written now lock that layout in.

Read first: `j3c-bootstrap/README.md` (layout, stage order, post-mortem), `j3c-bootstrap/tests/README.md`,
and plan 08 (`plans/boostrap_features.md`) for the language features the port relies on.

The rule that ties this plan together:

> **Port faithfully, restructure internally, change behaviour only after parity.**
> Until stage 2 matches stage 1, every user-visible output the oracle compares (fmt text, error text,
> program output) must match the Rust `j3c` byte for byte. Internal structure can differ (Part B).
> Visible improvements (several errors per run, new error codes, faster output) wait for Part D.
>
> **One exception: `check` is redesigned, not transliterated (B11).** Its IR doesn't have to match
> Rust's, so `dump-ir` and `.ll` are compared to Rust only where B11 says so. Stage 2 = stage 1 stays
> exact for everything.
>
> **Second exception: the tools (B12).** `lsp`, `mcp`, `scene`, `editor`, `doc`, `dist` aren't held to
> byte parity with Rust. Their own headless tests are the gate. The oracle pins the compiler, not the
> tools.

---

## Part A: setup (do all of it before porting any code)

Each item lists the footgun it removes.

### A1. A clean, tagged baseline

Footgun: the tree has about 276 modified files that aren't committed. A differential failure can't be
pinned to a version of the Rust compiler, and a bad `git checkout` loses a week of work.

Do: commit the working tree (the person decides how to split it), then tag it `bootstrap-base`. The
harness (A4) records the Rust `j3c` commit it compared against.

Done when: `git status` is clean and the tag exists.

### A2. Pin the stage-0 compiler

Footgun: `../.cargo/config.toml` turns on `j3c/.cargo/copy-j3c.sh`, so every `cargo build` in `j3c/`
overwrites `~/.bin/j3c`. That binary builds `j3c-bootstrap` and also serves as the oracle. Fixing a Rust
bug in the middle of the port (which will happen) would quietly change both the compiler and the expected
output, and a half-finished Rust edit would break the bootstrap build for reasons that have nothing to
do with the port.

Do:
- `j3c-bootstrap/stage0/j3c` is a copy of a known-good release build (gitignored), plus
  `stage0/VERSION` (tracked) holding the commit and `j3c version --verbose` output it came from.
- `j3c-bootstrap/tools/stage0.sh` rebuilds it from a given commit in a scratch worktree with
  `J3C_NO_INSTALL=1` and copies the result in. This is the only way it changes.
- Every bootstrap script calls `stage0/j3c` by path, never plain `j3c` from `PATH`.

Done when: running `cargo build` in `j3c/` doesn't change any harness result.

### A3. The new compiler is never called `j3c` until it replaces the old one

Footgun: the package is named `j3c`, so the built binary is `j3c`. One mistaken `cp`, or a `build.sh` that
picks up the wrong file, and stage 1 replaces the working compiler on `PATH`.

Do: set the output binary name to `j3c-boot` in `j3c-bootstrap/j3.toml` (add a `[package] bin = ...` key to
the Rust `j3c` if no such key exists yet). `build.sh` refuses to install anything from `j3c-bootstrap/`.
Renaming it to `j3c` is the last step of Part C.

### A4. The diff harness comes first

Footgun: "matches the Rust one" checked by eye, chunk by chunk, drifts. Every chunk needs the same
mechanical gate.

Do: `j3c-bootstrap/tests/diff.sh MODE [files...]` runs `stage0/j3c` and `j3c-boot` on the same inputs and
diffs their output. Modes, in port order:

| Mode | Compares | Gate for |
|---|---|---|
| `fmt` | `j3c fmt` output (the parse oracle, see A5) | syntax |
| `err` | stderr and exit code on `tests/bad/*.j3` and on every `j3c/src/compile/tests/diag/*.err` input | syntax, sema, lower |
| `ir` | `dump-ir` | sema, lower, ir |
| `llvm` | `dump-llvm` | backend |
| `run` | stdout and exit code of the built program (headless) | toolchain, cmd |
| `self` | stage 2 output equals stage 1 output on the whole corpus, plus on `j3c-bootstrap` itself | self-host |

The corpus is a tracked list, `tests/corpus.txt`: `speculative/0*`, `speculative/lang/*`,
`j3c/stdlib/tests/*`, every `examples/*` entry, and `j3c-bootstrap/src` itself. The harness also runs every
mode twice on stage 0 and fails if the two runs differ, so a nondeterministic oracle can't pass as a port
bug. (Spot check on 2026-11-16: `dump-ir`/`llvm` were stable on the `dyn`, `tuples` and `boxed_closures`
samples. Check the whole corpus.)

Output: one line per file (`ok` / `DIFF` / `CRASH`), the first 40 lines of each diff, and a summary line
that a chunk report can paste. The failure lists are tracked under `tests/known/<mode>.txt`, so a chunk
shows progress as a shrinking file instead of a claim.

Done when: `diff.sh fmt` runs against a stub `j3c-boot` and every line says `CRASH`, which is correct for
now.

### A5. A parse oracle

Footgun: there is no `dump-ast`, so the syntax stage's only check would be "prints the same parse errors".
That lets a wrong-but-parsing AST through until `dump-ir` fails three stages later, far from the cause.

Do: use `j3c fmt` as the oracle. It already prints the AST (plus comments). Port `syntax/fmt` along with
the parser, not later. If `fmt` hides AST detail that matters (precedence, for example), add
`j3c dump-ast` to the Rust compiler: an S-expression print from `parse/walk.rs`, under 200 lines.

### A6. Decide what "byte-compatible IR" means

Footgun: an exact `dump-ir` match forces the same temp numbering, local ids and lowering order, which
turns `lower` into a line-by-line transliteration. The README's merges (`check/traits.rs` and
`lower/bounds.rs`, generics from three files) have to keep that order, or the oracle fails on numbering
noise and someone "fixes" it by editing the oracle.

Decision: superseded by B11 for `check`. `diff.sh ir --norm` (renumber locals and temps in order of first
appearance on both sides) is still built: it gates the C2a slice, where the new lowering of plain code
should match Rust's up to numbering, and it's a debugging aid after that. `ir` and `llvm` against Rust
are advisory past C2a. `self` (stage 2 = stage 1) is always exact.

### A7. Refresh the stubs, and check coverage mechanically

Footgun: the stubs date from before most of plan 08 shipped. 33 Rust files have no `Port of:` line, and
these aren't small: the frame-string escape checker (`check/frame_str/*`), `derive`, `embed`,
`global_init`, `field_default`, `lower/{tuple,dyn_call,dyn_up,closure_boxed,for_iter,lazy,zeroed,defcheck,
scene_script}`, `lower/pat/*`, `ir/{tuple,dyn_ty}`, `parse/{expr,pat}`, `parse/parser/{types,value,embed}`,
`compile/loader/embed`, `emit/c_static`, `lsp/query/traits`. Many headers still say "waits on plan 08 A4",
and that work is done.

Do:
- Give each unmapped file a home in a stub (new part file or an existing one) and update the stale `J3:`
  lines.
- `j3c-bootstrap/tools/coverage.sh` lists every `j3c/src/**/*.rs` that isn't a test, takes away every path
  named in a `Port of:` line and every path in `j3c-bootstrap/DELETED.md` (one line each, with the reason:
  `scenefile.rs`, `lsp/json.rs`, `ir/build.rs`, `ir/samples*`, `stdlib/files.rs`, `csslint_main.j3`, and the
  C backend `emit/c*.rs`, see B8a), and
  fails if anything is left. `diff.sh` runs it first.

Done when: `coverage.sh` prints nothing.

### A8. The Rust compiler is frozen, with a ledger

Footgun: the oracle moves. Plan 08 changed about 190 files in a few weeks. If language work keeps landing
in Rust during the port, the port chases a moving target and the oracle mixes old and new behaviour.

Before freezing: finish plan 09 chunks 6-7 (Steam out of `j3c`). That removes `steamcheck`, `[steam]`
parsing, `dist/steam` and `dev_env` from the Rust tree, so the port shrinks instead of porting code that's
about to be deleted. Then delete the matching stubs (`steamcheck*`, `project/steam.j3`, `dist/steam.j3`)
before A7's coverage check. Any other open plan that edits `j3c/src` either lands before the freeze or
waits until C6.

Policy for the port's duration:
- Rust `j3c` changes only for: (a) bugs the port finds, (b) oracle tooling (`dump-ast`, the `bin` key),
  (c) things the person explicitly wants anyway. No new language features.
- Each such change gets a line in `j3c-bootstrap/PORT_DEBT.md`: the Rust commit, what changed, and the
  bootstrap file that has to mirror it (or "already mirrored"). Bumping stage 0 (A2) requires that ledger
  to be empty, or each open line to have a matching `tests/known/` entry.
- Stdlib edits go in `j3c/stdlib/` only. Both compilers embed the same tree, so they need no ledger line,
  but a bump of stage 0 is still needed for the oracle to see them.

### A9. Enforce the stage order

Footgun: "a stage may use only the stages above it" (README) is a convention. One shortcut `use` from
`syntax` into `sema` creates a cycle that's expensive to undo later.

Do: `tools/layers.sh` reads the stage table (a small tracked `tools/layers.txt`: stage name, path globs,
in order) and fails on any `use j3c.X` that points down the table. A `ponytail:` note in the script says
it's grep-based; a real check moves into `j3c check` once the compiler can read its own module graph.
`diff.sh` runs it.

### A10. Coding rules for the port (`j3c-bootstrap/AGENTS.md`)

These are footguns specific to writing a compiler in J3. Write them down before the first chunk, so every
agent follows them from line one.

1. **Stored strings are never frame strings.** Plan 08's escape check assumes that calls through fn
   values, closures, `dyn` tables and C functions keep nothing. A compiler stores strings in its AST, IR
   and diagnostics everywhere. Every string that outlives the expression creating it is a `Sym` from
   `base/intern` or an arena copy. Review checks for this in every chunk.
2. **Codegen and long text use `Text` / `t.fmt(...)`**, never plain interpolation. The stack path caps
   at 512 bytes and exits with code 102, and a single LLVM line passes that.
3. **No hash-order output.** Anything printed (IR dump, `.ll`, `.c`, diagnostics, the cache key list)
   iterates a `List`, a `SortedMap` or `Map.sortedKeys`, never raw `Map` slots. Stage 2 = stage 1 depends
   on it.
4. **Cross-stage references are `Id<T>`, not pointers.** AST nodes, types, fns and modules are handles
   into tables owned by the stage that made them (see B4). Pointers stay inside one stage's working
   memory. This keeps caching (B5) and threading (`load` already plans a per-wave pool) possible.
5. **One arena per compile session, plus scratch arenas per fn.** Never a global arena that only grows.
   The LSP and MCP servers live for hours (see B6).
6. **Exhaustive matches without `_`** in `syntax/walk`, `syntax/fmt`, the IR validator and every backend
   dispatch. Adding a variant must be a compile error everywhere that has to handle it (B7).
7. **Files under 600 lines**, split at real seams (the plan 08 rule). Each file keeps its header (what
   it does, `Port of:`, `J3:` notes).
8. **Headless only.** Windowed examples in the `run` corpus use `--headless` /
   `SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy`, wrapped in `timeout`.

### A11. Measure scale before writing `lower`

Footgun: the biggest J3 program compiled so far is about 160k lines (stdlib plus editor). `j3c-boot` will
be about 80k lines of its own plus the stdlib, built often, and it will compile big programs itself. A
compile-time or memory blowup found during `lower` costs weeks.

Do, twice:
- Now: time `stage0/j3c build` on `j3c-bootstrap` (stubs) and on the editor, and record wall time and
  peak RSS (`/usr/bin/time -v`) in `tests/SCALE.md`.
- After C1 (front end ported): stage 1 parses the whole corpus plus the stdlib, and that gets timed and
  measured the same way. If parsing is more than 3× the Rust time or RSS is more than 4× the Rust RSS,
  stop and fix it (allocation pattern, arena sizing, interning) before C2.

### A12. Decide the seed now, build it later

Footgun: once Rust is gone, a fresh clone needs some `j3c` binary to build `j3c`. Leaving this until the
switch makes the switch a two-week project.

Decision (recommended): keep the Rust compiler buildable at a tag (`rust-final`), and record in
`j3c-bootstrap/README.md` that the supported fresh-clone path is "build `rust-final`, use it to build
`j3c-boot`, then `j3c-boot` builds itself". No C seed (B8a: there is no C backend). If a clang-only
seed is ever wanted, commit `dump-llvm` output of `j3c-boot` for x86-64 Linux.

---

## Part B: the shape of the new j3c (designed now, built during the port)

The stubs already pick stages (`base` > `project` > `syntax` > `ui` > `load` > `sema` > `ir` > `lower` >
backends > `toolchain`/`cache` > `driver` > tools; `j3c-bootstrap/tools/layers.txt` is the source of truth). These items are about how the stages connect, so that
adding a feature, a backend, a pass or a tool stays cheap as the compiler grows. None of them changes
output, so they can be built during the port without breaking the oracle.

### B1. One driver, one options struct

Today `compile.rs` has eight entry points (`compile_path`, `_target`, `_headless`, `compile_manifest`,
`_target`, `_dev`, `compile_tool`, `check_path_lib`) plus `analyze_*`, and each new flag adds another.

New: `driver.j3` owns `struct Options { entry, target, mode: Build|Check|Lib|Tool|Dev, headless, fast,
opt, debug, ... }` and `struct Session { opts, arena, interner, diags, sources, tables... }`, plus one
function, `driver.run(&Session, upTo: Stage): Result<..>`. Every command and tool builds `Options` and
calls the driver. A new flag is one field plus the code that reads it. This replaces `compile.rs` and
sits after `toolchain`/`cache`, right above the tools, in the stage table.

### B2. Compiler core versus tools

Stages from `base` to `cache` are the compiler. Everything under `tools` in the README table (`cmd`,
`doc`, `lsp`, `mcp`, `scene`, `editor`, `dist`, `assets`, `steamcheck`, `textmate`) talks to it only through
`driver` and the read-only query functions of `sema` / `ir` (B5). Enforced by `layers.txt` (A9): a tool may
not `use j3c.lower`. This keeps the editor, LSP and MCP from depending on lowering internals, which is
where the Rust tree hurt most when the internals changed (`lsp/query/*` reached into `check`).

One binary still: ponytail, no plugin system and no separate tool binaries. Split them later if binary
size or build time asks for it.

### B3. Diagnostics as data

Today checking stops at the first error (`check_files` returns `Result<Program, CheckError>`), and there
are three error types.

New, during the port: a single `Diag { code, severity, span, msg, notes, fix }` list on the `Session`
(already in the `base/diag` stub). Every error site gets a stable code (`E0042`) now. While parity matters,
the renderer prints exactly what Rust printed (no code shown) and the driver stops after the first error,
the same as Rust. After parity (D1) the stop moves and codes become visible. Tests and the LSP key off the
codes, not message text, so rewording a message breaks nothing.

### B4. Tables and handles, with spans on the side

AST nodes, types, fns, modules and IR values live in per-stage `Pool<T>` tables addressed by `Id<T>`
(plan 08 A5). Spans live in a side table keyed by id, not inline in every node. Benefits: cheap equality
and hashing, stage outputs that can be cached or serialized (B5), threads can share read-only tables, and
`Id<Expr>` can't be passed where `Id<Stmt>` is expected.

### B5. The module is the unit of work

Every stage works one module at a time, with an explicit input:
- `load`: parse one file. Pure, so it can run in parallel (the stub already plans a pool per wave).
- `sema` (signatures): one module's declarations, given its dependencies' signatures.
- `lower`: one module's bodies, given all signatures. Can run in parallel per module once signatures are
  done.
- backend + `cache`: one object per module (as today).

A `Session` keeps a memo table keyed by `(stage, module, hash of inputs)`. A body-only edit doesn't
re-check dependents, because their signature hash didn't change. This is what makes the LSP fast and
the build cache finer than "one key per module object". Ponytail: a plain memo table, not a salsa-style
query engine. Build the memo table after parity. Shape the stage functions now (explicit inputs, no
reaching into global state), because adding that later is the expensive part.

### B6. Long-lived processes free memory

The LSP, MCP and `j3c editor` helpers run for hours. One arena per `Session` (A10.5), dropped when a
newer session replaces it; memo entries (B5) copied into a cache arena with a size cap. A test in the
harness: run the LSP over 1000 edits of one file, and RSS must level off.

### B7. Adding a language feature is a checklist the compiler enforces

Plan 08 showed what one feature touches: lexer, AST, walk, fmt, textmate grammar, LSP, sema, lower, IR,
both backends, the bytecode VM's rejection, the shader checker's rejection, docs, a `speculative/lang`
sample. In the new tree:
- Every one of those is an exhaustive `match` over the AST or IR enum (A10.6), so a new variant fails
  to compile until each place handles it, including the "reject with a clear error" places.
- `j3c-bootstrap/AGENTS.md` keeps the list as "adding syntax", "adding a type", "adding an IR op", each
  naming the files.

### B8. IR passes and backends are tables

- `ir/passes.j3` holds a list of `{ name, run: fn(&Session, &ir.Program) }` in order. `devmode` and
  `fast` are entries with a condition. That gives `--dump-after=PASS` and `--time-passes` for free (from
  `base/phase`), and a new pass is one entry.
- Backends are `dyn Backend` (`emit(prog, opts, out: &Text)`, `objectExt`, `link`). LLVM and bytecode
  are the two implementations. A web backend (plan 06) is one folder plus one table entry.
- The shader stage stays its own module (the README's choice). It reads IR through the same query
  functions the tools use.

### B8a. Backend roles

Each backend has one job, so new features don't land in two places for the same reason:
- **LLVM (`.ll` text, then clang)**: every build people run, dev and release. It provides the
  optimizer, every target (Windows, Android arm64, macOS), debug info and SIMD, and links no libLLVM.
  Kept as is.
- **C**: not ported. `emit/c*.rs` goes in `DELETED.md`, there is no `c` harness mode, and `[emit]
  backend = "c"` / `--emit c` become errors in `j3c-boot`. The only things it did were the no-clang
  fallback (clang is required anyway: the C path still needs a C compiler, and every target already
  uses clang) and a possible seed (A12 covers that). One codegen backend means every language feature
  lands once. This is the one deliberate output difference from the oracle: `--emit c` errors instead
  of building.
- **Bytecode**: the `.j3s` VM, as today.
- **Native dev backend (later, Part D)**: direct x86-64 machine code for dev builds, no clang. Measured
  2026-11-16 on desk-app (clean dev build, 24 threads): 1.1 s wall, `compile` phase 0.63 s, 10.0 s
  clang CPU against 3.25 s emit CPU. Clang is about 75% of codegen CPU, but on a 24-core machine
  that's about half a second of wall time. The gain is real on fewer cores and bigger games and
  modest here, so it waits until after parity and is only worth doing if those numbers grow. The
  backend table (B8) is the only part of it to decide now.

### B11. `check` is redesigned during the port

`check/` (about 17k lines, 8.6k in `lower/`) grew one feature at a time, and the same problem is solved
several ways: generic bodies are checked by cloning the whole `IrCx` with placeholder structs and throwing
the result away (`defcheck`), lazy generic methods are woken by scanning lowered IR for `FnAddr`s, `derive`
writes J3 source and parses it back, bound checks / generic instances / vtables have separate queues
drained at different times, there are three error types, and setup stops at the first error while lowering
recovers. Transliterating that keeps all of it. Exact IR parity would force it (temp numbering, lowering
order), so instead the new `sema` is written from scratch in this shape:

1. **Typing and lowering are separate passes.** `sema` types every expression into a typed tree (each
   node an `Id` with a resolved `Ty`, method / trait / generic instance already chosen). `lower` is a dumb
   walk from that tree to IR with no type decisions. Generic bodies are checked once by typing them with
   bound-only placeholder types: no `IrCx` clone, no throwaway lowering.
2. **One instantiation queue**, demand-driven: generic fns, generic struct methods, `dyn` vtables, `#[impl]`
   bound checks and `derive` methods are all "instance requested at span S", drained to a fixed point.
   It replaces `bound_queue`, `lazy_methods` + `wake`, `take_new_vtables` and `fn_insts` bookkeeping.
3. **One member lookup**: own method, then `#[impl]` fn, then default body, then built-in operator. Used
   by calls, operators, bounds, `dyn` tables and UFCS. Today `bounds.rs`, `binop.rs find_op`, `ufcs` and
   `traits.rs` each have a version.
4. **`derive` builds typed-tree nodes directly**, not J3 source text.
5. **One `Diag` type (B3) and one recovery rule** in every phase: a failed item or statement is poisoned
   and checking goes on. The renderer still stops after the first error (or prints the same set Rust
   prints) until Part D, so `err` output keeps matching.
6. **The frame-string check runs on the typed tree or the IR** (the implementer's choice), keeping the same
   rules and error text.

The IR data model (`ir/`: types, places, statements, `dump`, `validate`) is ported as it is. Only the way
`check` produces it changes. The backends don't care.

Target: noticeably smaller than the Rust `check/` (aim for about a third less) and a new language feature
touching fewer files. If a redesign item turns out bigger than the code it replaces, port the Rust approach
for that item and note why in the chunk report.

Gates while IR doesn't match Rust:
- `diff.sh err`: exact on every bad input and diag golden. This is what pins down the checker's
  behaviour.
- `diff.sh run`: exact program output on the whole corpus. This is what pins down lowering.
- `validate` passes on every IR the new compiler makes (built into `dump-ir` and the driver in dev mode).
- `ir --norm` / `llvm`: exact only for C2a (below), advisory after.
- `self`: exact, as before.

### B9. Tests are J3 files with expectations inside them

Instead of Rust's `compile/tests/*.rs` with inline strings: `j3c-bootstrap/tests/cases/**/*.j3`, each with
header comments such as `// expect-out: 42`, `// expect-err: E0042 at 4:5`, `// backends: llvm c`. One
runner (`j3c-boot test-compiler`) runs them all. Porting a Rust test turns it into a file, and a new
feature's tests are new files. The differential harness (A4) stays the main gate until the Rust compiler
is gone. After that, `cases/` is the suite.

### B10. Data stays out of code

Already in the README (`text/` embedded with `@embedDir`): doc pages, recipes, templates, help text.
Also put here: the error-code list (code, short title, long explanation for `j3c explain E0042`) as one
data file, the textmate grammar's keyword lists, and the default `j3.toml`. The compiler reads the same
file the docs show.

### B12. The tools are rebuilt in-process, not ported as they are

The Rust tools work around the fact that j3c is Rust and the editor is J3. `-e scene`, `-e render` and
`mcp` build the editor program (`target/j3_editor`, rebuilt per j3c version into `~/.cache/j3/editor`) and
spawn it headless once per call (`scene/mod.rs`, `mcp/ops.rs`). MCP talks to an open editor by polling
`target/j3-editor/events`. `editor/play.rs` and `mcp/ops.rs` spawn the host `j3c build` and parse its
stderr. Once j3c is J3, none of that is needed:

1. **Scene tools run in-process.** `j3c-boot` uses `j3c/editor/src/tool.j3` (and its `tool/` parts) and
   `scene3d` directly. No editor build, no spawn per call, no per-version editor cache for headless ops.
   `render` needs GL, so it may stay a subprocess (offscreen SDL) if linking GL into j3c is unwanted.
   Decide in C5 and note the choice here.
2. **The open editor is reached through `j3.instance`**, not the events file: `instance.request(name,
   args, ms)` from MCP/CLI, `App.onRequest` + `req.reply` in the editor. Real replies, no polling, no
   stale file. `live.j3` becomes the `onRequest` handler.
3. **One tool table** drives the CLI, MCP and docs. Each tool is one entry `{ name, args (name, type,
   default, help), help, run }`. The MCP schema (today a hand-written JSON string in `mcp/schema.rs`),
   `j3c -e scene --help`, `j3c -e mcp --tools` and the `editor` doc page are all generated from it, so they
   can't drift. Adding a tool is one entry plus its `run`.
4. **One JSON-RPC server loop** for LSP and MCP (both stdio JSON-RPC, today with separate framing and
   dispatch in `lsp/proto.rs` and `mcp/server.rs`). Built on `j3.data.json` and buffered `io.stdout`. Each
   server is a handler table on top of it.
5. **The LSP asks `sema`, not its own scanners.** Today it rechecks the whole program when any open
   document's hash changes (`lsp/mod.rs` `ProgramCache`), and completion partly scans source text
   (`lsp/complete/scan.rs`). New: the read-only query functions from B2 (type at span, members of a type,
   definition of a name, references), and per-module rechecks once the B5 memo table exists. Text
   scanning stays only where the source doesn't parse yet, and is marked as a fallback.
6. **`play` and MCP build through `driver.run()`** in-process instead of spawning `j3c build`, so build
   errors come back as `Diag` values, not parsed stderr. Running the built game stays a subprocess.
7. **Replies carry structured `Diag`s.** MCP `check`/`play` and the editor's error display get code,
   span and message as data (in the MCP reply as JSON next to the text), so an agent or the editor can
   jump to the error. Ties into B3.

Order: items 1, 2, 4 and 6 are plumbing and land in C5. Items 3, 5 and 7 change visible output (help
text, reply shape), which is allowed because the tools are exempt from byte parity (see the top). They
also land in C5, gated by the tools' headless tests, not `diff.sh`.

Gates: the editor's and MCP's existing headless tests pass, every MCP tool in `j3c -e mcp --tools` has a
table entry (checked by a test that diffs the generated schema against the tool list), the LSP smoke
test, and the B6 memory test.

---

## Part C: port order with gates

The README's order, with a harness gate for each step. Each chunk ends with `coverage.sh`,
`layers.sh` and its `diff.sh` mode clean (or with the remaining `tests/known/` entries listed in the
report), then `./build.sh` (installs only the Rust `j3c`, see A3).

- **C0. Part A.** Baseline, stage 0, binary name, harness, oracle, refreshed stubs, ledger, layers, AGENTS
  rules, first scale numbers.
- **C1. `base`, `syntax` (with `fmt`), `load`, plus `driver` skeleton (B1) and `Diag` (B3).** Gate:
  `diff.sh fmt` and `diff.sh err` (parse errors) clean on the corpus. Then the A11 scale check.
- **C2. `sema` (redesigned, B11), `lower`, `ir` (with `dump`, `validate`, the pass table from B8), and
  the LLVM backend.** The backend comes in here, not later, because past C2a the `run` mode is the
  lowering gate and it needs a backend. This is the biggest chunk by far. Split it:
  - **C2a. Plain code**: decls, structs, enums, exprs, statements, calls, no generics/traits/closures.
    Plus `ir`, `backend/llvm`, the minimum of `toolchain`/`cmd/run` to execute a program. Gate: `ir --norm`
    and `llvm` exact against Rust on the matching samples (`speculative/01`–`03`-style programs, the
    `lang` samples without generics), `err` and `run` clean on them. This proves the backend against
    Rust before the IR starts to differ.
  - **C2b–C2f**, by feature area: patterns, generics/traits (with the instantiation queue), closures and
    `dyn`, `derive`/tuples/embed and the rest, the frame-string check. Gate each on `err`, `run` and
    `validate` for its narrowed corpus (B11).
- **C3. The rest of `toolchain`, `cache`, `cmd/build`, `cmd/run`.** Gate: `diff.sh run` clean on the
  whole corpus, then `diff.sh self` (stage 2 = stage 1).
- **C4. `backend/bc`, `shader`, `devmode`, `ui`, `project`.** Gate: `.j3s` VM
  output, `dump-glsl`, `dump-ui` all clean. After this `j3c-boot` can build any game.
- **C5. Tools** (`lsp`, `doc`, `mcp`, `scene`, `editor`, `dist`, `assets`, `steamcheck`, `textmate`), each
  through `driver` only (B2), rebuilt per B12 (in-process scene tools, `j3.instance` to the editor, one
  tool table, one JSON-RPC loop, LSP on `sema` queries, structured `Diag` replies). Gates: B12's.
- **C6. Switch.** Tag `rust-final`. `j3c-boot` is renamed `j3c`, and `build.sh` builds it with the stage-0
  binary and then with itself. `j3c/src/` is deleted from `main` (the tag keeps it). The `j3c/AGENTS.md`
  contract moves to the new tree.

## Part D: after parity (behaviour changes, now allowed)

0. Speed baseline (after C5, before C6): time j3c-boot vs stage0 on the same inputs (self build cold/warm, the editor, a big game, `check` on the corpus, `doc`/`lsp` startup) and write the numbers down. Measure only; tuning is a separate pass, except fixes that are obvious or already listed here.

1. Several errors per run, with codes shown and `j3c explain` (B3, B10).
2. The memo table and finer cache keys (B5), parallel `lower`. The LSP switches to per-module rechecks
   (B12.5) once this lands.
3. Move the `j3c-bootstrap/tests/cases` runner into `j3c test` for user projects too, if it fits.
4. Optionally, a native x86-64 dev backend, if dev build times
   (re-measure with `target/compilation.log` on the editor and a big game, on 4-8 cores) show clang
   dominating wall time.
5. The open follow-ups from plan 08's status line (numeric trait, and so on) as a normal language plan.

## Chunks

**Tracking on the "j3c bootstrap" kanban board (`jzctl kanban`, see `jzctl kanban --help`).**
- When starting this plan for real, before any other work: create the board if it's missing
  (`jzctl kanban new 'j3c bootstrap'`) and add one card per open item: every chunk below and every
  Part A, B, C and D item (`A1`–`A12`, `B1`–`B12`, `C0`–`C6`, `D1`–`D5`). The card title is the id plus its
  title (`A4 The diff harness comes first`), and the body names the plan section and its "Done when"
  line. Part A and C0–C1 go in `Front Burner`, the rest in `Back Burner`.
- As work continues, keep the board current: move a card to `In Progress` when starting it and to `Done`
  as soon as it's implemented and its gate passes. Items split or dropped along the way get new cards or
  go to `Canceled`, with the reason in the body. Card numbers shift after every `mv`/`rm`, so run
  `jzctl kanban show 'j3c bootstrap'` before each change. Tick the matching `[ ]` box below too.

1. [x] Plan 09 chunks 6-7 land (Steam out of `j3c`). A1 baseline commit + tag, A2 stage 0 + `stage0.sh`, A3 binary name + `build.sh` guard.
2. [x] A4 `diff.sh` + `corpus.txt` + double-run determinism check, A5 fmt oracle (decide on `dump-ast`).
3. [x] A7 stub refresh + `coverage.sh` + `DELETED.md`, A9 `layers.txt` + `layers.sh`.
4. [x] A8 `PORT_DEBT.md`, A10 + B7 checklists in `j3c-bootstrap/AGENTS.md`, A6 `--norm`, A11 first scale
   numbers, A12 seed note in the README, B1-B12 written into the stub headers they affect (`driver.j3` new,
   `ir/passes.j3` new, `tests/cases/` layout).
5. [ ] C1 onward: one chunk list per stage, written into this plan (or a plan 12) when C0 is done.

## Waves (Part A run, `.swarm-opus/11-pre-bootstrap-a/`)

A1 (commit + tag `bootstrap-base`) is the person's step; everything else runs without it, and stage 0
is rebuilt from the tag once it exists.

- Wave 0 (contracts, 1 agent): A3 `[package] bin` key in Rust + `j3c-boot` name + `build.sh` guard; A2 `stage0/`,
  `tools/stage0.sh`, first `stage0/j3c`; `tools/coverage.sh` + `DELETED.md` skeleton; `tools/layers.txt`;
  delete the steam and `backend/c` stubs.
  - [x] w0 (`bin` key + test, stage0 via `stage0.sh --worktree`, coverage.sh 28 left + stale check, layers.txt with toolchain+cache as one stage, backend/c stubs deleted)
- Wave 1 (5 parallel):
  - [x] 1 A5 `j3c dump-ast` in Rust (`j3c/src/**`)
  - [x] 2 A4 `diff.sh` + `corpus.txt` + `known/` + determinism, A6 `--norm`, A9 `layers.sh`, A11 `SCALE.md`
  - [x] 3 A7 + B notes: stubs `base`, `syntax`, `load`, `sema`, `project`, `ui`
  - [x] 4 A7 + B notes: stubs `lower`, `ir`, `devmode`, `shader`, `backend`, `toolchain`, `cache`, `main`, new `driver.j3`, new `ir/passes.j3`
  - [x] 5 A7 + B notes: tool stubs (`cmd`, `pkgtool`, `doc`, `lsp`, `mcp`, `scene`, `editor`, `assets`, `dist`, `textmate`); A8 `PORT_DEBT.md`, A10 + B7 `AGENTS.md`, A12 README note
- [x] After wave 1: rebuilt `stage0/j3c` (`--worktree`, dirty), gate PASS; `known/{fmt,ast,err,ir,llvm}.txt` seeded.

## Waves (Part B run, `.swarm-opus/11-pre-bootstrap-b/`): port-independent skeletons

Real, compiling, unit-tested J3 for the B pieces that need no ported stage. Anything needing the parser,
sema, lowering or Rust-exact rendering stays a stub for Part C.

- Wave 0 (contracts): unit-test pattern + `tests/unit.sh`, reachability from `main.j3`, signatures for
  `base/{diag,ids,span,intern,phase}`, `driver`, `ir/passes`, `backend`, `cmd/testcompiler`, tool table,
  JSON-RPC loop; `text/errors.txt` + `text/syntax/keywords.txt` formats.
  - [x] w0 (unit tests run as throwaway packages under target/unit/; main.j3 `use`s modules for reachability; @embed paths file-relative; `Spans<T>`)
- Wave 1 (4 parallel):
  - [x] 1 B3/B4/B10 `base` (diag list + code table, ids/span side table, intern, phase) + the two data files filled
  - [x] 2 B1/B6/B8/B8a `driver` Session/Options/run, `ir/passes` table, `backend` table
  - [x] 3 B9 `test-compiler` runner (against stage0 now) + first `tests/cases/`
  - [x] 4 B12.3/B12.4 tool table + generators, JSON-RPC loop

## Waves (Part C run, `.swarm-opus/11-port/`)

### Milestone C1: `base` (what load needs), `syntax` with `fmt` and `dump-ast`, `load` — DONE (fmt/ast 508/508, parse errors clean, scale in budget, review fixes applied)
- Wave 0 (contracts): complete AST + token types, Parser struct/cursor/errors, cross-part parser signatures,
  lex/fmt/dump/walk/toml/fsx entry signatures, `fmt`/`dump-ast`/`check` command wiring.
  - [x] w0 (one Ast per Session, Expr/Stmt/Type/Pat in Pools + Spans; payload structs per variant (stage0 limit); parse core real; fmt/dump-ast/check wired)
- [x] Wave 1 (3 parallel, isolated worktrees; `diff.sh fmt` 500/500 ok, known/fmt.txt empty): lex + fmt (gate: `diff.sh fmt` clean); dump + walk (unit tests
  on hand-built ASTs vs stage0 `dump-ast`); base toml + fsx + syntax flow + extend.
- [x] Wave 2 (isolated; `diff.sh ast` 503/503, known/ast.txt empty): the whole parser (one chunk: the halves can't be tested apart; gate `diff.sh ast`),
  load core, load scripts + scene scripts.
- [x] Wave 3 (shared worktree): load core, load scripts + scene scripts, driver/check, dump depth fix, A11 scale check (within budget). `diff.sh err`: parse errors clean.
  (Chunk lists written when wave 1 is done.)

### Milestone C2a: plain code end to end (sema redesign start, lower, ir, LLVM, minimal toolchain) — DONE (gate rescoped: user-module ir parity 8/8; llvm samples exact; run ok stdlib-free; review fixes applied)
Corpus: `.swarm-opus/11-port/c2a-corpus.txt`. Even `printLn` pulls in generic stdlib declarations, so sema
resolves generic declarations from the start and leaves bodies uninstantiated unless demanded.
- [x] Wave 4 (contracts, 2 isolated): IR data model ported as is (+ cx tables, abi); `Ty` + typed tree + all stage signatures + command wiring.
- [x] Wave 5: sema declarations; sema bodies (typed tree); IR dump + validate; toolchain + run/build/dump commands.
- [x] Wave 6 (shared worktree): `lower`; LLVM backend. Gate: `ir --norm`, `llvm`, `run` exact on the C2a corpus.

### Milestone C2b: generics, traits/bounds/operators, patterns/match — DONE (gate rescoped: entry-module ir 13/13, run 13/13, check 13/13; full-file ir/llvm parity moved to C2c; review fixes applied)
Gate: full-file `ir --norm`, `llvm`, `run` exact on `.swarm-opus/11-port/c2b-corpus.txt`; C1/C2a gates green.
- [x] Wave 7 (contracts, 1 agent): typed-tree nodes for instances/patterns/trait calls, instantiation queue API, dispatch hooks to gen_/trait_/pat_ sema parts and inst_/pat_ lower parts, last stage0 layout pads.
- [x] Wave 8 (3 sema agents, shared worktree): generics + instantiation queue; traits/bounds/operators/UFCS; patterns/match/`?`.
- [x] Wave 9 (shared worktree): lower instances + trait calls; lower patterns/match; integration. Gate rescoped: entry-module ir, run and check exact on the C2b corpus; full-file ir/llvm parity moves to C2c (needs tuples, closures, frame strings, simd in the stdlib).

### Milestone C2c: tuples, closures, frame strings, simd/f16, dyn, derive, lint — DONE (full-file ir/llvm/run/check 25/25; unit 64; err 26/0; review fixes applied)
Gate: full-file `ir --norm`, `llvm`, `run` exact and `check` same on the C2b and C2c corpus files; `Typed.untyped` empty; staging deleted.
- [x] Wave 10 (contracts, 1 agent): hooks from the shared sema/lower files into per-feature part files, IR table stubs, order rules, `contracts-c2c.md`.
- [x] Wave 11 (5 agents, shared worktree, one feature each end to end): tuples + lint; closures; simd/f16/intrinsics; frame strings/Text.fmt; dyn + derive.
- [x] Wave 12 (integration): full-file parity, delete the untyped-fn staging, C2b review leftovers, shrink known lists.

### Milestone C2d: gpu/shader + `dump-glsl`, script AOT, `@uiChoice`, `--fast` — DONE (glsl 4/4 + es; c2d ir/llvm 23/23; known ir/llvm 142 → 62 `.j3ui` projects; self-compile IR exact; review fixes applied)
Gate: `diff.sh glsl` exact; full-file `ir --norm`, `llvm` exact on the C2d corpus; C2c gate stays green; `notYet` deleted. Survey: `.swarm-opus/11-port/survey-c2d.md`. (`.j3ui` is C2e; the script VM stays in C4.)
- [x] Wave 13 (contracts, 1 agent): layer move, prepare call sites, hooks for the 4 `notYet` sites, shader shared types, `dump-glsl`/`dump-ui` wiring, `diff.sh` `glsl`/`ui` modes, float-const fix, `contracts-c2d.md`, `c2d-corpus.txt`.
- [x] Wave 14 (4 agents, shared worktree): shader types/expressions; shader generator + `dump-glsl`; scripts AOT + `@uiChoice`; `--fast` + driver.
- [x] Wave 15 (integration): full-file parity on the C2d corpus and the other non-`.j3ui` entries, shrink known lists, delete `notYet`.

### Milestone C2e: `.j3ui` (markup parser, gen, span map, `dump-ui`) — DONE (dump-ui 62/62 + 159 error cases; ir/llvm exact on all UI projects; every known list empty; review clean)
Gate: `diff.sh ui` exact on the UI projects and `tests/bad-ui`, then full-file `ir --norm`, `llvm`. Survey: `.swarm-opus/11-port/survey-c2e.md`.
- [x] Wave 16 (contracts, 1 agent): `ui/map`, `ui/markup/types` in full, `G` and ~75 stub signatures, `loadUi`/remap/locator/`dump-ui` wiring, `contracts-c2e.md`, `c2e-corpus.txt`.
- [x] Wave 17 (4 agents, shared worktree): markup front end; generator core; tree walk (elements, control, components, slots); attributes + `ui.j3` surface + error cases.
- [x] Wave 18 (integration): `diff.sh ui` by tier, then ir/llvm on the 62 projects, span-map error cases.

### Milestone C3: toolchain rest, object cache, `build`, `run`, self-build — DONE (whole-corpus run 220/220; diff.sh self 229/0; llvm j3c-bootstrap exact; cache 0-clang rebuild; review 0 must-fix, 11 should-fix fixed; run.txt nondet entry gone)
Gate: whole-corpus `diff.sh run` clean (leftovers in `known/run.txt` with causes), `diff.sh self` (stage 2 = stage 1). Survey: `.swarm-opus/11-port/survey-c3.md`.
- [x] Wave 19 (contracts, 1 agent): cache API, driver cache-or-one-clang choice, `cmd/stamp.j3`, command routes, `diff.sh self` skeleton, `known/run.txt`, `contracts-c3.md`, `c3-corpus.txt`.
- [x] Wave 20 (4 agents, shared worktree): object cache; toolchain rest + preflight; `build` stamps, multi-target, log, `size`; `run`, commands, driver, harness.
- [x] Wave 21 (integration): self LLVM exact, whole-corpus run by tier, `diff.sh self`.

### Milestone C4: bytecode backend, dev mode, dev runner, package commands, `--hot` — DONE (bc 19/19 byte-equal; dev path no-clang rebuild; pkg 11/11; run 229; self 238; review 1 must-fix + 11 should-fix fixed)
- [x] Wave 22 (contracts): bc op table, Bc* types, devmode/devrun/pkg stubs, `diff.sh bc`, c4 corpus.
- [x] Wave 23 (5 chunks): A bc module + encode + dump-bc; B bc codegen; C devmode pass; D dev runner + run wiring; E pkg commands, pkgtool, --hot.
- [x] Wave 24 (integration): `.j3sb` byte-equal on the corpus, dev-mode run, self.
