`j3.gfx.paint` is a canvas-style 2D painter for games and HUDs: rects, rounded boxes with
borders, circles, rings, lines, text, images. Needs SDL2 and `app.useGl()` (it panics
without GL). Test with `j3c run --frames 5 --screenshot f.png --headless`, never a bare
`j3c run` (it opens a window); ship text with a .ttf in `assets/`.

- Every call queues one instance. Thousands of shapes and glyphs are one or a few draw calls;
  a texture change, a clip change, or `present` ends a batch. `j3.game.drawRect` and `drawText`
  share the batch, so mixing them keeps their order. Call `paint.flush()` only before raw GL.
- Coordinates are logical pixels from the top left, like `j3.game`. Colors are `j3.game.Color`.
- `pushClip` nests and intersects (axis-aligned). `pushTranslate`, `pushScale(origin, s)`,
  `pushAlpha`, and `pushView(pos, size)` each have a matching pop.
- Shapes are antialiased; `rect` is not, so it matches `drawRect` pixel for pixel.
- Main thread only. Next: `j3c doc j3.gfx.paint --short`.
