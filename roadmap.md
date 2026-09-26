# J3 roadmap

Non-goals (no separate spec file in this tree): No GC, no borrow checker, no traits, no drop, no capturing closures, no comptime interpreter in v1. Do not CFG-ify check. Do not start a second parser. Do not grow `j3c/src/stdlib/mod.rs`. Do not add libLLVM / crates.io.

2D (bounce, dungeon) and a GL voxel island ship on Linux. `[emit] target` is one triple or an array (`sys.j3` / `sys_windows.j3` / `sys_macos.j3`). `build` writes one binary per triple; `run` uses the host triple.

## Shipped

Language v1, typed IR, C and LLVM printers, `--fast`. Short-circuit, if-expr arms, `return`/`defer`, `+=`, interpolate, wrapping ints, string lvalues, `Arena` by value. Inclusive `..=`, default fields, `for` index ends with the loop. `@j3ui` in the parser (defense, gallery). Errors, C `#line`, LLVM `DILocation` and named locals (`DILocalVariable`, `dbg.declare`). `j3c lsp` hover, goto, scoped completion, signature help, outline, import quick-fix, format; diagnostics follow the error into another open buffer. `j3.toml` keys complete in the editor. `j3c fmt`, package-rooted assets.

`App` stages (`pause`, `timeScale`, `frameAlpha`), ECS (`join`, `Signal`), collide, `j3.game.space`, `j3.game.physics`, `j3.game.path`, `j3.game.anim`, `j3.game.input`, assets, `# j3tile 1`, UI (`field`, keyboard focus, `drag`, world labels), save (int, float, string, hex blob), particles, camera `Rig`. `j3.game.physics` is intentionally lightweight: 2D AABB separation and a 3D voxel-stopping body, not a contact solver. `j3.gfx` GL 3.3 mesh + one shadow map, up to 4 point lights, `drawAlpha`, `drawInstanced`, `drawLine`, `Orbit` and first-person `Look`. `j3.gfx.voxel` is a solid grid with a ray and an AABB test. `Vec3` / `Quat` / `Mat4` (translate, rotate, scale), `# j3mesh 1`. Island draws a second mesh (a bobbing lantern). `examples/minecraft` walks that grid: break, place, jump, optional corner shadows (F3). SDL_mixer supports sfx and voice groups, group volume, channel and music fades, and positional channels. `Music` loops via `playMusic` / `stopMusic`; `play` stays one-shot and `playLoops` takes the repeat count. Channel and music volume are 0..128. Dungeon loops `theme.wav`. Title continues the saved floor; a new run starts at 0. Death still retries the current floor in memory.

## Left

- **Scale** — one translation unit per `cc`, and mono re-walks the AST, when dungeon plus monomorphization hurts. Not before.
- **Debug** — gdb/lldb on the named locals. Lines and `DILocalVariable` are in the LLVM printer.
- **Tooling** — rename.
- **Packages** — `j3.toml` deps. Language-level tests stay `cargo test` plus a playable example until then.
- **Ship a Windows binary** — mingw `clang` and the SDL import libs. A missing lib stays a link error. Distribution is one binary plus an assets folder.
- **3D game bar** — a small 3D or 2.5D game with art, audio, save, and two desktops. A mixer graph is future work. Editor and hot reload wait until scenes are data.

Later, not this list: Wasm, consoles, threads, a J3 shader language, SIMD.

## Next

Nothing in the examples is waiting on a language change. Hold these until something forces them:

- **One translation unit.** Still one `.ll` and one `clang`, and generics re-lower into one IR context. Measure dungeon check vs emit vs clang first. Stop if check is cheap. Do not CFG-ify check to get there.
- **`j3.toml` deps, LSP rename, Wasm, threads, shaders.** Nothing in the examples is blocked on them.
- **Pack / install.** Dungeon rpath is `$ORIGIN/../lib`, and assets try three roots (`assets/`, `examples/dungeon/assets/`, `../examples/dungeon/assets/`). Matters when there is a build to hand someone.

## Language features

Friction, not blockers. The games compile without any of this. Spec still forbids traits, capturing closures, a borrow checker, drop, user attributes, operand swap, and a comptime interpreter. Literal context lands before a sweep that deletes `f32(2)`.

Shipped since this list was written: inclusive `for` (`..=`, test `inclusive_for_runs`), default field values (test `default_fields_fill_omitted`; stdlib structs may still omit defaults), and `for` index scope ending with the loop (test `for_index_ends_with_the_loop`). `let i` then `for i` is still a shadowing error (`let_then_for_still_shadows`).

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
