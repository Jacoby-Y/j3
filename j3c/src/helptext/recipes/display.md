Window settings every PC game needs. `j3.game` has the primitives (`setWindowMode`,
`setResolution`, `setVsync`, `setFpsLimit`, `windowFocused`, `displayCount`, `resolutionAt`);
`j3.game.display` is the menu layer: one `Settings` value, saved to a file.

- Bind a button or key to one call: `display.cycleMode(&v)`, `nextResolution(&v, 1)`,
  `nextDisplay`, `toggleVsync`, `nextFpsLimit`. They only edit `v`; `display.apply(&v)` makes the
  window match, `display.store(&v, path, mem)` saves. `toggleFullscreen(&v)` applies at once (F11).
- In `.j3ui`: `<button on:click={display.cycleMode(&w.video)}>{display.modeName(w.video.mode)}</button>`.
- `App.displaySettings(path)` loads and applies the file before the window opens, so a game saved
  in fullscreen never flashes a window. Missing file = the App's own size.
- Labels: `modeName`, `resolutionLabel(&v, &text)`, `fpsLabel(n, &text)`.
- Borderless is the desktop size (best alt-tab); Fullscreen switches the monitor to `v.w x v.h`.
- `game.windowFocused()` is false while another window has focus: pause or mute on it.
- Text needs `assets/font.ttf` (`j3c new --template ui` ships one; any `.ttf` works).
- Needs a real display. Under `--headless` the calls are no-ops, so test the logic with
  `stdlib/tests/display_test.j3` and the look by hand.
