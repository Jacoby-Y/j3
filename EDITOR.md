# J3 scene editor (v1)

## Implemented

- `j3c editor --new PATH` creates a runnable project with `j3.toml`, `j3.editor.toml`, a scene, a camera, a cube, and an example script (`scripts/spin.j3s`). `j3c editor PATH` opens an existing project and adds editor files without changing its game entry point.
- The window: a toolbar, the hierarchy and assets on the left, the 3D viewport, the inspector on the right, and a status bar with hints. Both side panels and the hierarchy/assets split are resizable by dragging, and remembered.
- **Get Asset** (toolbar) opens a tab that searches free CC0 models, materials, and skies online (Poly Haven, ambientCG, Kenney packs, with previews of each pack's models), shows the picked one's details, and adds it to the project; credits go in `assets/CREDITS.txt`.
- The session (open tabs, split views, per-tab cameras, selections, folds, console) is saved in `target/j3-editor/session` and restored on the next launch.
- Tabs drag to reorder, or onto a view's side to split it (up to 4 views side by side, resizable), or onto another view's tab strip.
- A console under the viewport shows the game's output, build errors and warnings, and crashes.
- Viewport:
  - Camera controls.
  - Picking of meshes by their triangles, and of cameras, groups, lights, and placed scenes by markers in front of them; clicking the same spot again selects the object behind; box select, and Ctrl+click to add to the selection.
  - Move/rotate/scale gizmos in world or local axes (L), which drag every selected object. Ctrl snaps to steps set under **Snap**, where the grid spacing is also set.
  - Front, side, and top orthographic views (1/3/7, 5 toggles ortho). Look through a camera with 0.
  - A selected camera shows a live preview of its view in the viewport's corner, which can be locked to stay up.
- Editing:
  - Right-click menus, and copy/cut/paste/duplicate of object subtrees.
  - Undo and redo, 100 steps. Each step keeps only the objects that changed.
  - Number fields drag (whole-number ones step by one) or take typed values. Text fields select with the mouse and Shift, jump and delete by words with Ctrl, and cut, copy, and paste the selection.
  - Colors use a picker (saturation/value square, hue bar, hex, 0-255 values, recent colors).
- Hierarchy: an **+ Add** menu for every object type (including Plane and Sky Box), unique default names, fold and unfold, search, drag to reorder or parent, and mark the main camera.
- Built-in meshes (`builtin:cube`, `sphere`, `quad`, `plane`, `sky`) made by scene3d, picked under Shape in the mesh inspector. Planes and quads have edge handles that resize one side. A sky box draws a generated cloudy sky (`builtin:sky-clouds`) behind everything, and mirrors reflect it.
- Scenes:
  - Several scenes open in tabs. Scenes can be placed inside other scenes (`INSTANCE` nodes), nested and cycle-checked.
  - A placed scene's objects can be edited per copy, including objects from scenes nested inside it. The overrides are saved in the outer scene (`over.<copy>.<id>/<id>/....<field>`, the ids leading down to the object), a middle scene's own overrides carry through when it's placed, and "Revert" restores the file's values.
- Lights and scene lighting:
  - Point lights; eight shine at once, including glowing materials set to light other objects.
  - Sun direction and color, sky/ambient color, fog, and sun shadows, stored in the scene and used by the editor and the game template alike.
- Scripts:
  - `.j3s` scripts attached to objects, with per-object props (`f32`, `int`, `bool`, `string`, `Vec3`, `NodeId`) from the script's `props` block; `j3c check` reports a missing or mistyped prop at the scene line.
  - `NodeId` props follow copies of the object they point at.
  - Play runs scripts on the VM: a script edit swaps into the running game, or relaunches it when the script's layout changed, without compiling the game.
- Assets:
  - Import `.gltf`/`.glb`/`.j3mesh` by path, file picker, or drag-and-drop onto the window, as pasted objects or as a reusable scene file.
  - glTF alpha-mask and alpha-blend materials import; see-through meshes draw last, farthest first.
  - Drag assets into the viewport or onto hierarchy rows: meshes and scenes are placed where dropped, materials and images go on the mesh they land on.
  - Rename, move, and delete assets from their right-click menu. Scene references are rewritten, and deleted files go to `target/j3-editor/trash/`. Objects whose files are gone are tagged "missing".
  - Materials (`.j3mat`), edited in their own tab on a preview mesh, with undo:
    - tint, glow (optionally lighting other objects), highlights, mirror and roughness, transparency mode, opacity, refraction index (glass bends what's behind it), glass edges, and tiling;
    - an image, or tileable noise (`j3.gfx.noise`: value, Perlin, cells, stripes; fbm/ridged/billow layers, warp, contrast, bands, cut, invert, and a 2-4 stop color ramp).
  - Meshes without UVs get box-projected UVs on load.
  - Shader effects (`.j3fx`: a `key=value` header, `---`, then GLSL). A material effect is a hook after lighting and glow, before fog, with an optional vertex `displace`. A camera effect is a stack of full-screen passes over the camera's color and depth. Params show in the material and camera inspectors. Saves recompile at once, and GLSL errors go to the console with the file's line. Play, `-e render`, and the viewport's camera view all run them. **New effect** and **New camera effect** write starters.
- Play:
  - **Play** saves, builds, and runs the scene in the open tab. **Stop** (or Play again) closes or restarts the game.
  - Build errors show in the console, and the full log is kept in `target/j3-editor/build.log`.
  - Builds are skipped when sources and options are unchanged, and clang is skipped when the generated code is unchanged (`J3C_TIMINGS=1` prints step times).
- URP (per scene, inherited by scenes that place it): mirrors and glass trace rays through the scene's triangles (BVH on the GPU, `j3.gfx.bvh`), splitting at every glass surface by Fresnel and absorbing glass tint over distance, with bounces, rays per pixel, and traced (tinted) sun shadows. The scene inspector shows "*Using URP" and where it comes from.
- See-through objects cast partial shadows and glass tinted ones, through a second, translucent shadow map.
- **Rescan** (F5) reloads assets and `j3.editor.toml`. Unsaved scenes are autosaved every minute, and a crash copy is offered back on the next open.
- **Hot reload:** a scene or material file changed on disk (by an agent, `j3c -e scene`, a text editor, git) is picked up within a second. A tab with no unsaved edits takes the file as one undo step; one with unsaved edits merges object by object, keeping yours where both sides changed the same object. Placed scenes rebuild when their file changes.
- **Built-in shapes:** cube, sphere, cylinder, cone, capsule, torus, quad, plane, sky (Shape buttons and **+ Add**).
- **Procedural meshes:** `assets/gen/<name>.j3` scripts build meshes with `j3.gfx.meshgen` (box, cylinder, cone, frustum, sphere, capsule, torus, lathe, extrude, heightField, transform stack, colors) and write `assets/gen/<name>.j3mesh`. `j3c -e gen`, Rescan, builds, renders, and plays run the changed ones. New projects get a tree.
- **Snap to ground** in the object menu.
- **Scene props:** the scene inspector's **Props** section declares typed settings (float, int, bool, text, material, mesh, scene) with defaults, and **Rename** moves a prop and the values scenes placing it already set; a placed copy's inspector sets them, saved as overrides in the outer scene, with Revert. A variant scene's values reach the copy nested inside it. Code reads them with `scene3d.prop` / `propNumber` / `propInt` / `propFlag` on `scene3d.copyOf(scene, id)`.
- **Agent tools** (see below): `j3c -e scene`, `j3c -e render`, `j3c run --frames/--screenshot/--headless`, `j3c -e gen`, and `j3c -e mcp`.

## Agent use

`j3c -e mcp` is an MCP server (stdio). Add it once with `claude mcp add -s user j3 -- j3c -e mcp` (the editor's **Agent** button copies this). It follows the project `j3c editor` has open, else the one around the current directory; `--project PATH` pins one. The Agent panel can turn agent access off and shows recent requests. Tools: `scene_list`, `scene_get`, `scene_add`, `scene_add_many` (one step, all or nothing), `scene_set`, `scene_delete`, `place` (ground, ontop, scatter, grid: modular pieces at cell x,z, floor, quarter turns), `undo` (editor closed: puts back the scene file from before the last tool change; 20 kept in `target/j3-editor/history/`), `render` (returns the image), `check` (scenes and code), `play` (runs N frames headless, returns the last frame and output), `console`, `import`, `asset_search`, `asset_online_search` and `asset_fetch` (Poly Haven, ambientCG, Kenney; see below), `gen`. Objects are named by name, `Parent/Child` path, or `#id`; rotations are degrees.

While `j3c editor` has the project open, scene tools and `render` act on the live editor: each change is one undo step, nothing is saved until you save (or the agent passes `save`), and renders include unsaved edits. Scenes the editor doesn't have open are edited as files. The same operations are on the command line: `j3c -e scene --help`, `j3c -e render --help`; `j3c -e mcp --tools` lists every tool's arguments next to its CLI command. Every edit reply says whether it went to the open editor or the file.

How it works: the scene operations are `j3c/editor/src/tool.j3` and `render.j3`, run by the editor binary in headless `cli`/`render` modes (`J3_EDITOR_MODE`), so `scene3d` stays the only reader and writer of scene files. `j3c/src/scene.rs` runs them and turns replies into JSON; `j3c/src/mcp.rs` is the server. Live commands go through the events file as `cmd` lines (`live.j3`), and replies come back on the editor's stdout as `J3EDITOR REPLY` lines, which the host appends to `target/j3-editor/replies`. Renders use SDL's offscreen video driver, so no display is needed.

## Setup and use

On Linux, install Rust/Cargo, `clang`, SDL2, OpenGL 3.3 (`libGL`), and SDL2_ttf. SDL2_image is needed only for JPEG textures (PNG decodes without it). From the repository root:

```sh
cd j3c
cargo build --release
./target/release/j3c editor --new ../my-game
# Reopen it later:
./target/release/j3c editor ../my-game
```

Use the left panel to add objects or choose assets, the right panel to edit them, and **Save** or **Play** when ready. For an existing project, run `j3c editor PATH`, then load and draw `j3.scene3d` from its game code; the new-project template shows the integration.

## Agent reference: where to extend it

Read [`j3c/AGENTS.md`](j3c/AGENTS.md) for compiler and language constraints before changing the editor.

| Area | Source of truth |
|------|-----------------|
| CLI entry and help | `j3c/src/cli.rs` dispatches `editor` to `j3c/src/editor.rs`. |
| Project setup and desktop host | `j3c/src/editor.rs` creates project files, reads `j3.editor.toml`, generates the script and asset catalogs, builds the editor once per `j3c` version into `~/.cache/j3/scene3d/editor/` (`J3_EDITOR_SRC=path/to/editor/src` builds from disk instead of the embedded copy), launches it, and handles the editor's `J3EDITOR` requests on stdout: PLAY/STOP, IMPORT/PICK, SCRIPT, RESCAN, RENAME/DELETE. Messages back to the editor are lines appended to `target/j3-editor/events`. |
| Editor window and interactions | `j3c/editor/src/` (one module per file, entry `main.j3`) owns the viewport, panels, selection, transforms, undo (diff-based `Delta` steps), autosave, overrides of placed objects, the material panel, and commands sent to the host. |
| Scene format and runtime API | `j3c/stdlib/j3/scene3d.j3` loads/stores scenes, expands placed scenes (`INSTANCE`, `expand`, `refresh`, `uses`), applies per-placement overrides (`over.*` lines), manages nodes, per-object fields, lights, scene lighting (`scene.env.*`), and materials, uploads meshes, and draws them (see-through ones last). It is embedded through `j3c/src/stdlib/mod.rs`. |
| Asset conversion | `j3c/src/gltf.rs` imports static glTF/GLB and `.j3mesh` into project assets and writes each import to its own `target/j3-editor/import-<n>.j3scene`, which the editor pastes into the scene, or moves it to `assets/imported/<name>.j3scene` to be placed as a scene. |
| Supporting UI/rendering | `j3c/stdlib/j3/ui.j3`, `gfx.j3` (lit shader, `Surface`, `ViewState` for extra views), `noise.j3` (material noise), `bvh.j3` (the triangle BVH URP traces), `mesh.j3`, and `game.j3`. |

Project-owned files are `j3.editor.toml`, `scenes/*.j3scene`, `assets/`, and `scripts/*.j3s`. `j3c` makes `j3.scene3d.scripts` from the scenes in memory (`j3c/src/compile/scene_scripts.rs`); nothing is generated into `src/`. `target/j3-editor/` holds catalogs, the events log, import output, build logs, autosaves, editor prefs, and the asset trash. See [`j3c/editor/README.md`](j3c/editor/README.md) for the user-facing script and prop format.

## MCP Fixes

Hang-ups from building `examples/editor_village` (a Kenney-kit medieval village) through MCP, worst first.

1. **MCP server goes stale after a j3c rebuild.** Every scene call says "No such file" because the install step (`mv -f`) replaced the binary the running server was started from. **Fixed (plan 002).**
   Fix: in `scene::run_tool`, when the frontend exe is gone, call `editor::frontend` again (it rebuilds and relocks) and retry once. Hold the cache's shared `in-use.lock` for the server's lifetime so cleanup skips its build.
2. **Missing SDL2_image fails silently.** Textures render white in the editor, renders, and games. **Fixed (plan 001):** 8-bit PNGs decode in J3 (`canvas.loadPng`) without it; JPEG still needs it, and `check` warns when a project has JPEGs and no SDL2_image.
   Fix: decode PNG in J3 (inflate plus unfilter, next to `canvas.savePng`) and use it in `loadRgba` when `IMG_Load` isn't available. Until then, print one "SDL2_image not installed; textures are off" line and show it in `check`.
3. **JSON errors are vague** ("bad array" for an exponent the parser didn't handle; exponents now parse). **Fixed (plan 001).**
   Fix: give `lsp::json` errors the byte offset and the nearby text, and have `gltf` prefix them with the file name.
4. **`import` takes one file per call.** **Fixed (plan 001):** `paths`, a folder, or `*` wildcards. A model whose `assets/imported/<name>.j3scene` already exists is skipped (and placed from the existing scene); `again: true` imports a copy. Matching is by file name only, so a different kit's `tree.glb` needs `again`.
   Fix: take `paths: []` or a folder (with a glob), import them in one call, and return one line per model.
5. **No texture bake at import.** Palette kits (Kenney's colormap) needed a Python pass that samples the texture at each triangle's UV center. **Fixed (plan 001):** `import` with `bake: true` (8-bit PNG textures).
   Fix: an `import` option `bake: true` that does the same in `gltf.rs` (needs the PNG decoder from item 2) and leaves `texture` empty.
6. **No recolor for imported models.** The nature kit's teal greens and a red roof variant meant rewriting mesh files. **Fixed (plan 001):** a `tint` multiplier on meshes and placed scenes (`scene_set`, CLI, inspector); hue shift skipped until multiply can't reach a color. Known edge: tinting a mesh inside a placed scene stores an override with the multiplied value, so that mesh stops following later tint changes on its instance.
   Fix: instance fields `tint` and `hueShift` (and/or a color remap `from→to` with a tolerance) applied at upload, settable from `scene_set` and the inspector. A material override per instance also works.
7. **`scene_add` can't start a new scene.** **Works (plan 004):** the error was the stale server (plan 002).
   Fix: `scene_add` (and `j3c -e scene add`) creates the file with the three header lines when it's missing and the path ends in `.j3scene`.
8. **Modular kits cost a call per piece** (about 30 per house). **Fixed (plan 004):** `scene_add_many` and `place` op `grid`; a batch is requests joined by byte 30 that `tool.apply` runs as one change.
   Fix: a `scene_add_many` tool (a list of add specs in one undo step), and a `place` op `grid` (a piece at cell x,z, a floor, rotation in 90° steps, a cell size).
9. **`check` warns "no camera" on part scenes** (houses only ever placed in other scenes). **Fixed (plan 002).** It only sees scenes placed by other scene files; a scene the game loads from code (`scene3d.load`) and also places elsewhere won't get the warning.
   Fix: warn only for the main scene and scenes nothing places (`j3.scene3d.editor.uses` already knows).
10. **The iso overview render frames the huge ground plane**, so with fog the village is a pale tile. **Fixed (plan 002).**
    Fix: frame bounds without flat meshes much wider than the rest (or use the 90th percentile of object positions), and turn fog off for overview views.
11. **No undo when the editor is closed.** A bad batch means deleting by hand. **Fixed (plan 004):** `undo` tool and `j3c -e scene undo`; no redo.
    Fix: before each file-mode write, keep the previous text in `target/j3-editor/history/<scene>.<n>` (last 20), plus an `undo` tool that restores the newest.
12. **The game template is bare.** A first-person walker with collision, a settings panel, and drawing that follows the player (shadow box, sun settings) were all written from scratch.
    Fix: stdlib `j3.scene3d.fpwalk` (Look, WASD, jump, boxes from scene instances like the sample's `player.j3`), a `scene3d.drawFrame(scene, look, env)` that does the shadow, sky, and camera passes, and a `j3c new --template fps` using both.
13. **J3 gaps:** no `%`, no `text.contains`, and a module's constants can't be used as `scene3d.INSTANCE` in expressions.
    Fix: add `%` to the lexer, parser, and emit (int and float), add `text.contains` and `text.find`, and resolve `module.CONST` paths in expressions.
    **Fixed (plan 003):** `%` works for ints and floats on both backends (no `%=`, same as `/=`), `text.find`/`text.contains` are in `j3.text`, and `mesh.STRIDE` works after `use j3.gfx.mesh;` (not yet inside `const` initializers or `match` patterns).
14. **No render scale for URP.** gfx has `makeTarget` but nothing draws a target to the screen.
    Fix: `gfx.present(target)` (a full-screen textured quad or `glBlitFramebuffer` with linear filtering) and a `scene3d` render-scale setting that draws URP into a smaller target.
15. **The generated `behaviors.j3` hard-codes the main scene**, so changing `j3.editor.toml` needs a hand edit until the editor regenerates it. **Fixed (plan 002).**
    Fix: regenerate the dispatch in `j3c build`/`run` and in MCP `play` whenever `j3.editor.toml` is newer, or read the scene from the toml at runtime.
16. **The editor build cache keeps growing** (about 80 builds in `~/.cache/j3/editor`). **Fixed (plan 002).**
    Fix: check that `cleanup_frontends` runs on every launch and keeps only the newest few not in use; remove failed `*.tmp-*` directories older than an hour.
17. **`play` screenshots only `j3.app` games.** **Fixed (plan 002).**
    Fix: move the frame and screenshot hooks from `App.run` into `game.present`, so any game that presents gets them.

## Performance

### URP
Tracing runs on the GPU (the BVH is in textures and the lit fragment shader walks it), and the BVH is rebuilt only on change.

Plan 006 (2026-09-25), village at the spawn view, 1280×720 headless on an RTX 5060 Ti, camera-pass GPU ms from `J3STATS`: URP without traced shadows went from 1.7 to 0.2 ms, and traced sun shadows from 45 ms (21 FPS) to about 2 ms (about 250 FPS). Ultra (4 bounces, 4 samples, traced shadows) is about 2.5 ms.
  - **Done: separate lit programs.** Variants by flag (traced sun, mirror, glass; `TRACE_LEVEL`, so the compiler drops unused code), compiled on first use. Each draw picks the smallest one it needs. Even never-run trace code slowed every pixel it was compiled into, so this was most of the gain (traced shadows 14.8 to 1.7 ms).
  - **Done: mirrors trace without the path stack.** `mirrorRay` follows one ray (glass it meets lets it straight through, tinted), and only glass uses `follow` and its 12-path stack. That stack alone made the village pond (30% of the screen) cost 8 ms; now 1.2 ms (104 to 393 FPS, `VILLAGE_POND=1`). The closest-hit search also visits the nearer child first now, which helps long grazing rays like reflections off water.
  - **Done: ultra at the pond** went from 8.0 to 2.9 ms (99 to 217 FPS; a grazing view 5.8 to 2.4 ms). Rays per pixel now scale with roughness (all of them from roughness 0.2 up), since a near-mirror's jittered rays land on the same spot and the water is 0.01. And surfaces seen in mirrors and glass trace their sun ray only where the shadow map is unsure, like the main surface.
  - **Done: traced shadows only where the shadow map is unsure.** Pixels trace only at the map's soft edges, under see-through casters, or off the map, and only within `urpShadowNear` meters (default 40; 0 means no limit; inspector, `scene_set`, and the village's slider). Elsewhere the map's answer stands. gfx notices when nothing was drawn into the map and then traces everywhere.
  - **Done: any-hit shadow rays.** A shadow ray stops at the first opaque hit and only walks the see-through loop when it crossed glass. Correction to the old note: that's one sun ray per pixel, continued through up to 8 see-through surfaces, not 8 rays.
  - **Done: better BVH.** It uses a binned surface-area split (12 bins), which traces about 20% faster than the midpoint split. Rebuilding the village takes 168 ms instead of 116 ms. The shader's stack holds 46 (`bvh.depth` is checked, the village is 31 deep), and it indexes texels with shifts instead of divides.
  - **Done: 262144 traced triangles** (was 65536; the village has 198k, so traced shadows used to miss two thirds of it). A clamp or a tree too deep prints one warning.
  - **Done: honest `J3STATS`.** `--frames` runs wait for the GPU each frame (an offscreen swap never blocks), so GPU-bound frames show. The plan 007 FPS numbers below counted CPU time only.
  - Tried and dropped: near-first order for shadow rays (any hit, so order doesn't help). Drawing only the nearest glass layer (a depth fill of all glass first) made 6 nested cubes 6× faster but lost the inner cubes at 2 bounces, since rays run out before reaching them.
  - Still slow: glass in view. A 3.5k-triangle scene with 6 small glass pieces costs about 5 ms at 960×540, mostly the `follow` path stack (GPU registers). Next: a glass path without the stack (trace the refraction and treat each split's reflection as a `mirrorRay`), or a separate half-resolution pass.
  - The BVH is rebuilt whole when anything moves (168 ms for the village), so dragging in a big scene hitches. Refit instead (plan 006 step 10).
  - Not done, not needed at these numbers: a render scale (`gfx.present`, MCP Fixes #14), an opaque depth prepass, and a half-resolution screen-space shadow pass.
  - A **Trace in editor** toggle under URP, saved per scene (`scene.env.urpEditor`, on by default). With it off, the editor draws plainly and Play still traces. scene3d needs an `editorMode(true)` flag set by the editor, checked in `traceScene`. Inherited URP merges it like the other settings.
  - Store triangles more compactly than 9 texels each.

### Findings from the village (`examples/editor_village`)
About 20 modular houses (around 30 Kenney pieces each), about 1000 scattered instances (trees, grass, flowers, rocks), a 260 m ground, and URP at 2 bounces and 1 sample.

Plan 007 (2026-09-25), headless at the spawn view on an RTX 5060 Ti, from `J3STATS`: 4629 draws and about 180 FPS before; 113 draws and about 770 FPS after. Per step: reusing the shadow pass's world matrices 180 → 197 FPS; culling 4629 → 2640 draws, 310 FPS; instanced batching 2640 → 113 draws, 734 FPS; reusing the trace signature 770 FPS. The frame is still CPU-bound (`scene` about 1.1 ms of 1.3 ms).
- **Done (plan 006): traced sun shadows** went from 21 to about 250 FPS headless; see URP above. `VILLAGE_PRESET=low|medium|high|ultra|traced` picks a preset for headless runs.
- **Done: batching.** `drawWith` groups visible opaque nodes by mesh, material, texture, tint, and sides into instanced draws (`gfx.drawInstanced`, 64 copies per GL draw, shadow pass too). The lit and depth shaders take the model from the instance buffer when `uInst` is set, so instanced draws get the full `Surface`. Glass, cutout, and blended nodes still draw one by one.
- **Done: per-frame CPU.** The camera pass reuses the world matrices and trace signature the shadow pass just computed. Each is still computed once a frame; dirty flags (plus a `scene3d.moved` for code that writes positions) are the next step if `scene` in the profiler says so.
- **Done: culling.** `gfx.visible` tests each node's world box against the clip volume (camera, or the light's box in the shadow pass); anything it drops GL would have clipped anyway. The far plane (`setClip`, which the village ties to the fog) does the distance cut.
- Every draw re-uploads about 40 uniforms (projection, lights, sun...) in `drawMesh`. With batching that's about 100 times a frame, so it no longer matters; per-pass uniforms are the fix if draw counts climb again.
- The sample's `main.j3` rewrites the scene's lighting lines every frame with `setEnvironment`, since the settings panel can change them (a ponytail note there). Cheap now; only write on change if it shows up in a profile.
- The sample's collision builds one box per instance by scanning every node for each instance, which is O(instances × nodes) at startup. Fine at this size.
- **Done: frame profiler.** F3 in any game shows fps, frame/CPU/`scene3d.draw` ms, GPU ms per pass (shadow, camera, HUD; timer queries read two frames late), draws, triangles, and a frame-time graph (`game.frameStats()` for code). Its text uses the first font the game loaded; a game without one gets the graph only. The editor's status bar shows draws · tris · ms, the village settings panel too, and `j3c run --frames N` / MCP `play` print a `J3STATS` line of run averages. Traced-triangle counts aren't in it yet.

### Editor
- Done: hot reload stats each watched file and reads only those whose modification time moved (x86_64 Linux `stat` layout in `editor/src/util.j3`).
- Done: `hitTriangles` (picking, `place`, `ground`) searches meshes past 256 triangles through a BVH it builds on first use and keeps (16 meshes at once).
- `place` still visits every mesh in the scene; a scene-level BVH if scenes get big.

### Builds
- A changed source still re-emits and recompiles the whole program (about 0.7 s on the example project: 0.18 s emit, 0.45 s clang). Emitting one `.ll` per module and caching the objects would cut that; debug metadata makes the split the tricky part.

## Still left

### Animation
- glTF animation: a clip file next to the scene, plus `anim.Track` sampling in scene3d.
- Skinning: joints and weights in the vertex layout (`bindLayout` in gfx.j3) and bone uniforms.
- Morph targets: CPU morphing, which breaks the shared GPU mesh cache.
- Then an "open animation" scene prop (for example a door's `openAnim`).

### Shader effects: what's left
- URP traced hits (mirrors, glass) and the shadow map ignore material effects, and a displaced mesh casts its undisplaced shadow. Porting an effect to the trace shader is its own job.
- Material effects draw with the plain lit variant, so a surface with an effect gets no traced shadows, mirrors, or glass.
- The corner camera preview doesn't run camera effects; the viewport's camera view (0) and Play do.
- Outline effects need a normal buffer.
- Underwater switching (a camera inside a volume) is a script's job: it can set the camera's `effects` field.
- The hook runs before fog; an effect that should skip fog needs a flag.
- Renaming a `.j3fx` doesn't rewrite the `.j3mat` files or camera `effects` fields that use it.
- `j3c -e scene check` finds missing effect files but can't compile GLSL (no GL); broken effects show in `-e render` output only as a stock-looking render.
- Limits: 32 effect files (then one console line, and the rest don't load), 32 compiled effects, 8 params and 16 floats per effect, 8 effects per camera, and post targets for 4 view sizes (more views with camera effects reallocate every frame). A changed file is found by comparing its text, and a rebuild leaks the old text (a few KB per save).
- `uTime` is f32 seconds, so it gets coarse after hours of running (2 ms steps after about 4.6 hours). Effects that need long-running smooth time should wrap it themselves (`mod(uTime, 600.0)`).
- `check` finds missing camera effect files, not missing `fx` files named by materials.
- Editing a `.j3fx` reloads the materials using it from their files, so an open material tab's unsaved edits stop showing until its next edit.
- Glass's grab texture (not an effect thing) is also sized per view, so split views with glass reallocate it every frame.
- The `examples/*/src/main.j3` games don't call `beginEffects`; the game templates and `scene3d.drawFrame` do.
- A J3 compiler bug was hit: passing a struct's `[16]f32` field by value to a function makes the LLVM backend emit bad IR (`defined with type 'ptr' but expected '[16 x float]'`), and a local `&G.surf.fxVals` breaks the C backend. Worked around with slices.

### Editor extension API
The editor is built once per j3c version. Extensions would need a per-project editor build that also compiles the project's `editor/*.j3` hooks, cached by those files' hash.

### Online asset browser: what's left
The **Get Assets** tab and the MCP tools `asset_online_search` / `asset_fetch` search and fetch Poly Haven models and skies, ambientCG materials, and Kenney packs through `src/online.rs`, with credits in `assets/CREDITS.txt`. Still open:
- Poly Haven texture sets (ambientCG covers materials) and resolutions other than 1k.
- Normal and roughness maps (see Rendering gaps); ambientCG materials use only their color map.
- Kenney pack cards have no picture (the pack list is hand-kept); the models inside a pack do.
- Maybe Sketchfab: it needs a user token, and each model has its own license.

### Other platforms
The blockers:
- libc calls in `editor/src/util.j3` (getenv, strlen, access, unlink, mkdir, open, lseek, read, close);
- the `.exe`-less frontend path and the cache dir in `editor.rs`;
- the default font path;
- the GL library name;
- the zenity file picker.

### Editing gaps
- Props can be set only on a placed scene's own copies, not on scenes nested inside it from further out (their copy ids aren't stable keys).
- Renaming a prop updates open tabs that place the scene without an undo step of their own.

### Rendering gaps
- Only point lights (8), and they cast no shadows.
- Without URP, glass refraction is screen-space and mirrors show only the sky or sky box. URP hits use material colors, not textures.
- The alpha cutoff is fixed at 0.5.
- Noise has no simplex type, since simplex doesn't tile on a square texture.
- No normal or roughness maps: there are no tangents in the forward shader, so Poly Haven models look flat. The glTF importer warns when it drops them.

See [the detailed editor guide](j3c/editor/README.md) for controls, scripts, and import limits.
