# Composer (agents)

Write a song with commands, listen with `analyze`, look with `--render`, and leave the file for the
person to open in the window. Read `README.md` for every command. The loop:

```
j3c-composer run songs/NAME "song tempo=100 key=Am bars=8; chords Am F C G Am F E7 E" 
j3c-composer run songs/NAME "track Bass; fill Bass fifths step=1/2 v=95" "track Drums; beat Drums kick x...x...x...x..."
j3c-composer run songs/NAME "track Lead synth=lead; notes Lead 1:1 1/2 E5 _ C5 D5 E5 _ - A4" analyze
j3c-composer run songs/NAME --render /tmp/NAME.png --view roll:Lead     # then read the PNG
j3c-composer run songs/NAME "dump Lead bars=1-4"                       # notes as text: pos:pitch/len
```

Or keep the commands in `songs/NAME.cmp.txt` and rebuild from scratch with
`rm songs/NAME.j3song; j3c-composer run songs/NAME --script songs/NAME.cmp.txt analyze`
(`songs/demo.cmp.txt` is the worked example).

`analyze` is your ear: it lists notes out of key (and not in the bar's chord), held non-chord tones
on a beat, same-pitch overlaps, peak/RMS, samples the limiter squashed, each track's level while it
plays, a buried melody, parts in one range or both low (mud), parts that repeat one bar, and a song
that is one phrase played again. Aim for 0 clashes you did not mean (change the chord, e.g. `E` to
`E7`, when the melody means it), no level or range lines, and a peak around 0.5-0.8 with few
limited samples. You cannot hear the result; `--wav` writes it for the person.

Chord parts: `fill Keys voiced rhythm=x__..x_.` voice-leads four voices through the chord lane in
one line (no hand-written `notes` per voice). Feel: `song swing=0.6` swings the off-beat 8ths at
render time, so write straight 16-step `beat` patterns, not triplet grids; digits in a pattern are
velocities (`beat Drums hat 4.3.4.324.3.4.32` for ghost notes). Space: `track Keys reverb=0.3`,
`drive=0.2` on drums, `song noise=0.3` for hiss. Game music: `--wav F --loop` cuts at the last bar
and folds the tails onto the start, so it loops cleanly (`songs/lofi.cmp.txt` uses all of these).

Longer songs: write a phrase, name it (`section Verse bars=1-8`), `copy all bars=Verse to=17`, then
change the copy (another fill style, a new lead line, a drum fill in its last bar). `humanize all`
last, so the parts stop sounding quantized (drums keep their timing unless `time=` is given).

Traps: positions are `BAR:BEAT` 1-based or a plain beat from 0; `fill` and `beat` replace what was in
those bars (for that drum); a new song has 8 bars (`bars=` changes it) and notes past the end are
refused; `#` starts a comment, so it
is not a sharp outside a note name (`F#4` is fine). Windowed checks: `SDL_VIDEODRIVER=offscreen
SDL_AUDIODRIVER=dummy`, never open a window yourself.

Code: one module per file in `src/`. `cmd.j3` is the one command language (CLI, console, agents):
add features there first, with a `helpText` line and a README row (parts in `src/cmd/`: `args` parsing, `fills`, `notes`, `inspect`, `analyze`; the dispatch and `edit` stay in `cmd.j3`; `gui.j3` has parts in `src/gui/`). `view.j3` draws for both the
window (`gui.j3`) and `--render` (`shot.j3`). `synth.j3` is deterministic (same file, same samples;
`selftest` checks it). J3 forbids shadowing any name in a function.
Run `tools/composer/composer selftest` after changing `src/`.
