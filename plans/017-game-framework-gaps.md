# Plan 017: Fill the everyday game framework gaps

## Status and intent

- **Status:** TODO; **priority:** P2 overall (the first two phases are the first useful slice); **effort:** L across independent phases.
- **Scope:** game-facing J3 APIs and their small platform hooks, plus one playable 2D reference project. Recheck the live APIs before implementation; the working tree already contains uncommitted features.
- **Goal:** let a small desktop game go from authored art and levels to a resizable, controller-friendly, multi-screen game without each project rebuilding the same support code.

## Research comparison (2026-09-25)

J3 already has a fixed-step `App`, 2D camera and source-rectangle drawing, ECS, action bindings, one-layer tile files, UI, particles, audio groups, saves, and a separate 3D scene/editor pipeline. The gaps below are *completeness and workflow* gaps, not absent core rendering or game-loop support. LibGDX supplies some of these directly; MonoGame often supplies primitives and tutorials rather than a ready-made high-level system.

| Opportunity | J3 now | Reference and useful direction |
|---|---|---|
| Resize-safe 2D coordinates | `game.Camera` has `pos`/`zoom` and `toScreen`/`toWorld`; `windowSize` and resizable windows exist. No reusable fit/fill/extend viewport policy or shared mouse-to-world mapping. | [LibGDX viewports](https://libgdx.com/wiki/graphics/viewports) provide fit, fill, stretch and extend policies; [MonoGame's scaling guide](https://docs.monogame.net/articles/getting_to_know/howto/graphics/HowTo_Scale_Sprites_Matrix.html) builds a matrix from a design resolution. |
| Sprite and skinned UI workflow | `game.drawTextureSrc`/`drawTextureEx` and `anim.Frame` support atlas rectangles, but projects own atlas metadata; `ui` draws built-in shapes and has no nine-slice drawable. Each draw reaches SDL's renderer; actual batching cost needs measurement. | [LibGDX SpriteBatch/regions](https://libgdx.com/wiki/graphics/2d/spritebatch-textureregions-and-sprites), [texture packing](https://libgdx.com/wiki/tools/texture-packer), and [ninepatches](https://libgdx.com/wiki/graphics/2d/ninepatches); [MonoGame SpriteBatch](https://docs.monogame.net/articles/getting_to_know/howto/graphics/HowTo_Draw_A_Sprite.html). |
| Asset preparation and lifetime | `assets.Assets` is a synchronous load-once cache for textures, audio and fonts; it keeps entries until `destroy` and even caches failed handles. `-e gen` incrementally runs mesh scripts, while glTF import is editor-oriented. | [LibGDX AssetManager](https://libgdx.com/wiki/managing-your-assets) tracks shared resources, dependencies and load progress; [MonoGame's content pipeline](https://docs.monogame.net/articles/getting_to_know/whatis/content_pipeline/CP_Overview.html) turns source art and data into build outputs. |
| Authored 2D worlds | `tile.TileMap` stores one `u8` grid with ids clamped to 0–4 and up to 32 marks. Dungeon interprets these itself. No layered terrain/decor/collision or direct Tiled import. | [LibGDX Tiled maps](https://libgdx.com/wiki/graphics/2d/tile-maps) support layers and objects. [MonoGame's tilemap tutorial](https://docs.monogame.net/articles/tutorials/building_2d_games/13_working_with_tilemaps/) constructs this in game code and notes that Tiled import comes from an extension, not MonoGame core. |
| Player input and screen flow | `input.Actions` binds a key and pad button to ids; `game` handles two controller slots with no public connection events or rebinding API. `ui` can block its own interactions, and `scene.Scenes` stores ids, but games wire modal input, load/unload and transitions themselves. | [LibGDX input multiplexing](https://libgdx.com/wiki/input/event-handling) lets UI consume input before gameplay; [MonoGame's gamepad guide](https://docs.monogame.net/articles/getting_to_know/howto/input/HowTo_DetectGamePadInput.html) calls out connection handling. [LibGDX scene2d](https://libgdx.com/wiki/graphics/2d/scene2d/scene2d) combines UI event routing and timed actions. |
| Sound control | `game` has music, groups, volume, fades and positional channels. There is no reusable per-voice state/ownership layer for menu, ambience and gameplay sounds. | [MonoGame SoundEffectInstance](https://docs.monogame.net/api/Microsoft.Xna.Framework.Audio.SoundEffectInstance.html) exposes per-instance state, loop, pan and volume; this suggests a modest voice API, not a new mixer graph. |

## Work, in order

### 1. Virtual 2D viewport and pointer conversion (P1, S)

Add a J3 `Viewport` value with `fit`, `fill`, `stretch` and `extend` policies, logical size, window rectangle, scale, and `screenToWorld`/`worldToScreen`. Apply it consistently to camera drawing, pointer picking and `ui` hit tests. Keep current pixel-space APIs usable. Under `fit`, a 16:9 world must letterbox predictably in 4:3, keep a HUD in a deliberate safe area, and ignore clicks in the bars. Drawing and pointer conversion must agree after every resize. Centralize the small SDL viewport/clip hook if the present drawing API cannot express this cleanly.

**Check:** a tiny interactive scene at 800×450, 1024×768 and a narrow window; compare screenshots and click the same logical button and tile at each size. Exercise `toWorld` round trips and zero-size/minimized windows.

### 2. Atlas metadata, sprite drawing, and nine-slice UI (P1, M; depends on 1 for UI sizing)

Add an atlas manifest of named rectangles and pivots, parsed in J3. Add a `Sprite`/`Region` helper that draws a frame with tint, origin, flip and scale using the existing `game` calls, and let `anim.Clip` refer to atlas frames without copying rectangle tables into every game. Expose the pivot to SDL's texture drawing hook; `drawTextureEx` currently passes a null rotation center, and its GL path omits tint/flip/angle, so either give the new helper matching behavior on that path or document it as SDL-only. Provide a nine-slice drawable for `ui` panels/buttons, preserving the existing built-in theme as a default. Build atlas pages from PNG inputs with a deterministic asset tool (including padding/extrusion and explicit filtering choice), then load the output through `assets`.

**Performance gate:** measure dungeon/gallery with many visible sprites before adding a custom 2D batch. If SDL's current renderer is already adequate, stop at atlas and draw ergonomics. If CPU draw overhead dominates, add a batch that groups only compatible draws while preserving order, alpha, clip and camera semantics; report draw count and frame-time change. Do not assume one texture atlas alone merges draw calls.

**Check:** animated character, flipped sprite, scaled nine-slice panel, and overlapping transparent sprites look the same at each viewport size; generated atlas is reproducible and has no texture bleed.

### 3. Explicit asset sets and build outputs (P2, M; depends on 2)

Extend `assets.Assets` with named asset sets for a screen/level, explicit `load`/`release`, shared-handle ownership, retryable failure and useful errors. Add staged `update`/progress for a loading screen. A single large decode may still block on SDL: measure that pause and only promise frame-budgeted loading when the underlying work can actually be split. Extend `-e gen` or a sibling build step to track source files and parameters for atlas/map outputs; a change to an input should rebuild its dependents, while unchanged inputs keep their outputs. Keep source assets outside the output directory. Package/install work belongs to `roadmap.md`, not this phase.

**Check:** title and level share a font/texture; unloading one set leaves the other valid, unloading both frees resources exactly once, a missing file can be retried, and changing one atlas source rebuilds only its dependent output. A loading screen shows honest progress and remains responsive for ordinary assets.

### 4. Layered 2D maps and an import path (P2, M; depends on plan 016's J3 JSON reader if importing Tiled JSON)

Keep the existing `# j3tile 1` parser for current projects. Introduce a versioned layered map format with arbitrary tile ids, multiple visual layers, a collision layer, and named objects with typed properties. Provide an offline Tiled JSON importer into that format, with clear errors for unsupported infinite/chunked maps or encoding variants. Runtime games read only the J3 map; importer and renderer share coordinate/origin rules. Give `j3.game.tile` a draw helper using phase 2 atlas regions and cull to the viewport, but leave game-specific collision interpretation to the game.

**Check:** author a two-layer dungeon room with a door trigger and spawn in Tiled, import it, render at three window sizes, and confirm collision and marker locations. Old dungeon maps load unchanged.

### 5. Input contexts, rebinding, and controller lifecycle (P2, M)

Let a game define action sets for gameplay, menu and modal UI, with topmost/consumed input taking precedence; reuse `input.Actions` as the binding table. Support two bindings per action, keyboard and gamepad remapping, axis dead zones, press/release on both devices, and serialize choices through `save`. Expose connected/disconnected status for the two supported pads and safely close/reopen stale SDL handles on hotplug. Keep unsupported controller indices explicit rather than silently pretending they work.

**Check:** menu click/confirm cannot also fire the attack action; remapped controls survive restart; unplugging a pad during play stops actions cleanly and reconnecting resumes them; keyboard remains usable throughout.

### 6. Screen lifecycle and modest audio voices (P2, S; depends on 3 and 5)

Add a thin controller around `scene.Scenes` for enter/exit, pause/resume, associated asset sets and a timed fade. Keep scene ids and game-owned state rather than introducing a second ECS or scene graph. Add an audio voice handle with per-channel query/stop/volume/pan and a screen-owned voice list that stops on exit. Reuse the existing SDL_mixer groups and music fades. Pitch shifting and a mixer graph wait for a demonstrated game need and backend support.

**Check:** title → loading → play → pause → play → title, with repeated transitions, no duplicate music, no lingering level sound, correct asset releases, and gameplay input blocked while the pause screen is active.

## Integrating proof

Build one small 2D reference game/project that uses a packed animated sprite, authored layered room, fitted viewport, skinned menu, remappable controller action, title/loading/play/pause flow and a looping ambience voice. Prefer shrinking duplicated code from `examples/dungeon` or `examples/gallery` after the new APIs are useful rather than rewriting both games at once. Verify headless parser/data tests where possible, plus manual resize, controller hotplug, screenshots and frame profiling on Linux; recheck other desktop targets when plan 013 lands. Document APIs through `j3c doc` and one concise guide.

## Boundaries and coordination

- Plans [009](009-animation.md), [010](010-shader-effects.md), [012](012-online-asset-browser.md), [014](014-editor-polish.md), [015](015-rendering-gaps.md) and [016](016-move-tool-logic-to-j3.md) already own 3D animation, shader effects, remote asset browsing, editor UI, 3D rendering and tool-logic migration. This plan does not duplicate those efforts.
- Keep gameplay rules, map interpretation, atlas metadata and transition policy in J3. Restrict Rust changes to build orchestration or platform plumbing that J3 cannot express. Respect the no-crates compiler constraint.
- The 2D feature set is the target. Advanced physics, networking, mobile/touch support, a general retained UI scene graph and a mixer graph need a separate game-driven case.
- Finish each phase as a usable slice. If profiling disproves the batching need or staged loading cannot meet the responsiveness goal, record the measurement and stop that subpart without delaying the rest.
