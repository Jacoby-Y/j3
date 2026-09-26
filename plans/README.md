# Implementation Plans

Written on 2026-09-25 from EDITOR.md's "MCP Fixes", "Performance", and "Still left"
sections, after building `examples/editor_village` through MCP. The tree has uncommitted
work on top of `b086a5b`, so each plan drift-checks by comparing its "Current state"
excerpts to the live files.

Plan 016 follows a repo review of Rust game/editor logic. Plan 017 follows a comparison of
J3's game APIs with the official LibGDX and MonoGame documentation.

Each executor: read the plan fully before starting, honor its STOP conditions, work on the
plan's branch in the main checkout (the editor and example sources are untracked, so a
separate worktree won't have them), and update your row when done.

`cargo build` and `cargo test` in `j3c/` run `.cargo/copy-j3c.sh`, which moves the new
binary to `~/.bin/j3c`. That side effect is expected; plan 002 explains why it breaks
long-running MCP servers.

## Execution order & status

| Status | Plan | Title | Priority | Effort | Depends on |
|--------|------|-------|----------|--------|------------|
| DONE | 002 | Agent tool robustness | P1 | M | — |
| DONE | 001 | Import pipeline (PNG decode, batch import, bake, recolor) | P1 | L | — |
| DONE | 003 | J3 language gaps (`%`, `text.contains`, `module.CONST`) | P2 | M | — |
| DONE | 004 | Faster scene authoring (batch add, grid, file undo) | P2 | M | 002 |
| DONE | 005 | First-person starter kit (`j3.scene3d.fpwalk`, `drawFrame`, fps template) | P2 | M | — |
| DONE | 007 | Scene drawing performance and a frame profiler | P2 | L | — |
| DONE | 006 | URP tracing performance (traced shadows first) | P1 | L | 007 steps 1-2 (soft) |
| DONE | 014 | Editor polish (editing gaps, UI polish, editor speed) | P2 | M | — |
| DONE | 012 | Online asset browser (Poly Haven, ambientCG, Kenney) | P2 | L | 001 (textures) |
| DONE | 010 | Shader effects (`.j3fx`) | P2 | L | — |
| TODO | 015 | Rendering gaps (lights, planar reflections, textured traces, cutoff) | P3 | M | 001 (for C); coordinate with 006/010 |
| TODO | 011 | Editor extension API | P3 | L | 002 (nice to have) |
| TODO | 008 | Incremental builds | P3 | L | — |
| TODO | 013 | Editor and tools on Windows and macOS | P3 | L | — |
| TODO | 016 | Move editor and asset logic into J3 | P2 | L | 001, 004 |
| TODO | 017 | Everyday game framework gaps | P2 | L | 016 JSON reader for phase 4 only |
| TODO | 009 | glTF animation, skinning, morph targets | P2 | L | — (deferred to last) |

Status values: TODO | IN PROGRESS | DONE | BLOCKED (with one-line reason) | REJECTED (with one-line rationale)

## Dependency notes

- **002 first.** It fixes the stale MCP server (the root cause is the binary being replaced
  under a running server, not cache cleanup) and the editor cache growth. It also changes
  004's step 1: `scene_add` on a new file probably already works.
- **001 before 012 and 015-C.** Textures need PNG decoding without SDL2_image.
- **006, 010, and 015 all edit the lit shader** in `gfx.j3`. Run them one at a time and
  rebase the later ones.
- **007's profiler (steps 1-2)** makes 006's measurements easier; 006 can run alone with its
  own minimal timer.
- **014 phase H and 007 step 7 overlap** (editor watch/pick/place speed). Whichever runs
  first does it; mark it done in the other.

## Open decisions for the user

- 010: per-effect shader programs (recommended) vs one uber-shader.
- 011: hook naming convention (recommended) vs a new attribute; restart-to-reload
  (recommended) vs hot rebuild.
- 015-E: implement tileable simplex noise, or won't-do.
