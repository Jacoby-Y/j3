# Plan: make structures, items and ores cheap to add

**Status: done.** What changed from the plan below is listed here; the plan text is kept as written.

- Phase 0 findings (from `j3c doc lang` and trying it): `const` holds only int/float/bool/string and struct field defaults must be literals, so definitions are `#[singleton]` arrays filled by `content.load()` and read through accessors (singletons are module-private, and a field of a call result is not a place, so accessors return a struct copy to a local). The compiler resolves modules from the entry file's `src/` root with no second root, so the checks and the benchmark live in `src/checks/` and `src/bench/` (own module paths, never imported by the game) instead of `tests/src/`.
- Phase 3: instead of dense per-kind lists with a `kindOf` lookup, every machine owns one slot and each kind has its own typed table indexed by slot (`w.belts`, `w.drills`, `w.kilns`); update loops walk slots in order, so order is stable and removals never reorder other machines. The anchor-to-slot hash stays for random access. `World` embeds `Machines` and `RenderCache` structs with `use`.
- Phase 4: the scripts keep the per-step loops with a `period` prop; the fixed step is `gameplay.simulate` (60 Hz, 120 catch-up steps).
- Phase 7: placement refuses with "MACHINE LIMIT REACHED". The 160k-face scratch buffer and the per-voxel `facing`/`part` memory were left as they are (measured startup and frame time improved; revisit if memory matters).
- Phase 1 gate: a throwaway third ore (iron) needed two id constants per kind, two model files, and table rows only. Reverted; `docs/architecture.md` has the recipe.
- Docs: `README.md`, `docs/mechanics.md`, `docs/architecture.md`.

Covers audit points 3-15. Not covered: git hygiene, save/load.

**Goal.** Adding a new structure (e.g. assembler) or ore (e.g. iron) is one definition entry plus one behavior file. It does not touch `world.j3`, `models.j3`, `transport.j3`, `gameplay.j3` or `item.j3` by hand.

**Language limits** (from `j3c doc lang`): `const` is only int, float, bool or string. There are no const struct tables, no traits, no closures. So definitions are **runtime tables built once at startup** (arrays filled by `defs.register*`), and per-kind behavior is a single `match` on a kind enum in one dispatch file. Phase 0 confirms what is available.

## Phase 0: spike (small, do first)
- Check what `j3c doc lang` leaves open: can an `enum` carry explicit int values and cast to `u8`? Are there function-pointer types? Can a struct hold `[]` of structs allocated from an Arena (already done in `World`, so yes)?
- Check that the benchmark and test projects can import modules from a sibling dir (decides the layout in Phase 6).
- Write the result at the top of this file. If function pointers exist, dispatch tables replace the `match` in Phase 3.

**Gate:** `j3c check` and `j3c run tests` pass on the unmodified tree, and a benchmark baseline is recorded (`python3 benchmarks/run.py baseline`). Every later phase must keep both green and may not regress the factory case more than about 5%.

## Phase 1: one definition table (points 3, 5, 10, 11, 12, 13)

New `src/data/defs.j3`:

- `ItemDef`: id, name, price (0 = unsellable), `block` it places (or 0), recipe cost, hotbar slot, icon source.
- `BlockDef`: id, color, `solid`, is-machine, size (x, z, height), model id, batch group, facing-sensitive.
- `OreDef`: surface-node block, vein block, item yielded, node model, cargo model, colour, mining time, spawn offset in chunk. Copper and coal become rows, not parallel code paths.
- `Defs` holds arrays indexed by id. `defs.register*` calls in a single `defs.load()` at boot (and in the test fixture).
- `defs.name(id)`, `defs.color(id)`, `defs.price(id)` and friends **panic on an unknown id**. This removes the silent "EMPTY" and magenta fallbacks (point 13).

Replace the if-chains with lookups: `item.name/blockFor/hotbarItem/recipe`, `block.color`, `market.price`, `world.height/storage/machine`, `models.machine/ghost` and the `drawWorld` group mapping.

Kill the magic numbers (point 10):
- Derive `KINDS`, `HOTBAR`, recipe count and the batch-group count from the table. Item ids become contiguous and stable, with 0 = none.
- `batchOffset` and the `MAX_MACHINES * 19` allocation are computed from the number of batch groups.
- `QUEUE = 4` becomes one named constant used by transport and models.
- Barrier bands, `sectionChunk /2` and `SECTION_X` used for Z get named constants (`SECTIONS_PER_CHUNK`, `BARRIER_BANDS`, `SECTION_Z`).

Rename (point 12): `block.COPPER` to `COPPER_NODE`, `block.COAL` to `COAL_NODE`, `COPPER_ORE` to `COPPER_VEIN`, `COAL_ORE` to `COAL_VEIN`. Mechanical, done in one commit with the tests as the safety net. `world.ore` becomes `isNode`, `finiteOre` becomes `isVein`.

Asset loading: `models.create` loops over the model list in `defs` (name, path, flags such as glow) and fills `Models` by model id. The 13 hand-named loaders and the 13-field `Models` struct go away. `Game`'s hand-initialised `Texture {...}` fields move into an `Icons` struct created by one function.

**Done when:** adding a throwaway ore (e.g. iron) to `defs.load()` plus a `.j3voxel` file yields a node, a vein, an item, a sell price and cargo drawing with no other source edit. Revert the throwaway after the test. This also proves point 11.

## Phase 2: shared geometry helpers (point 9)

New `src/footprint.j3` with the only copies of:
- `ahead(w, anchor, dir, n)`: the cell n steps along a direction (replaces `ports.receiver`'s math, `transport.neighbor`, `furnace.port` and `machines.rear`, which then become thin callers).
- `partOffset(part)` and `anchorOf(x, y, z, part)`: the only place `% 2`, `/ 2 % 2`, `/ 4` appear (`world.anchorAt`, `breakAt`).
- `resetState(w, slot)`: the one place machine state is zeroed (`placeMachine`, `breakAt`).

Pure refactor, covered by the existing tests.

## Phase 3: per-kind machine state and dispatch (points 3, 4 of the audit's high-priority list, plus 9)

Split `World` into **grid** and **machines**:
- `Grid`: `cells`, `facing`, `part`, unlocked, dirty flags, seed.
- `Render cache`: section and chunk meshes, LOD state, scratch (a `TerrainCache` struct owned by `render`, not `World`).
- `Machines`: dense per-kind tables. Each kind has its own struct of arrays, so `belt.queueLen/cargo/entry/progress`, `drill.buffer/item/clock`, `furnace.stock/stockKind/fuel/burn/item/clock`, `store.slot`. No column is shared across kinds. A machine's anchor cell stores its dense index in its own kind's table, found by a small `kindOf(anchor) -> (kind, index)` lookup. This removes the hash and the repeated `world.state` calls. A swap-remove keeps each list dense.
- Machine order: a per-kind list means removing one machine can only reorder machines of that kind. Process belts in a stable order (sort the belt list by anchor index when it changes, not every frame) so transfers are deterministic (point 15).

Behavior API, one file per kind (`belt.j3`, `drill.j3`, `furnace.j3`, `store.j3`), each exposing the same four functions: `place`, `remove`, `update(dt)`, `accepts(item)/take(item)`. One `dispatch.j3` has the only `match kind` in the program. `transport.update`'s chain of `if !transferred { ... take }` becomes `dispatch.take(kind, target, item)` on whatever the belt faces. A new structure is a new behavior file plus one dispatch arm.

**Phase 3 is the largest and riskiest.** Do it one kind at a time, belt first (it has the most code touching it), running tests after each kind. Keep the old arrays until the last kind moves, then delete them.

## Phase 4: scripts and the update loop (points 5, 14)

- Make `drill.j3s` and `furnace.j3s` call the Phase 3 behavior API only. Expose real `props` (`period`, `capacity`, `smelts_per_coal`) so tuning stays in scripts and hot reload survives `World` changes. If a script cannot yet call the typed API, move that logic into `.j3` and keep only tunables in the script.
- Replace the variable `dt` with a **fixed-step accumulator** (60 Hz) for machine simulation. Player and rendering keep the variable `dt`. Cap catch-up steps (e.g. 5) so a hitch cannot spiral. The benchmark and production then run the same step.
- Drills and furnaces that finish more than one batch inside a step are covered by the fixed step. A test checks that a 1 s hitch yields the same output as 60 steps.

## Phase 5: gameplay input (point 6 of the structure notes) and frame cost

- Split `gameplay.update` into `interact.j3` (what E, F and clicks do), `hotbar.j3` and `session.j3` (inventory open/close). `interact.j3` owns one ordered list of interactions, each `matches(hit, held)` + `run`. E no longer lives in a nested if/else chain; a new structure can register its own interaction.
- Compute `pick` and the placement plan once per frame in `update` and store them in `Game`; `draw` reads them.
- Merge the seven `Texture` fields into the `Icons` struct from Phase 1.

## Phase 6: tests and benchmarks (points 6-8 in the audit)

- Move check files out of `src/` into `tests/src/` (if Phase 0 shows multi-root imports work; otherwise a `checks/` subfolder under `src/` with a distinct module path `industry.checks.*`, not `module industry;`).
- Split `automation_checks.j3` (810 lines) by structure: `belt_checks`, `drill_checks`, `furnace_checks`, `store_checks`, `market_checks`, `placement_checks`, `world_checks`. Add a tiny runner in `checks.j3`: each check is a named function, the runner prints `ok`/`FAIL <name>` for all and exits non-zero if any failed. This replaces first-panic-wins.
- Add checks for the new contracts: unknown id panics, every defined item has a name, every placeable block has a model, every ore has a node and cargo model, adding the throwaway ore works (Phase 1 gate), and the fixed-step equivalence (Phase 4).
- Replace the string patching in `benchmarks/run.py --profile-stages` with a real optional timing hook in `gameplay` (a `Stats` struct filled when `J3_PROFILE` is set, read by `benchmark.j3`). Delete the patcher and its asserts.
- Benchmarks and release: add a release-build mode to `run.py` (`j3c build --release`) and report opt 0 vs release separately.

## Phase 7: capacity limits (scalability)

- Placement checks capacity before it mutates: machines, stores and face budget each have a `can*` check and a UI message ("MACHINE LIMIT REACHED"). `allocateState`/`shadowVertices` panics stay as internal asserts only.
- Face budget: size `scratch` per section instead of one 160k-face buffer shared by chunk-level shadow merges, or split the shadow merge into per-chunk passes. Measure first; only do this if the Phase 0 baseline shows the cap is near.
- `facing`/`part` (2 of the 3 bytes per voxel): store only for machine cells in a small hash or per-machine record once Phase 3 lands. About 49 MB saved. Do this last and only if memory matters; it changes every place that reads `facing`.
- Mesh rebuild stays synchronous. A worker-thread rebuild is out of scope until the profile says it matters.

## Phase 8: docs (point 6 of the structure notes)

- Fix README: hotbar keys (1 = pickaxe, 2 = conveyor, 3 = drill, 4 = furnace, 5 = container, 6 = market) and the furnace needing coal.
- Split README into `README.md` (run, controls), `docs/mechanics.md`, `docs/architecture.md`, with `PERFORMANCE.md` for numbers. `docs/architecture.md` gets a "How to add a structure" and a "How to add an ore" recipe (3-5 steps each, written from the Phase 1 and 3 results).
- Update `AGENTS.md` for this project with the new layout, and mark each file in `plans/` done or open.

## Order and sizing

| Phase | Risk | Notes |
|---|---|---|
| 0 spike | none | Decides Phases 3 and 6 details |
| 1 definition table | medium | Biggest payoff, mostly mechanical |
| 2 helpers | low | Quick, de-risks Phase 3 |
| 3 state + dispatch | **high** | One kind per step, tests after each |
| 4 scripts, fixed step | medium | Needs Phase 3 API |
| 5 gameplay split | low | Independent, can go anytime after Phase 1 |
| 6 tests/bench | low | Start the runner early so Phases 1-5 get named failures |
| 7 capacity | low-med | Measure before doing |
| 8 docs | none | After 1-3 so the recipes are real |

Do Phase 6's runner (not the moves) right after Phase 0. It is cheap and makes every later phase easier to verify.

## Risks and open questions
- Phase 3 could slow the factory benchmark if the dense lookup is worse than expected. The Phase 0 baseline is the check.
- Table-driven dispatch needs a `match` on kind (no traits). If that grows past about 6 kinds, revisit once Phase 0 shows what function pointers allow.
- Renames in Phase 1 touch the 810-line check file. Do it in the same commit as the code change so tests never run against a half-renamed tree.
- Not verified: I haven't read `ui.j3`, `inventory_ui.j3`, `terrain_mesh.j3` or `gen/*` in full. Phase 1 may find more hard-coded ids there.
