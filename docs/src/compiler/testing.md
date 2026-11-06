# Testing Headless

Windowed programs can run with no display, which is how tests, CI, and AI agents check them.

## Run without a window

```text
timeout 60 j3c run --frames 120 --screenshot shot.png --headless
```

This runs the project for 120 frames on SDL's offscreen video with silent audio, saves the last frame, and prints a `J3STATS` line with frame timings. The screenshot path is relative to where you ran `j3c`, not the project.

A built binary has no `--headless` flag. Set the SDL variables on the same command line instead:

```text
j3c build
SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy timeout 20 ./target/mygame
```

## Environment variables

Any program using `j3.game` or `j3.app` understands these:

| Variable | Effect |
|---|---|
| `J3_FRAMES=N` | Stop after `N` frames |
| `J3_SCREENSHOT=f.png` | Save the last frame |
| `J3_CAPTURE=out.png:N` | Save frame `N`, then quit |
| `J3_FIXED_DT=0.016` | Fixed frame time, so runs repeat exactly |
| `J3_STEADY=60:120` | Fail if idle frames do layout, restyle, or allocate |
| `J3_IDLE_SKIP=1` | Sleep through idle frames (`=0` forces it off) |

A `.j3ui` UI that is still does not redraw: `App` waits for input, resize, `Dom.requestFrame`, the caret blink or a 250 ms safety tick. Tests, captures, and the `offscreen` and `dummy` video drivers never sleep. `J3_IDLE_STATS=1` prints `idle: loops=N presented=N` at exit. Details: `j3c doc j3ui` (Idle frames).

## Golden images

A golden test compares a screenshot against a saved, known-good image.

```text
j3c golden examples/idle-breakout            # compare
j3c golden examples/idle-breakout --bless    # accept new images
```

The states to capture are listed in the project's `golden.toml`. To compare two images by hand:

```text
j3c -e imgdiff a.png b.png --tol 2 --out diff.png
```

It exits with 1 when they differ.

## Hot reload

`j3c run --hot` rebuilds and restarts the program when a `.j3` or `.j3ui` file changes. A failed build keeps the old program running. Scripts (`.j3s`) and `.j3ui` styles reload without a restart.

More detail: `j3c doc testing`.
