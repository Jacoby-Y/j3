# Plan 019: Two-level URP trace

## Status

- **Status**: DONE (2026-09-26)
- **Category**: performance
- **Touches**: `j3c/stdlib/j3/scene3d.j3` (trace section), `gfx.j3` (`Traced`, uploads, the lit shader's trace code), `math.j3` (`Mat4.inverseAffine`)

## Why

URP traced one BVH over every triangle in world space and rebuilt all of it (transform, `bvh.build`, tens of MB uploaded) whenever any traced object moved. The village's `scripts/sway.j3s` moves every crop every frame, so the village ran at about 6 FPS with URP on.

## What it is now

- **Meshes** (`traceMeshes`): every loaded `SHARED` mesh's triangles in its own space with its own BVH, uploaded once. The upload is redone only when a mesh loads, unloads or reloads (`meshSignature`).
  - Triangles are 7 texels: corner, two edges, three normals, color.
  - Copies of a mesh share one set of triangles, so `TRACE_MAX` counts each unique mesh once.
- **Objects** (`traceObjects`): each traced node's world-to-object rows and its look (tint, mirror, glow, ior, roughness, opacity, cloud, mesh root), 6 texels each, plus a BVH over their world boxes.
  - The object BVH comes from `bvh.build`, fed each box as the triangle (lo, hi, center).
  - This part is redone when `traceSignature` changes, and it's cheap.
- **Shader**: `traceRay` and `occluded` are each one loop with one stack.
  - The loop walks the object tree. At a leaf it pushes the leaf's objects as negative entries.
  - Popping one moves the ray into that object's space (unnormalized, so distances stay world distances) and walks its mesh tree above a saved stack depth, then returns to world space.
  - A first version nested a mesh-walk function inside the object loop. The NVIDIA driver crashed in `glLinkProgram` on some variants, which showed up as editor crashes (`free(): invalid pointer`, SIGSEGV).
  - The driver's disk shader cache hid it once a variant compiled. Test shader changes with `__GL_SHADER_DISK_CACHE=0`.
  - `hitNormal`, `hitMat`, `hitFace` replace the per-triangle material reads.
  - Uniforms: `uTop` on unit 8, `uObjects` on unit 9. `uInst` was taken by the instanced vertex shader.

## Numbers

Headless village, 600 frames, wall clock including startup, RTX 5060 Ti:

| Build | default | `VILLAGE_PRESET=traced` | `VILLAGE_POND=1` |
|---|---|---|---|
| One tree, rebuilt every frame | 6.2 | 5.9 | 6.1 |
| One tree, small motion skipped (stopgap) | 90 | 47 | 57 |
| Two levels (one loop) | 253 | 172 | 178 |

## Later

- Plan 015's textured traces need UVs per triangle; add a texel to the mesh layout.
- Mesh uploads redo every mesh when one changes. Append new ones if editor imports lag.
