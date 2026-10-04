# voxel industry 1: half-meter grid audit and nine-slot hotbar

Folder: `examples/industry`. Original draft came from a voice note; this version is checked against the code.

## Goal

Make the 0.5 m grid and the 2 × 2 × 1-cell conveyor (1 × 1 × 0.5 m) consistent everywhere, then replace the four-slot placeholder hotbar with nine block-only slots (only the conveyor for now) with a real conveyor icon.

## Audit result (what already works)

Cell space is the unit for world, physics and ray picking; meters only appear at the GPU boundary (`Mat4.scale(world.VOXEL)`, `lookCells`/`camera`, `cellLine`).

| Area | File | State |
|---|---|---|
| Grid constants (48 × 16 × 48 cells, `VOXEL = 0.5`) | `src/world.j3` | ok |
| Conveyor place/break (anchor = low-X/low-Z, `part` = `dx + dz*2`) | `src/world.j3` | ok; mining any part clears all four cells |
| Placement bounds, occupancy, player overlap per cell | `src/main.j3` `canPlace` | ok, but width is hard-coded by block id |
| Player body (half-extents in cells 0.6 × 1.7 → 0.6 × 1.7 m), speeds divided by `VOXEL` | `src/player.j3` | ok |
| Hover outline (2 × 1 × 2 cells for conveyor, 1 cell otherwise) | `src/render.j3` `hover` | ok |
| Preview mesh (2 × 2 cells, 1 cell tall) + direction arrow on the top face | `src/render.j3` | ok |

## Defects found

1. `main.draw` always draws `render.preview(g.preview, 22, 5, 21, EAST)`: a leftover test ghost floating in the world. Remove.
2. `heldBlock` falls through to foundation for any unknown slot, so an empty slot would place foundation. It must return air and `canPlace` must refuse air.
3. Hotbar has four slots wired by four `if`s in `update` and four in `heldBlock`; stone/dirt/foundation are placeholders, not conveyor-scope blocks.
4. Hotbar icons are flat colored squares; the conveyor slot is a square with a drawn arrow.

## Steps

1. Remove the stray preview call (defect 1).
2. Hotbar data: `item.HOTBAR = 9` and `item.hotbarItem(slot)` returns an item id (0 = empty). Slot 0 is the conveyor; 1–8 are empty. Stone/dirt/foundation leave the hotbar (terrain still uses their blocks). `heldBlock` maps item to block, air when empty; `canPlace` returns false for air.
3. Controls: keys 1–9 select a slot, and the mouse wheel cycles (wraps). Right click on an empty slot does nothing; left-click mining is unchanged.
4. Icon: the renderer has no render-to-texture for a 3D object (`gfx.grab` only copies the current camera pass into the glass-surface texture, not an exportable image). Use the documented programmatic-art path instead: draw an isometric 1 × 1 × 0.5 slab with `j3.gfx.canvas` (top lit, left/right darker, belt stripe, arrow) once at startup, upload with `game.textureFromCanvas`, draw it with `drawTextureAt` into the slot. Reused every frame; freed at shutdown.
5. Selection feedback: nine evenly spaced slots centered from `windowSize` (fluid layout), selected slot gets a bright border plus number labels dropped (no font dependency), empty slots drawn dim. The live rotation arrow stays on the conveyor slot so R is visible.
6. README controls updated (1–9, wheel).

## Verify

- `j3c check` passes.
- `j3c run --frames 120 --screenshot shot.png --headless` and look at the picture (hotbar, no floating ghost).
- The user tests feel in the GUI (placement at odd half cells, rotation, mining a slab, number keys, wheel); do not launch the windowed game to judge feel.

## Open questions (defaults chosen, change if wrong)

- Should stone/dirt/foundation return as placeable items later (e.g. from mining)? Default: no, hotbar is conveyor only.
- Should the 2 × 2 footprint center on the aimed cell instead of anchoring at its low corner? Default: keep the anchor rule, since the user asked only for "any half-meter cell".
