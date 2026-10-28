# Stage 02: `j3.gfx.paint`, a batched 2D painter

## Done

**Shipped**
- `j3c/stdlib/j3/gfx/paint.j3` (`j3.gfx.paint`, GL only, main thread only): `rect`, `roundRect(pos, size, r, c)`, `roundRectEx(pos, size, Corners{tl,tr,br,bl}, fill, bw, border)`, `circle(center, r, c)`, `ring(center, r, width, c)`, `line(a, b, width, c)` (round caps), `text(font, pos, msg, c)`, `image(tex, pos, size)`, `imageSrc(tex, src, srcSize, pos, size, tint)`, `pushClip/popClip`, `pushTranslate(d)`, `pushScale(origin, s)`, `popTransform`, `pushAlpha/popAlpha`, `pushView(pos, size)/popView`, `flush()`. Transform, alpha, and clip stacks live in paint (CPU-side).
- The engine is in `j3.game` (it owns the GL loader and `platform`, and `game` cannot import `gfx.paint`): `pub fn p2Push(kind, x, y, w, h, u0, v0, du, dv, c1, c2, p0..p3, bw, tex)`, `p2Flush()`, `p2Clip(on, x0, y0, x1, y1)`, `p2PushView/p2PopView`, `p2Text(font, pos, msg, color, scale)`; stubs in `game_headless.j3`. One shader, one unit quad, one instance buffer (64 B per shape, `glDrawArraysInstanced`, grown by doubling, never shrunk). Kinds: 0 flat rect (no sampling), 1 textured, 2 round rect + border (border composited over the fill, like CSS `background-clip: border-box`), 3 circle, 4 ring, 5 capsule line. SDF + `fwidth` AA.
- Routing done and kept: `game.drawRect`, `drawTexture*`, `drawText` in GL mode all queue into the same batch (`hudDraw` is now a one-line `p2Push`). ASCII glyphs are one shelf-packed sheet per font (`atlasSheet`, 1024 px wide, 1 px gaps), so a line of text is one batch. `p2Flush` is called by `j3.gfx` (`glClear`, `glBindFramebuffer`, `glDrawArrays`, `glDrawArraysInstanced`), `urp/forest`, `urp/raytrace`, `present`, `readWindow`, `updateTextureFromCanvas`, `hudDelete`. New raw GL call sites must call `game.p2Flush()` first.
- `bench2d` scenes `paint-rects N`, `paint-text N`, `paint-mixed`, `shapes` (golden page), optional 3rd arg = frames to run. `tools/bench2d/golden/shapes.png` (3.6 MB, uncompressed capture).
- `j3c doc recipes paint` (`helptext.rs`, checked by `recipes_check`), TASKS line, threads doc lists paint as main-thread only, `j3c/AGENTS.md` mention.

**Numbers** (Intel Core Ultra 7 270K Plus, RTX 5060 Ti, driver 580.173.02, headless `SDL_VIDEODRIVER=offscreen`, median of 3, cpu_ms median/p95; bench now calls `paint.flush()` inside the timed region, so submit cost counts, as in the baseline)

| scene | release cpu_ms | draws | dev cpu_ms | stage 01 baseline |
|---|---|---|---|---|
| paint-rects 10000 | 0.136 / 0.144 | 1 | 0.87 | rects 10000: 6.70, 10000 draws |
| paint-text 1000 | 0.107 / 0.114 | 1 | 0.44 | text 1000: 5.85, 9000 draws |
| paint-mixed | 0.376 / 0.387 | 201 | 1.09 | mixed: 9.27 (24.6x faster), 14200 draws |
| rects 10000 (`game.drawRect`) | 0.149 | 1 | | |
| mixed (`game.*`) | 0.376 | 201 | | |
Targets: rects 0.14 ms (<= 1.0), 1 draw (<= 3); text 0.11 ms, 1 draw; mixed 24.6x (>= 10x). Memory: max RSS 110060 KB at 300 frames, 110360 KB at 10000 frames (paint-mixed). The 201 draws in mixed are the 200 distinct textures. `gpu_ms` offscreen is not a real GPU time.

**Proof**
- `imgdiff golden/shapes.png` vs a fresh capture (`J3_CAPTURE=...:3 J3_FIXED_DT=0.016`, also with `__GL_SHADER_DISK_CACHE=0`, release and dev): `0 of 921600 pixels differ (tol 0)`.
- Routing, old `j3.game` (stage 01 stdlib) vs new, `imgdiff --tol 2`: `mixed`, `text 1000`, `rects 3000` each `0 of 921600 pixels differ`. fps-shooter HUD panel (`LV 1 0:00 0 KILLS`) identical before/after in a pixel region compare (the rest of that game is randomised per run).
- `j3c check` passes on fps-shooter, 3d-editor-scene, editor_village, scripted, bench2d; gallery shows only the two environment errors (no `SDL2_mixer`, missing font path). `cargo test`: 11 failures, none new (missing examples, an lsp file, `phase::` tests from other uncommitted work).

**Deviations**
- Engine lives in `j3.game`, `j3.gfx.paint` is the API on top (import cycle otherwise).
- Straight alpha blend (`SRC_ALPHA`, `ONE_MINUS_SRC_ALPHA`), not premultiplied, so existing HUD pixels stay identical.
- Instanced quads instead of per-vertex data: one write of 16 floats per shape, which is what made the CPU targets easy.
- Gallery can't run here, so routing was verified with seeded `bench2d` scenes plus the fps-shooter HUD.
- The recipe loads its font through a helper so `j3c check` doesn't flag the absent `assets/font.ttf` in the recipe test.
- `pushView` is relative to the current view and clips by viewport only; the clip stack is view-local.

**Gaps**
- `ponytail:` clip is an axis-aligned scissor; transform is translate plus uniform scale (no rotation); alpha is per shape, not group opacity (upgrade: render-to-texture layer).
- Text under `pushScale` is the unscaled atlas stretched (blurry); non-ASCII text goes through `j3.game`'s string cache.
- An atlas glyph wider than its `TTF_SizeUTF8` width is cut; fonts over 100 px may exceed the 64 KB scratch used per glyph.
- One draw per texture change: many distinct textures (stage 05 images) batch poorly until there's an atlas for images.
- `profZone(ZONE_UI)` now fires at flush time, so a flush inside a 3D pass moves the GPU timer zone to UI.

**Next stage needs to know**
- Use `paint.*` for everything stage 05 draws; call `paint.flush()` before anything that touches GL by hand.
- Border is drawn inside the box and over the fill (CSS border-box); a transparent border shows the fill.
- J3 gotchas: no `|`/`<<` on ints (use `+`/`*`), no multiline string literals, `let a: [N]T;` needs `= ...` (use a singleton), shadowed names are errors.
- Run windowed tests headless (`SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy`, `timeout 10`). With `J3_FRAMES` plus `J3_SCREENSHOT` a project's game can be captured with no code change.
