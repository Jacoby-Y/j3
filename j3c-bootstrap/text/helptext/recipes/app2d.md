`j3c new mygame --app` writes these files. Needs SDL2. `j3c run` opens a window; agents add
`--frames N --screenshot f.png --headless` (`j3c doc testing`).

- `App<World>.create(title, w, h, boot)` makes the window. `boot` runs once with the app's
  arena and returns the World; allocate from that arena, which lives until quit.
- Register systems before `run()`: `startup` once, `first`/`add`/`last` at a fixed 60 Hz,
  `always` every frame with dt, `draw` last (`j3c doc j3.app`).
- The window can resize; place things from `windowSize()` (`j3c doc j3.game windowSize`), or fix the size with `game.setVirtualSize(1920, 1080)` (letterboxed, `j3c doc j3.game setVirtualSize`).
- Next: `j3c doc j3.game --short` (draw, input, audio), `j3c doc j3ui` (menus and HUDs),
  `j3c doc recipes canvas` (make the art in code).
