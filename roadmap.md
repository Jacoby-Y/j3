# J3 roadmap

Non-goals (no separate spec file in this tree): No GC, no borrow checker, no traits, no drop, no capturing closures, no comptime interpreter in v1. Do not CFG-ify check. Do not start a second parser. Do not grow `j3c/src/stdlib/mod.rs`. Do not add libLLVM / crates.io.

2D (bounce, dungeon) and a GL voxel island ship on Linux. `[emit] target` is one triple or an array (`sys.j3` / `sys_windows.j3` / `sys_macos.j3`). `build` writes one binary per triple; `run` uses the host triple.

## Shipped

Language v1, typed IR, C and LLVM printers, `--fast`. Short-circuit, if-expr arms, `return`/`defer`, `+=`, interpolate, wrapping ints, string lvalues, `Arena` by value. Inclusive `..=`, default fields, `for` index ends with the loop. `@j3ui` in the parser (defense, gallery). Errors, C `#line`, LLVM `DILocation` and named locals (`DILocalVariable`, `dbg.declare`). `j3c lsp` hover, goto, scoped completion, signature help, outline, import quick-fix, format; diagnostics follow the error into another open buffer. `j3.toml` keys complete in the editor. `j3c fmt`, package-rooted assets.

`App` stages (`pause`, `timeScale`, `frameAlpha`), ECS (`join`, `Signal`), collide, `j3.game.space`, `j3.game.physics`, `j3.game.path`, `j3.game.anim`, `j3.game.input`, assets, `# j3tile 1`, UI (`field`, keyboard focus, `drag`, world labels), save (int, float, string, hex blob), particles, camera `Rig`. `j3.game.physics` is intentionally lightweight: 2D AABB separation and a 3D voxel-stopping body, not a contact solver. `j3.gfx` GL 3.3 mesh + one shadow map, up to 4 point lights, `drawAlpha`, `drawInstanced`, `drawLine`, `Orbit` and first-person `Look`. `j3.gfx.voxel` is a solid grid with a ray and an AABB test. `Vec3` / `Quat` / `Mat4` (translate, rotate, scale), `# j3mesh 1`. Island draws a second mesh (a bobbing lantern). `examples/minecraft` walks that grid: break, place, jump, optional corner shadows (F3). SDL_mixer supports sfx and voice groups, group volume, channel and music fades, and positional channels. `Music` loops via `playMusic` / `stopMusic`; `play` stays one-shot and `playLoops` takes the repeat count. Channel and music volume are 0..128. Dungeon loops `theme.wav`. Title continues the saved floor; a new run starts at 0. Death still retries the current floor in memory.

## Left

- **Scale** — one translation unit per `cc`, and mono re-walks the AST, when dungeon plus monomorphization hurts. Not before.
- **Better panics** — see the section below. Lines and `DILocalVariable` are already in the LLVM printer, so gdb/lldb work when one is installed.
- **Tooling** — rename.
- **Packages** — `j3.toml` deps. Language-level tests stay `cargo test` plus a playable example until then.
- **Ship a Windows binary** — mingw `clang` and the SDL import libs. A missing lib stays a link error. Distribution is one binary plus an assets folder.
- **3D game bar** — a small 3D or 2.5D game with art, audio, save, and two desktops. A mixer graph is future work. Editor and hot reload wait until scenes are data.

Later, not this list: Wasm, consoles, threads, a J3 shader language, SIMD.

## Next

Nothing in the examples is waiting on a language change. Hold these until something forces them:

- **One translation unit.** Still one `.ll` and one `clang`, and generics re-lower into one IR context. Measure dungeon check vs emit vs clang first. Stop if check is cheap. Do not CFG-ify check to get there.
- **Rename, `j3.toml` deps, Wasm, threads, shaders.** Nothing in the examples is blocked on them.
- **Pack / install.** Dungeon rpath is `$ORIGIN/../lib`, and assets try three roots (`assets/`, `examples/dungeon/assets/`, `../examples/dungeon/assets/`). Matters when there is a build to hand someone.

## Build speed

fps-shooter (54 modules, ~45k lines with stdlib), `J3C_TIMINGS=1` shows the stages. Shipped: `check` 0.80s -> 0.17s (import-cycle DFS walked every path of the import DAG), cold build 1.45s -> 0.75s (emit runs inside the parallel clang workers, biggest module first, `mold` when on PATH), struct edit 54 -> 3 modules recompiled (object key hashes only the layouts a module names), rebuilt/touched j3c no longer invalidates objects (key hashes the compiler's bytes). Open: `lower_module` clones per-module maps (0.06s), LSP still rechecks everything per edit, no function-level pruning.

## Better panics

Shipped (LLVM backend unless noted):

- Every compiler runtime check names itself: `file:line: index out of bounds` (also slice range, divide by zero, interpolation overflow, missing return) and exits **102**. The C backend prints the reason without `file:line`.
- `panic(msg)` prints `file:line: msg` and exits **101** (the compiler rewrites it to `panicAt`).
- `j3c run` says `program exited N (panic | runtime check failed | killed by signal S)`, and notes when stderr was empty. A signal death is `128+S`.
- Debug builds (`[emit] debug = true`, x86-64 Linux) install a crash handler for SIGSEGV/SIGABRT/SIGFPE/SIGBUS/SIGILL, with an alternate stack so stack overflow reports. It prints raw addresses; `j3c run` turns them into `function at file:line` through `nm` and `addr2line`.

Still open:

1. The `.j3s` VM's `abort` op and the C backend do not report `file:line`.
2. The trace stops at 32 frames and needs `nm` + `addr2line` installed; without them it prints raw addresses. A binary run outside `j3c run` prints the raw `j3 at:` lines.
3. Handler for aarch64, macOS and Windows.
4. The original fps-shooter silent exit is unconfirmed; it should now say which check failed.

Not in scope: exceptions, unwinding, or `defer` on panic.

## Language features

Friction, not blockers. The games compile without any of this. Spec still forbids traits, capturing closures, a borrow checker, drop, user attributes, operand swap, and a comptime interpreter. Literal context lands before a sweep that deletes `f32(2)`.

Already shipped from this list: inclusive `for`, default field values (stdlib structs may still omit defaults), and `for` index scope ending with the loop. `let i` then `for i` is still a shadowing error.

1. **Literals on the left of `+` `-` `*` `/`.** Gameplay is `f32`. A literal with no context becomes `int` or `f64`. The checker types the left operand with no expected type, then types the right from the left. `far * 2` can be `f32`. `2 * far` cannot. `math.perspective` writes `f32(2) * far * near`. `lookAt` writes `f32(0) - f.x` (nine times in `math.j3`). About 37 `f32(0) -` sites and about 70 `f32(<number>)` casts. Unary `-f.x` already typechecks. `f32(tx * tw)` stays an explicit int-to-float cast. Spike: an untyped literal on the left takes the right operand's type, or the destination type, so `m[14] = 2 * far` is `f32`. A `2f32` suffix is the alternative if the cast should stay visible. Do not change the no-context default from `f64` to `f32`.

Not a language change:

- **`Vec2.xy`.** Mirror of `Vec3.xyz`. That is the brace cleanup for draw code. Stdlib only.
- **Int `match`.** Slot ids, roam directions, and tile bytes are `if n == 1` ladders. Payloadless enums and `match` already exist. Tile bytes are a file format.
- **UFCS, `f16`, labeled break.** No cluster of call sites. One `hit` flag in the dungeon map generator is the labeled-break story.
- **Block-scoping every `let`.** Wider than the `for` pain, and it fights the no-shadow rule on purpose.

## `@j3ui`

`@j3ui` is in the parser. Defense and gallery call it. `j3c doc j3ui` is the syntax, and it is generated from the same widget names the parser accepts. `j3c doc j3.ui` lists each widget. `button` runs its body on a click. `buttonOff` draws that shape and never clicks. `panel`, `row`, `scroll`, and `card` close with `end`. `pushId` closes with `popId`. A body on any other widget is an error.


create a new plan to make a raytracing demo. the demo is simple:
- 3D scene in a white box room
- a simple white light at the top of the room
- 3 spheres: glass, metal, plastic
- glass tests raytracing light refraction
- metal tests raytracing reflections
- plastic tests simple materials, defaulting to a dull plastic
- add a toggleable HUD to change settings such as: glass/metal/plastic sphere material properties and radius, light brightness, light RGB color values, etc.

obviously, a lot of these things aren't very possible, so first give me a concise list of features that'd need to be added for this to be possible
