# Architecture

Read [mechanics.md](mechanics.md) for what the game does. This page is how the code is laid out and what to touch when you add something.

## Layout

| Path | Job |
|---|---|
| `src/main.j3` | Entry: creates the app and wires `session`, `gameplay`. |
| `src/data/` | Ids and the definition tables. `defs.j3` holds the tables and accessors, `content.j3` registers every row, `block.j3`, `item.j3`, `conveyor.j3` hold ids and small helpers. |
| `src/world.j3` | The voxel grid, machine placement and removal, machine slots, section dirty marking. `World` embeds `Machines` and `RenderCache`. |
| `src/machine/` | One module per machine kind (`belts` (belts, splitters and lifters, by `route`/`speed` on the block row), `drills`, `furnace`, `container`, `market`), their shared `state.j3` tables, `ports.j3` (what a belt end faces), and `dispatch.j3` (the only place that switches on machine kind). |
| `src/gen/` | World generation: hills (`elevation`), noise-field caves (`caves`, built on `noise3`), veins, surface nodes. |
| `src/session.j3` | `Game` (everything one run owns), boot, startup, opening and closing screens. |
| `src/gameplay.j3` | The frame: `simulate` (fixed-step machines), `update`, `draw`. |
| `src/interact.j3` | What keys and the mouse do to the aimed block. |
| `src/placement.j3` | Resolves where a structure would go and why not. |
| `src/render.j3`, `render_cache.j3`, `terrain_mesh.j3`, `terrain_lod.j3`, `visibility.j3`, `lighting.j3`, `models.j3` | Rendering: section meshes, LOD, culling, instancing, models. |
| `src/ui.j3`, `inventory_ui.j3`, `item_icons.j3`, `model_icons.j3`, `bitmap.j3` | HUD, inventory screens, icons. |
| `src/console.j3`, `commands.j3` | The F3 debug console: line editing, suggestions and drawing; and the command table (`help`, `add`, `unlock`) that runs and completes them. A new command is a row in the tables in `commands.j3` plus its function. |
| `src/subvoxels.j3`, `j3voxel.j3`, `model_mesh.j3` | Loading and meshing `.j3voxel` models. |
| `scripts/drill.j3s`, `furnace.j3s`, `power.j3s` | Tunable machine rules (props for period, speed, watts) that run every simulation step. |
| `src/machine/power.j3`, `generator.j3`, `src/wiring.j3`, `src/wire_draw.j3` | Wires and networks, the coal generator, the wire tool, and drawing wires. |
| `src/checks/` | The checks `j3c run tests` runs (never imported by the game). |
| `src/bench/` | The benchmark harness `benchmarks/` runs. |

## Definition tables

`data/content.j3` is the one place that says what exists: models, blocks, items, ores. Ids are small integers declared in `block.j3` and `item.j3` (and model ids at the top of `content.j3`); ids are contiguous from zero and registered in id order. Everything else asks `defs`: `item.name`, `item.price`, `block.color`, `world.height`, `models.drawWorld`, the inventory icons, the build menu and the generators all read the tables, and an unknown id panics rather than guessing.

`world.createSeed` calls `content.load()`, so any entry point (game, checks, benchmark) has the tables.

## Machine state

Each placed machine owns a **slot**. `world.slotOf(w, anchor)` finds a machine's slot, `world.slotAnchor(w, slot)` goes back. `w.slotKind[slot]` is its block id and `w.slotHigh` bounds loops:

```
for slot in 1..w.slotHigh + 1 {
    if w.slotKind[slot] != block.FURNACE { continue; }
    ...
}
```

State lives in one typed table per kind, indexed by slot: `w.belts[slot]`, `w.drills[slot]`, `w.kilns[slot]`, and storage in `w.stored` / `w.storedSlots`. Only the table of a machine's own kind means anything; a slot is zeroed when its machine is placed or removed. Slots are reused lowest-first, so update order is stable.

Cell data (`cells`, `facing`, `part`) says what each voxel is; a machine's cells all carry its block id and a part offset from the anchor (low X, low Z, bottom).

## The frame

`update`: `simulate(dt)` runs machines in fixed 1/60 s steps (power, furnace, belts, then drills, so a step processes existing batches before belts deliver new input; at most 120 catch-up steps after a hitch) → input → `aim` (one raycast and one placement plan stored on `Game`) → `interact` keys and mouse. `draw`: sections and shadow, instanced models, hover and ghost, HUD.

## Limits

`MAX_MACHINES` 8192, `MAX_STORES` 128, `MAX_FACES` 160,000 faces in the shared mesh buffer. Placement refuses past the first two with a message. The mesh buffer is an assertion: a single section or the combined shadow mesh of one chunk would have to be adversarially complex to reach it.

## How to add an ore

Example: iron.

1. `data/block.j3`: `IRON_NODE` and `IRON_ORE` ids (next free numbers). `data/item.j3`: `IRON` (next free item id).
2. `data/content.j3`: two model ids (`M_IRON_NODE`, `M_IRON_CARGO`) and `model(...)` rows; a `terrain(...)` row for each block; an `items()` row (name, price, `cargo` model, icon file); an `ores()` row with the node x offset in its chunk.
3. `assets/models/`: the node model (2 × 2 × 2 voxel cluster) and the cargo model, 24 × 32 × 24 canvas.

That is all. Surface nodes, veins (vein number modulo the ore count picks the ore), drill mining, belt cargo drawing, the HUD name, icons, sale price and mining feedback all follow from the rows. `checks/table.j3` verifies the rows are consistent.

## How to add a plain item or block

Declare the id, add a row in `content.j3` (`items()` and, for a placeable block, `blocks()`). `ItemDef` fields: `price`, `block` (what placing it makes), `cost` and `hotbar`/`tab` (craftable), `cargo` (drawn on belts), `smelts`, `fuel`, `icon` (model file) or cube colors.

## How to add a structure

Example: an assembler.

1. Ids and rows as above: block id, item id, a model (and an overlay model if it animates), `machine(...)` in `blocks()`, an `items()` row with `cost`, `hotbar` and `tab`.
2. State: a `FooState` struct and a `foos` table in `machine/state.j3`; allocate it in `state.create` and zero it in `world.resetSlot`.
3. Rules: `machine/foo.j3` holds the logic (intake, what it holds, ports).
4. `machine/dispatch.j3`: add an arm to `deliver` if belts can feed it, `running` if it animates, `interact` if keys act on it, and `describe` for its HUD text. These are the only switches on machine kind.
5. Simulation: add a stage to `gameplay.simulate` (`STAGES`, `runStage`), or a script in `scripts/` if the rules are tunable.
6. A suite in `src/checks/` and a line in `checks/main.j3`.

## Checks and benchmarks

`j3c run tests` runs `src/checks/main.j3`: each suite names itself, a failed check prints `FAIL <suite>: <message>` and the run continues; the exit status is non-zero if anything failed. `python3 benchmarks/run.py <label>` runs identical-setting benchmarks (see `benchmarks/README.md`); `--release` builds optimized, `--profile-stages` adds per-stage machine timing.
