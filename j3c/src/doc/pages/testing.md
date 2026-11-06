Testing and looking, headless
=============================
Agents never open a window. A visible window blocks the session and nobody is watching it.
Run the real program with no display, save a frame, and read the PNG.

    j3c check                      types, libraries, files that load calls name. Does not run.
    j3c run --frames 120 --screenshot shot.png --headless
                                   the project for 120 frames on SDL's offscreen video and silent
                                   audio; saves the last frame (relative to where you ran j3c);
                                   prints a J3STATS line. Wrap it: `timeout 60 j3c run ...`.
    j3c build && SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy timeout 20 ./target/<name>
                                   a built binary has no --headless flag: set both variables on
                                   the same command line, or a window opens.

`j3c run --hot --headless` keeps running and, when a `.j3`/`.j3ui` file changes, rebuilds and restarts the program
(a failed build keeps the old one). `J3_HOT_QUIT` / `J3_HOT_WIN` / `J3_HOT_STATE` are file paths it sets for the
handshake; tests print from the `App.hot` load hook to see state survive (`j3c doc j3ui`, Dev loop).

Without `--headless` or those variables a run opens a real window. Any game run needs one of them.

An Android package (`[emit] target` has an Android triple) runs on the host with `--host`: `j3c check --host`,
`j3c run --host --headless --frames 60 --screenshot shot.png`. It ignores `[emit] target` only and builds in place
(`j3c doc android`).

`--frames N` (and `J3_CAPTURE=...:N`) runs exactly N frames: the game's `draw` is called N times and
`j3.game.frameCount()` reaches N. The `J3STATS frames=` line counts N minus 3, because the first three
frames are warm-up and left out of its averages.

Environment (read by `j3.game` / `j3.app`, so any App supports them; see `j3c doc j3.app`)

    J3_FRAMES=N, J3_SCREENSHOT=f.png   what `--frames` and `--screenshot` set
    J3_CAPTURE=out.png:N               save frame N (1-based, before it is shown), then quit
    J3_FIXED_DT=0.016                  every frame's dt is that many seconds, so runs repeat

Goldens (checked-in PNGs that must keep matching):

    SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy J3_FIXED_DT=0.016 J3_CAPTURE=out.png:30 \
        timeout 20 ./target/<name>
    j3c -e imgdiff golden/a.png out.png [--tol N] [--max N] [--out diff.png]

Another window shape: `game.setVirtualSize(1920, 1080)` letterboxes everything to that size, so a
capture at a 1400x700 or 1000x900 window shows black bars (`J3_CAPTURE` and `screenshot` are the
whole window, bars included, in real pixels). `game.screenRect()` is the drawn area. With the size off,
output is unchanged. `big_ui_test` reads `UI_VIRTUAL=1920x1080` with `UI_SIZE=WxH`.

Window modes (`game.setWindowMode` borderless/fullscreen, monitors, vsync) do nothing useful under the
offscreen driver. Test the settings logic with `stdlib/tests/display_test.j3` (`j3.game.display`), and the
look by hand on a real display.

`imgdiff` prints how many pixels differ by more than `tol` per channel (default 0), exits 0 when that
is at most `max` (default 0), 1 when more or when the sizes differ, 2 when it cannot read a file.
`--out` writes a diff image (changed pixels red). Update a golden only for a change you meant, and
look at the new PNG first.

`j3c golden <dir> [name...] [--bless]` does the whole loop for a project from `<dir>/golden.toml`: per
`[[state]]` it runs `j3c run . --headless --frames N` (offscreen SDL, dummy audio, `J3_FIXED_DT=0.016`,
`J3_CAPTURE=target/golden/<name>.png:<frame>`), compares with `golden/<name>.png`, and prints `PASS name` or
`FAIL name (why)`; exit 1 on any FAIL. A missing golden is a FAIL; `--bless` copies the captures over the goldens
(named states only, else all). Names run just those states. Diff images and run logs land in `target/golden/`.

    [defaults]                    # for every state: frame (30), tol (per channel), max (pixels),
    frame = 30                    # frames (default frame+2), timeout (seconds, 300)
    [state.settings]              # golden/settings.png; any of those keys may be set here too
    text = "target/ui-a11y.txt"   # optional: this file is also compared with golden/settings.txt
    [state.settings.env]          # env for this run; the app picks the state from it
    APP_GOLDEN = "settings"
    [check.steady]                # a run with no capture; PASS at rc 0 and no output line starting FAIL
    frames = 200                  # omit it to let the program end itself (a self-test)
    [check.steady.env]
    J3_STEADY = "60:120"

The file is the same small TOML subset as `j3.toml` (sections, `key = value`); an unknown key, a bad number or a
repeated section is an error `golden.toml:LINE: ...` (exit 2). Each run gets its own process group and is killed
at the timeout (FAIL).

Checks run only without names and without `--bless`. `examples/idle-breakout/golden.toml` is the plain pattern,
`examples/desk-app/golden.toml` adds text goldens and checks.

Make a frame repeatable: fixed dt (above), fixed seeds, and a state the game sets up itself, for
example `j3.game.env("MYGAME_STATE")` in `boot` choosing a tab or a seeded world. For `.j3ui` also call
`root.dom.setFrameMs(16.0)` so fades and transitions land the same each run.

Steady frame (an idle UI should cost nothing). `J3_STEADY=a:b` (1-based frames, a < b) is read by
`j3.app`: at the end of frame a it notes the layouts, restyles and `App.mem` use, and at the end of
frame b it prints one line and exits (rc 1 when any is non-zero, else 0):

    steady layouts=0 restyles=0 arena_growth=0

    J3_STEADY=60:120 j3c run --headless --frames 200       (a built binary: offscreen SDL, same variable)

Layouts and restyles count boxes laid out and styles recomputed in every `.j3ui` root, so an idle
frame leaves both unchanged; `arena_growth` growing means something allocates per frame. Warm up
first (the first frames lay everything out and a few frames after a state change still settle), and
set a fixed state before measuring. A malformed value prints a warning and is ignored. `examples/desk-app/golden.toml`
uses it as a `[[check]]`; `speculative/29_ui_steady` shows both outcomes (`STEADY_CHURN=1` changes a value every frame: rc 1).

Idle frame skipping (`App` sleeps while a `.j3ui` UI is still; `j3c doc j3ui`, Idle frames) is off in every test mode above and under the
`offscreen` / `dummy` video drivers, so frame counts hold. To test it headless, force it: `J3_IDLE_SKIP=1 J3_IDLE_STATS=1 timeout -s TERM 2 target/app`
prints `idle: loops=N presented=N` at exit (SIGTERM quits cleanly); idle `examples/desk-app` shows ~29 frames in 2 s instead of ~18000.
`j3c/stdlib/tests/idle_test.j3` checks the decision and `Dom.idleMs` with a fake clock.

UI inspector (`j3.ui.inspect`, the devtools of a `.j3ui` app). `root.dom.setInspector(true)` turns it on; with `J3_INSPECT=1` F12
toggles it as well (`j3c run` sets that for you, `J3_INSPECT=0` turns it off; a built binary needs it set by hand). The element under the pointer gets margin / border / padding / content bands and a `tag.class#id  W x H` label;
a click pins an element (Escape unpins, Up / Down walk to the parent / first child, Left / Right to the siblings) and a side panel
shows its box, computed style (non-default), inline `style:` overrides first (they win over every rule), matched rules in cascade
order with `file:line` in the `.j3ui` or CSS file (overridden declarations are dimmed and struck through), and custom properties; the
wheel over the panel scrolls it. While on, the app gets no clicks, wheel or keys, and its hover, active state and cursor stay frozen as
they were when the inspector came on. For
agents, `J3_INSPECT_DUMP=<selector>:<frame>` (`#id`, `.class`, a tag, or a node number; frame 1-based) prints the same text to stdout
at that frame, with no window changes needed, and `inspect.text(&root.dom, id)` returns it (valid until the next call). Off, it costs one bool check a frame.

`.j3ui` debugging

    j3c dump-ui ui                 the J3 generated from the components (errors point at the .j3ui)
    use j3.ui.domhtml.dumpHtml;   dumpHtml(&root.dom, "target/ui-snapshot.html")
                                   after `root.render(...)` (it runs `layout()` itself; on a Dom you
                                   drive by hand, after `d.layout()`): the node tree as static HTML + CSS,
                                   with checked, disabled, selected and typed values (never a password's),
                                   readable as text, diffable, or openable in a browser
    root.dom.simInput(x, y, down, wheel)
                                   fake mouse input from the next frame; a click is one frame with
                                   `down` then one without. Drive it from your own test hook.
    root.dom.simKeys("text", "S+Left C+a Backspace")
                                   fake keyboard for the next frame: typed text and key names (Left Right
                                   Up Down Home End Backspace Delete Enter Escape Tab Space a c v x, `S+`
                                   Shift, `C+` Ctrl). `examples/big_ui_test/src/sim.j3` scripts both from `UI_SIM`.
                                   These reach the Dom only; code reading `j3.game` (a canvas game, `j3.term.view`) needs:
    game.simKey(Key.Enter, true)   fake SDL input on the next `poll`, seen by `keyPressed`/`keyDown`/`keyReleased`
    game.simText("ls")             (`simKey(k, false)` releases), `textTyped`, `textComposing` + `textComposingCaret`
    game.simPreedit("にほ", 3)       and `droppedFile`; wakes an idle-sleeping loop. Works under `--headless`
    game.simDrop("/tmp/a.png")     (`j3c/stdlib/tests/sim_input_test.j3` types `ls` Enter into a pty).
    game.simMouse(100, 100)        fake cursor in window pixels (as SDL reports them); `mousePos` maps it through
                                   `setVirtualSize` like a real one, and it replaces the real cursor from then on
    game.simButton(Mouse.Left, true)   `mousePressed` true for one frame on the next `poll`, `mouseDown` until
                                   `simButton(.., false)` (then `mouseReleased` one frame); a down+up in one frame
                                   still gives each its own frame
    game.simWheel(-1)              `mouseWheel` on the next `poll` (whole notches). `mouseDelta` does not move.
    use j3.ui.testkit;            a script of fake input, one call per frame from your draw hook (`f` = your frame number):
                                   testkit.clickText(d, f, 3, "Size")     press on frames 3..5, check on 6
                                   testkit.clickTextIn(d, f, 8, "dialog", "Rename")   text inside a tag
                                   testkit.clickTag(d, f, 12, "button", 2)   the 3rd <button>
                                   testkit.typeInto(d, f, 18, "input", 0, "ro")   click on 18..20, type on 21
                                   testkit.simKeys(d, f, 30, "", "C+a Backspace")   several key names in one frame
                                   testkit.dragText(d, f, 3, "Sword", "Slot 2")   drag an item onto an element, frames 3..8 (dragTag, drag, dragTo)
                                   testkit.hasText(d, "Saved"), countText, textNode, tagNode
                                   text matches a whole text node (first in tree order; not display:none, not a part of
                                   a split `Rename {name}`). A target that is missing, hidden, size 0 or off screen
                                   prints `testkit: frame N: no text "X"`, sends no input and counts once in
                                   `testkit.misses()` (`resetMisses()` between suites): fail your test on it. Examples: `examples/desk-app/src/selftest.j3`
                                   (DESK_TEST=1), `speculative/14_ui_slots`, `j3c/stdlib/tests/testkit_test.j3`.
    hot reload                     only under `j3c run` (not headless captures): style edits apply in
                                   about 250 ms; `j3c run --hot` also rebuilds and restarts on markup, script and
                                   .j3 edits (`app.hot(save, load)` keeps state). CSS warnings print at start.

Scenes only (no game code): `j3c -e render --view iso --out shot.png`. More: `j3c doc recipes check`.
Speed: the J3STATS line has fps, frame, draws; measure release builds (`j3c build --release`).
UI speed: F3 shows frame time, F4 shows where UI time goes (CPU: build, style, layout, paint, effect setup;
GPU: paint batches, `backdrop-filter` blur, `filter`/blend layers; instances and their area in screens).
`J3_F4=1` starts it open for a screenshot, and a run that painted UI also prints a `J3UISTATS` line with the
same numbers (GPU slots are -1 without timer queries, e.g. on GLES).
