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
| TODO | 015 | Rendering gaps (lights, planar reflections, textured traces, cutoff) | P3 | M | 001 (for C); coordinate with 006/010 |
| TODO | 011 | Editor extension API | P3 | L | 002 (nice to have) |
| TODO | 008 | Incremental builds | P3 | L | — |
| TODO | 013 | Editor and tools on Windows and macOS | P3 | L | — |
| TODO | 016 | Move editor and asset logic into J3 | P2 | L | 001, 004 |
| TODO | 017 | Everyday game framework gaps | P2 | L | 016 JSON reader for phase 4 only |
| TODO | 009 | glTF animation, skinning, morph targets | P2 | L | — (deferred to last) |
| DONE | 022 | `.j3ui` components and `j3.gfx.paint` (8 stages, own folder: `022-j3ui-components/`) | P2 | XL | — |
| IN PROGRESS | 023 | `.j3ui` for desktop apps: stages 1–3 DONE; paused before 4 (dynamic components), then rich input, data grid, CSS gaps and gamepad focus. [Restart handoff](023-handoff.md). | P1 | XL | 022 |
| TODO | 025 | Steam integration (later): Steamworks via FFI, achievements, cloud saves, overlay, Steam Input, depot packaging | P3 | L | 013, 017 item 5, 023 stage 16 |
| TODO | 024 | Accessibility platform bridge: AT-SPI over libdbus FFI first, then UI Automation and NSAccessibility | P2 | XL | 023 (old stage 12 step 1, done) |
| DONE | 026 | UI/CSS features: drag-and-drop, flip, transitions, focus-visible, grid, text, colors, transforms, nesting, selectors, elements, stacking, inspector, hot reload, i18n, gamepad, a11y gaps, vertical-align, box-decoration-break, overflow axes; AT-SPI chunks 32-33 skipped, stay plan 024 ([chunks](026-ui-css-features.md)) | P1 | XL | 023 |
| DONE | 027 | UI open issues from 026: rotated clip, RTL bidi neutrals, Windows check, spacing transitions, em in transform, url() vars, scroll stacking, tabindex, radio, glyph ([chunks](027-ui-open-issues.md)) | P2 | L | 026 |
| DONE | 028 | Run J3 apps on Android: APK build, SDL2 `SDLActivity`, GLES 3.0, touch, asset copy (implemented, untested on device; chunk 11 is the device test) ([chunks](028-android.md)) | P3 | XL | — |

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
