# Industry

A first-person voxel automation game: mine copper and coal, drill the surface nodes, belt ore into furnaces, store or sell what comes out, and unlock chunks as you grow. Everything is generated from a seed; models are `.j3voxel` files.

```sh
j3c check                 # typecheck
j3c run                   # play (needs SDL2 and libGL)
j3c run tests             # every check; prints failures and exits non-zero on any
timeout 60s j3c run --frames 120 --screenshot shot.png --headless
```

## Controls

| Key | Action |
|---|---|
| WASD, Space, Shift | walk, jump, run |
| Mouse | look (click to capture, Escape to release) |
| 1–6, wheel | select a hotbar slot |
| Left click (hold) | mine ore with the pickaxe, remove blocks and structures |
| Right click | place the selected block or structure |
| R | rotate the selected structure's direction |
| Shift (while aiming at a belt) | stack a belt on top instead of extending the line |
| E | at a chunk boundary: unlock it. On a container or market: open it. On a drill: collect. On a furnace: add coal |
| F | on a furnace: add ore |
| Tab | inventory and build menu |

The pickaxe starts in slot 1. Craft structures in the build menu (one copper ore each) and they fill the next hotbar slots.

## Where to read next

- [docs/mechanics.md](docs/mechanics.md): the full rules (ore, drills, belts, furnaces, storage, market, limits).
- [docs/architecture.md](docs/architecture.md): how the code is organized, and how to add an ore, item, or structure.
- [tests/README.md](tests/README.md): what the checks cover.
- [benchmarks/README.md](benchmarks/README.md) and [benchmarks/PERFORMANCE.md](benchmarks/PERFORMANCE.md): FPS benchmarks and results.
- `assets/models/*.j3voxel`: all 13 models on a fixed 24 × 32 × 24 canvas at 1/8-voxel resolution; file `(12, 0, 12)` is local `(0, 0, 0)`. Keep the canvas and origin when editing; cropping shifts ports and moving-part pivots. Edit with `../../tools/voxel-editor/vox assets/models/NAME.j3voxel`. Models load at startup; a missing or invalid file stops startup with its path. Ship `assets/` alongside the executable and run from the project root.
- `plans/`: handoff notes.

## Untested features

New features are added here until a person has played them. Remove an entry once it has been tested.

- Cave system: connected tunnels, branches, chambers, and outer-ring surface entrances.
- Underground ore veins: finite 1–3 block veins, mined with the pickaxe for 1–6 ore.
- Chunk unlock cost: coins charged per chunk, rising with chunks open and +50% per ring from the start.
- Electric system: coal generator, electric post, copper wire (crafted from ingots in the POWER tab), electric drill and electric smelter; wire tool (hold wire, right click two machines; click a wired pair again to take it back); brownout when demand exceeds supply.
- Iron: ore nodes and underground veins from the second chunk ring out; smelts to iron ingots in either smelter.
- Conveyor Mk.2 (3x speed, iron ingots), splitter (round-robin over the three sides it was not fed from), and lifter (moves cargo up one block onto the belt above; aim a belt at it to stack on top), each in normal and Mk.2.
- Debug console (F3): type a command, Tab completes the highlighted suggestion (Up/Down pick), Enter runs it, Esc or F3 closes. Commands: `help`, `add [item or coin] [amount]`, `unlock chunk-ring` (opens the next ring one chunk at a time, 10ms apart).
- Noise-field world: layered hills, winding noise caves (tunnels, caverns, thin links), sinkhole shafts on the outer ring, drifting ore nodes. Cave look and feel have only been checked from slices and tests, not played.
- Chunk unlock in slices: buying a chunk (E) or `unlock chunk-ring` generates it a few ms per frame (terrain, caves, veins, nodes), and it unlocks when done. Cave noise is sampled on a 4-voxel lattice, so caves are slightly smoother than before. Only one chunk generates at a time.
- Faster meshing and cave generation: plane masks use int walks, section bounds come from one pass, cave richness is looked up once per chunk. Meshes should look identical; mining and chunk unlock should feel less laggy.
