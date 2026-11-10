# Voxel editor

## Issues

### Runtime loader silently accepts malformed records (confirmed)

The palette-name and Add-recolor undo fixes passed rechecks during a game project migration. A separate probe of the current `runtime/j3voxel.j3` found these validation gaps:

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

### Workspace tabs

Many models at once: `vox a.j3voxel b.j3voxel`, or `vox models/` (every `.j3voxel` in the folder), one tab each (max 8). The tab strip sits under the top bar; **+ Tab** adds a blank one, **Close Tab** / Ctrl+W closes (twice if unsaved), Ctrl+Tab / Ctrl+Shift+Tab cycles, and the **Open...** chooser opens in a new tab. Each tab keeps its own undo history, camera and clip. The workspace is written to `.vox-workspace` in the folder you started in whenever the tabs change and when the window closes (closing never asks): each tab's file, camera, clip and playhead, the active tab, tool, Rig tab, ortho/grid and panel sizes. Unsaved edits (including unsaved clips and poses) go to `.vox-drafts/` and come back still unsaved. Plain `vox` with no arguments restores it all (file names without spaces). *Untested in the window* (compiles, selftest ok).

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
color and an optional name. Anything unknown is ignored. A rig adds `part`, `vp`, `pose` and `clip` lines
(see **Rigging**), overlays add `ov` and `ot` lines (see **Overlays**); files without them load exactly as before.

- **In a game:** copy `runtime/j3voxel.j3` into `src/`. `j3voxel.load(path, mem)` returns the
  grid and palette, and `j3voxel.mesh(&v, mem, scale)` returns vertices for `gfx.upload` (corner
  shading baked in, x/z centered, bottom on y = 0).
- **In the scene editor:** `export-mesh out.j3mesh [scale]` (or the *Export .j3mesh* button)
  writes a mesh the scene editor imports like any asset. Scale defaults to 0.1 units per voxel. It is the
  whole model at rest as one static mesh: parts, poses and overlays are not in it.
- **As a dep:** `[deps] voxrig = "../tools/voxel-editor/runtime"`, then `use voxrig.j3voxel;` / `use voxrig.j3rig;` (`j3c help manifest`).
- **A rigged model in a game:** `runtime/j3rig.j3` (see **Rigging** and **Overlays**).

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
   `--yaw D --pitch D`, `--size WxH`, `--perspective`, `--no-grid` also work; `--parts` colors each part differently and `--no-joints` hides the joint crosses (pink, active one yellow). The grid lines are
   voxel edges; the red edge is the x axis, blue is z, green is y.
3. **Check exactly** - `vox run FILE "dump y 3"` prints one layer as characters (`.` air, `1-9
   a-z A-Z` the color index), rows are z and columns x; `dump x 5` / `dump z 5` slice the other
   ways, and `dump` alone prints every y layer. `info` lists size, bounds, and voxels per color.
4. **Edit** - run more commands on the same file; `undo` only reaches back within one batch
   (the window keeps its own history).
5. **Export** - `export-mesh ship.j3mesh`, or hand over the `.j3voxel` itself.
6. **Import** - `import other.j3voxel 4 0 0` stamps another model in at an offset and merges its
   palette (so you can build a character from parts), and `open` replaces the model. Imported voxels
   join the active part; the other file's parts, poses and overlays are not brought along.
7. **Rig and dress** - see **Rigging** and **Overlays** below; `models/blocky*.vox.txt` are worked examples
   (`--script` runs one command per line; a `#` comment ends the line, `;` and all).

A color is a palette index, a palette name, a common word (`red green blue yellow orange purple
pink brown white black gray cyan tan skin`, added to the palette on first use), or `#rrggbb`.
End any shape with `hollow`, `erase` (cut it out), or `paint` (recolor only what exists).
Shapes outside the grid are clipped, and a command that places nothing says so.

```
new SX SY SZ | size SX SY SZ | crop [pad] | clear | name N | open F | save F | undo | redo
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

## Rigging

Parts are rigid limbs; a part's **joint** is its pivot, turning it and its children about the
parent. Each voxel belongs to at most one part (`vp` lines; 0 = none), new voxels join the
*active* part, and `shift`/`crop`/`flip`/`rotate` carry parts and pivots along. Pivots are in voxel-edge
coordinates (voxel `x` spans `x..x+1`, so a joint on a face is a whole number). Characters face +z, so
`.L` is +x. Poses are degrees about x, y, z; the live pose is not saved, named poses are.

```
part Arm.L parent Torso pivot 12 22 4 limit -150 150 -90 90 -10 120   file: joint + limits
vp 0 0 0 2 4                                                          file: part layer (x y z part [run])
pose wave Arm.L 0 0 -120                                              file: saved pose
clip wave 0.8 loop wave_a wave_b                                      file: animation over saved poses
```

Build by hand (voxels you draw join the active part, and `part new` makes the new part active):
`part new Torso; box ...; part pivot-auto Torso; part new Head parent Torso; box ...; part pivot-auto Head`.
`pivot-auto` puts a joint where the part touches its parent (a root gets the middle of its voxels); set one
yourself with `part pivot NAME X Y Z`, which `rig check` warns about when it is off the part. `part active none`
stops new voxels joining a part, and `new` clears the whole rig.
Or let it guess a human: `rig humanoid [TORSO_HALF]` sorts the voxels by body proportions (head
top 18%, arms outside the torso, legs below) and puts joints where parts touch; then fix with
`part assign`, `part split` (cut a part at a plane, e.g. an arm into upper and lower), `part pivot`.
`rig tree` shows the hierarchy, `rig check` prints `error:`/`warning:` lines (orphan voxels, empty
parts, pivots off their part, loops), `rig mirror FROM TO` copies a joint across x for left/right.

```
part new NAME [parent P] [pivot X Y Z] | part rm|active NAME | part rename OLD NEW | part parent NAME P|-
part pivot NAME X Y Z | part pivot-auto NAME | part limit NAME x|y|z MIN MAX
part assign NAME X0 Y0 Z0 X1 Y1 Z1 | part assign NAME color C | part assign NAME rest
part split NAME x|y|z N NEWNAME | rig tree | rig check | rig humanoid [N] | rig mirror FROM TO | rig clear
pose set PART RX RY RZ | pose reset | pose save|load|rm NAME | pose list
clip new NAME SECONDS loop|once POSE... | clip rm NAME | clip list | clip at NAME T
```

`shift`, `crop`, `flip` and `rotate` carry parts, pivots and overlay pixels along. `mirror` and `copy` move
voxels with their part numbers but not joints or overlay pixels (use `rig mirror FROM TO` or build each side), and `hollow`, `erase` or `replace ... air` can leave an overlay pixel on a face that is gone
(`rig check` finds those).

**Posing cheat sheet** (a character facing +z, a limb hanging down from its joint): rotation is `RX RY RZ` in degrees,
applied as y, then x, then z, and a child turns in its parent's frame, so an elbow hinge follows the shoulder.
Negative `RX` swings a hanging limb forward (+z) and positive swings it back, so raising an arm forward is
`-90`; `RZ` swings it sideways (positive moves the limb toward +x on the +x side: out for `.L`, in for `.R`);
`RY` turns a head or the torso about the vertical (negative turns toward -x). Elbows and knees want a limit
(`part limit Elbow x -150 0`) so a pose can not bend them backwards; `pose set` refuses angles outside
the limits. Render a pose with `pose load NAME` before `--render`, and compare it from `--view front`
and `--view right`; a joint cross that sits off the limb in `--render` (leave joints on) is a wrong pivot.

**Animation:** a clip runs through saved poses spread evenly over its seconds (`loop` returns from the
last pose to the first). `clip at walk 0.3` poses the model at 0.3 s, so a render of each time is a
frame to review. A clip key may carry its own time, `clip walk 0.8 loop a@0 b@0.3 c@0.5` (seconds from the start; no times means evenly spread). In the window, the Rig tab shows a **timeline** under the view (drag its top edge to resize): pick or make a clip, Play (Space), drag the red playhead or a key, and edit the pose with the handles (click a joint to select it, drag a ring to turn it, drag the orange diamond on a wrist to reach with the arm); with Auto-key on the edit becomes a key at the playhead, otherwise press Set key. Drag the edges of the side panels to resize them. Up to 64 poses and 16 keys per clip. **In a game:** copy `runtime/j3rig.j3` and its `runtime/j3rig/` folder (parts of the same module) beside `j3voxel.j3` (rename the modules as that
file says): `j3rig.load` / `upload`, then each frame `sample` (or `pose`, `blend`, `setAngle` by part name)
into an angles array and `j3rig.draw(&rig, Mat4.translate(...), angles, mats)`. Parts are meshed
separately and turned about their joints, with no skinning, so a part never bends.

`--render` shows the live pose (`pose load NAME` first for a saved one); add `--parts` to color each part
differently and check what `rig humanoid` or `part assign` decided. Pose angles beyond a part's limits are refused.

### Overlays

An **overlay** is a pixel layer on one part: clothing, armor, face paint, markings. One face is one pixel,
the same size as a voxel, so there is no sub-voxel geometry. It moves with its part and costs no
voxels. `grow 0` paints the part's own surface; `grow N` (1 to 4) puts the pixels on a shell N whole
voxels out from the part, like armor. A shell needs room: the part's box plus N must fit inside `size`.

```
overlay new helmet Head grow 1            overlay list | overlay rm|clear|prune|show|hide NAME | overlay grow NAME N
overlay box helmet 0 0 0 17 33 9 all steel   paint every exposed face in a box (a color, or `#rrggbb`)
overlay box helmet 8 33 0 9 33 9 py gold     only the faces on one side
overlay erase helmet 4 28 9 13 29 9 pz       cut a visor slit
overlay paint helmet 5 30 9 pz red           one face: cell X Y Z and its side
overlay export helmet helmet.png | overlay import helmet helmet.png
```

Faces are `px nx py ny pz nz` (the +x side, -x side, ...); front is +z. The shell is the part grown by N
(square corners) minus any cell a model voxel or an earlier overlay already holds, so armor never swallows
a neighboring limb. A face against the part or another voxel is hidden and takes no pixel. Colors are
plain rgb (the palette is not touched). Changing `grow` clears the overlay's pixels; if you reshape the part
later, `rig check` reports painted faces that are no longer on the surface and `overlay prune NAME` drops them.
`shift`, `crop`, `flip` and `rotate` carry overlays along.

**PNG:** `overlay export` writes a net of six panels: top and bottom on the first row, then right (`px`),
front (`pz`), left (`nx`), back (`nz`); image size `(2d + 2w)` x `(d + h)` for a `w` x `h` x `d` box of cells (the
layout is drawn at the top of `src/overlaypng.j3`). Paint in any pixel editor (alpha 128 or more is painted,
the rest is empty; unpainted faces export as faint gray) and `overlay import` it back. Faces hidden behind
another face in the same column of a concave shape have no pixel; paint those with the commands.
Renders show overlays (`--no-overlays` hides them); in a game `j3rig` draws them with their part
(`j3rig.setOverlay(&rig, "helmet", false)` hides one). The window's Rig tab has an **Overlays** section
and a *Paint overlay* tool (Erase mode clears a face); paint on the rest pose (`pose reset`), since tools
act on the unposed model.

The window's **Rig** tab (right panel) does the same by hand: pick / paint a part onto voxels, place a
joint on a face, joint and rotation sliders, save and load poses, *Color by part*. `runtime/j3voxel.j3`
itself ignores parts and overlays; use `j3rig.j3`.

## Layout

`src/model.j3` grid, parts layer, palette, shapes, undo, overlay pixel pool; `rig.j3` parts, joints, poses, clips,
check, humanoid; `overlay.j3` overlay shells and painting; `overlaypng.j3` the PNG net; `format.j3` the file;
`cmd.j3` the command language;
`mesh.j3` / `view.j3` / `shot.j3` triangles, the camera pass, and the hidden-window PNG renderer;
`pick.j3` ray to voxel; `world.j3` `edit.j3` `files.j3` `panels.j3` `gui.j3` the window;
`selftest.j3`. Big modules (`model`, `rig`, `cmd`, `format`, `panels`, `runtime/j3rig`) keep their rest in a folder beside the file, `module` line repeated (`src/model/*.j3`, ...). `runtime/j3voxel.j3` the game-side loader and `runtime/j3rig.j3` for rigs, clips and overlays. One rule of the language to know
when editing: a name may be declared once per function, even in sibling blocks.
