# J3 3D scene editor

```sh
j3c editor --new ./my-game
j3c editor ./my-game
```

The first command creates a runnable J3 project with a camera and a cube using
the example `scripts/spin.j3s` script. The
second opens an existing project. If that project has no editor scene, the
command adds `j3.editor.toml` and `scenes/main.j3scene` without changing its
entry point. To render that scene in an existing game:

- Load it with `j3.scene3d.load` during boot. `scripts.scene()` (`use j3.scene3d.scripts;`) names the scene Play picked.
- Call `scene3d.upload` after `gfx.init`.
- Each frame:
  - take the lighting with `scene3d.environment`;
  - call `scene3d.shadows` (fog and the sun shadow pass) and `scene3d.lights`;
  - start the camera pass with the environment's sun and sky;
  - call `scene3d.draw`.
- Call `scene3d.release` before closing the GL context if the game unloads the scene early.

The new-project template shows the complete pattern.

The window has a toolbar (Save, Play/Stop, Undo/Redo, the Move/Rotate/Scale
tools, Local axes, Focus, Snap, Keys, and Console), the hierarchy and project assets on
the left, the 3D viewport in the middle, the inspector on the right, and a status
bar that hints at what the pointer can do. **Keys (H)** shows the shortcut sheet
in the viewport. Drag the edge of either side panel to resize it, and the short bar
between the hierarchy and the assets to share the height differently; the sizes
are kept in `target/j3-editor/prefs`.

The editor remembers its session in `target/j3-editor/session`: the open tabs and
their order, split views, each tab's camera, view mode, selection and folded
objects, the folded asset folders, and the console. It's written on quit and every
minute, and the next launch reopens it (files that are gone are skipped).

Viewport controls:

- Right drag orbits, middle drag pans, and the wheel zooms.
- Left click selects the nearest mesh under the pointer (tested against its triangles), or a camera, group, light, or placed scene whose marker is in front of it. Click the same spot again to select the next object behind, which cycles back to the front. Click empty space to deselect; double-click to focus.
- Ctrl+click adds to the selection or removes from it. Dragging from empty space draws a selection box; it selects meshes it touches any part of, and other objects whose origin it covers.
- Hovered meshes get a faint outline. Cameras, groups, and lights are drawn as wire markers with name tags.

The selected object shows a gizmo:

- Drag an arm (Move/Scale) or a ring (Rotate) to edit that axis. Every selected object moves together.
- Hold Ctrl while dragging to snap to the steps set under **Snap**, where the grid spacing is set too. These settings are kept in `target/j3-editor/prefs`.
- Move arms and rotate rings follow world axes, or the object's own axes with **Local (L)**. Scale arms always follow the object's axes.

Keys 1, 3, and 7 show the front, side, and top views in orthographic projection;
5 toggles orthographic, and orbiting away from a front, side, or top view goes back to
perspective. 0 looks through the selected camera (or the main one); orbiting,
panning, or Escape leaves it. While a camera is selected, a preview of what it sees
shows in the viewport's top-right corner (shaped like the game window, from `App.create` in `src/main.j3`); clicking it (or **Lock**) keeps
that camera's preview up whatever is selected (saved with the session); **Unlock**
lets it go. The editor's frame rate shows in the viewport's bottom-left corner.

Right-click an object (in the viewport or the hierarchy) for these actions:

- Rename, Focus
- Cut, Copy, Paste, Paste as child, Duplicate
- Add child, Move to top level, Reset transform, Delete
- Snap to ground; **Place on top of...**, then click the object to set the selection on (Esc cancels)
- **Scatter copies...**: copies of the object's mesh or scene around the middle of the view (count, spread, seed); also in the Add menu and on empty space as **Scatter...**
- For cameras: View through camera and Set as main camera

Commands act on every selected object. Right-click empty space for Paste and Add;
objects added from the viewport land where you clicked. Copies include children,
their script props, and a placed scene's overrides. Paste puts the copy next to the selection.

| Key | Action |
|-----|--------|
| W / E / R / L | Move / Rotate / Scale tool / local axes |
| F | Focus the camera on the selection |
| F2 | Rename the selection |
| Up / Down | Previous / next hierarchy row |
| Left / Right | Fold the row (or go to its parent) / unfold it |
| Delete | Delete the selected objects and their children |
| Ctrl+C / Ctrl+X / Ctrl+V / Ctrl+D | Copy / Cut / Paste / Duplicate |
| Ctrl+Z / Ctrl+Y (or Ctrl+Shift+Z) | Undo / Redo (100 steps) |
| Ctrl+S / Ctrl+P | Save / Play or restart the game (saves every open scene) |
| Ctrl+J | Show or hide the console |
| Ctrl+N / Ctrl+W | New scene / close the scene tab |
| Ctrl+Tab / Ctrl+Shift+Tab | Next / previous scene tab |
| 1 / 3 / 7 / 5 | Front / side / top view / orthographic on or off |
| 0 | Look through a camera, or stop |
| F5 | Rescan assets and reread `j3.editor.toml` |
| H | Show or hide the shortcut sheet |
| Escape | Close a menu or panel, leave a text field or camera view, or clear the selection |

Letter keys and the clipboard shortcuts are ignored while a text field has
focus. Text fields work like desktop text boxes:

- Click to place the caret, drag or Shift+click to select, and double-click to select a word.
- Left, Right, Home, and End move the caret; with Shift they select. Ctrl+Left/Right jump by words.
- Backspace and Delete repeat when held; with Ctrl they delete a word. Typing or pasting replaces the selection.
- Ctrl+C, Ctrl+X, and Ctrl+V copy, cut, and paste the selection through the system clipboard, and Ctrl+A selects all. Enter finishes editing.

Drag a number box left or right to change it (whole-number boxes step by one), or
click it without dragging to type an exact value; Enter or clicking away keeps it,
and Escape drops it.

Add groups, meshes, planes, sky boxes, cameras, and lights from the **+ Add** menu
above the hierarchy (it also imports models and places other scenes); they land in
the middle of the view at the top level. New objects get unique names ("Mesh",
"Mesh2", ...). In the hierarchy:

- Drag a row onto the middle of another row to parent it, onto its top or bottom edge to place it before or after that row, or onto empty tree space to move it to the top level. The object keeps its place in the world.
- The arrow left of a row folds its children.
- The search field shows the objects whose names match, with their parents.
- Shift+click selects a range of rows, and Ctrl+click adds one.

The first camera in the scene is the one games start with (tagged "main camera");
**Set as main camera** moves another one first.

The inspector edits the local transform. Drag a number box left or right to
change it (hold Shift for fine steps), or click it to type. Rotation is shown in
degrees and stored in radians.

A mesh's **Shape** is a built-in cube, sphere, quad, plane, or sky, or **File...**
for a `.j3mesh` from the assets. Built-in shapes are stored as `builtin:<name>` and
made by `j3.scene3d` at load, so games need no files for them.

- **Plane** is a flat 1x1 built-in plane. While one plane (or quad) is selected,
  squares on its edges resize it: drag one and the opposite edge stays put (Ctrl
  snaps the size to the move step).
- **Sky Box** is a built-in inside-out sphere with a generated cloudy sky image
  (`builtin:sky-clouds`; set the texture to use your own equirect image). It's drawn
  behind everything around the camera, unlit, and mirrors reflect it.

**Save** writes the scene file. **Play** saves, builds in the background, and runs
the scene in the open tab in another window; the editor stays usable while it
builds. While the game runs, the button reads **Stop**, and Play (Ctrl+P) restarts
it. Closing the window with unsaved changes asks whether to save.

**Console** (or Ctrl+J) opens a panel under the viewport, resizable by its top
edge. It shows what the running game prints (stdout, and stderr in red), build
errors and warnings, how long the build took, and a crash's exit code. It opens by
itself on build errors and crashes, and the toolbar button counts new problems while
it's closed. **Problems only** hides plain output, and the wheel scrolls back; new
lines follow while you're at the bottom. A game printing more than 500 lines a
second per stream has the rest skipped and counted. The full compiler output is
also saved at `target/j3-editor/build.log`.

Play is quick when little changed: the build is skipped when every source file and
option is the same as last time (scene edits never need a rebuild), and clang is
skipped when the generated code is unchanged. `J3C_TIMINGS=1 j3c build` prints how
long each step took.

Unsaved scenes are copied to `target/j3-editor/autosave.*` every minute. A clean
save, close, or quit removes the copy; if the editor crashed, the next time the
scene opens you are asked whether to recover it.

### Colors

Every color (lights, mesh and placed-scene tints, scene lighting, materials) is a
swatch with its hex code. Click the swatch to open the picker:

- drag in the square for saturation and brightness, and on the bar for hue;
- type a hex code (`#FF8800`) or 0-255 red, green, and blue values;
- click the left half of the old/new pair to go back, or a recent color below.

Enter or a click outside keeps the color, and Escape puts the old one back. One
visit to the picker is one undo step. Recent colors are kept in the prefs file.

### Lights and scene lighting

A light is a point light with a color and a range. Eight point lights shine at
once: the scene's lights first, then objects whose material glows with **Lights up
other objects**. With nothing selected, the inspector
shows **Scene lighting**:

- sun direction and color;
- the sky color, which is also the fog and ambient color;
- where fog starts and becomes full;
- whether the sun casts shadows, and the size of the area that does.

These are saved in the scene and used by the game template.

### Scenes, tabs, and scenes inside scenes

Each open scene gets a tab above the viewport, with its own camera, selection,
and undo history. Drag a tab along a tab strip to reorder it. Drop it on the left
or right quarter of a view to split that view in two (up to four side by side), or
on another view's tab strip to move it there; a view left without tabs closes. Drag
the line between views to resize them. Click in a view to work in it; the
inspector follows the view you're in. **+** (or Ctrl+N) makes `scenes/<name>.j3scene` and opens it;
Ctrl+Tab and Ctrl+Shift+Tab flip between tabs, and **x** (or Ctrl+W) closes one,
asking first if it has unsaved changes. Scene files show up under **Assets**:
click one to open it in a tab.

A scene can be placed inside another scene, like a reusable component or a
prefab. Right-click empty space (or an object, for a child) and pick **Add
scene: <name>**, or right-click a scene in **Assets**. The placed scene is one
object in the hierarchy (tagged `scene`) that you move, rotate, scale, copy, and
parent like any other. Clicking any part of it in the viewport selects it. Its
contents are read from the scene file. Open that scene in its tab (**Open scene**
in the inspector or the menu) to change every copy; each placed copy updates when
it is saved. Scenes may nest, but a scene can't contain itself. Play saves every
open scene.

To change one copy only:

- Turn on **Edit placed objects** in its inspector, which lists its objects in the hierarchy (tagged `placed`). In the viewport, Alt+click selects one of them directly.
- Edit its transform, name, mesh, texture, material, script, and script props. The changes are saved in the outer scene as overrides for that copy. This works for objects from scenes nested inside the placed one too, and a placed scene's own overrides of what it places come along.
- **Revert to the scene file** drops them.
- Placed objects can't be deleted or re-parented; do that in their own scene. Objects from scenes nested deeper can't be overridden.

At runtime `scene3d.load` adds each placed scene's objects under its node (they
have `owner` set to that node's id), with their scripts, prop values, and
overrides, so game code needs no changes. `scene3d.store` writes only the scene's
own objects and their overrides.

### Assets

To import, do one of these:

- Enter a path in the field under **Assets** and press **Import**.
- Choose a file with **Pick file...** (it uses `zenity`).
- Drop a file on the window.

This copies a `.j3mesh` or converts a static `.gltf` or `.glb` into project assets.

The asset list is a folder tree of `assets/` and `scenes/`; click a folder to fold
it.

The importer keeps:

- the glTF node hierarchy, triangles, positions, normals, and UVs;
- base color, double-sided materials, and PNG or JPEG base color textures;
- alpha mask (cut at 0.5) and alpha blend materials.

A mesh used by several nodes is written once. Unsupported parts (animation,
skinning, morph targets, texture transforms, shear, primitives that aren't
triangles or can't be read) are skipped, and the status bar says
`Imported with N warnings`. Other details:

- A texture URI may contain percent-encoded characters such as `%20` for a space.
- Missing or unreadable textures become warnings; their meshes still import without the texture.
- Files requiring Draco, meshopt, or another unsupported glTF extension are rejected with a clear error asking for an uncompressed export.
- A failed import removes the files it wrote.
- Paths may be absolute or relative to the project directory.

**Get Asset** in the toolbar opens a Get Assets tab: an online search of free CC0
assets (Poly Haven models and skies, ambientCG materials, Kenney model packs). Pick a
source and a kind, type a word and press Enter, then click a card to see it on the
right:

- a model is added as a reusable scene under `assets/imported/` (**Add to project**),
  or also placed in your first scene tab (**Place in ...**);
- a material becomes `assets/materials/<id>.j3mat` with its color texture;
- a sky becomes `assets/skies/<id>.png`, for a `builtin:sky` mesh's texture;
- a Kenney pack lists its models; click one to import it.

Each asset gets a line in `assets/CREDITS.txt`. Downloads need `curl` and are cached
in `~/.cache/j3/assets/` (safe to delete).

The imported objects are added to the scene the import started from, as one undo
step, and the asset list refreshes. With **Import as a reusable scene** ticked, the
import is saved as `assets/imported/<name>.j3scene` instead and placed once, so it
can be placed again like any scene. The scene file changes only when you save.

Click a mesh under **Assets** to attach it to the selected mesh object, or to
create one if no mesh is selected. Click an image or a material to put it on the
selected mesh. Or drag an asset out of the list:

- a mesh or scene dropped in the viewport is added where the pointer is, resting on
  the mesh under it or on the ground; dropped on a hierarchy row, it becomes that
  object's child;
- a material or image dropped on a mesh (in the viewport or the hierarchy) goes on
  that mesh; on a placed object it's saved as an override.

Each drop is one undo step. Meshes made without UVs (like the default cube) get
box-projected ones when loaded, so images and noise show on them.

A mesh's **Transparency** button cycles between opaque, cutout
(texture alpha under 0.5 is a hole), and see-through with an opacity; see-through
meshes draw after the rest, farthest first.

Right-click any asset for **Rename or move...** and **Delete file**. Renaming or
moving rewrites every scene that refers to the file, open or not. Deleting moves
the file to `target/j3-editor/trash/`. Objects whose mesh, texture, material, or
placed scene file is gone are tagged `missing`. The scene named in
`j3.editor.toml` can't be renamed or deleted from the editor. **Rescan (F5)** picks
up files added outside the editor and changes to `j3.editor.toml`.

### Materials

**New material** makes `assets/materials/material<N>.j3mat` and opens it in a
material tab. Right-click a material under **Assets** and pick **Edit material**
(or click the material's name in a mesh's inspector) to open an existing one. The
tab shows the material on a sphere, cube, or plane (chosen on the left) that you
orbit like the scene, and the inspector holds its settings:

- **Color:** a tint that multiplies the pattern, and a pattern that is plain, an
  image (type its path or click an image under **Assets**), or noise.
- **Noise:** a type (smooth blobs, Perlin clouds, cells, or stripes), how many
  features fit across the texture, a seed, layers (one, fbm detail, ridges, or
  billows, with count, size step, and strength), swirl (domain warp), contrast,
  brightness, bands (posterize), a hard cut, invert, and the texture size. Cells
  can measure the nearest point, the second nearest, or the cracks between cells.
  Colors come from a ramp of 2 to 4 stops. Noise always tiles, and a preview of the
  baked texture shows above the settings.
- **Surface:** highlight strength and sharpness, **Mirror** (1 reflects everything
  at every angle; lower values mostly at grazing angles), and roughness. Without URP
  mirrors show the sky or the sky box; with URP they reflect the scene.
- **Glow:** a color the surface shows unlit. **Lights up other objects** makes every
  object using the material a point light of that color, with a strength and range.
- **Glass and transparency:** each object's own setting, or opaque, cutout, or
  see-through for every object using the material; an opacity (below 1 is
  see-through even when the mode is each object's own); a **refraction index**
  (presets for water, glass, and diamond) that makes the surface glass; for glass,
  **cloudiness** (0 clear, 1 frosted); and glass edges (clear where the surface
  faces you, solid at its rim).

Glass reflects by Fresnel (more at grazing angles), bends what's behind it, and
tints it more where it's thicker. Mirrors show their tint as a metal color, and the
sun makes a small, sharp glint on smooth surfaces (broader on rough ones).
See-through objects cast shadows by how see-through they are, and glass casts a
tinted shadow.
- **Tiling:** how often the texture repeats across the mesh, and its shift.

Edits show at once on every object using the material, in every tab. **Save**
(Ctrl+S) writes the file; closing the tab without saving puts the objects back as
the file has them. Undo and redo work inside the tab.

Without URP, glass bends a copy of what was drawn behind it (the grid, objects, and
other glass warp through it), so things not in view can't show through, and mirrors
reflect only the sky.

### Effects

A `.j3fx` file is a shader effect: `key=value` header lines, a `---` line, then GLSL.

```
kind=material
param.bands=int:3
param.rim=color:1,0.8,0.4
param.strength=float:0.5
param.on=bool:true
---
vec3 effect(Surface s, vec3 lit) {
  float l = max(dot(lit, vec3(0.299, 0.587, 0.114)), 0.001);
  vec3 toon = lit * (floor(l * float(uFx_bands)) / float(uFx_bands)) / l;
  float rim = pow(1.0 - max(dot(s.normal, s.view), 0.0), 3.0);
  return uFx_on ? toon + uFx_rim * rim * uFx_strength : lit;
}
```

Each `param.<name>=<type>:<default>` (`float`, `int`, `bool`, or `color` as `r,g,b`)
is a `uFx_<name>` uniform; 8 params at most. `uTime` (seconds) and `uResolution`
(view pixels) are built in. Blank lines and `#` lines in the header are skipped.

- **Material effects** (`kind=material`, the default) go on a material: click the
  file under **Assets** with a material tab open, or type its path under
  **Effect**. `effect(Surface s, vec3 lit)` runs after lighting and glow, before fog.
  `s` has `pos`, `normal`, `view` (toward the eye), `uv`, `albedo`, and `alpha`.
  `discard` works, for dissolves. To move vertices too, put
  `vec3 displace(vec3 pos, vec3 normal)` (world space) inside `#ifdef J3_VERTEX`, and
  `effect` in its `#else`. The file is compiled into the vertex shader as well, with
  `J3_VERTEX` defined. The params show under **Effect**
  and are saved in the `.j3mat` as `fx.<name>`.
- **Camera effects** (`kind=camera`) go on a camera: select it and click the file
  under **Assets**. They run in order over the whole frame (Up, Down, and x reorder
  and remove them under **Effects**). `vec4 effect(vec2 uv)` reads `uColor` (the
  picture so far), `uDepth`, `linearDepth(uv)` (world units from the eye), `uNear`,
  and `uFar`. Params are per camera, as `fx.<file stem>.<name>`, and placed copies
  can override them. Look through the camera (0) to see them. Play shows them too.

**New effect** and **New camera effect** under **Assets** write starters to
`assets/fx/`. Saving the file recompiles it at once. GLSL errors go to the console
with the `.j3fx` line, and a broken effect draws as if it weren't there. Agents set
`effects=a.j3fx|b.j3fx` and `fx.<stem>.<param>=` on cameras with `set`.

Limits: `displace` also runs in the shadow map pass, so displaced meshes cast displaced
shadows, but URP traces ignore material effects (reflections show the undisplaced mesh),
surfaces with an effect skip URP's traced shadows, mirrors, and glass, and the corner
camera preview shows no camera effects. `examples/editor_village/assets/fx/wind.j3fx`
bends the crops with `displace`.

### URP

With nothing selected, **Rendering (URP)** in the inspector turns on traced mirrors
and glass for the scene. The scene still draws normally; only surfaces with a
mirror or a refraction index follow real rays through the scene's triangles (kept
in a BVH on the GPU). Every glass surface a ray meets splits it into a reflection
and a refraction (Fresnel), glass absorbs its tint over the distance travelled
inside, and mirrors reflect their tint, so glass and mirrors show each other.
Settings:

- **Bounces:** how many mirror or glass surfaces a ray passes through (1-8).
- **Rays per pixel:** rays averaged on rough mirrors (1-16).
- **Traced sun shadows:** every surface traces its sun shadow instead of reading
  the shadow map, so shadows through glass are tinted and partial exactly.

A scene placing a scene that uses URP uses it too; its inspector shows
"*Using URP" and which placed scene turned it on. Games need nothing extra: the
tracer runs whenever the drawn scene uses URP. Traced hits use material colors, not
textures. Copies of a mesh share its triangles; at most 262144 triangles across the
distinct meshes are traced.

### Scripts

A script is a `.j3s` file anywhere in the project (`j3c doc lang`, Scripts). Type
its path (`scripts/spin.j3s`) in an object's **Script** field to attach it; one
script can be on many objects, and each gets its own instance. **Create script**
makes `scripts/<name>.j3s` with a `props` block and the two functions the scene
calls:

```j3
use j3.scene3d.{ Scene, NodeId };

props {
    node: NodeId;          // the object this copy is on (unless the scene sets it)
    speed: f32 = 1.0;      // shown in the inspector
}

pub fn start(scene: &Scene) {
}

pub fn update(scene: &Scene, dt: f32) {
    let n = scene.node(node);
    n.rotation.y += speed * dt;
}
```

The inspector shows the script's props; press **Rescan (F5)** after changing a
props block. Values are stored per object as `param.<id>.<prop>` lines, and a
prop keeps its default until an object sets it. A prop with no default must be
set on every object the script is on; `j3c check` points at the scene line when
one isn't, or when a value has the wrong type.

| Prop type | Inspector | Scene line |
|-----------|-----------|------------|
| `f32`, `f64` | number box | `param.<id>.<prop> f=1.5` |
| `int` and sized ints | number box, rounded | `param.<id>.<prop>=3` |
| `bool` | switch | `param.<id>.<prop>=1` |
| `string` | text field | `param.<id>.<prop> s=text` |
| `Vec3` | three number boxes | `.x` / `.y` / `.z` numbers |
| `NodeId` | button; click it, then an object | `param.<id>.<prop> r=<object id>` |

A `NodeId` prop follows copies: when an object and the object it points at are
copied or placed together, the copy points at the other copy.

The game runs attached scripts through `j3.scene3d.scripts`, which `j3c` makes
from the project's scenes: `scripts.scene()` is the scene Play picked (else the
one in `j3.editor.toml`), `scripts.start(&scene)` makes every instance and runs
its `start`, `scripts.update(&scene, dt)` runs every `update`, and
`scene.script::<Spin>(node)` finds one object's instance
(`const Spin = @script("scripts/spin.j3s");`). New projects already call these.

**Play** builds the game in dev mode when it has scripts: editing a script while
the game runs swaps the new code in at the next frame, and a change to a script's
props or state relaunches the same game binary. Neither compiles the game again.

The editor targets Linux with SDL2, OpenGL 3.3, and SDL2_ttf. It bundles
a UI font for its panels; projects can supply their own meshes
and image textures (image loading needs SDL2_image). The scene format is a line
based J3 save file with `scene.*`, `node.<index>.*`, `param.<id>.*`, and
`over.<instance>.<source>.*` keys. Object IDs are stable even when nodes are
removed or reordered. Rotations in the file (`node.<i>.rx/ry/rz`) are radians,
applied as `T * Rz * Ry * Rx * S`; the tools (`j3c -e scene`, MCP) and the
inspector use degrees. A generator that writes scene text itself must convert:
`j3c -e scene check` warns on a stored rotation above two turns. Rotating about +Y
by `a` sends local +x to `(cos a, 0, -sin a)`, so a slab pointing along `(dx, dz)`
needs `ry = -atan2(dz, dx)`.
