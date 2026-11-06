Show colored process or build output in a `.j3ui` app. `j3.ui.ansi.AnsiLog` parses the bytes once, when you
`append` them, into lines of `Seg`s; the component draws the segs with `{#each}` and `style:` slots (`j3c doc j3ui`,
Dynamic style). Nothing is parsed per frame, so a long log costs only its layout.
Needs SDL2, SDL2_ttf, and libGL; the screen is the `components` recipe's (`j3c doc recipes components`).

- `AnsiLog.new(fg, bg, maxLines)`: `fg`/`bg` are the pane's colors (`0xRRGGBB`), used only where a default color has to
  become a real one (inverse, dim, hidden). Everything else leaves `fg`/`bg` empty, and an empty `style:` value
  inherits the pane's CSS.
- `log.append(bytes)` takes chunks split anywhere, mid-escape too. The last line shows before its LF arrives.
  Palette: the 16 ANSI colors, the 256-color cube and gray ramp, and 24-bit color.
- `log.line(i)` is line `i`'s `[]Seg { text, fg, bg, weight, slant, deco }`.
- Kept: SGR (colors, bold, dim as a darker color, italic, underline, strike, inverse, hidden). Dropped without
  a trace: cursor moves, clears, OSC titles, every other escape. A CR followed by text wipes the line, so a
  progress bar shows its last frame; CRLF is a plain line break. Tabs pad to 8 columns.
- Align columns with a monospace font: add an `@font-face { font-family: "Mono"; src: url("../assets/mono.ttf"); }`
  to `ui/theme.css` and put `font-family: "Mono"` on the log.
- `AnsiLog.new(fg, bg, maxLines)` owns its memory. With `maxLines > 0` only the newest lines are kept; once as many
  have been dropped again, the live lines are copied to a fresh arena and the old one freed (CR-wiped text too), so a
  log that runs for days stays bounded. `0` keeps everything. `log.clear()` empties it, `log.destroy()` frees it.
- Read lines with `log.count()` and `log.line(i)`. For a long log use a `virtual` list (`j3c doc j3ui`, Virtual lists):
  `<div class="log" virtual row-height={18}>{#each log.lines.items[log.head..log.lines.len] as ln, i (i)}`
  `<div class="ln">{#each log.segs.items[ln.first..ln.first + ln.count] as s}<span ...>{s.text}</span>{/each}</div>{/each}</div>`.
