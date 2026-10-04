# Voxel editor

## Issues

### Runtime loader silently accepts malformed records (confirmed)

The palette-name and Add-recolor undo fixes passed rechecks during the industry migration. A separate probe of the current `runtime/j3voxel.j3` found these validation gaps:

- `size 1 1 1`, `p 1 ffffff`, `v 0 0 0 257`: palette index 257 wraps to 1 instead of reporting invalid input.
- `size 1 1 1`, `v 0 0 0 1`: an undefined palette entry is accepted and renders black.
- `size 1 1 1`, `p 1 ffffff`, `v -1 0 0 1`: the numeric reader ignores the minus sign; the invalid coordinate is silently clipped instead of rejected.
- `size 65 1 1`, `p 1 ffffff`, `v 0 0 0 1`: dimensions beyond the editor's documented 64-cell limit are accepted.

Each comma-separated record above is a separate line of input to `parse`. The returned `sx` stayed nonzero for all four probes. The palette-257 probe returned cell index 1, and the size-65 probe returned `sx = 65`.

**Improve:** validate whole numeric tokens, dimensions before allocation, palette indices before narrowing to `u8`, used palette references, and coordinates/runs. Return a detectable failure for malformed models; document any intentional clipping policy. Add regression checks for these inputs and a valid black palette entry. Industry's local loader now rejects these cases; the tool's runtime loader was not changed.

A 3D editor for voxel models, written in J3, and a command line for agents. Models are `.j3voxel`
text files.

```
tools/voxel-editor/vox                      # the window (builds on first run)
tools/voxel-editor/vox models/tree.j3voxel  # open a model
tools/voxel-editor/vox selftest             # checks edits, the file format, and picking
```

`vox` is a script that runs `j3c build` when `src/` changed, then runs the program from the
directory you are in, so relative paths mean what you expect. Needs SDL2 and libGL. The window
uses the DejaVu font (`J3_VOXEL_FONT=/path/to.ttf` picks another).

## Using the window

Left-drag draws. Right-drag orbits, middle-drag (or shift + right-drag) pans, the wheel zooms.
`F2` shows every key.

| Tool | Key | Drag |
|---|---|---|
| Pencil | `B` | paints a stroke; Brush sets its size (shift + wheel) |
| Box | `U` | press a corner, drag to the other; Depth is how far it goes from the face (ctrl + wheel) |
| Sphere | `O` | press the center, drag the radius |
| Cylinder | `C` | press the center, drag the radius; Depth is its length along the face |
| Line | `L` | press one end, drag to the other |
| Fill | `G` | click a voxel: its whole connected region takes the color ("Fill one layer" stays flat) |
| Pick color | `I` | click a voxel (or alt + click with any tool) |

**Click does** (`1` `2` `3`): *Add* puts voxels in front of the face you point at, *Erase* cuts
into it, *Paint* recolors voxels that are already there. Every tool honors it, so Box + Erase
digs a trench and Sphere + Paint tints a ball.

Shapes live-preview: the model changes as you drag and `Esc` puts it back. A shape stays in the
layer of the face you pressed on. **Mirror** X / Y / Z reflects every edit across the middle of
the model (a cyan plane shows where). `H` makes shapes hollow. Undo/redo are `Ctrl+Z` /
`Ctrl+Y`, 40 steps deep.

**Palette** (right panel): click a swatch to paint with it, `+` adds one, the sliders and hex box
edit the chosen color, and a name makes it usable in commands (`box ... skin`). The preset
buttons swap in a whole palette (voxels keep their numbers), *Trim unused* drops colors nothing
uses, and *Save/Load palette* keep palettes in `palettes/NAME.j3pal` to reuse across models.

The box at the bottom takes the same commands as the CLI (below); output shows over the view.
If an agent (or you, in a text editor) changes the open file, the window reloads it. Unsaved
edits are never overwritten.

Models are up to 64 voxels on a side; 1 voxel is 1 unit in the editor.

## The `.j3voxel` format

```
# j3voxel 1
name robot
size 16 24 12
p 1 3b3b3b outline
p 2 e63946 red
v 0 0 0 1          one voxel at x y z with palette color 1
v 2 0 0 2 4        run: 4 voxels along +x from (2,0,0)
```

Text, one item per line, any order after `size`, `#` comments. Axes: x right, y **up**, z toward
the viewer in front view. `v` takes palette indices 1..255 (0 is air); `p` gives an index a hex
color and an optional name. Anything unknown is ignored.

- **In a game:** copy `runtime/j3voxel.j3` into `src/`. `j3voxel.load(path, mem)` returns the
  grid and palette, and `j3voxel.mesh(&v, mem, scale)` returns vertices for `gfx.upload` (corner
  shading baked in, x/z centered, bottom on y = 0).
- **In the scene editor:** `export-mesh out.j3mesh [scale]` (or the *Export .j3mesh* button)
  writes a mesh the scene editor imports like any asset. Scale defaults to 0.1 units per voxel.

## Agents: `vox run`

```
vox run FILE [COMMAND...] [--render out.png --view all] [--script cmds.txt] [--no-save]
```

Loads `FILE` (a new 16x16x16 model when it does not exist), runs each COMMAND (an argument can
hold several, separated by `;`), saves, optionally renders a PNG, and prints the model's
`info`. A command that fails stops the batch, says why, and saves nothing. This is the loop:

1. **Make** - `vox run models/ship.j3voxel "size 24 12 32; palette preset gray; box 4 0 8 19 4 31 gray; ..." --render ship.png`
2. **Look** - read `ship.png`. `--view all` is one image: iso (top left), front (top right),
   right side (bottom left), top (bottom right). Other views: `iso front back left right top`;
   `--yaw D --pitch D`, `--size WxH`, `--perspective`, `--no-grid` also work. The grid lines are
   voxel edges; the red edge is the x axis, blue is z, green is y.
3. **Check exactly** - `vox run FILE "dump y 3"` prints one layer as characters (`.` air, `1-9
   a-z A-Z` the color index), rows are z and columns x; `dump x 5` / `dump z 5` slice the other
   ways, and `dump` alone prints every y layer. `info` lists size, bounds, and voxels per color.
4. **Edit** - run more commands on the same file; `undo` only reaches back within one batch
   (the window keeps its own history).
5. **Export** - `export-mesh ship.j3mesh`, or hand over the `.j3voxel` itself.
6. **Import** - `import other.j3voxel 4 0 0` stamps another model in at an offset and merges its
   palette (so you can build a character from parts), and `open` replaces the model.

A color is a palette index, a palette name, a common word (`red green blue yellow orange purple
pink brown white black gray cyan tan skin`, added to the palette on first use), or `#rrggbb`.
End any shape with `hollow`, `erase` (cut it out), or `paint` (recolor only what exists).
Shapes outside the grid are clipped, and a command that places nothing says so.

```
new SX SY SZ | size SX SY SZ | crop [pad] | clear | name N | open F | save [F] | undo | redo
import F [dx dy dz] | export-mesh F [scale]
set X Y Z C | erase X Y Z | paint X Y Z C
box X0 Y0 Z0 X1 Y1 Z1 C [hollow]     sphere CX CY CZ R C [hollow]
ellipsoid CX CY CZ RX RY RZ C         cylinder X Y Z R LEN x|y|z C [hollow]
line X0 Y0 Z0 X1 Y1 Z1 C              fill X Y Z C [plane x|y|z]     replace C C|air
shift DX DY DZ | flip x|y|z | mirror x|y|z [+] | rotate x|y|z [turns] | hollow
copy X0 Y0 Z0 X1 Y1 Z1 DX DY DZ [move] | sym [x] [y] [z] | sym off
color I #RRGGBB [name] | addcolor #RRGGBB [name] | colorname I NAME
palette | palette preset default|earth|pastel|gray|neon | palette load F | palette save F | palette trim
info | dump [x|y|z] [N] | help
```

Sphere and ellipsoid radii are in voxels around the center voxel: radius 0 is one voxel, radius 2
spans 5. Cylinders start at the given voxel and run LEN voxels toward +axis (negative runs back).
`mirror x` copies the low half over the high half (`mirror x +` the other way), `flip` reverses
the whole model, and `sym x` makes every later edit appear on both sides (it resets each batch).

## Layout

`src/model.j3` grid, palette, shapes, undo; `format.j3` the file; `cmd.j3` the command language;
`mesh.j3` / `view.j3` / `shot.j3` triangles, the camera pass, and the hidden-window PNG renderer;
`pick.j3` ray to voxel; `world.j3` `edit.j3` `files.j3` `panels.j3` `gui.j3` the window;
`selftest.j3`; `runtime/j3voxel.j3` the game-side loader. One rule of the language to know
when editing: a name may be declared once per function, even in sibling blocks.
