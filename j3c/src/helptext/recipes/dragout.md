Drag a file out of the window into another app (Discord, a browser, a file manager): press on the
square and pull. `j3c doc j3.game dragFiles` has the rules.

- `game.dragFiles(paths)` starts the drag. Call it while the left button is held, after the
  pointer has moved a few pixels from the press. Paths must be absolute files; to drag data you
  only have in memory (a screenshot, a rendered image), write it to a file first.
- `game.dragState()` is `Dragging` while the pointer carries the files, then `Dropped` or
  `Cancelled` for one frame. Escape cancels. The window sees no mouse motion during the drag;
  the release comes back as `mouseReleased(Mouse.Left)`.
- The files go out as `text/uri-list` (`file:///path`, percent-encoded), which chats, browsers
  and file managers take. The other app reads the file when it is dropped: keep it on disk.
- Linux X11 through its own `libX11.so.6` connection (XDND). Wayland sessions work through
  XWayland, which SDL2 uses by default; SDL on native Wayland, headless, Windows and macOS get
  `false`.
- Test the false path headless; test the drop by hand (drag onto a browser tab or a chat).
