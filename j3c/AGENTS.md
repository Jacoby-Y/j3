# j3c — agent map

J3 compiler crate. **Language law is `../speculation.md`.** If this crate and that doc disagree, the doc wins — then fix the crate (or the examples in `../speculative/`).

Host: Rust, no crates.io deps. **Emit is LLVM by default** (text `.ll` + host `clang`). C is optional (`--emit c` or `[emit] backend = c`). No libLLVM, no crates.io. `--fast` strips OOB `Abort`. The IR with places is the backend contract; C and LLVM are printers.

## Commands

Run from this directory (`j3c/`):

```
cargo test
cargo run -- dump-ir
cargo run -- dump-ir samples
cargo run -- dump-c
cargo run -- dump-llvm
cargo run -- dump-syntax ../editors/vscode/syntaxes/j3.tmLanguage.json
cargo run -- run examples/hello.j3
cargo run -- run --emit c examples/hello.j3
cargo run -- run --opt 2 examples/hello.j3
cargo run -- run samples
cargo run -- check ../examples/bounce
cargo run -- new /tmp/mygame
cargo run -- doc j3.app
cargo run -- guide
cargo run -- help manifest
cargo run -- run ../examples/bounce
cargo run -- run ../examples/dungeon
cargo run -- run ../examples/island
cargo run -- build --fast ../examples/bounce
cargo run -- fmt examples/hello.j3
cargo run -- lsp
cargo run -- help
```

`run` / `build` / `check` with no path use `./j3.toml` and stop if it is missing (`j3c new`, `j3c guide`). `dump-ir` with no path still parses `examples/hello.j3` + `stdlib/j3` and prints typed IR. `dump-ir samples` prints `ir/samples.rs` after `validate` (place picture). `dump-c` / `dump-llvm` with no path emit that hello; `run examples/hello.j3` executes it (`printLn("Hello, world!")`). `dump-syntax` writes the editor TextMate grammar from lexer/parser tables (shipped at `../editors/vscode/syntaxes/j3.tmLanguage.json`). `run samples` is the place tests. `run foo.j3` compiles a real file. `run` / `build` also take a directory with `j3.toml` (`../examples/bounce`, `../examples/dungeon`, `../examples/island`). A project run executes the binary with **cwd = package root** (`J3_ROOT`). `fmt` pretty-prints from the same lexer (comments stay). `lsp` is stdio JSON-RPC: hover, goto-def, scoped completion (including `use`), signature help, document symbols, import quick-fix, diagnostics, format — same parse + check, no second frontend. Diagnostics for an error in another open buffer are published on that buffer.

`/target` must stay small: no incremental, no extra crates. Host `clang -g` / `cc -g` is on unless `--fast`. Flags live in `Cargo.toml` and `.cargo/config.toml`. Do not add clap, serde, inkwell, llvm-sys, or anyhow.

## Pipeline

```
parse  →  check  →  ir  →  emit LLVM (default) or C
LIVE      LIVE      LIVE    LIVE
```

Hello + `01`–`08` compile and run. `09` typechecks shipped `j3.game`. `stdlib/j3/game.j3` wraps SDL2 + image/mixer/ttf (color, src/dest draw, bakeText, gamepad, `quitRequested`, window min/max/resize, one-shot `play` / `playLoops`, looping `Music`, channel and music volume) plus `initGl` (GL 3.3 core) / `SDL_GL_SwapWindow` when `App.useGl`; `08` runs against `game_headless.j3` (same pub names). `j3.math` is f32 trig/clamp/lerp + `Vec2` / `Vec3` / `Quat` / `Mat4` (identity/mul/translate/rotate/scale/perspective/lookAt/ortho/transformPoint). `j3.mesh` is `# j3mesh 1` (10 floats per vertex) or `# j3mesh 2` (those plus `u v`). `gfx.upload` takes either length; a length divisible by both stays 10 floats, and `uploadUv` forces the 12-float layout. `j3.voxel` is a solid grid (`0` is air): get/set, exposed-face count, DDA `ray`, AABB `overlaps`, corner occlusion (`corners`, `shade`). `gfx.emitFaceAo` bakes that shade into the vertex color and keeps the darker diagonal. `j3.gfx` is a GL 3.3 forward renderer (`Mesh`, `Orbit`, `Look`, directional shadow map, `use lib "GL"`). `j3.game` can capture the mouse (`captureMouse`, `mouseDelta`). `j3.rand` is a seedable LCG. `j3.ecs` is generational entities + `Column<T>` + deferred `Commands` + `Events<T>`. `j3.app` is `App<W>` (arena + window + loop; user `main` only `create`/`startup`/`first`/`add`/`last`/`always`/`draw`/`useGl`/`run`). `j3.collide` is AABB / tile slide. Framework modules: `res` `scene` `assets` `tile` `ui` `save` `fx` `debug` `gfx` `mesh`. `--fast` strips `Abort` (OOB). Import cycles, occur-check, `pub` exports, and non-zero singleton init are check-time. `j3.sys` is `sys.j3` on Linux, `sys_windows.j3` or `sys_macos.j3` when `[emit] target` says so.

Do not start a second parser. The LSP lives in `src/lsp/` and calls `compile::analyze_checked`. Cranelift is not a required middle step. New printers plug into `emit::Backend` and consume the same typed IR.

The **library** (`src/lib.rs`) is what the LSP depends on. The **bin** (`src/main.rs`) only calls `cli::run`.

## Crate root

| Path | What it is |
|------|------------|
| `Cargo.toml` | Package + small-`/target` profiles. Keep `[dependencies]` empty. |
| `.cargo/config.toml` | `incremental = false`, `debuginfo=0` |
| `src/main.rs` | Bin entry. No logic. |
| `src/lib.rs` | Module list. |
| `src/cli.rs` | Argv. Commands: `new`, `check`, `doc`, `guide`, `dump-ir`, `dump-c`, `dump-llvm`, `run`, `build`, `fmt`, `lsp`. |
| `src/scaffold.rs` | `j3c new` / `--app`. Writes `AGENTS.md`. |
| `src/doc.rs` | `j3c doc` from the parser. |
| `src/helptext.rs` | `j3c guide` and `j3c help manifest`. |
| `src/project.rs` | `j3.toml` + `src/` layout. Tiny TOML subset; `[emit]` backend/opt/debug/fast/target/cpu/clang/flags. |
| `stdlib/j3/` | Stdlib in J3. SSOT. Shipped with the crate, not mixed into `src/`. |
| `examples/hello.j3` | Built-in hello for `dump-*` and `j3c run examples/hello.j3`. |
| `src/compile.rs` | Load entry + stdlib modules, parse, check. |
| `src/parse/` | Grammar: lex + recursive descent. Shared with `lsp` + `fmt`. `syntax.rs` is keyword/attr/intrinsic/type names + TextMate dump (`dump-syntax`). |
| `src/fmt.rs` | Token reprint; comments copied from source gaps |
| `src/lsp/` | Stdio LSP. `json.rs` is a tiny parser (no serde). Hover, goto, completion, signature, symbols, import edits |
| `src/check/` | AST → typed IR. |
| `src/stdlib/mod.rs` | Old IR bootstrap of sys/io/hello. Do not grow; parse is the path. |
| `src/diag.rs` | rustc-shaped errors (snippet, notes, help, suggestions) |
| `src/span.rs` | `FileId`, `Span`. Parser fills these in. |
| `src/intern.rs` | Interned `Name` strings. |
| `src/emit/mod.rs` | Printers + `run_program`. Consumes IR only. Never parse J3. |
| `src/emit/opts.rs` | `Backend` + `EmitOpts` from `[emit]` |
| `src/emit/c.rs` | Modules → C text (`j3_*` names). Host `int main` calls `j3.sys.start` if present, else user `main`. |
| `src/emit/c_expr.rs` | Places, operands, rvalues, statements |
| `src/emit/c_ty.rs` | IR `Ty` → C type names / slice typedefs |
| `src/emit/llvm.rs` | Modules → LLVM IR text. Host `@main` calls `j3.sys.start` if present, else user `main`. |
| `src/emit/llvm_expr.rs` | Alloca locals; clang `-O` mem2reg’s |
| `src/emit/llvm_ty.rs` | IR `Ty` → LLVM type names (`ptr`, `%j3_string`) |
| `src/emit/host.rs` | `clang` / `cc` + extras from `j3.toml` (`-l`, `-L`, files, flags). Run cwd / `J3_ROOT` |

## IR (`src/ir/`)

Every expression, statement, and binding has a concrete `Ty`. There is no `Option<Ty>`. If a pass cannot name a type, it is a compiler bug.

**Places are required now**, not in a later lowering. Field/index of `&T` name a slot (no memcpy). Auto-deref is `Proj::Deref`. `+=` is `StmtKind::AssignOp` and binds the place once. Local `use` aliases are the same `local + proj` as the field they name.

| File | Contents |
|------|----------|
| `mod.rs` | Crate docs + re-exports |
| `ids.rs` | `LocalId`, `StructId`, `FieldId`, `EnumId`, `VariantId`, `FuncId`, `GlobalId` |
| `ty.rs` | `Ty` (interned id), `TyKind`, `IntTy`, `FloatTy`. `int` = `i64`, `float` = `f64` (same interned `Ty`; dumps print the alias) |
| `abi.rs` | Size/align + SysV LP64 field offsets (`@sizeof`, bump). Slice/string `{ptr,len}` |
| `cx.rs` | `IrCx`: intern names/types, store structs/enums/funcs/globals, `common` builtin tys |
| `place.rs` | `Place { ty, access, root, proj }`, `PlaceRoot::{Local,Global}`, `Proj::{Deref,Field,Index,Ptr,Len}`, `Access` |
| `operand.rs` | `Operand::Copy(Place)` (memcpy/load) or `Const` (`Bytes` for `string` literals). Copying `&T` is a pointer copy (fine). Copying `Arena` by value is the header-alias bug |
| `rvalue.rs` | Produces a value: `Use`, `Ref`, `Binary`, `Call`, `Syscall`, `Aggregate`, `Cast` |
| `stmt.rs` | Always type `void`. `Assign`, `AssignOp`, `Defer`, `If`, `While` (cond is a value; source `while` re-tests in the body), `Return`, `Expr`, `Abort` (OOB/`/0`; `--fast` strips), `Block`, `InterpFill` |
| `wrap.rs` | Overflow wraps; `/0` is `Abort` (never panic the host). Shared by fold + printers |
| `fast.rs` | `--fast` pass: drop `Abort` and abort-only `If` |
| `op.rs` | `BinOp` / `UnaryOp`. Primitive math is compiler-known; user `operator+` is a **call** |
| `body.rs` | `Body`, `LocalDecl`, `Block`, `LocalKind::{Param,Let,Const,Tmp}` |
| `entity.rs` | `StructDef`, `EnumDef`, `Func`, `TryCatch`, `Vis`, `FuncKind::{J3,Lib,Decl}` |
| `module.rs` | Module name + lists of ids. Bodies live on `Func` in the cx |
| `build.rs` | Verbose `BodyBuilder` for hand-written IR (`deref`, `field`, `index`, `slice_ptr`, `slice_len`, `ref_tmp`, `assign_op`) |
| `dump.rs` | Text dump. No spans (still dummy) |
| `validate.rs` | Place projections match types; assign/`&` honor `Access`; calls match fn sigs; break/continue in a loop |
| `samples.rs` | Hand-built module locking places. Tests live here |
| `goldens/` | `dump_module` snapshots for place samples + `01`–`08` user modules |

### Place samples (what `dump-ir samples` must keep showing)

- `scale`: `(*this).x *= copy s` — auto-deref preserves a place
- `bump`: `xs[copy i] += 1` — index bound once
- `tick`: `arena_ref = &(*(*g).mem)` — `&` of the Arena **header**, not a copy of it; `(*g).running = false`

If a change makes those strings go away, the IR is wrong.

`run samples` also executes `main` in that module: scale a `Vec2`, bump a slice, `tick` must clear `g.running`. Failures `_exit(1)` via `@syscall(60, …)`.

## Stdlib

Source of truth: `stdlib/j3/*.j3`. `j3c run` parses `sys` + `io` + the entry file. `j3.sys.start` calls user `main` then `exit(0)`. mmap/Arena IR is later. `src/stdlib/mod.rs` is leftover hand-built IR — do not add to it.

## How data is owned

```
IrCx
  names, tys, structs, enums, funcs
Module
  name, ids into the cx
Func.kind = J3 { body }
  locals, params, block of Stmt
Place
  LocalId + Vec<Proj>  (not a nested tree of places)
```

Need a new interned type? `cx.intern_ty(TyKind::…)`. Need a builtin? add it in `IrCx::new` / `CommonTys`. Need a new statement? `stmt.rs` + dump arm + validate arm + a sample if it touches places.

C / LLVM names: structs/funcs are `j3_<module>_<name>` (funcs also `_<id>`). Slices are `j3_sl_<elem>` (`[]int` → `j3_sl_i64`), `string` → `j3_string`. Store `module` on the IR entity so names stay stable if another module is in the same `IrCx`. Do **not** intern places or switch to CFG. Parser exists for the hello subset; grow the grammar there, not a second frontend.

`[emit]` in `j3.toml`: `backend` (`llvm` default, or `c`), `opt` (`0`/`1`/`2`/`3`/`s`/`z` — `0` compile speed, `2` runtime), `debug`, `fast`, `target` (one triple or an array; `build` emits each, `run` uses the host triple), `cpu`, `clang`, `flags`. No `[emit]` inherits `[cc] opt`. CLI `--emit` / `--opt` override. If clang is missing and LLVM was not forced, fall back to C.

## Style

- Small files. No macros. Explicit matches. Named fields.
- Verbose is fine if a human can tweak it.
- Comments point at `speculation.md` sections, not at novels.
- Do not introduce `Option<Ty>` “until check fills it in.” Check produces IR; IR is already typed.

## Do not build

Borrow checker, drop/RAII, trait solver, comptime interpreter, capturing closures, GC, user-defined `#[attributes]`, operand swap, Cranelift, libLLVM/inkwell.

Unknown `#[foo]` / `@foo()` will be compile errors in the parser. `@ident(...)` is only compiler **calls** (`@syscall`). Attributes are `#[attr]` on the next item.

## Language examples

Acceptance programs: `../speculative/01`–`08` compile and run (`08` via headless `j3.game`). `09` typechecks shipped SDL `j3.game`. `06` uses `j3.option` / `j3.result`.
