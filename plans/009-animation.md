# Plan 009: glTF animation, skinning, morph targets

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, do NOT update `plans/README.md`
> (the reviewer does that).
>
> **Drift check (run first)**: `git rev-parse --short HEAD` should be
> `b086a5b`. The tree has a lot of uncommitted work on top of it (the whole
> scene editor, `gltf.rs`, `scene3d.j3`, …), so the commit alone proves
> nothing. Compare every excerpt in "Current state" with the live file at the
> cited `path:line` (a few lines of slack is fine; different code is not). On a
> real mismatch, STOP.

## Status

- **Priority**: P2
- **Effort**: L (phases 1–2: M; phase 3: M; phase 4: M, optional)
- **Risk**: MED (new file format, new vertex layout, shader edits)
- **Depends on**: none
- **Category**: feature
- **Planned at**: commit `b086a5b` + uncommitted tree, 2026-09-25

## Why this matters

`EDITOR.md` → "Still left" → Animation lists four gaps:

- glTF animation: a clip file next to the scene, plus `anim.Track` sampling in scene3d.
- Skinning: joints and weights in the vertex layout (`bindLayout` in gfx.j3) and bone uniforms.
- Morph targets: CPU morphing, which breaks the shared GPU mesh cache.
- Then an "open animation" scene prop (for example a door's `openAnim`).

Today every imported model is frozen in its bind pose, and the importer only
warns. Doors, windmills, characters, and anything from an asset store that
moves cannot be used. This plan adds them in four independent-ish phases.
Phases 1 and 2 are the minimum useful slice. Phase 3 (skinning) needs phase
1. Phase 4 (morph targets) is optional; ship without it if it gets hairy.

## Current state

### Importer (`j3c/src/gltf.rs`, 967 lines, Rust, no crates)

It drops animations, skins, and morph targets with warnings:

```rust
// j3c/src/gltf.rs:594
    if primitive.get("targets").is_some() {
        warn(&mut ctx.warnings, "morph targets were ignored");
    }
```

```rust
// j3c/src/gltf.rs:680
    if node.get("skin").is_some() {
        warn(&mut ctx.warnings, "skinning was ignored");
    }
```

```rust
// j3c/src/gltf.rs:807
    if root
        .get("animations")
        .map(arr)
        .is_some_and(|xs| !xs.is_empty())
    {
        warn(&mut warnings, "animations were ignored");
    }
```

Accessors support `SCALAR`/`VEC2`/`VEC3`/`VEC4` of u8/u16/u32/f32 only
(`gltf.rs:212-224`); `MAT4` is rejected, and `normalized` is ignored.
`Accessor::scalar` (`gltf.rs:246`) returns raw integer values.

Nodes become `Part`s (`gltf.rs:473`), written by `write_scene`
(`gltf.rs:748`) as ids `1..n` in `Part` order, parents first. Each glTF node
becomes a GROUP part; its mesh primitives become MESH child parts
(`gltf.rs:698-727`). Rotations are written as Euler radians by
`quat_euler` (`gltf.rs:487`), which inverts `Rz * Ry * Rx`, the order
`scene3d.local` uses:

```j3
// j3c/stdlib/j3/scene3d.j3:574
pub fn local(n: Node): Mat4 {
    let m = Mat4.translate(n.position.x, n.position.y, n.position.z);
    let rx = Mat4.rotate(Vec3.xyz(1, 0, 0), n.rotation.x);
    let ry = Mat4.rotate(Vec3.xyz(0, 1, 0), n.rotation.y);
    let rz = Mat4.rotate(Vec3.xyz(0, 0, 1), n.rotation.z);
    let scale = Mat4.scale(n.scale.x, n.scale.y, n.scale.z);
    return m.mul(rz).mul(ry).mul(rx).mul(scale);
}
```

`import(project, scene, source)` writes the scene to the path it is given.
Callers pass a fragment under `target/j3-editor/`. The editor then pastes it
into the open scene, or, with "Import as a reusable scene", moves it to
`assets/imported/<name>.j3scene` via `keep_import` (`j3c/src/editor.rs:876`).
The editor's import handler is `j3c/src/editor.rs:1304-1318`:

```rust
            let as_scene = parts.next() == Some("scene");
            import_count += 1;
            let file = format!("import-{import_count}.j3scene");
            let fragment = root.join("target/j3-editor").join(&file);
            let result = import_asset(root, &fragment, path).and_then(|w| {
                let placed = if as_scene { Some(keep_import(root, &fragment, path)?) } else { None };
```

The MCP `import` tool (`j3c/src/mcp.rs:419-425`) always calls `keep_import`.

Test: `shared_meshes_warnings_and_cleanup` (`gltf.rs:906`) asserts
`"skinning was ignored"` is one of exactly 2 warnings (`gltf.rs:918-920`). Phase 3 changes that
assertion.

### `j3.game.anim` (`j3c/stdlib/j3/game/anim.j3`, 146 lines)

`Key { t, pos: Vec3, rot: Quat, scale: Vec3 }` (`anim.j3:65`) and
`Track { keys: []Key }` with `sample(t): Mat4` (`anim.j3:91`). It holds the
end keys, lerps pos/scale, and nlerps rot in `poseBlend` (`anim.j3:128`). The
sprite `Clip` name is taken (`anim.j3:16`). There is no file loader.

### `j3.math`

`Quat` has `identity`, `xyzw`, `fromAxisAngle`, `mul`, `normalize`, `rotate`,
`toMat4` (`math.j3:494-553`). There is no quat→Euler. `Mat4.affineInverse`
exists (`math.j3:422`). `atan2` and `asin` exist (`math.j3:45,60`).

### scene3d (`j3c/stdlib/j3/scene3d.j3`, 3146 lines)

- `Node` (`scene3d.j3:47-92`) has `position`, `rotation` (Euler radians), `scale`, `owner`, `source`, `scene`.
- `copyOf(scene, id)` (`scene3d.j3:1831`) returns the nearest INSTANCE at or above `id`, else 0.
- `graft` (`scene3d.j3:1495`) copies placed nodes. `owner` is the *outermost* instance, and `source` is set only at depth 1 (`scene3d.j3:1533-1534`). So ids cannot find nodes of a nested copy. Name paths can.
- Mesh cache: `Shared { asset, texture, gpu, image, verts, lo, hi, refs }` (`scene3d.j3:2052`). `reload` (`scene3d.j3:2682`) reuses `sharedSlot(asset, texture)` and uploads with `gfx.uploadUv` (12 floats per vertex). `meshVerts` (`scene3d.j3:2262`) box-projects UVs unless the file is 12-float.
- `drawWith(scene, transforms)` (`scene3d.j3:3013`) → `drawNode(n, model)` (`scene3d.j3:3105`) → `gfx.draw`/`drawCutout`/`drawBlended`/`drawDoubleSided`.
- Props: `declareProp(scene, name, kind, value)` (`scene3d.j3:1785`), `prop`/`propNumber`/`propInt`/`propFlag` (`scene3d.j3:1844-1876`). The kind is a free string.

### gfx (`j3c/stdlib/j3/gfx.j3`, 3159 lines)

```j3
// j3c/stdlib/j3/gfx.j3:1541
fn bindLayout(stride: i32, uv: bool) {
    glEnableVertexAttribArray(0);
    ...
    glVertexAttribPointer(3, 1, GL_FLOAT, 0, stride, 36 as ptr);
    if uv {
        glEnableVertexAttribArray(4);
        glVertexAttribPointer(4, 2, GL_FLOAT, 0, stride, 40 as ptr);
```

`STRIDE12: i32 = 48` (`gfx.j3:124`). The shaders are string functions:
`depthVert` (`gfx.j3:1416`, uses locations 0 and 4), `litVert`
(`gfx.j3:1426`, locations 0–4), `litVertInst` (`gfx.j3:1436`, locations 5–8
for the instance matrix, a separate program). `drawMesh` (`gfx.j3:2232`) is the one
draw path for depth and lit programs. `uMat(loc, m)` uploads one mat4
(`glUniformMatrix4fv(loc, 1, …)`).

### Editor (`j3c/editor/src/`)

- Prop kinds: `const KINDS: string = "float|int|bool|text|material|mesh|scene";` (`ui_props.j3:18`), and `propKind` in `tool.j3:433-436` for the CLI/MCP.
- The INSTANCE inspector section is `ui_inspector.j3:357-370` and ends with `copyProps(ui, w, n)`.
- `drawViewport(w)` (`viewport.j3:146`) draws with `cachedWorlds(w)` (`gizmo.j3:181`), which is cached until `w.worldsReady = false`.
- Frame time: `game.ticks()` (ms), as `drawFps` uses it (`overlay.j3:202-208`).
- `overrides.j3:60-95` writes `over.*` lines when a placed node's fields differ from `w.placedBefore`. Animated preview must not leave node fields changed, or it will write overrides.

### Checks that run without GL

`j3c/examples/scene3d_check.j3` prints `ok`. It is locked by
`scene3d_check_j3_runs` in `j3c/src/compile.rs:900`.

## Design

### The clip file: `<scene>.j3anim`

The file sits next to the scene it animates, with the same stem:
`assets/imported/door.j3scene` → `assets/imported/door.j3anim`. It is line
text, like `.j3mesh`. Tokens are space-separated. `#` lines are comments.
Names are the rest of the line, so they may contain spaces.

```
# j3anim 1
clip 1.250000 Open
track 3 door/Frame/Hinge
0.000000 0 0 0  0 0 0 1  1 1 1
0.625000 0 0 0  0 0.3826834 0 0.9238795  1 1 1
1.250000 0 0 0  0 0.7071068 0 0.7071068  1 1 1
clip 2.000000 Close
...
```

- `clip <length> <name…>` starts a clip.
- `track <keyCount> <path…>` starts a node track. Then `keyCount` lines follow: `t px py pz qx qy qz qw sx sy sz`, sorted by `t`. That is exactly `anim.Key`.
- `weights <keyCount> <targetCount> <path…>` (phase 4 only) is followed by `keyCount` lines `t w0 … w(targetCount-1)`.
- A **path** is node names joined by `/`, starting at the scene file's top-level node. For an import that is the root group named after the file, so paths look like `door/Frame/Hinge`. At runtime a path is resolved under a placed copy (its children are the file's top-level nodes), or from the scene's own top level when copy is `0`. Paths work at every nesting depth and survive paste. Ids do neither.
- The importer resamples glTF's separate T/R/S channels onto the union of their key times, so each track is a plain `anim.Track`. STEP gets an extra key 0.0001 s before each next key. CUBICSPLINE uses the middle (value) element as linear and warns `cubic spline animation was imported as linear`.

To make paths unique, the importer renames duplicate sibling names
`Name`, `Name_2`, `Name_3`, and replaces `/` and `|` in names with `_`.

### Runtime API (all new, minimal)

`j3.math`:
- `Quat.toEuler(this): Vec3`, the same formula as `gltf.rs` `quat_euler`, radians, inverse of `local`'s `Rz*Ry*Rx`.

`j3.game.anim`:
- `Track.sampleKey(this: &Track, t: f32): Key`. `sample` becomes `pose(this.sampleKey(t))`, with the same behavior.
- `pub struct NodeTrack { path: string; track: Track; }`
- `pub struct NodeClip { name: string; length: f32; tracks: []NodeTrack; }`
- `pub fn parseClips(src: []u8, arena: &Arena): []NodeClip` and `pub fn loadClips(path: string, arena: &Arena): []NodeClip` (empty when the file is missing).
- `pub fn clipNamed(clips: []NodeClip, name: string): int` (index or -1).

`j3.scene3d`:
- `pub fn animPath(scenePath: string): string` swaps `.j3scene` for `.j3anim`.
- `pub fn findPath(scene: &Scene, copy: int, path: string): int` returns the node id at `path` under copy `copy` (0 = the scene's own top level), else 0. It matches the first path segment against children of `copy`, then walks down.
- `pub fn bindClip(scene: &Scene, copy: int, clip: &NodeClip, arena: &Arena): []int` returns one node id per track (0 for unresolved). Call it once; ids are stable.
- `pub fn animate(scene: &Scene, ids: []int, clip: &NodeClip, t: f32)` writes `position`, `rotation` (`toEuler`), and `scale` of each bound node from `sampleKey(t)`. `t` is clamped by the track. For looping, the caller passes `t - floor(t / length) * length`.

Game code, a door that opens:

```j3
let clips = anim.loadClips(scene3d.animPath(door.scene), arena);
let open = anim.clipNamed(clips, scene3d.prop(&scene, doorId, "openAnim", "Open"));
let ids = scene3d.bindClip(&scene, doorId, &clips[open], arena);
// each frame
scene3d.animate(&scene, ids, &clips[open], time);
scene3d.draw(&scene);
```

### Skinning (phase 3)

- `# j3mesh 3`: 20 floats per vertex = the 12-float layout + `j0 j1 j2 j3 w0 w1 w2 w3`, where joint indices are floats and weights sum to 1. After the vertices comes `joints N` and then N lines of 16 floats: inverse bind matrices, column-major, in skin joint order.
- Scene node field `joints s=<path>|<path>|…`, written on the MESH part of a skinned glTF node. The paths use the same rules as clip paths. They resolve relative to `copyOf(scene, meshId)`: the placement of the file that defines the mesh.
- gfx: `MAX_BONES = 48` (keeps `uBones` inside GL 3.3's 1024 vertex uniform components). `bindLayout` enables locations 5 (`aJoints`, vec4 at byte 48) and 6 (`aWeights`, vec4 at byte 64) for stride 80. `litVert` and `depthVert` get `uniform int uSkinned; uniform mat4 uBones[48];` and use `skin = Σ w_i * uBones[int(j_i)]` when `uSkinned > 0`. One program, one branch: no new programs. `gfx.uploadSkinned(verts): Mesh` and `gfx.skin(bones: []Mat4)` set state for the next draws; an empty slice turns it off, like `castShadow`.
- Bone matrix: `bone[j] = inverse(meshWorld) * world(joint_j) * ibm_j`. `uModel` then puts it back in world space. Use `Mat4.affineInverse`.
- The translucent shadow program and instanced draws stay unskinned: they draw the bind pose. URP traces, picking, and bounds use the bind pose too. These are documented limits.

### Morph targets (phase 4, optional)

- `<mesh>.j3morph` next to the `.j3mesh`: `# j3morph 1`, then `<targets> <vertexCount>`, then per target per vertex `dx dy dz dnx dny dnz`.
- A mesh with a `.j3morph` sibling skips `SHARED` and gets its own GPU mesh. Its base verts are kept on the node, and `unload` frees it directly. `animate` writes weights into the node (`weights: [8]f32`, `morphDirty: bool`). `drawWith` rebuilds the verts on the CPU when dirty and calls `gfx.updateUv`.

## Commands you will need

From `j3c/` unless noted. Don't use cargo except where a step says so: `cargo test` rebuilds and copies `~/.bin/j3c`.
The stdlib and editor are read from disk with the env vars below, so J3
changes need no rebuild.

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| scene3d/anim self-check | `J3_STDLIB=$PWD/stdlib timeout 60 ~/.bin/j3c run --file examples/scene3d_check.j3` | prints `ok` |
| Editor typechecks and builds | `J3_EDITOR_SRC=$PWD/editor/src J3_STDLIB=$PWD/stdlib timeout 300 ~/.bin/j3c -e scene --project ../examples/3d-editor-scene list` | exit 0, lists objects |
| Importer tests (Rust) | `timeout 600 cargo test gltf -- --test-threads=1` | all pass |
| scene3d test (Rust wrapper) | `timeout 600 cargo test scene3d_check_j3_runs -- --test-threads=1` | pass |

Never launch the editor or a game window. Use `timeout` on everything.

## Scope

**In scope**:
- `j3c/src/gltf.rs` (animations, skins, morph export; unique names; `MAT4` and `normalized` accessors; tests)
- `j3c/src/editor.rs` import handler: an import that wrote a `.j3anim` is kept as a reusable scene (and `keep_import` moves the `.j3anim` along)
- `j3c/stdlib/j3/math.j3` (`Quat.toEuler`)
- `j3c/stdlib/j3/game/anim.j3` (node clips, loader)
- `j3c/stdlib/j3/gfx/mesh.j3` (phase 3: `# j3mesh 3` stride 20 and `inverseBinds`)
- `j3c/stdlib/j3/scene3d.j3` (path lookup, `animate`, skin/morph draw, `joints` field)
- `j3c/stdlib/j3/gfx.j3` (phase 3: layout and shaders; `uploadSkinned`, `skin`)
- `j3c/editor/src/ui_props.j3`, `tool.j3` (the `animation` prop kind), `ui_inspector.j3`, `viewport.j3`, `types.j3`, `main.j3` (preview)
- `j3c/examples/scene3d_check.j3` (new checks)
- Docs: `EDITOR.md` (move the Animation items to Implemented), `j3c/AGENTS.md` one-line mentions for `j3.game.anim` and `j3mesh 3`

**Out of scope**:
- Animation blending/cross-fades, events, root motion, IK
- An animation *editor* (keyframing in the editor)
- Skinned translucent shadows, skinned instancing, and skinned URP traces (bind pose is fine)
- More than 48 bones per skin (the importer warns and imports unskinned)
- Sparse accessors (they already fail)
- `plans/README.md`

## Git workflow

Branch `plan-009-animation`. One commit per phase, e.g.
`Import glTF node animation into .j3anim clips and play them in scene3d.`
Do not push. Do not update `plans/README.md`.

## Steps

### Phase 1: node animation clips

#### Step 1: `Quat.toEuler` and `Track.sampleKey`

In `math.j3`, add `fn toEuler(this: Quat): Vec3` to `Quat`. Port
`quat_euler` from `gltf.rs:487` line for line (normalize first).

In `anim.j3`, split `poseBlend` into `keyBlend(a, b, u): Key` (the current
lerp/nlerp) and `pose`. Add `Track.sampleKey(t): Key` with the same
hold/search logic as `sample`, and make `sample` return `pose(this.sampleKey(t))`.
An empty track returns `Key.at(0, Vec3.zero(), Quat.identity(), Vec3.xyz(1,1,1))`.

In `scene3d_check.j3` `main`, add: for a quat from `fromAxisAngle(Vec3.xyz(0,1,0), 1.2)`
and one from a composed X*Y*Z rotation, build a Node with that `toEuler` rotation.
Check that `scene3d.local(node)` matches `q.toMat4()` within 0.001 on all 16 entries.
`panic` on mismatch.

**Verify**: the scene3d self-check → `ok`.

#### Step 2: clip file parser

In `anim.j3` add `NodeTrack`, `NodeClip`, `parseClips`, `loadClips`, and
`clipNamed` as in Design. Reuse `save.parseFloat` for numbers.
`gltf.rs` writes `{:.6}`, so there are no exponents. Walk tokens with a small cursor like
`mesh.j3`'s `Cursor`/`skip`. Copy name strings into `arena`. Unknown
line heads and short key lines make `parseClips` return what it has so far. Never panic
on a bad file.

In `scene3d_check.j3` add a check that parses a two-clip literal (build the
bytes with `Text`), with one track of 3 keys whose path contains a space.
Check the clip count, names, lengths, the path, and `sampleKey(0.3125).pos`,
which is halfway between keys 0 and 1.

**Verify**: self-check → `ok`.

#### Step 3: `findPath`, `bindClip`, `animate`, `animPath`

Add them to `scene3d.j3` next to `copyOf` (`scene3d.j3:1831`). `findPath` is a
linear scan per segment over `scene.nodes[0..len]` matching `parent` and
`name`. Scenes are small; add a `// ponytail:` note naming the O(n·depth) cost.
`animate` writes node fields only. It doesn't touch `params` or `data`, so it
never dirties the file.

In `scene3d_check.j3`: build a scene `Root/Arm` with `scene3d.add`. Put an
INSTANCE-less copy (copy 0) and a clip that moves `Root/Arm` from x=0 to x=2
over 1 s. Bind it, `animate(t=0.5)`, and check `position.x == 1` (±0.001) and that
`findPath(…, "Root/Nope")` is 0. Also graft a scene: write the same
scene to `SUB` (the file the check already uses for placed scenes) and place
it with an INSTANCE. `findPath(scene, instanceId, "Root/Arm")` must find the
placed Arm, and animating it must move only that copy.

**Verify**: self-check → `ok`.

#### Step 4: importer writes `.j3anim`

In `gltf.rs`:
1. Add `"MAT4" => 16` to the accessor type match (for phase 3). Add a `normalized: bool` to `Accessor`. `scalar` then divides u8 by 255 and u16 by 65535 when it is set.
2. Unique sibling names: in `write_scene`'s caller (`convert`), before `write_scene`, rename duplicates among parts that share a parent (`Name`, `Name_2`, …). Replace `/` and `|` in names with `_`.
3. Record `node_part: HashMap<usize, usize>` (glTF node → part index) in `Ctx` inside `append_node`.
4. `fn part_path(parts, index) -> String` joins names from the root part down with `/`.
5. `fn write_clips(root, buffers, parts, node_part, out: &Path, warnings)`. For each animation (name, else `Clip<i>`): group channels by target node, skipping `weights` in this phase with the warning `morph target animation was ignored`. Collect the union of input times per node and sample each path at each time: LINEAR lerps (nlerp for rotation), STEP holds, CUBICSPLINE reads element `3*k+1` as linear and warns once. A missing path uses the node's rest TRS; read it from the glTF node (`translation`/`rotation`/`scale`), not the Euler part. Write the format above to `out.with_extension("j3anim")`. Clip length is the max time.
6. Call it from `convert` when `animations` is non-empty. Remove the `animations were ignored` warning.
7. On import failure, the existing cleanup removes `assets/imported/<prefix>_*`. The `.j3anim` sits next to the fragment scene instead, so remove `scene.with_extension("j3anim")` in `import`'s error branch too.

In `editor.rs`: `keep_import` also renames `fragment.with_extension("j3anim")`
when it exists, to the kept scene's stem. In the `J3EDITOR IMPORT` handler, set
`as_scene = true` when `fragment.with_extension("j3anim")` exists after
`import_asset`. Append ` (kept as a scene for its animation)` to the note. A
pasted copy would lose its clip.

Tests (in `gltf.rs` `mod tests`, new fn `animation_clips`): write a `.gltf`
with an external `anim.bin` (write the bytes from the test with
`f32::to_le_bytes`; no base64 needed). It has two nodes (`Door` with child `Hinge`,
and a second child also named `Hinge`), one animation with a LINEAR translation
channel on the first `Hinge` (times `[0, 1]`, values `[0,0,0, 2,0,0]`) and a STEP
rotation channel on the same node. Assert:
- no `animations were ignored` warning;
- the `.j3anim` beside the scene exists and starts with `# j3anim 1`;
- it contains `track ` and the path `m/Door/Hinge` (stem `m`);
- the scene contains `name s=Hinge_2`;
- the key count equals the union of times plus the STEP extra keys.

Update `shared_meshes_warnings_and_cleanup` only if its asserted warnings change.
They should not in this phase.

**Verify**: `timeout 600 cargo test gltf -- --test-threads=1` passes.

#### Step 5: editor preview

Minimal scrubber on a selected placed copy:
- `types.j3` `World`: `animClips: []NodeClip`, `animFor: int` (instance id the clips are for), `animClip: int`, `animTime: f32`, `animPlay: bool`, `animFrom: int` (ticks). Initialize them in `main.j3` next to `importAsScene: false`.
- `ui_inspector.j3`, in the INSTANCE block right before `copyProps` (`ui_inspector.j3:369`): when `w.animFor != n.id`, load `anim.loadClips(scene3d.animPath(n.scene), <editor arena>)` and reset the fields. If there are clips, show a section `ANIMATION`: a button that cycles the clip name, a `Play`/`Pause` toggle, and `ui.number("time", &w.animTime, 0.01)` clamped to `0..length`.
- `viewport.j3` `drawViewport`: if `w.animFor` is set, the instance still exists, and it is selected, then advance `animTime` by `(game.ticks() - w.animFrom) / 1000` while playing (looping), bind with `bindClip`, **save** the TRS of the bound nodes, `animate`, compute fresh matrices with `scene3d.worlds` into a scratch array, draw with those, and **restore** the TRS before returning. Don't touch `w.worldsReady`. This way nothing reaches undo, overrides, or the saved file.

**Verify**: the editor build command exits 0. Do not open the window. The user tests visually.

#### Step 6: docs

`EDITOR.md`: move "glTF animation" to Implemented, one bullet: clips in
`<scene>.j3anim`, `scene3d.animate`, the inspector preview. `j3c/AGENTS.md`: in the
long stdlib line, change "`j3.game.anim` is sprite clips and a rigid transform
track" to add "and node clips (`.j3anim`, `loadClips`)".

**Verify**: `git diff --stat` shows only in-scope files.

### Phase 2: `animation` prop kind

#### Step 7

- `ui_props.j3:18`: `KINDS` gains `|animation`.
- `tool.j3:433-436` `propKind`: accept `"animation"`.
- `ui_props.j3` `copyProps`: for kind `animation`, load `anim.loadClips(scene3d.animPath(n.scene), scratch(0))`. Show a button with the current value that cycles through the clip names, calling `setProp`. With no clips, fall back to the text field that exists now.
- The value is a clip name (a string). Code reads it with `scene3d.prop` + `anim.clipNamed`, as in the Design example.

**Verify**: the editor build command exits 0, and
`timeout 60 ~/.bin/j3c -e scene --project ../examples/3d-editor-scene --help`
still lists `prop.<name>=<type>:<default>`. (The `--help` text doesn't list
kinds. If `scene.rs` validates kinds, add `animation` there too.)

### Phase 3: skinning

#### Step 8: mesh format and gfx

- `mesh.j3`: `headerStride` returns 20 for `j3mesh 3`. Add `pub const STRIDE_SKIN: int = 20`. Add `pub fn inverseBinds(src: []u8, arena: &Arena): []f32` (16 floats per joint, read after the vertex block's `joints N`, empty if absent).
- `gfx.j3`: `const STRIDE20: i32 = 80`. In `bindLayout`, when `stride == STRIDE20`, also enable 5 and 6 (`glVertexAttribPointer(5, 4, GL_FLOAT, 0, stride, 48 as ptr)` and `(6, 4, …, 64 as ptr)`); otherwise disable them. Add `uploadSkinned(verts)` (like `uploadUv`, stride 20), `pub const MAX_BONES: int = 48`, and `pub fn skin(bones: []Mat4)` (stores up to 48, count 0 = off).
- Shaders: edit `depthVert` and `litVert` as in Design. `vNrm = mat3(uModel) * mat3(skin) * aNrm;`.
- `drawMesh`: find `uSkinned`/`uBones[0]` locations once in init, like the other `G.l*`/`G.d*` fields. Per draw, set `uSkinned` and, when on, `glUniformMatrix4fv(loc, count, 0, &bones[0].m[0])` for both the depth and lit programs. `uMat` stays as is.

**Verify**: the scene3d self-check → `ok` (compiles gfx). The editor build command exits 0.

#### Step 9: importer writes skins

In `gltf.rs`, for a node with `skin` and `mesh`:
- Joints over 48: warn `skins over 48 joints were imported unskinned` and fall back to today's path.
- Otherwise the primitive writes `# j3mesh 3`. Each vertex is the 12 floats (UV zeros when untextured) plus `JOINTS_0` (u8/u16 as floats) and `WEIGHTS_0` (normalized; renormalize to sum 1). After the vertices write `joints N` and the IBMs (identity when `inverseBindMatrices` is absent).
- The MESH part gets `joints s=<paths joined by |>` (add `joints: String` to `Part`, and write it in `write_scene` only when non-empty).
- One mesh used by two different skins: warn `a mesh shared by several skins uses the first skin` and keep the first.
- Remove `skinning was ignored`. Update the old test's assertion accordingly: it now expects the TEXCOORD warning only if the test mesh has no JOINTS, so the skinned node falls back. Pick the simplest correct assertion and explain it in a comment.

New test `skinned_mesh`: one triangle, `JOINTS_0` u8 and `WEIGHTS_0` f32, two
joints, an IBM accessor. Assert `# j3mesh 3`, a `joints 2` line, and
`joints s=m/…|m/…` in the scene.

**Verify**: `timeout 600 cargo test gltf -- --test-threads=1` passes.

#### Step 10: scene3d draws skinned meshes

- `Node`: `joints: string` (parsed in `setField`, written in `write` for MESH when non-empty, and copied by `graft` as-is, since paths need no remap). Add `jointIds: []int` (runtime, not saved).
- `Shared`: `ibm: []f32`.
- `reload`: for a stride-20 file, upload with `gfx.uploadSkinned`, keep a 12-float copy for `verts` (bounds, picking, URP), and store `ibm`. Resolve `jointIds` with `findPath(scene, copyOf(scene, n.id), path)` per `|` segment.
- `drawWith` → `drawNode`: when `jointIds.len > 0` and the shared `ibm` has the same count, build bones from `transforms` (use `index` to map ids to slots) and call `gfx.skin(bones)` before the draw and `gfx.skin(empty)` after.
- Self-check: build a skinned scene in memory (no GL): nodes, a `joints` string. Check `findPath` resolves every segment. Add a pure helper `pub fn bones(scene, transforms, meshSlot, ibm, out)` and check that with identity IBMs and the joint moved +1 x, `bones[0]` translates by +1 x relative to the mesh.

**Verify**: self-check → `ok`; editor build exits 0.

### Phase 4 (optional): morph targets

#### Step 11

Only if phases 1–3 are merged and green. Importer: write `<mesh>.j3morph`
(position and normal deltas for the expanded triangle vertices, up to 8
targets, and warn past 8). Write `weights` tracks in the clip. Runtime: see
Design. `reload` must not insert morph meshes into `SHARED`. `unload` must
free them directly: check `unload`'s search (`scene3d.j3:2657`), which assumes
SHARED ownership. Self-check: a 1-triangle base plus a 1-target delta at
weight 0.5 gives half the delta (a pure CPU helper `morph(base, deltas, weights, out)`).

**Verify**: self-check `ok`; `cargo test gltf` passes; editor build exits 0.

## Test plan

- `scene3d_check.j3` covers Euler round trips, clip parsing, path binding (own and placed copy), animate, bone math, and morph math. Run it through `scene3d_check_j3_runs`.
- `gltf.rs` tests `animation_clips` and `skinned_mesh` cover importer output. The existing test is updated for the removed skin warning.
- Visual check (user, not you): import a glTF with a door animation as a reusable scene, place it, select it, scrub.

## Done criteria

- [ ] `J3_STDLIB=$PWD/stdlib timeout 60 ~/.bin/j3c run --file examples/scene3d_check.j3` prints `ok`
- [ ] `timeout 600 cargo test gltf -- --test-threads=1` passes
- [ ] `timeout 600 cargo test scene3d_check_j3_runs -- --test-threads=1` passes
- [ ] Editor build command exits 0
- [ ] `rg "animations were ignored|skinning was ignored" j3c/src` has no hits (phase 3 done); `morph targets were ignored` stays unless phase 4 shipped
- [ ] EDITOR.md "Still left → Animation" lists only what was not done
- [ ] Only in-scope files changed; `plans/README.md` untouched

## STOP conditions

- An excerpt in "Current state" doesn't match the live file (beyond a few lines' shift).
- `Quat.toEuler` round trip fails for a non-degenerate rotation after porting `quat_euler` exactly: the rotation order assumption is wrong. Report which matrix entries differ.
- `graft` or the paste path changes node names, so paths no longer resolve. Report before renaming anything in scene3d.
- GL uniform limits: if the lit program fails to link after adding `uBones[48]` (check the program log `gfx` prints), STOP. Do not lower the bone count without asking.
- Phase 4 needs changes to how `SHARED` refcounts work beyond "don't insert and free directly". STOP and report; phase 4 is optional.

## Maintenance notes

- Paths are the contract between `.j3anim`, `joints`, and the scene. A renamed node breaks its tracks silently: `bindClip` returns 0 for it. The inspector could warn later.
- `animate` writes node fields. Editor code that animates must restore them in the same frame, or overrides get written.
- Adding a vertex layout means `bindLayout`, `pickStride` (`gfx.j3:1561`), and `mesh.headerStride` must agree. `pickStride` should never guess 20; skinned meshes only go through `uploadSkinned`.
- `MAX_BONES` is also in the shader strings; change both.
