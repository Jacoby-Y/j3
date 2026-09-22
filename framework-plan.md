---
name: Game framework features
overview: Add the missing game-framework pieces as small J3 stdlib modules (and a few extensions of app, ecs, game, ui, gfx, save, debug), prove each one with a typechecked call site, then update the compiler docs to match.
todos:
  - id: runner
    content: Pause, timescale, and frameAlpha() in app/game without changing draw signatures
    status: completed
  - id: ecs
    content: Column.join and Signal<T> in ecs.j3
    status: completed
  - id: sim
    content: j3.space, j3.physics (2D + thin 3D), j3.path
    status: completed
  - id: present
    content: drawTextureEx, j3.anim clips and transform tracks, camera Rig
    status: completed
  - id: feel
    content: Audio groups/fade/position, j3.input actions, save float/string/blob
    status: completed
  - id: ui
    content: Text input, field, focus, drag, world labels; widget tables
    status: completed
  - id: debug-gfx
    content: Debug lines/AABBs, point lights, drawAlpha, drawInstanced
    status: completed
  - id: docs
    content: Finish the roadmap edits, audit docs against the shipped API, and record final checks
    status: completed
  - id: acceptance
    content: Confirm the gallery call site demonstrates field, drag, and keyboard focus behavior
    status: completed
isProject: false
---

# Game framework features

Pure J3, same style as [j3c/stdlib/j3/collide.j3](j3c/stdlib/j3/collide.j3) and [j3c/stdlib/j3/fx.j3](j3c/stdlib/j3/fx.j3). No new native library, no shader language, no capturing closures. Listeners and systems stay free functions. Every new `stdlib/j3/*.j3` file is added to the embed tables in [j3c/src/stdlib/mod.rs](j3c/src/stdlib/mod.rs). New `j3.game` entry points get matching no-op stubs in [j3c/stdlib/j3/game_headless.j3](j3c/stdlib/j3/game_headless.j3). Each feature gets `///` on its public items and one call site in an example that `cargo test` already typechecks.

## Runner

In [j3c/stdlib/j3/app.j3](j3c/stdlib/j3/app.j3), `run` already keeps leftover `acc` and then calls `draws` as `fn(&W)`.

- `pause(on)` skips `first` / `update` / `last`. `always` and `draw` still run, so a menu can sit on top.
- `timeScale(s)` multiplies the `dt` added into `acc`.
- Before the draw loop, store `acc / TICK` (clamped to 0..1) where `j3.game` can read it as `frameAlpha()`. Draw signatures stay `fn(&W)`. Bounce’s draw lerps with that alpha.

## ECS

In [j3c/stdlib/j3/ecs.j3](j3c/stdlib/j3/ecs.j3), `Column.matching` / `matching2` already scratch-allocate an entity list. Add `join(ecs, other)` so a second column is passed as `&Column<U>` instead of raw mask slices. A third component is a second join.

Add `Signal<T>`: a growable `[]fn(T)`, `listen`, and `emit` that calls listeners immediately. `Events<T>` stays the polled queue. Listeners are free functions; they cannot close over a world.

## Simulation modules

- `j3.space` — uniform grid. `insert(id, rect)`, `query(rect)` into scratch, `clear` each step. Physics and games share it.
- `j3.physics` — 2D `Body` (pos, vel, size, gravity scale). `step(dt)` integrates, then resolves AABB overlaps from [j3c/stdlib/j3/collide.j3](j3c/stdlib/j3/collide.j3) using the grid. 3D is a `Body3` (pos, vel, half-extents) that stops on `j3.voxel` `overlaps`. No contact manifolds.
- `j3.path` — A* on a byte grid. Walkable is a tile-byte range, same idea as `tileSolid`. Returns scratch cell indices.

Wire physics into bounce, path into defense.

## 2D presentation

- [j3c/stdlib/j3/game.j3](j3c/stdlib/j3/game.j3): bind `SDL_RenderCopyEx` and `SDL_SetTextureColorMod`. `drawTextureEx(tex, dest, angle, flip, tint)` plus the camera variant.
- `j3.anim` — `Clip` (source rects + fps) and `Player.tick(dt)` / `frame()`. A transform `Track` of keys `(t, pos, rot, scale)` with `sample(t): Mat4` for props. That is the skinned-clip stand-in: one rigid transform, not GPU skinning.
- [j3c/stdlib/j3/fx.j3](j3c/stdlib/j3/fx.j3) — `Rig` around `Camera`: `follow`, decaying `shake`, `zoom`.

Use the sprite player and `drawTextureEx` from bounce or gallery; sample a `Track` from island’s lantern.

## Feel

- Audio, still in `game.j3` (mixer fn pointers already live there). `play` returns a channel. Fixed groups: sfx, voice. `setGroupVolume`, `fadeIn` / `fadeOut` via `Mix_FadeInChannel` and `Mix_FadeOutChannel`, `setPosition(channel, angle, distance)` via `Mix_SetPosition`. Music fade uses `Mix_FadeInMusic` / `Mix_FadeOutMusic`.
- `j3.input` — `Actions` table keyed by an int the game chooses. `bind(id, key, pad, padIndex)`, `down` / `pressed` / `released` through existing `j3.game` edges.
- [j3c/stdlib/j3/save.j3](j3c/stdlib/j3/save.j3) — keep `key=int`. Add `key f=`, `key s=`, and `key b=` (hex). `getFloat` / `putFloat`, `getStr` / `putStr`, `getBlob` / `putBlob`.

Dungeon’s save is the call site for a float or string key.

## UI

[j3c/stdlib/j3/ui.j3](j3c/stdlib/j3/ui.j3) and the widget tables in [j3c/src/parse/syntax.rs](j3c/src/parse/syntax.rs):

- `field` — one call, edits a `j3.text.Text` buffer. `game.j3` `poll` starts `SDL_StartTextInput` and records `SDL_TEXTINPUT` / backspace. Headless returns no characters.
- `focus` — `Ui` stores a focused widget id. Tab / shift-tab move it. A focused `field` or `button` takes keys.
- `drag` — one call, true while the pointer holds that id. `dragDelta()` is the movement since press. Existing `drop` stays a visual only.
- `worldLabel(cam, world, msg)` — `Camera.toScreen` then `drawText`. `worldLabel3(viewProj, world, msg)` projects with `Mat4` and `windowSize`.

Gallery is the typecheck for `field`, `focus`, and `drag`. Its widgets page tells users how to move focus and activate the count button from the keyboard; focused buttons use the theme's hot color.

## Debug draw

[j3c/stdlib/j3/debug.j3](j3c/stdlib/j3/debug.j3) keeps `Stats`. Add `drawLine`, `drawRect` (outline), and `drawAabb` in screen and camera space, via `SDL_RenderDrawLine` on `j3.game`. `gfx.drawLine(a, b, color)` is the 3D line. Overlay stays something the game calls.

## 3D in gfx

[j3c/stdlib/j3/gfx.j3](j3c/stdlib/j3/gfx.j3) already has one forward shader, `drawCutout`, and a directional shadow.

- Up to 4 point lights as uniforms (`light(i, pos, color, radius)`), looped in the existing fragment shader.
- `drawAlpha(mesh, model, a)` enables blending for that draw.
- `drawInstanced(mesh, models)` with a fixed cap (64) and `glDrawArraysInstanced`. One extra instance mat4 buffer. No new mesh format.

Minecraft or island calls `light` and `drawInstanced` once.

## Docs last

`j3c doc` already prints `///` and lists modules by scanning stdlib, so the new modules show up once the files exist. The hand-written maps do not.

- [AGENTS.md](AGENTS.md) and [j3c/AGENTS.md](j3c/AGENTS.md): verify module lists and the `j3.game`, `j3.gfx`, and `App` summaries against the stdlib. They have been updated in the current worktree; retain them and fix any mismatches found in the audit.
- [j3c/src/doc.rs](j3c/src/doc.rs): confirm `j3c doc j3ui` describes the implemented field/focus/drag behavior. The prose is updated; make only corrections revealed by the audit.
- [j3c/src/parse/syntax.rs](j3c/src/parse/syntax.rs) and `editors/vscode/syntaxes/j3.tmLanguage.json`: ensure every `@j3ui` widget form has matching editor highlighting. The checked-in grammar currently matches `dump-syntax`; regenerate it if the table changes.
- Gallery acceptance: `examples/gallery/src/widgets.j3` demonstrates field editing and pointer drag, and tells users how to use Tab/Shift-Tab and Enter/Space. Focused buttons have a visible theme-color highlight.
- [roadmap.md](roadmap.md): remove shipped physics, audio groups/fade/position, and draw interpolation from **Left** / **Next** and leave them in **Shipped**. Keep the stated limits of the lightweight physics API, and leave Wasm, threads, shader language, and hot reload as future work.
- [j3c/src/helptext.rs](j3c/src/helptext.rs): inspect for stale framework descriptions; edit only if it names the old surface.

## Finish criteria

- All public framework features in this plan have `///` documentation and a representative typechecked call site; specifically, the Gallery visibly exercises field editing, drag, and keyboard focus.
- The root and compiler maps, generated editor grammar, `j3c doc`, and roadmap agree with the shipped API. No shipped feature remains listed under **Left** or **Next**.
- Run `cargo test` from `j3c/`; require the stdlib parse, example typecheck, grammar snapshot, and docs tests to pass. Record any warning or runtime dependency that limits the result.

## Completion record

- `cargo test`: 146 passed, 0 failed. This includes the Gallery project typecheck, TextMate grammar snapshot, and documentation tests.
- `cargo run -- doc j3ui`: lists the parser's widget forms and documents keyboard focus and focused-button activation.
- The compiler build reports three existing unused/dead-code warnings in `src/lsp/complete.rs` and `src/lsp/query.rs`; no test failures.
