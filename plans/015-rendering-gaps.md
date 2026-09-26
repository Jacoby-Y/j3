# Plan 015: Rendering gaps (lights, reflections without URP, textured traces, cutoff, noise)

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, do NOT update `plans/README.md`
> (the reviewer does that).
>
> **Drift check (run first)**: the tree has uncommitted work on top of
> `b086a5b`, so check by comparing the "Current state" excerpts below to the
> live files, not by SHA. On a mismatch, STOP.

## Status

- **Priority**: P3
- **Effort**: M (5 independent phases)
- **Risk**: MED (the lit shader is shared by everything)
- **Depends on**: 001 phase 1 (PNG decoding) for phase C's textured traces to be visible without SDL2_image. Coordinate with 006 (URP tracing) and 010 (shader effects), which also edit the lit shader: land whichever goes first, then rebase.
- **Category**: rendering
- **Planned at**: 2026-09-25

## Why this matters

EDITOR.md "Still left > Rendering gaps". Scenes with more than 8 lights silently lose
light. Without URP, mirrors show only the sky. URP reflections lose textures. Cutout
foliage can't tune its threshold. The village sample showed the texture one directly: its
palette textures had to be baked into vertex colors to show up in water reflections.

## Current state

**8 point lights, no shadows from them.** The lit shader (`j3c/stdlib/j3/gfx.j3:1433`) has
`uniform vec3 uLP[8]; uniform vec3 uLC[8]; uniform float uLR[8];` and `pointLit(...)`.
`scene3d.j3:1217-1235` (`lights`) feeds scene LIGHT nodes, then emissive meshes
("ponytail: first ones in scene order get the free slots; nearest-to-camera if scenes have
many"). The first loop (LIGHT nodes) doesn't check `used < LIGHTS`.

**Reflections without URP.** The lit shader's `skyAlong(d, rough)` samples the sky/sky box
for mirrors; glass refraction uses a framebuffer grab (`uGrab`, `gfx.grab()` called from
`scene3d.j3:3059`).

**Traced hits use material color only.** `buildTrace` packs 36 floats per triangle
(`scene3d.j3` near 2920-2947: `surf[count*36 + 27] = look.reflect; +31 ior; +32 rough;
+33 opacity; +34 cloud`). No UVs or texture id, so hits can't sample albedo.

**Alpha cutoff fixed at 0.5.** In the lit and depth shaders:
`if (uCutout > 0.5 && texture(uAlbedo, vUv).a < 0.5) discard;` (appears several times in
gfx.j3). `j3c/src/gltf.rs` warns "alpha cutoffs other than 0.5 were imported as 0.5".

**Noise kinds.** `j3c/stdlib/j3/gfx/noise.j3:13-17`: `VALUE PERLIN CELLS STRIPES` (no simplex,
because simplex doesn't tile on a square texture).

## Design (independent phases, cheapest first)

- **A. Cutoff per material and node (S).**
  - `uCutout` becomes the threshold itself (0 = off, else the cutoff), so shaders do
    `if (uCutout > 0.0 && a < uCutout) discard;`.
  - `.j3mat` gets `cutoff f=`; nodes get `cutoff` (saved like `opacity`).
  - The glTF importer writes `alphaCutoff` instead of warning.
- **B. More lights (M).**
  - Keep 8 per draw, but choose them per object: in `drawWith`, for each mesh node, pick
    the 8 lights nearest its bounds' center (lights collected once per frame into a list),
    and upload only when the set changes from the previous draw (compare ids).
  - Guard the first loop at `scene3d.j3:1220` with `used < LIGHTS`.
  - Spot lights: add `cone` (degrees) and a direction from the node rotation, and extend
    `pointLit` with a smoothstep cone factor (`uLS[8]` vec4: dir + cos). Light shadows
    are out of scope.
- **C. Textured traces (M).**
  - Add per-triangle UVs (3×2 floats) and a texture slot to the trace data; the triangle
    record grows from 36 floats.
  - Textures go into one texture array or atlas: simplest is a `sampler2DArray` of
    same-size layers (resize on upload to 256×256; ponytail: lower detail in reflections
    is fine).
  - `shadeHit` samples it when the slot is ≥ 0.
  - This grows memory, so it's opt-in per scene: `scene.env.urpTextures`, default on only
    if the atlas fits.
- **D. Reflections without URP (M).** Cheapest useful option: **a planar reflection for
  flat mirrors and water** (the village pond).
  - A material flag `planar=1` renders the scene once more, mirrored about the mesh's
    plane, into a half-resolution target (`gfx.makeTarget`), with a clip plane at the
    surface.
  - The lit shader samples it by screen position for that material.
  - Only the first planar surface on screen gets one (ponytail).
  - Non-flat mirrors keep the sky.
- **E. Tileable simplex (S), or won't-do.** Either implement 4D simplex sampled on a torus
  (x,y → cos/sin pairs), which tiles, as `SIMPLEX`, or document won't-do in EDITOR.md.
  Recommend doing it only if someone asks; PERLIN covers the look.

## Commands you will need

| Command | Purpose |
|---|---|
| `cd j3c && cargo build --release && cargo test --release` | build and tests |
| `timeout 120 ./target/release/j3c -e render <proj> --out /tmp/x.png` | visual check (headless) |
| `timeout 120 ./target/release/j3c run <proj> --frames 60 --screenshot /tmp/y.png --headless` | game-path check |
| `timeout 60 ./target/release/j3c run --file examples/scene3d_check.j3` | scene3d self-check |

Test scenes: `examples/3d-editor-scene` (glass and metal), `examples/editor_village` (pond,
many lanterns), and `examples/forest`/`raytrace` must keep working (they share gfx).

## Scope

In: phases A-D, and E as a decision. Out: shadows from point or spot lights, clustered
lighting, SSR, and reflection probes (possible follow-ups to D).

## Git workflow

Branch `plan-015-rendering-gaps`. One commit per phase.

## Steps

1. **Baseline renders.** Render the four test scenes (headless) and keep the PNGs in the
   scratch dir for comparison.
2. **Phase A.** Verify:
   - a J3 self-check that a material with `cutoff f=0.3` loads the value
     (`scene3d_check.j3` style)
   - baseline renders are unchanged (default 0.5)
   - a `cargo test` in `gltf.rs` that an `alphaCutoff: 0.3` material imports as 0.3 with no
     warning
3. **Phase B.** Verify:
   - a scene with 20 lights in a row: the render shows light at every one
   - a pure J3 helper `nearest8(lights, point)` has a self-check
   - baseline renders are unchanged (they have ≤ 8 lights)
4. **Phase C.** Verify: a scene with a textured quad reflected in a metal sphere shows the
   texture in the reflection (needs PNG loading: SDL2_image installed, or plan 001 done).
   `TRACE_MAX` memory is documented.
5. **Phase D.** Verify: a render of the village pond with URP off shows houses and trees in
   the water, and URP-on renders are unchanged.
6. **Phase E.** Decide with the user; record the outcome in EDITOR.md.
7. **Docs.** EDITOR.md "Rendering gaps" (remove the done items), `j3c/editor/README.md`
   (material `cutoff`, `planar`, and light `cone`), and the `j3c doc j3.scene3d` doc
   comments.

## Test plan

Self-checks for the pure helpers (A, B), a gltf unit test (A), and before/after headless
renders for every phase on the four test scenes.

## Done criteria

- Phases A-D are merged; the baseline scenes look the same except for intended changes.
- More than 8 lights light their surroundings.
- Mirrors and water reflect the scene with URP off (planar).

## STOP conditions

- Plan 006 or 010 changed the lit shader's structure since this plan was written: re-read
  and adapt the splice points, and stop if the change is structural (for example a
  separate traced program).
- Phase C's texture array would need GL above 3.3.
- Phase D's extra scene pass costs more than about 30% FPS on the village with URP off:
  make it half-res or every other frame, then stop if still too slow.

## Maintenance notes

- Any new per-material shader input needs: the `.j3mat` key, `fillMaterial`, the
  `Surface` field, the uniform upload, and the trace record (if URP should see it).
