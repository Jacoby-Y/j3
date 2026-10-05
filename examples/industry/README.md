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
| E | at a chunk boundary: unlock it. On a container or market: open it. On a drill: collect. On a furnace or smelter: open its screen (ore, coal, ingots) |
| F | on a furnace or smelter: take its ingots |
| Tab | inventory and build menu |

The pickaxe starts in slot 1. Craft in the build menu (copper ingots, so smelt first, or craft ingots from raw ore in PRODUCTION) and structures fill the next hotbar slots.

## Where to read next

- [docs/mechanics.md](docs/mechanics.md): the full rules (ore, drills, belts, furnaces, storage, market, limits).
- [docs/architecture.md](docs/architecture.md): how the code is organized, and how to add an ore, item, or structure.
- [tests/README.md](tests/README.md): what the checks cover.
- [benchmarks/README.md](benchmarks/README.md) and [benchmarks/PERFORMANCE.md](benchmarks/PERFORMANCE.md): FPS benchmarks and results.
- `assets/models/*.j3voxel`: all 13 models on a fixed 24 × 32 × 24 canvas at 1/8-voxel resolution; file `(12, 0, 12)` is local `(0, 0, 0)`. Keep the canvas and origin when editing; cropping shifts ports and moving-part pivots. Edit with `../../tools/voxel-editor/vox assets/models/NAME.j3voxel`. Models load at startup; a missing or invalid file stops startup with its path. Ship `assets/` alongside the executable and run from the project root.
- `plans/`: handoff notes.

## Untested features

New features are added here until a person has played them. Remove an entry once it has been tested.

- QoL batch: a drill placed on the ground beside a node turns to face it (R still wins when it already faces one); a belt hands items to a belt offset by one voxel; walking eases in and out (`GROUND_EASE`/`AIR_EASE` in `src/player.j3`); resource nodes snap to the 1m grid horizontally; stone boulders on the surface, 3x3x3 with 1-4 top corners missing (always one in the starting chunk, `structures.boulder`); mining or breaking anything needs the pickaxe in hand; a splitter whose chosen exit fills up picks another; click (either button) a furnace, smelter or generator holding its ore or coal to load it (Shift: as much as fits). Tests only, not seen in play.
- QoL batch 2: chunk locks float 2 voxels above the highest ground under them; storage is 3 / 6 stack slots (no item cap, no cap text); clicking a structure with the other tier of it (conveyor, splitter, lifter, storage, drill, smelter) swaps it in place and returns the old one (wires on it are dropped); wiring uses left click, shows a preview line to your aim (or ahead, capped at reach), wires are thicker and reach 12 voxels
- Crafting takes time (0.5 s conveyor/splitter/lifter, 2 s the rest) and chains: click recipes to queue them (Shift: 5, Ctrl: all you can afford now, right click: cancel one and refund it). A tile shows a fill bar for the craft in progress and the number queued; queueing takes the materials immediately. Copper and iron ingots craft by hand in PRODUCTION (1 raw ore each, 2 s).
- Inventory cursor and world drops: click a stack to pick it up (right click: half), click a slot to put it down (right click: one, a different item swaps); shift-click sends a stack between hotbar and grid, or between your inventory and the open structure or furnace. Closing the screen or clicking outside it throws what you hold; Q throws one hotbar item (Ctrl+Q the stack). Thrown items (`src/drops.j3`) fall as small spinning cubes in the item's color and are picked up by walking within 1.5 m. An item can now fill several slots, so right click then put down splits a stack (stacks hold 99; the inventory is limited only by its 36 slots). Breaking a block or structure drops its item on the ground instead of adding it to the bag, so a full bag no longer blocks breaking (ore from the pickaxe still goes straight in). Tests only, not seen in play.
- Furnace screen: E on a furnace or electric smelter opens ore, coal and ingot stores beside your inventory (click an inventory item to add it, click a store to take it; Shift or right click moves all); F takes the ingots. Ingots wait in a ten-slot output store when no belt takes them. Storage and market screens now use the same layout (inventory left, structure panel right). Checked by tests only, not seen in play.
- Voxel particles (`src/fx/`): mining chips while you hold the mouse on a block, debris when it breaks, smoke and embers from running furnaces and generators, sparks from the electric smelter, dust from drills; console `fx [dust|debris|smoke|spark|ember|steam]` bursts one where you aim. Checked by tests only; where the emitters sit on each model (`emitters()` in `content.j3`) is a guess from the model height, and the look and cost with many machines have not been seen. Translucent particles (`alpha`, `fade` on an effect row, blended draw per opacity bucket) exist and smoke and wisp (furnace, generator and drill plumes) use them; the blending has not been seen in game.
- Cave system: connected tunnels, branches, chambers, and outer-ring surface entrances.
- Underground ore veins: finite 1–3 block veins, mined with the pickaxe for 1–6 ore.
- Chunk unlock cost: coins charged per chunk, rising with chunks open and +50% per ring from the start.
- Chunk locks: a voxel lock sits mid-wall on every open chunk edge that faces a buyable chunk; look at it and press E to buy (replaces the walk-near prompt). Also new: random node chance/positions, multi-block mining progress, coyote jump and hold-to-jump. Tests only.
- Electric system: coal generator, electric post, copper wire (crafted from ingots in the POWER tab), electric drill and electric smelter; wire tool (hold wire, right click two machines; click a wired pair again to take it back); brownout when demand exceeds supply.
- Iron: ore nodes and underground veins from the second chunk ring out; smelts to iron ingots in either smelter.
- Conveyor Mk.2 (3x speed, iron ingots), splitter (round-robin over the three sides it was not fed from), and lifter (moves cargo up one block onto the belt above; aim a belt at it to stack on top), each in normal and Mk.2.
- Container Mk.2 (10 iron ingots, 6 stacks of 99 vs the container's 3) shares the container's art, tinted blue. Recipe rebalance from `structures.md`: stone and copper wire now appear as costs. Checked by tests only.
- Chain placement: aiming at a belt chain extends its end (up through lifters, down through lowerers; splitter ends it; no conveyor facing back). Tests only, not watched in play.
- Lowerer: press R on a held lifter to flip it between lifter and lowerer (moves cargo down onto the belt below). Tests only, not watched in play.
- Belt item movement rework: side entries get a wider gap, splitters pick an exit at their centre (skipping full or refusing ones) and no longer flip items mid-queue, lifters take items in from the edge, and lifted items enter the belt above at its centre (merging into gaps). Checked by tests only, not watched in play.
- Debug console (F3): type a command, Tab completes the highlighted suggestion (Up/Down pick), Enter runs it, Esc or F3 closes. Commands: `help`, `add [item or coin] [amount]`, `unlock chunk-ring` (opens the next ring one chunk at a time, 10ms apart).
- Noise-field world: layered hills, winding noise caves (tunnels, caverns, thin links), sinkhole shafts on the outer ring, drifting ore nodes. Cave look and feel have only been checked from slices and tests, not played.
- Chunk unlock on a worker thread: buying a chunk (E) or `unlock chunk-ring` generates it off the frame thread (terrain, caves, veins), and it unlocks a frame or so after the worker finishes (nodes are placed then). Cave noise is sampled on a 4-voxel lattice, so caves are slightly smoother than before. Only one chunk generates at a time.
- Threaded section meshing: dirty sections are copied, meshed on worker threads (`src/mesher.j3`), and uploaded a couple per frame, nearest to the camera first. A mined block now updates a frame or two late instead of stalling the frame; check that edits never flicker, leave holes, or show stale geometry, and that frame time stays flat while a chunk loads.
- Faster meshing and cave generation: plane masks use int walks, section bounds come from one pass, cave richness is looked up once per chunk. Meshes should look identical; mining and chunk unlock should feel less laggy.
