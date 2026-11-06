Terminals in J3 (Linux) — emulate one, or be a TUI
==================================================

`j3.term` is a stack of small modules. Names and signatures are in each module's doc: `j3c doc j3.term.grid`,
`j3c doc j3.term.screen`, and so on. This page says which to pick and what to watch for.

Start from a program: `j3c doc recipes tui` (a full-screen terminal app), `j3c doc recipes pty` (run a
program and read its screen), `j3c doc recipes ansi` (colored output in a `.j3ui` pane). The windowed emulator
is `examples/terminal`.


The modules
-----------
    j3.term             Color, Style, Cell, ATTR_* bits. Pure data.                         pure
    j3.term.ansi        Parser: bytes in, callbacks out (print, exec, csi, esc, osc);       pure
                        `sgr`, `strip`, `toRuns` for logs
    j3.term.width       cpWidth: columns for a code point (0, 1, 2)                         pure
    j3.term.grid        Grid: cells, cursor, modes, scrollback, `dumpText`; fed by a Parser pure
    j3.term.input       Decoder (bytes to key, paste, mouse, focus events); `encodeKey`,    pure
                        `encodePaste`, `encodeMouse` (events to bytes for a program)
    j3.term.pty         spawn a program on a pseudo-terminal                                OS, Linux
    j3.term.tty         isTty, size, raw mode for the real terminal                         OS, Linux
    j3.term.screen      Screen: back buffer, diffed `flush`, `poll` events                  OS, Linux
    j3.term.view        View: draws a Grid in a window, maps keys and mouse to bytes        GL, main thread
    j3.ui.ansi          AnsiLog: escapes to styled spans for a `.j3ui` pane                 GL, main thread

Pure modules do no OS calls and run on any thread. `pty`, `tty` and `screen` make Linux syscalls; on other
targets they report an error. `view` and `j3.ui.ansi` draw with `j3.gfx.paint`, so main thread only.


Two directions
--------------
    emulator:  program --pty--> bytes --Parser--> Grid --View--> window        (keys go back through `input.encodeKey`)
    TUI:       your app --draw--> Screen --diff--> bytes --tty--> real terminal   (keys come back through `Screen.poll`)

The same `Grid` is the test oracle for `Screen`: replay the bytes `flush` wrote into a Grid and compare cells.
Use `Screen.openFds` to run a TUI on pipes.


Emulator features, one example each
-----------------------------------
Reflow on resize (`Grid.resize` -> `Grid.reflow`): the primary screen and scrollback rewrap, so a line printed at
80 columns, cut to 3 rows at 30, comes back as one row at 80. Rows that wrapped by printing past the edge rejoin;
rows that ended in a newline stay apart. The alt screen is cut or padded.

    let cols = v.colsFor(w);  let rows = v.rowsFor(h);
    if cols != g.cols() || rows != g.rows() { g.resize(cols, rows); pty.resize(cols, rows); }

Drawing: italic, combining marks, ligatures (`View.italic`, `Cell.marks`, `View.ligatures`):

    v.italic = game.loadFont("assets/mono-italic.ttf", 16);   // else ATTR_ITALIC draws upright
    v.ligatures = game.textShaping();                          // `->`, `!=` as one glyph (Fira Code)
    // e + U+0301 is one cell with mark(0) == 0x301, drawn with its base; up to 2 marks per cell

Synchronized output (`?2026 h` ... `?2026 l`, `grid.MODE_SYNC`): while `g.syncHeld(game.ticks())` is true the
program is mid-frame and the host keeps showing its last whole frame. `View.draw` does that by itself. The hold
ends at `?2026 l` or after `grid.SYNC_TIMEOUT_MS` (150 ms), which clears the mode so a crashed program cannot freeze
the screen. An app that paints a Grid itself does the same:

    if !g.syncHeld(game.ticks()) { paintGrid(g); remember(g); } else { paintRemembered(); }

Translucent window, opaque text (`App.transparent()`, `game.setBackgroundAlpha(a)`): the View's default background
draws at alpha `a`; text, colored cells, the cursor and everything else stay opaque. Needs a compositor.

    App<World>.create("term", 960, 540, boot).useGl().transparent().run();   // boot: game.setBackgroundAlpha(0.85);

Window class (`App.windowClass(name)` / `game.setWindowClass`, before the window opens): X11 `WM_CLASS` and the
Wayland `app_id`. A shipped `.desktop` file (`[dist] build_outputs = ["deb"]`) says `StartupWMClass=<package name>`, so
keep the class equal to the package name for the dock to group it; right-click actions come from `[dist.actions]`
(`j3c doc ship "More [dist] keys"`):

    [dist.actions]
    new-window = { name = "New Window", exec = "--new-window" }

Cursor blink (`View.blinkMs`, the View owns it): 0 (default) is steady; `v.blinkMs = 530` hides the cursor every
other 530 ms for the blinking DECSCUSR shapes (0, 1, 3, 5; 2, 4, 6 stay steady), restarts the phase visible on each
key or text `input` sends (`v.blinkReset()` does it by hand), and calls `game.wakeAt` for the next edge, so an
idle-sleeping `.j3ui` app wakes twice a second for it and not more.

Line height (`View.setLineHeight(percent)`): a cell taller than the font (`150` is 1.5x) keeps the glyphs, cursor,
underline and strike vertically centered (`textDy` is the space above). Resize the grid and pty after it.

    v.setLineHeight(130);  g.resize(v.colsFor(w), v.rowsFor(h));

Font swap (zoom, config reload): `v.setFont(font)` re-measures the cell like `View.new` (line height back to 100,
so call `setLineHeight` again), keeps palette, cursor color, faces and scroll, and allocates nothing. A new `View`
per swap would leave the old one in the arena. Resize the grid and pty after it.

Headless input (`game.simKey`, `simText`, `simPreedit`, `simDrop`): fake SDL input on the next `poll`, so
`View.input` and anything else reading `j3.game` see it under `--headless` (`stdlib/tests/sim_input_test.j3`).


Restoring the terminal
----------------------
`Screen.open` registers a restore with `sys.onExit`: cursor, alternate screen, paste and mouse modes, cooked mode.
It runs when `main` returns, on `sys.exit`, and on panic. Only one Screen at a time (a second `open` takes
over the registration). Signals are not covered: J3 has no signal handlers, so SIGTERM, SIGHUP and SIGKILL
leave the tty raw; `reset` fixes it. Ctrl+C arrives as a key, not a signal, because raw mode turns ISIG off.


Known limits
------------
- Resize reflows the primary screen and scrollback: rows that wrapped by printing past the edge (`Grid.wrapped(line)`,
  so a copy joins them) are rejoined and split at the new width; the alt screen is cut or padded.
- `examples/terminal` has no selection or copy, and no mouse motion reporting.
- Up to 2 combining marks (BMP) ride on a cell (`Cell.marks`, `mark(i)`); more are dropped. Width comes from
  `cpWidth` per code point, not per grapheme.
- `View`: italic needs a font (`View.italic`, e.g. the family's Italic file), else it draws upright; the `blink`
  text attribute is not drawn (cursor blink is, below). Its default colors are `defaultFg`, `defaultBg`,
  `cursorColor` (0xRRGGBB) and `pal` (the 256 palette entries), all settable any time.
- `View.input` sends a frame's typed text before that frame's keys ("ls" then Enter in one poll is `ls\r`).
- Mouse reports to a program are SGR (1006) only; the legacy X10 and UTF-8 mouse encodings are not written.
- Kitty keyboard protocol: the emulator side only. `Grid` keeps the flag stack (`CSI > u` push, `CSI < u` pop,
  `CSI = u` set, `CSI ? u` query; per screen) in `modes` (`grid.kittyFlags(modes)`), and `input.encodeKey` /
  `encodeKeyEvent` (press, repeat, release) encode by it. No keypad, modifier-key or base-layout reports.
  `Decoder` (the TUI side) does not read `CSI u` keys.
- Kitty graphics: `Grid` reads `ESC _ G ... ESC \` (`Sink.apc`; a custom `Sink` forwards it: `fn apc(this, b) {
  this.grid.apc(b); }`): direct/file/temp-file transmit, RGB/RGBA/PNG, zlib, put, query, delete; placements scroll
  with the text and `View.draw` paints them. Call `pty.resizePx(cols, rows, view.cellW, view.cellH)` so programs
  see the window in pixels (`kitty +kitten icat` needs it). No shared memory, animation or Unicode placeholders;
  resize and partial scroll regions do not move images. Sixel is not read.
- `pty` and `sys` use Linux amd64 syscall numbers, like the rest of `sys.j3`.
