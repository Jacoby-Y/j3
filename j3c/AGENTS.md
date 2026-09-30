# j3c — agent map

J3 compiler crate. The contract is this file, `../roadmap.md`'s non-goals, and `../speculative/` (01–08 run, 09 typechecks, `10_j3ui.j3` typechecks). If this file and those disagree with an example, the example's test in `compile.rs` is what `cargo test` locks.

Host: Rust, no crates.io deps. **Emit is LLVM by default** (text `.ll` + host `clang`). C is optional (`--emit c` or `[emit] backend = c`). Script bytecode is a third printer over the same checked IR. The runtime bytecode VM in J3 is allowed; the comptime interpreter remains out of scope. No libLLVM, no crates.io. `--fast` strips OOB `Abort`. The IR with places is the backend contract.

## Commands

Run from this directory (`j3c/`):

```
cargo test
cargo run -- dump-ir
cargo run -- dump-ir samples
cargo run -- dump-c
cargo run -- dump-llvm
cargo run -- dump-bc ../speculative/11_script/scripts/door.j3s
cargo run -- run ../examples/scripted
cargo run -- run ../examples/fps-shooter/tools/arena.j3s
cargo run -- -e script-props scripts/door.j3s
cargo run -- dump-syntax ../editors/vscode/syntaxes/j3.tmLanguage.json
cargo run -- run examples/hello.j3
cargo run -- run --emit c examples/hello.j3
cargo run -- run --opt 2 examples/hello.j3
cargo run -- run samples
cargo run -- check ../examples/gallery
cargo run -- new /tmp/mygame
cargo run -- doc j3.app
cargo run -- doc lang
cargo run -- doc j3ui
cargo run -- guide
cargo run -- help manifest
cargo run -- run ../examples/gallery
cargo run -- build --fast ../examples/gallery
cargo run -- fmt examples/hello.j3
cargo run -- lsp
cargo run -- help
```

`run` / `build` / `check` with no path use `./j3.toml` and stop if it is missing (`j3c new`, `j3c guide`). `check` typechecks, then reports a missing host library and a missing file passed to `loadFont` / `tryLoadFont` / `loadTexture` / `tryLoadTexture` / `loadRgba` / `loadSound` / `tryLoadSound` / `loadMusic` / `tryLoadMusic` / `setVolume` / `setMusicVolume` / `stop` / `stopMusic`. It does not run the program. `run` uses that same check and does not launch when it fails. A cross-compile target skips the library probe. `dump-ir` with no path still parses `examples/hello.j3` + `stdlib/j3` and prints typed IR. `dump-ir samples` prints `ir/samples.rs` after `validate` (place picture). `dump-c` / `dump-llvm` with no path emit that hello; `run examples/hello.j3` executes it (`printLn("Hello, world!")`). `dump-syntax` writes the editor TextMate grammar from lexer/parser tables (shipped at `../editors/vscode/syntaxes/j3.tmLanguage.json`). `run samples` is the place tests. `run foo.j3` compiles a real file. `run` / `build` also take a directory with `j3.toml` (`../examples/gallery`, `../examples/fps-shooter`). A project run executes the binary with **cwd = package root** (`J3_ROOT`). `fmt` pretty-prints from the same lexer (comments stay). `lsp` is stdio JSON-RPC: hover, goto-def, scoped completion (including `use`), signature help, document symbols, import quick-fix, diagnostics, format — same parse + check, no second frontend. Diagnostics for an error in another open buffer are published on that buffer.

`/target` must stay small: no incremental, no extra crates. Host `clang -g` / `cc -g` is on unless `--fast`. Flags live in `Cargo.toml` and `.cargo/config.toml`. Do not add clap, serde, inkwell, llvm-sys, or anyhow.

## Pipeline

```
parse  →  check  →  ir  →  emit LLVM (default), C, or script bytecode
LIVE      LIVE      LIVE    LIVE
```

Hello + `01`–`08` compile and run. `09` typechecks shipped `j3.game`. `speculative/10_j3ui.j3` typechecks (`@j3ui`); defense and gallery projects typecheck. `stdlib/j3/game.j3` wraps SDL2 + image/mixer/ttf (color, src/dest draw, bakeText, gamepad, `quitRequested`, `holdClose` / `closeRequested` (window close asks instead of quitting), window min/max/resize, one-shot `play` / `playLoops` (return a channel), sfx and voice groups, fade, `setPosition`, looping `Music`, channel and music volume, `drawTextureEx`, `drawLine`, `frameAlpha`, `textInput` / `textTyped`) plus `initGl` (GL 3.3 core) / `SDL_GL_SwapWindow` when `App.useGl`; `08` runs against `game_headless.j3` (same pub names). `tryLoadFont`, `tryLoadTexture`, `tryLoadSound`, and `tryLoadMusic` return an empty handle when that shared library is missing; `loadFont`, `loadTexture`, `loadSound`, and `loadMusic` panic with the soname (`j3c run` then prints the apt package). `j3.math` is f32 trig/clamp/lerp + `Vec2` / `Vec3` / `Quat` / `Mat4` (identity/mul/translate/rotate/scale/perspective/lookAt/ortho/transformPoint). `j3.gfx.mesh` is `# j3mesh 1` (10 floats per vertex) or `# j3mesh 2` (those plus `u v`). `gfx.upload` takes either length; a length divisible by both stays 10 floats, and `uploadUv` forces the 12-float layout. `j3.gfx.voxel` is a solid grid (`0` is air): get/set, exposed-face count, DDA `ray`, AABB `overlaps`, corner occlusion (`corners`, `shade`). `gfx.emitFaceAo` bakes that shade into the vertex color and keeps the darker diagonal. `j3.gfx` is a GL 3.3 forward renderer (`Mesh`, `Orbit`, `Look`, directional shadow map, up to 4 point lights, `drawAlpha`, `drawInstanced`, `drawLine`, `use lib "GL"`). `j3.game` can capture the mouse (`captureMouse`, `mouseDelta`). `j3.rand` is a seedable LCG. `j3.game.ecs` is generational entities + `Column<T>` (`join`) + deferred `Commands` + `Events<T>` + `Signal<T>`. `j3.app` is `App<W>` (arena + window + loop; user `main` only `create`/`startup`/`first`/`add`/`last`/`always`/`draw`/`useGl`/`pause`/`timeScale`/`run`; `run` stores `frameAlpha` before draw). `j3.game.collide` is AABB / tile slide. `j3.game.space` is a hash grid. `j3.game.physics` is 2D bodies plus a 3D body that stops on voxels. `j3.game.path` is grid A*. `j3.game.anim` is sprite clips and a rigid transform track. `j3.game.input` is action bindings. `j3.scene3d.fpwalk` is a first-person walker with box colliders from scene objects; `scene3d.drawFrame` draws a scene from a `Look`. `j3.scene3d.fx` parses `.j3fx` shader effects (header, params, GL log line mapping); `scene3d` caches and compiles them (`effectSlot`, material `fx`, camera `beginEffects`/`endEffects`), and `gfx` builds them (`effectProgram`, `postProgram`, `postPass`). Any new lit uniform goes in `LitLocs`, or effect programs miss it. `j3.gfx` also has up to 4 point lights, `drawAlpha`, `drawInstanced` (64), `drawLine`, `setViewRect` (camera pass and `mouseRay` in a window sub-rect), and `toScreen` (world point to window pixel). Framework modules: `ui` `gfx`, and the `game/` (`anim` `input` `path` `physics` `space` `tile` `fx` `collide` `ecs`), `gfx/` (`mesh` `voxel` ...), and `data/` (`res` `scene` `assets` `save` `debug`) folders; a folder is part of the module name (`j3.game.ecs`). `--fast` strips `Abort` (OOB). Exit codes: `panic` 101, failed runtime check 102 (`Abort(AbortKind)` prints `file:line: reason`), signal 128+n; debug builds on x86-64 Linux add a crash handler (`emit/llvm_crash.rs`) that `emit::host::symbolize` turns into a trace. Import cycles, occur-check, `pub` exports, and non-zero singleton init are check-time. `j3.sys` is `sys.j3` on Linux, `sys_windows.j3` or `sys_macos.j3` when `[emit] target` says so.

Do not start a second parser. The LSP lives in `src/lsp/` and calls `compile::analyze_checked`. Cranelift is not a required middle step. New printers plug into `emit::Backend` and consume the same typed IR.

The **library** (`src/lib.rs`) is what the LSP depends on. The **bin** (`src/main.rs`) only calls `cli::run`.

## Crate root

Optional `j3.gfx.urp.raytrace` is a fullscreen sphere/room path tracer with a
custom-shader hook (its README has a sample). Optional
`j3.gfx.urp.forest` routes `j3.gfx` camera color and depth through an external
framebuffer, samples the same alpha-cutout canopy shadow map for volumetric
light, and composites analytic glass and metal spheres.
Both need a GL context from `App.useGl()` and release GL resources before that
context closes. The URP entry points are in `stdlib/j3/gfx/urp/README.md`.

Scene editing that only the editor does lives in `stdlib/j3/scene3d/editor/` (`j3.scene3d.editor.overrides`, `props`, `paste`, `uses`): per-copy overrides, declaring and renaming props, pasting placed objects, and the placement loop check. Games load, draw, and read props with `j3.scene3d` alone.

Procedural 2D art lives in `stdlib/j3/gfx/canvas.j3`: RGBA8 `Canvas`, pixel/smooth modes, clipped primitives, seeded noise, and PNG export. `j3.game.textureFromCanvas` uploads it for SDL or GL; `updateTextureFromCanvas` updates a rectangle. `j3c doc recipes canvas` generates assets without SDL. Keep its hot drawing loops allocation-free and clip before iterating. The scaffolded `AGENTS.md` teaches agents to use the API and compose more readable game art.

| Path | What it is |
|------|------------|
| `Cargo.toml` | Package + small-`/target` profiles. Keep `[dependencies]` empty. |
| `.cargo/config.toml` | `incremental = false`, `debuginfo=0` |
| `src/main.rs` | Bin entry. No logic. |
| `src/lib.rs` | Module list. |
| `src/cli.rs` | Argv. Commands: `new`, `check`, `doc`, `guide`, `dump-ir`, `dump-c`, `dump-llvm`, `run` (`--frames`, `--screenshot`, `--headless`, `--file`), `build`, `fmt`, `lsp`, `editor`, `scene`, `render`, `gen`, `mcp`. |
| `src/scaffold.rs` | `j3c new` / `--app`. Writes `AGENTS.md`. |
| `src/doc.rs` | `j3c doc` from the parser. |
| `src/helptext.rs` | `j3c guide` and `j3c help manifest`. |
| `src/project.rs` | `j3.toml` + `src/` layout. Tiny TOML subset; `[emit]` backend/opt/debug/fast/target/cpu/clang/flags. |
| `stdlib/j3/` | Stdlib in J3. SSOT. Shipped with the crate, not mixed into `src/`; optional OpenGL passes are in `urp/`. |
| `examples/hello.j3` | Built-in hello for `dump-*` and `j3c run examples/hello.j3`. |
| `src/compile.rs` | Load entry + stdlib modules, parse, check. `.j3s` files load through `@script` paths (`compile/script_deps.rs`) and scene attachments. |
| `src/compile/dev.rs` | Dev-mode IR pass: script structs become `{ handle: ptr }`, script literals `j3.script.devNew`, `pub` script fns `devCall` stubs, plus a thunk per native fn a script may call (called now, or `pub` in a module a script `use`s), registered from `main`. |
| `src/compile/scene_scripts.rs` | Scans `*.j3scene` for `node.<i>.script`, checks props/params/hooks against each script (errors point at the scene line), and makes `j3.scene3d.scripts` in memory (instances, `start`/`update`, `find__*` behind `scene.script::<T>(node)`). |
| `src/devrun.rs` | `j3c run` with scripts: writes `target/j3s/*.j3sb`, keys the dev binary on `.j3` sources + lowered native IR (so script edits skip clang), compiles the VM as its own `-O2` object below opt 2, and watches `.j3s` (live swap via `target/j3s/reload`, relaunch on a layout change). |
| `src/check/script.rs` | Script → struct/methods/globals lowering and script-scope rules. |
| `src/parse/` | Grammar: lex + recursive descent. Shared with `lsp` + `fmt`. `syntax.rs` is keyword/attr/intrinsic/type names + TextMate dump (`dump-syntax`). |
| `src/fmt.rs` | Token reprint; comments copied from source gaps |
| `src/lsp/` | Stdio LSP. `json.rs` is a tiny parser (no serde). Hover, goto, completion, signature, symbols, import edits |
| `src/mcp.rs` | `j3c -e mcp`: MCP server (newline JSON-RPC on stdio, reuses `lsp/json.rs`). Scene (incl. `scene_add_many` batches), place (incl. grid), undo (file backups in `scene.rs`), render, check, play, console, import, asset_search, asset_online_search, asset_fetch, gen tools; live-editor mode through `target/j3-editor/events` + `replies` |
| `src/scene.rs` | `j3c -e scene` and `j3c -e render`: runs the editor binary's headless `cli`/`render` modes (`editor/src/tool.j3`, `render.j3`) and formats replies as text or JSON |
| `src/gen.rs` | `j3c -e gen`: runs `assets/gen/*.j3` mesh scripts (`j3c run --file`), cached by script hash; also before project builds |
| `src/online.rs` | Poly Haven / ambientCG / Kenney search and fetch through `curl` (zips unpacked with `png::inflate`, `.hdr` skies tone-mapped to PNG), cache in `~/.cache/j3/assets`, `assets/CREDITS.txt`; MCP `asset_online_search`/`asset_fetch` and the editor's Get assets |
| `src/png.rs` | Recompresses canvas PNGs (filters + fixed-Huffman DEFLATE) and base64 for MCP images |
| `src/check/` | AST → typed IR. |
| `src/stdlib/mod.rs` | Embeds `stdlib/j3/*.j3` into the binary. `J3_STDLIB` overrides. |
| `src/diag.rs` | rustc-shaped errors (snippet, notes, help, suggestions) |
| `src/span.rs` | `FileId`, `Span`. Parser fills these in. |
| `src/intern.rs` | Interned `Name` strings. |
| `src/emit/mod.rs` | Printers + `run_program`. Consumes IR only. Never parse J3. |
| `src/emit/bc.rs` | Versioned `.j3sb` printer over checked script IR: typed opcodes, register 0 is the frame pointer, fused compare-branch, `abi.rs` offsets. The opcode table is shared with `stdlib/j3/script.j3` (the VM, written in J3). |
| `src/emit/opts.rs` | `Backend` + `EmitOpts` from `[emit]` |
| `src/emit/c.rs` | Modules → C text (`j3_*` names). Host `int main` calls `j3.sys.start` if present, else user `main`. |
| `src/emit/c_expr.rs` | Places, operands, rvalues, statements |
| `src/emit/c_ty.rs` | IR `Ty` → C type names / slice typedefs |
| `src/emit/llvm.rs` | Modules → LLVM IR text. At opt 0, one object per module under `target/j3c-obj`, cached by an IR hash (`src/emit/incr.rs`); opt 1 and above stay one clang invocation. `[emit] incremental = false` forces that. Host `@main` calls `j3.sys.start` if present, else user `main`. |
| `src/emit/llvm_expr.rs` | Alloca locals; clang `-O` mem2reg’s |
| `src/emit/llvm_ty.rs` | IR `Ty` → LLVM type names (`ptr`, `%j3_string`) |
| `src/emit/host.rs` | `clang` / `cc` + extras from `j3.toml` (`-l`, `-L`, files, flags). Opt 0 links cached per-module objects. Run cwd / `J3_ROOT` |

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

Source of truth: `stdlib/j3/*.j3`, compiled into the binary with `include_str`. A copied `j3c` does not need that directory. `J3_STDLIB` replaces the embedded copy. `j3c run` parses `sys` + `io` + the entry file. `j3.sys.start` calls user `main` then `exit(0)`. mmap/Arena IR is later.

Scenes: rotation in `.j3scene` files is radians, in the tools (`j3c -e scene`, MCP, inspector) degrees. Code that writes scene text directly converts.

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

C / LLVM names: structs are `j3_<module>_<name>_<id>` (that id is local to one compile). Function symbols are `j3_<module>_<name>_<sig>`, where the suffix hashes the signature and, for a method, its type arguments — not the allocation order, so an object cache can reuse them. Slices are `j3_sl_<elem>` (`[]int` → `j3_sl_i64`), `string` → `j3_string`. Store `module` on the IR entity so names stay stable if another module is in the same `IrCx`. Do **not** intern places or switch to CFG. Parser exists for the hello subset; grow the grammar there, not a second frontend.

`[emit]` in `j3.toml`: `backend` (`llvm` default, or `c`), `opt` (`0`/`1`/`2`/`3`/`s`/`z` — `0` compile speed, `2` runtime), `incremental` (default true: opt 0 keeps one object per module), `debug`, `fast`, `target` (one triple or an array; `build` emits each, `run` uses the host triple), `cpu`, `clang`, `flags`. No `[emit]` inherits `[cc] opt`. CLI `--emit` / `--opt` override. If clang is missing and LLVM was not forced, fall back to C.

## Style

- Small files. No macros. Explicit matches. Named fields.
- Verbose is fine if a human can tweak it.
- Comments name the rule. Do not cite speculation.md.
- Do not introduce `Option<Ty>` “until check fills it in.” Check produces IR; IR is already typed.

## Do not build

Borrow checker, drop/RAII, trait solver, comptime interpreter, capturing closures, GC, user-defined `#[attributes]`, operand swap, Cranelift, libLLVM/inkwell.

Unknown `#[foo]` / `@foo()` will be compile errors in the parser. `@ident(...)` is only compiler **calls** (`@syscall`). Attributes are `#[attr]` on the next item.

## Language examples

Acceptance programs: `../speculative/01`–`08` compile and run (`08` via headless `j3.game`). `09` typechecks shipped SDL `j3.game`. `10_j3ui.j3` typechecks (`@j3ui`); defense and gallery projects typecheck. `06` uses `j3.option` / `j3.result`.
