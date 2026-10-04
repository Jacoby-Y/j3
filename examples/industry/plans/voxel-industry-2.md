# voxel industry 2

Folder: `/home/jacoby/Documents/code/ai-stuff/j3/examples/industry`

Build the next automation milestone: expand the current single-biome voxel world through explicitly unlocked chunks, and establish a working copper-to-conveyor loop with a resource node and crude drill. Defer refining, storage, and selling for now.

1. Use the existing 48 × 16 × 48 grid as the initial chunk; add neighboring chunks that generate only when unlocked. Keep the player within the active chunk, show a subtle dark-to-transparent gradient at its boundary, and display a nearby “Press E to unlock” prompt that unlocks the adjacent chunk for free.
2. Generate one copper resource node per chunk, starting with a 2 × 2 × 2-voxel node in the initial chunk. Render it as a stone base with rough copper pieces jutting out.
3. Add a placeable, horizontally directed crude drill. It mines a neighboring node and produces one discrete resource item (depending on ore it's facing toward) every four seconds; only output when a conveyor immediately behind its output can accept the item.
4. Give conveyors logical item positions and steady movement, with spacing and back-pressure when blocked; hand items off to another conveyor when it can accept them. Add focused checks for chunk unlock/bounds, drill output timing and connection requirements, and conveyor spacing and handoff.

---

Drafted from a voice note by a smaller model. Take it with a grain of salt. A later agent should ask the user questions before treating anything ambiguous as decided.
