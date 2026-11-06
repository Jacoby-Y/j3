# Composer

A small DAW written in J3 with a built-in synth (no samples). Agents write songs as text with
commands; a person opens the same `.j3song` file in the window to see the chords and notes, hear
it, and edit it.

```
j3c-composer songs/demo.j3song                      # the window
j3c-composer run songs/demo "help"                  # headless, for agents and scripts
j3c-composer selftest                               # prints ok
```

`tools/composer/composer` is the same program, rebuilt when `src/` changes. `./build.sh` at the
repo root installs `~/.bin/j3c-composer`. The window needs SDL2 + libGL; `run --wav` needs nothing.

## The file

```
# j3song 1
song tempo=96 meter=4 key=Am bars=8
chords Am F C G Am F E7 E          # one per bar; `-` keeps the last
track Bass synth=bass vol=0.9 pan=0 tone=0.5
  n 0 0.5 A2 95                    # beat from the song start, length in beats, pitch, velocity
track Drums synth=drums vol=0.9 pan=0 tone=0.6
  n 0 0.25 kick 100
```

Synths: `sine lead pad bass pluck keys drums`. Drum pitches: `kick snare clap hat ohat tom crash`
(General MIDI numbers, so `36` works too). `tone` is filter brightness 0..1. A track named like a
synth (`Bass`, `Pad2`) gets that synth.

## Commands

`composer run FILE "cmd; cmd" --script F --wav out.wav [--loop] --render out.png [--view arrange|chords|roll:TRACK] [--size WxH] [--no-save] [--out F]`.
The window's console (bottom right) runs the same commands. `help` prints them all:

| | |
|---|---|
| `song tempo=96 key=Am meter=4 bars=16 swing=0.6 noise=0.3`, `tempo 96`, `key F#m` | song settings; `swing` 0.5 (straight) .. 0.75 delays the off-beat 8ths when played (stored notes stay on the grid), `noise` 0..1 adds tape hiss and vinyl crackle |
| `chords Am F C G [at=5]` | a chord per bar: `C Am F#m7 Gmaj7 Bdim Caug Dsus4 A5 C6 G9 Cadd9 Bm7b5 G/B` |
| `track NAME synth=lead vol=0.8 pan=-0.3 tone=0.6 reverb=0.3 drive=0.2`, `deltrack`, `rename`, `mute`, `unmute`, `solo`, `unsolo` | tracks; `reverb` 0..1 is the send to the song's reverb, `drive` 0..1 saturation |
| `note TRACK 3:2.5 1/2 E5 [vel]` | one note; position `BAR:BEAT` (1-based) or a beat from 0 |
| `notes TRACK 1:1 1/2 E5 _ C5 D5 - A4 v=90` | a line of notes a step apart; `-` rest, `_` holds |
| `fill TRACK root\|fifths\|block\|stab\|arp\|arpdown\|arpupdown [bars=1-8] [step=1/2] [oct=3] [v=80]` | parts from the chords |
| `fill TRACK voiced [rhythm=x__..x_.] [bars=] [oct=3] [v=]` | four voice-led chord voices on a rhythm (`x` hit, `_` hold, `.` rest, spread across the bar); a 9th chord drops its root, each voice moves to the nearest tone of the next chord |
| `beat TRACK kick x...x...x...x... [bars=1-8] [v=90]` | a drum per bar; `X` accents, digits `1`-`9` set velocity (`9` = 127, `1` = 15), and `dump` prints them back |
| `clear TRACK\|all [bars=]`, `copy TRACK\|all bars=1-4 to=5 [times=3]`, `transpose TRACK\|all 12 [bars=]`, `undo` | editing |
| `humanize TRACK\|all [bars=] [v=8] [time=0.02]` | velocities up or down by up to `v`, starts early or late by up to `time` beats (same script, same result); drums keep their timing unless `time=` is given, and no note is moved into the one before it at the same pitch |
| `section Chorus bars=9-16`, `section Chorus off` | name bars; then `bars=Chorus` and `to=Chorus` work in every command (`copy all bars=Chorus to=25`) |
| `info`, `dump TRACK [bars=]`, `analyze` | looking: notes per bar, drum grids, sections; `analyze` adds clashes, each track's level, parts in the same range or both low (mud), parts that repeat one bar, a song that is one phrase played again, same-pitch overlaps (where), loudness |

Notes, `fill`, `beat`, and `section` past the last bar are refused (`bars 16` first); `copy` and
`chords` grow the song and say so.

A failed command changes nothing and says what to fix; `run` then saves nothing.

`--wav F --loop` writes exactly `bars * meter` beats with the release and reverb tails added back
onto the start, so a game can loop the file with no gap or click. Effects (swing, reverb, drive,
noise) are deterministic: the same file gives the same samples.

## The window

- **Top:** Play/Stop (Space), Loop (L) over the selected bars or the whole song, Snap (click to cycle 1 .. 1/6), Fit (F), Save (Ctrl+S).
- **Left:** tracks with mute and solo; the selected track's synth (click to cycle), volume, pan, tone; song tempo, bars, meter, key.
- **Chord lane:** each bar's chord and its Roman numeral in the key. Click a bar to type chords from there in the console. Drag across bars (or the arrangement) to select them: L loops them, Ctrl+D duplicates them after themselves (Shift: every track), Delete clears them (Shift: every track).
- **Ruler:** click or drag to move the play position.
- **Piano roll:** rows outside the key are darker; each bar's chord tones are tinted (the root more). Click empty space to draw a note (drag sets its length), drag a note to move it, drag its right edge to resize, right-click deletes. With a note chosen: arrows move it (Shift+Up/Down an octave), Delete removes it. Wheel scrolls pitch, Shift+wheel scrolls time, Ctrl+wheel zooms, middle-drag pans.
- **Velocity lane:** drag across notes to set their velocity.
- Ctrl+Z undoes (32 steps). F2 prints the commands; Esc hides output and clears the selection.

## Untested in the window (the person runs it)

Audio out on real hardware (checked only with `SDL_AUDIODRIVER=dummy`), note audition on click,
dragging notes and velocities, slider undo steps, close-with-unsaved warning.

## Not built yet

Redo, multi-note selection, patterns/clips as reusable objects (use `section` and `copy`), section names in the window and `--render`, automation, delay and other effects, samples, MIDI import/export, an MCP server (the CLI is the agent interface).
Playback renders the whole song on each edit (0.1 s for 20 s of music); a long song will hitch.
