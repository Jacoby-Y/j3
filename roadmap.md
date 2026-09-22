# J3 roadmap

Spec (`speculation.md`) wins. No GC, no borrow checker, no traits, no drop, no capturing closures, no comptime interpreter in v1. Do not CFG-ify check. Do not start a second parser. Do not grow `j3c/src/stdlib/mod.rs`. Do not add libLLVM / crates.io.

2D (bounce, dungeon) and a GL voxel island ship on Linux. `[emit] target` is one triple or an array (`sys.j3` / `sys_windows.j3` / `sys_macos.j3`). `build` writes one binary per triple; `run` uses the host triple.

## Shipped

Language v1, typed IR, C and LLVM printers, `--fast`. Short-circuit, if-expr arms, `return`/`defer`, `+=`, interpolate, wrapping ints, string lvalues, `Arena` by value. Errors, C `#line`, LLVM `DILocation` and named locals (`DILocalVariable`, `dbg.declare`). `j3c lsp` hover, goto, scoped completion, signature help, outline, import quick-fix, format; diagnostics follow the error into another open buffer. `j3.toml` keys complete in the editor. `j3c fmt`, package-rooted assets.

`App` stages, ECS, collide, assets, `# j3tile 1`, UI, save, particles. `j3.gfx` GL 3.3 mesh + one shadow map, `Orbit` and first-person `Look`. `j3.voxel` is a solid grid with a ray and an AABB test. `Vec3` / `Quat` / `Mat4` (translate, rotate, scale), `# j3mesh 1`. Island draws a second mesh (a bobbing lantern). `examples/minecraft` walks that grid: break, place, jump, optional corner shadows (F3). `Music` loops via `playMusic` / `stopMusic`; `play` stays one-shot and `playLoops` takes the repeat count. Channel and music volume are 0..128. Dungeon loops `theme.wav`. Title continues the saved floor; a new run starts at 0. Death still retries the current floor in memory.

## Left

- **Scale** — one translation unit per `cc`, and mono re-walks the AST, when dungeon plus monomorphization hurts. Not before.
- **Debug** — gdb/lldb on the named locals. Lines and `DILocalVariable` are in the LLVM printer.
- **Tooling** — rename.
- **Packages** — `j3.toml` deps. Language-level tests stay `cargo test` plus a playable example until then.
- **Ship a Windows binary** — mingw `clang` and the SDL import libs. A missing lib stays a link error. Distribution is one binary plus an assets folder.
- **3D game** — audio buses still later. Physics is 2D, then 3D, a J3 module over `use lib`. Editor and hot reload only after scenes are data. Bar: a small 3D or 2.5D game with art, audio, save, and two desktops.

Later, not this list: Wasm, consoles, threads, a J3 shader language, SIMD.

## Next

Nothing in the examples is waiting on a language change. Hold these until something forces them:

- **One translation unit.** Still one `.ll` and one `clang`, and generics re-lower into one IR context. Measure dungeon check vs emit vs clang first. Stop if check is cheap. Do not CFG-ify check to get there.
- **Physics.** `j3.collide` overlap and tile slide cover dungeon. A `j3.physics` module waits until a 2D game outgrows that.
- **`j3.toml` deps, LSP rename, Wasm, threads, shaders.** Nothing in the examples is blocked on them.
- **Draw interpolation.** `App.run` keeps leftover `acc` and draw callbacks take `&W` only. Every `draw` signature would grow an alpha. Do it when something on screen stutters.
- **Pack / install.** Dungeon rpath is `$ORIGIN/../lib`, and assets try three roots (`assets/`, `examples/dungeon/assets/`, `../examples/dungeon/assets/`). Matters when there is a build to hand someone.

## Language features

Friction, not blockers. The games compile without any of this. Spec still forbids traits, capturing closures, a borrow checker, drop, user attributes, operand swap, and a comptime interpreter. First slice: left-hand literals, `..=`, default fields. Inclusive ranges do not depend on the others. Defaults land before anyone rewrites `Hit` and `Texture` by hand. Literal context lands before a sweep that deletes `f32(2)`.

1. **Literals on the left of `+` `-` `*` `/`.** Gameplay is `f32`. A literal with no context becomes `int` or `f64`. The checker types the left operand with no expected type, then types the right from the left. `far * 2` can be `f32`. `2 * far` cannot. `math.perspective` writes `f32(2) * far * near`. `lookAt` writes `f32(0) - f.x` (nine times in `math.j3`). About 37 `f32(0) -` sites and about 70 `f32(<number>)` casts. Unary `-f.x` already typechecks. `f32(tx * tw)` stays an explicit int-to-float cast. Spike: an untyped literal on the left takes the right operand's type, or the destination type, so `m[14] = 2 * far` is `f32`. A `2f32` suffix is the alternative if the cast should stay visible. Do not change the no-context default from `f64` to `f32`.
2. **Inclusive `for`.** `for i in 0..n` is exclusive. Column fills and ray bounds are `while y <= h` (`examples/minecraft`, `j3.voxel`, `examples/island`). About 23 `while … <=` walks. Nested `x` / `y` / `z` can already be three `for`s with different names. `for y in 0..=h` desugars to the existing range IR. `0..(h + 1)` is the off-by-one people write today.
3. **Default field values.** No defaults and no positional literals in v1. A miss is `Hit { ok: false, x: 0, y: 0, z: 0, px: 0, py: 0, pz: 0, face: 0 }`. `Texture { raw: 0, w: 0, h: 0 }` and `Mesh { vao: 0, vbo: 0, count: 0 }` repeat. `Slot.none` takes a dummy `T`. Call sites are only `Slot<int>.none(0)` in dungeon and bounce. A field written `= 0` or `= false` may be omitted. A field without a default is still an error if omitted. No traits. Positional `Vec2 { 16, 16 }` couples every literal to field order. `{ x, y }` shorthand does not clean `Vec2 { x: 16, y: 16 }` in `examples/dungeon/src/draw.j3`, because those values are not named `x` and `y`.
4. **`for` index ends with the loop.** Shadowing a name in the same function is a compile error. `for` inserts the index and never removes it. `assets.texture` scans with `i`, then `i = 0` and copies the cache. Minecraft `boot` fills `cells` with `i`, then reuses `i` for the hand meshes. A second `for i` in that function is illegal. `List.push` uses `for` because it has one loop. Many other `let i = 0; while` loops are the only loop in their function and can be `for` today. Drop the `for` name at the end of the loop. `let i` then `for i` stays an error.

Not a language change:

- **`Vec2.xy`.** Mirror of `Vec3.xyz`. That is the brace cleanup for draw code. Stdlib only.
- **Int `match`.** Slot ids, roam directions, and tile bytes are `if n == 1` ladders. Payloadless enums and `match` already exist. Tile bytes are a file format.
- **UFCS, `f16`, labeled break.** No cluster of call sites. One `hit` flag in the dungeon map generator is the labeled-break story.
- **Block-scoping every `let`.** Wider than the `for` pain, and it fights the no-shadow rule on purpose.

## `@jui`

Held. `Ui` already has `panel` / `row` / `end`, `pushId` / `popId`, `label`, and `button`. This is one intrinsic in the existing parser, same class as `@sizeof`. Not a `.jui` file, not an `@` defined in stdlib, not a comptime interpreter. The block sits in the function that already has `w`. Desugar runs before check, and errors point at the tag.

Wait until `button` can refuse a click and a list can scroll. Those land on `Ui` first. The braces desugar to them.

`@j3ui name(params) { }` is a function. The body is the call sugar: a bare method name gets the receiver, `panel` and `row` close with `end`, `pushId` with `popId`, `button` is `if ui.button`. A `{ }` on any other call is an error. `for` and `if` stay J3. Call it like a function, once per row, with different arguments. A component inside another is that call. The first parameter is the `Ui` from the caller. Nothing in the body sees a local from outside. A literal `@j3ui(ui: &Ui, ...) { }` is the same function written as a value, still parameters only. There is no capturing closure.

```j3
fn main() {
    let app = App<World>.create("shop", 1280, 720, boot);
    app.draw(draw);
    app.run();
}

@j3ui shopRow(ui: &Ui, w: &World, kind: int, name: string, cost: int, maxed: bool) {
    if maxed {
        label(`{name} is maxed`);
    } else {
        row() {
            label(`{name}   {cost} leaves`);
            button("buy") {
                buy(w, kind);
            }
        }
    }
}

@j3ui shop(ui: &Ui, w: &World) {
    panel(280, 248) {
        label("shop");
        shopRow(ui, w, 0, "grab speed", costSpeed(w.speed), w.speed >= 5);
        gap(6);
        shopRow(ui, w, 1, "grab reach", costReach(w.reach), w.reach >= 4);
        gap(8);
        label(`speed {w.speed}    reach {w.reach}`);
    }
}

fn draw(w: &World) {
    let ui = Ui.begin(w.font, 16, 188);
    ui.theme = Theme.yard();
    shop(ui, w);
}
```