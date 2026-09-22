# J3

AOT game language. Spec wins over examples; examples win over compiler comments.

| Path | What |
|------|------|
| `j3c/AGENTS.md` | Language + compiler contract (there is no `speculation.md`) |
| `roadmap.md` | Path to a language/framework that can ship full games |
| `speculative/` | `01`–`09` example programs (acceptance tests; `01`–`08` compile, `09` typechecks shipped `j3.game`) |
| `j3c/` | Compiler crate — **map: `j3c/AGENTS.md`**. Fresh project: `j3c new`, `j3c guide`, `j3c doc`, `j3c check` |
| `editors/vscode/` | Cursor/VS Code: highlighting (`j3c dump-syntax`), `j3c lsp` (completion, imports, signature, outline), `j3.toml` keys, Run/Build/Format |
| `examples/bounce/` | Sample arcade: `App<World>` + paddle/ball (`j3.toml` + `src/`). Needs SDL2. |
| `examples/dungeon/` | ECS crawler: stages, scenes, assets cache, tile format, UI, save, particles, looping `theme.wav`. SDL2 + image/mixer/ttf. |
| `examples/island/` | Voxel island: GL 3.3 mesh + shadow map + orbit camera + a second transformed lantern (`j3.toml` + `src/`). Needs SDL2 + libGL. |
| `examples/minecraft/` | Block world: first person, break/place, jump, corner shadows (`j3.toml` + `src/`). Needs SDL2 + libGL. |
| `examples/leaves/` | Leaf cleaning: first person, walk the yard, click leaves, spend them on grab speed and reach, clear the lot for a time (`j3.toml` + `src/` + `assets/`). Needs SDL2 + image/mixer/ttf + libGL. |
| `j3c/stdlib/j3/` | Stdlib in J3 (`sys`, `io`, `math`, `rand`, `memory`, `option`, `result`, `text`, `game`, `ecs`, `app`, `collide`, `res`, `scene`, `assets`, `tile`, `ui`, `save`, `fx`, `debug`, `gfx`, `mesh`, `voxel`). `j3.sys` is `sys.j3` / `sys_windows.j3` / `sys_macos.j3` by `[emit] target`. SSOT; `j3c` parses `sys`+`io` for hello |
