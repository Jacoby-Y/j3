Run from the industry project root:

```sh
j3c check
j3c run tests
timeout 60s j3c run --frames 120 --screenshot shot.png --headless
```

The test project shares the real source and scripts through symlinks; `assets` also points to the real model files. The checks live in `src/checks/` (one file per area, `main.j3` runs them all). A failed check prints `FAIL <suite>: <message>`, the run continues, and the exit status is non-zero if anything failed. Performance numbers are in `benchmarks/PERFORMANCE.md`.

Table checks verify the definition tables are complete and consistent: every item named and priced, every machine with a model, every ore with a node, vein and cargo model, recipes within the hotbar. Simulation checks verify fixed steps give the same result however frames are sliced. Capacity checks fill the machine table and verify placement refuses with a reason. Checks cover lazy adjacent chunk unlocking, whole-player bounds, node generation, all four drill directions, four-second output, required rear connection, blocked output, belt spacing, steady movement, item-preserving handoff, machine removal/replacement, and the actual player placement flow from aiming at copper through snapped output connection and mining.

Furnace checks cover the 2 × 2 × 4 occupied volume, upper-part removal, all four port directions, matching belt heights, rejection of non-port sides, two-second smelting, blocked output, and copper-to-ingot item conservation. Placement checks keep the same selected direction across changes in camera yaw and player position.

Model checks validate every vertex and face normal in all 13 file-backed assets, all cardinal rotations, and all drill animation poses against the exact 1/8-voxel grid. Shared internal subvoxel faces are culled, the bit clears its housing, intakes stay recessed, and furnace fire remains emissive. Asset checks cover optional palette names, uppercase hex, palette index 255, runs, missing/empty files, canvas dimensions, invalid palette references, malformed records, and out-of-bounds runs. Model-backed item icons must be readable and fit their atlas cells; the pickaxe icon matches its loaded silhouette.

Container checks cover the 2 × 2 × 3 occupied volume, all four simultaneous intakes in every container rotation, wrong-height rejection, raw copper and ingot intake, mixed-item capacity, full-storage back-pressure and recovery, bounded take/deposit transfers, independent inventories, protected nonempty removal, empty removal/replacement, and free slot-four placement. The real drill-to-furnace pipeline also deposits ingots in storage without losing or duplicating items.

Inventory grid checks cover occupied-stack ordering, empty cells, single-item transfers, whole-stack transfers, and removal of exhausted stacks without stale cells. Furnace intake remains directional.

Market checks cover slot-six placement, the complete footprint, withdrawals before sale, close settlement through gameplay, repeated and empty closes, preserved ordinary containers, protected nonempty removal, replacement storage, mixed-stack prices, all four conveyor intakes in every rotation, wrong-height rejection, invalid ids, and immediate automatic sales while the manual queue is full.

Placement checks cover the 0.5 m grid for all five structures, conveyor endpoint previews from the start/middle/tail through turns, selected rotation, Shift stacking on the aimed belt, blocked endpoints, successive extensions, and closed-loop rejection.

Build menu checks cover copper costs for all five recipes, the initial pickaxe, stable inventory positions and slot-three selection, placement consumption and recovery, grass/dirt/stone replacement, full-inventory mining refusal, and unsellable tools/structures remaining in a market queue.

World generation checks cover all ring caps and smooth height seams over multiple seeds, the extra depth and protected floor, rare outer-ring entrances, connected cave seams with four-block clearance, 48 separated veins per chunk, seed reproducibility and unlock-order independence, and chunk mesh invalidation. Finite ore checks exercise the real pickaxe inventory path: 1–6 yields, whole-drop capacity, removal exactly once, existing resource item mapping, and drill exclusion. Surface nodes remain inexhaustible. Compact machine state is released and cleared on reuse; the existing automation scenarios use a small synthetic terrain fixture so their placement and timing checks do not depend on random world generation.
