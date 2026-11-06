# bench2d

2D draw baseline for `j3.ui.dom` (DOM path) and `j3.gfx.paint` (direct paint), plus the `shapes` golden page.

| Scene | Args | What |
|---|---|---|
| `ui` | `N` | N BallCards through `j3.ui.dom` (paints; `ui 1000` = 2001 draws, the DOM hotspot) |
| `rects` / `text` | `N` | N rects / N text runs |
| `mixed` | | rects + text |
| `paint-rects` / `paint-text` | `N` | same via `j3.gfx.paint` |
| `paint-mixed` | | mixed via paint |
| `shapes` | | every paint primitive and edge case (the golden) |

Usage: `bench2d <scene> [N] [frames]`. Default 300 frames, first 30 skipped; a 3rd arg runs that many frames (watch memory). The binary is `target/bench2d`. It opts out of idle-frame skipping, so every frame draws.

## Run (headless, from the repo root)

```
j3c build --release tools/bench2d
cd tools/bench2d
SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy timeout 10 target/bench2d rects 1000
```

Output is one line: `scene=rects n=1000 cpu_ms=median/p95 gpu_ms=... draws=...`. Compare median across changes; p95 shows spikes.

## Golden

`golden/shapes.png` is the `shapes` scene at frame 3 with a fixed step. Capture, then diff:

```
cd tools/bench2d
SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy J3_CAPTURE=/tmp/shapes.png:3 J3_FIXED_DT=0.016 \
  timeout 10 target/bench2d shapes
j3c -e imgdiff golden/shapes.png /tmp/shapes.png --out /tmp/diff.png   # exit 0 = same, 1 = differs (red in diff.png)
```

`golden/ui.png` is `ui 20` at frame 3 (DOM path; all black means nothing painted):

```
SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy J3_CAPTURE=/tmp/ui.png:3 J3_FIXED_DT=0.016 timeout 10 target/bench2d ui 20
j3c -e imgdiff golden/ui.png /tmp/ui.png
```

Add `--tol N` / `--max N` to allow noise. If the change is intended, look at the capture, then `cp /tmp/shapes.png golden/shapes.png`. More: `j3c doc testing`.
