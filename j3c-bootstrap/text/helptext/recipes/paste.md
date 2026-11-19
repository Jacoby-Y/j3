Paste an image, GIF, or file list copied in a browser, image viewer, or file manager: Ctrl+V lists
what the clipboard offers and writes a PNG to `pasted.png`. `j3c doc j3.game clipboardData` has the rules.

- `game.clipboardTypes()` is the MIME types on offer, in the owner's order ("image/png",
  "image/gif", "text/uri-list", "text/plain"). X11's UTF8_STRING/STRING/TEXT come back as "text/plain".
- `game.clipboardData(mime)` is the bytes for one type, or empty when it isn't offered. Big data
  (X11 INCR, tens of MB) is read in full; an owner silent for about 1 s gives empty.
- Both return views of buffers the next call replaces: write or copy them before calling again.
- Main thread only. X11 reads on its own display connection; Wayland without `DISPLAY` runs
  `wl-paste` (package `wl-clipboard`). Headless (`SDL_VIDEODRIVER=offscreen`) both are empty.
- Files from a file manager are "text/uri-list": one `file:///path` per line (percent-encoded).
