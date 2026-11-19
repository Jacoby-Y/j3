A full-screen terminal program: `Screen.open` takes the terminal (raw mode, alternate screen), you draw
into a back buffer, `flush` writes only the cells that changed, and `poll` returns keys. `j3c doc term` is the overview.

- Run it from a real terminal: `j3c run <dir>`. Piped, it has no raw mode; tests use `openFds` (see `j3c doc j3.term.screen`).
- `put`, `box`, `fill`, `clear` only edit the back buffer. Nothing shows until `flush`.
- After `EV_RESIZE` the screen is already resized; draw again before `flush`.
- The terminal is restored on `close`, on return from `main`, and on panic. A kill signal skips it (`reset` fixes the tty).
- Style is `Style { fg, bg, attr }`: `Color.default()`, `Color.palette(n)`, `Color.rgb(r, g, b)`, `ATTR_*` bits.
