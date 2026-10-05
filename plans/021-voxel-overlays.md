# Plan 021: voxel overlays (pixel layers on rig parts)

**Status: DONE** (phases A1, A2, B, C, D). `src/overlay.j3`, `src/overlaypng.j3`; docs in `tools/voxel-editor/README.md` section Overlays; demo `models/blocky-armor.vox.txt`.

Voxel editor, `tools/voxel-editor/` (read its `README.md` section **Rigging** first; rigs are phase 1-4 of the earlier rigging work and are done).

## Goal

An **overlay** is a named pixel layer attached to one rig part: painted onto the part's surface (`grow 0`) or on a shell `grow` whole voxels out from it (`grow N`, armor or clothing). One face is one pixel, the same size as a voxel, so no sub-voxel geometry. It turns with its part. Transparent pixels draw nothing. Authored by commands (agents), by PNG (people), and in the window.

## Design decisions

- **Overlay** = `{name, part, grow 0..4, visible}`; up to 8; creation order is priority.
- **Pixel** (texel) = one face of one cell: `(x, y, z, face)` plus `rgb`. Opaque or absent (no alpha). Stored sparse, sorted by key, in one shared pool on the model (65536 texels) so undo stays a plain copy.
- **Shell cells** (derived, never stored) = Chebyshev dilation of the part's voxels by `grow` (N voxel layers all around, square corners), minus cells holding any model voxel (so a chestplate never swallows an arm), minus cells claimed by an earlier overlay. For `grow 0` the cells are the part's own voxels.
- **Exposed face**: face `f` of an overlay cell whose neighbor is neither a shell cell nor a model voxel. Only exposed faces take pixels (faces against the part or its neighbors are hidden anyway). This also covers the rim faces where a shell ends (a sleeve cuff).
- **Shell must fit in the grid** (`part bounds +- grow` inside `size`); `overlay new` / `overlay grow` error with the `size` needed. This keeps coordinates in-grid and lets picking and meshing reuse grid code.
- **Colors are plain rgb**, not palette indices, so a PNG can use any color and the palette is not polluted.
- **File** stores the *resolved* pixels, so the runtime needs no shell logic:
  ```
  ov armor Torso 1 visible        overlay: name, part, grow, visible|hidden
  ot armor 4 12 2 pz 00a8a8 3     pixel: overlay, cell x y z, face (px nx py ny pz nz), rrggbb, optional run along +x
  ```
  Unknown lines are ignored by older readers, so old files and tools keep working.
- **Voxel edits move pixels**: `shift`, `crop`, `flip`, `rotate` remap the cell and the face (like parts and pivots); `resize` leaves them; `mirror` does not touch overlays (a warning says so).
- **Changing `grow` or the part's shape** can strand pixels (no longer an exposed face). `overlay grow` clears the overlay's pixels (it says so); `rig check` warns about stranded ones and `overlay prune NAME` drops them.

## Phases

### A1. Data, file, commands (headless; `vox selftest`)

- `src/model.j3`: `Overlay` struct, `MAXOVERLAYS`, `Texel` (`key: int`, `rgb: int`), pool + counts on `Model`; add everything to `Snap` / `capture` / `restore`; remap in `shift` (so `crop`), `flip`, `rotate` next to the part-pivot remap.
- New `src/overlay.j3` (keep `model.j3` from growing): `find`, `add/remove`, `shell(m, id, out: []u8)` (separable dilation over `m.tmp`-style scratch), `exposed(m, id, x, y, z, face)`, `paint(m, id, x, y, z, face, rgb)`, `erase`, `prune`, `stats`, key pack/unpack (`x`,`y`,`z` 0..63, face 0..5).
- `src/format.j3`: write/read `ov` and `ot` lines (RLE along +x), reject unknown parts/overlays with a line number.
- `src/cmd.j3` (help text, `editing()`, `looking()`, `run`; docs in README + AGENTS.md):
  - `overlay new NAME PART [grow N]`, `overlay rm NAME`, `overlay grow NAME N`, `overlay show|hide NAME`, `overlay list` (name, part, grow, shell cells, exposed faces, painted faces)
  - `overlay paint NAME X Y Z FACE COLOR`, `overlay box NAME X0 Y0 Z0 X1 Y1 Z1 FACE|all COLOR` (paints every exposed face of the overlay inside the box; `FACE` limits to one direction), `overlay erase NAME X0 Y0 Z0 X1 Y1 Z1 [FACE|all]`, `overlay clear NAME`, `overlay prune NAME`
  - `overlay` colors: `#rrggbb`, palette index or name, or the common color words, resolved to rgb without adding to the palette (small `rgbOf` beside `colorOf`).
- `rig check` also reports: overlay part missing, shell outside the grid, stranded pixels.
- `src/selftest.j3`: a 2x2x2 part with `grow 1` has 56 shell cells and 96 outer faces; `overlay box ... all` paints them all; carve-out around a neighboring part; `flip x` and `rotate` keep pixels on the same visible faces; file round trip; undo/redo; `grow` clears; `prune`.

Done when: `vox selftest` prints `ok` and `vox run F "overlay ...; overlay list"` behaves.

### A2. Rendering

- `src/mesh.j3`: after the part passes, emit one face per visible opaque pixel (`gfx.emitFace`, flat color, no AO; AO would need the shell in the occlusion grid, add later if wanted) using the owning part's matrix; `floatCount` counts them too (count-only mode of the same walk so the two cannot disagree).
- `src/shot.j3` + `main.j3`: `--no-overlays` flag; `View.overlays` toggle for the window (`w.view.rev = 0` when it changes, like `tint`).
- `rig` joint behavior: shell pixels move with the part like any face.
- Demo: add an `armor` overlay to `models/blocky.vox.txt` (helmet `grow 1` on Head, chestplate `grow 1` on Torso, painted bands) and render `models/blocky-armor.png`.

Done when: the blocky render shows the armor, it follows `pose load wave_a`, and `vox selftest` passes.

### B. PNG import / export (people can paint in any pixel editor)

- **Atlas**: six projected panels in a fixed net, `w` x `h` x `d` being the bounds of the overlay's cells:
  top (w x d), bottom (w x d), then a second row of right (d x h), front (w x h), left (d x h), back (w x h); image size `(2d + 2w)` x `(d + h)`. One pixel is the *outermost exposed face* in that column along the panel's axis. For box-shaped parts and shells this reaches every face; faces hidden behind another face in the same column (U-shapes) cannot be reached from a PNG, `export` reports how many, and commands still paint them.
- `overlay export NAME FILE.png` writes the current pixels (template for empty ones: transparent, with a faint outline of reachable faces so people see the layout).
- `overlay import NAME FILE.png` replaces the pixels: alpha >= 128 is opaque, the rest nothing; it reports pixels that map to no exposed face.
- Use stdlib `j3.gfx.canvas.loadPng(path, arena)` (8-bit RGBA PNG; verify the pixel byte order with a one-pixel test first) and `Canvas.savePng`.
- Selftest: export then import into a fresh overlay returns the same pixel set; image size matches the formula.

### C. Window

- Rig tab section **Overlays**: list (click = active overlay), New (active part), Delete, grow stepper, eye toggle, import/export path field.
- New tool `OVPAINT` (and erase with mode 2): click or drag over faces to paint with the current palette color; the cursor shows the face. Picking needs the shell: refactor `pick.cast` to take a cell array so the editor can cast against `model solids + shell of the active overlay`.
- Part-paint tools keep working; overlays never change part assignment.

### D. Runtime (`runtime/j3rig.j3`)

- Parse `ov` / `ot` lines (resolved pixels, no shell logic needed): build one extra mesh per overlay and part (`has[]` style flags), shown or hidden per overlay.
- `j3rig.setOverlay(&rig, "armor", visible)`; `draw` draws overlay meshes with the part matrix after the part mesh.
- Verify with the scratch project used for the rigging runtime: parse blocky, count overlay floats, compare to the editor's `overlay list` face count.

## Risks / open points

- Undo cost: the shared texel pool is 512 KB per undo slot (about 40 MB total); lower the pool if memory matters.
- Unreachable faces in the PNG atlas for concave shapes (documented, commands cover them).
- Flat shading on overlay faces next to AO-shaded body faces; add shell AO if it looks wrong.
- Real Minecraft skin compatibility is out of scope (only grow 0 would map one to one).

## Verification

`tools/voxel-editor/vox selftest` after every phase; `vox run models/blocky.j3voxel --script models/blocky.vox.txt --render ...` for the visual check; the scratch runtime project for phase D; no window launching (the person running it does that).
