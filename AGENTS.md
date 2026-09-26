# J3

AOT game language. Spec wins over examples; examples win over compiler comments.

| Path | What |
|------|------|
| `j3c/AGENTS.md` | Language + compiler contract (there is no `speculation.md`) |
| `roadmap.md` | Path to a language/framework that can ship full games |
| `speculative/` | `01`–`08` compile, `09` typechecks shipped `j3.game`, `10_j3ui.j3` is the `@j3ui` sample |
| `j3c/` | Compiler crate — **map: `j3c/AGENTS.md`**. Fresh project: `j3c new`, `j3c guide`, `j3c doc`, `j3c check` |
| `editors/vscode/` | Cursor/VS Code: highlighting (`j3c dump-syntax`), `j3c lsp` (completion, imports, signature, outline), `j3.toml` keys, Run/Build/Format |
| `examples/bounce/` | Sample arcade: `App<World>` + paddle/ball (`j3.toml` + `src/`). Needs SDL2. |
| `examples/dungeon/` | ECS crawler: stages, scenes, assets cache, tile format, UI, save, particles, looping `theme.wav`. SDL2 + image/mixer/ttf. |
| `examples/island/` | Voxel island: GL 3.3 mesh + shadow map + orbit camera + a second transformed lantern (`j3.toml` + `src/`). Needs SDL2 + libGL. |
| `examples/minecraft/` | Block world: first person, break/place, jump, corner shadows (`j3.toml` + `src/`). Needs SDL2 + libGL. |
| `examples/leaves/` | Leaf cleaning: first person, walk the yard, click leaves, spend them on grab speed and reach, clear the lot for a time (`j3.toml` + `src/` + `assets/`). Needs SDL2 + image/mixer/ttf + libGL. |
| `examples/defense/` | Tiny tower defense: drag/zoom map, waves, towers (`j3.toml` + `src/` with `world.j3` and `hud.j3`). Needs SDL2 + image/mixer/ttf. |
| `examples/gallery/` | UI gallery: widgets, controls, layout, style, inventory (`j3.toml` + `src/`). Needs SDL2 + image/mixer/ttf. |
| `examples/canvas/` | Headless `j3.gfx.canvas` asset generator (sprite and tile PNGs); `examples/canvas_preview/` displays them with SDL2. |
| `examples/raytrace/` | URP room raytracer with glass, metal, plastic, and a settings HUD. Needs SDL2 + ttf + libGL. |
| `examples/ocean/` | Procedural wave height field raytraced with floating glass and metal spheres; shader in `assets/ocean.frag`. Needs SDL2 + ttf + libGL. |
| `examples/forest/` | Asset-backed autumn trees, shadowed volumetric sunshafts, glass/metal spheres, and a URP composite pass. Needs SDL2 + image/ttf + libGL. |
| `j3c/stdlib/j3/` | Stdlib in J3. Top level: `sys`, `io`, `math`, `rand`, `memory`, `option`, `result`, `text`, `app`, `game`, `gfx`, `ui`, `scene3d`. Folders: `game/` (`ecs`, `input`, `anim`, `fx`, `collide`, `space`, `physics`, `path`, `tile`), `gfx/` (`mesh`, `meshgen`, `voxel`, `bvh`, `canvas`, `noise`, `urp/`), `data/` (`save`, `assets`, `res`, `scene`, `debug`), `scene3d/` (`fpwalk` first-person walker; `editor/` scene editing only the editor does). A folder is part of the name: `use j3.game.ecs;`. `j3.sys` is `sys.j3` / `sys_windows.j3` / `sys_macos.j3` by `[emit] target`. SSOT; `j3c` parses `sys`+`io` for hello |
| `EDITOR.md` | Scene editor status and plans; **Agent use** covers `j3c -e mcp`, `j3c -e scene`, `j3c -e render`, `j3c -e gen` |
| `j3c/stdlib/j3/gfx/canvas.j3` | Headless RGBA8 canvas for procedural 2D art; PNG export and SDL/GL texture upload through `j3.game`. `j3c doc j3.gfx.canvas` is the API. |
