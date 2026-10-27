Flex item longhands and visual ordering (plan 023, 15c1–15c2); the `TextCSS` component in the lower half shows `white-space: pre`/`pre-wrap`, `justify`, `line-clamp`, `visibility` (goldens: `j3c golden speculative/32_ui_flex`). Needs DejaVu Sans at the system path used in src/main.j3. Headless only:

```
SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy __GL_SHADER_DISK_CACHE=0 J3_STDLIB=$PWD/j3c/stdlib timeout 180 j3c/target/release/j3c run speculative/32_ui_flex --headless --frames 40 --screenshot /tmp/flex.png
```

Asserts callback counts/visual order through nested ordered slot canvases, conditional removal/fade/reentry, keyed moves,
and dynamic order. At frame 3 the existing keyed/slot canvas row switches to row-reverse; callback and hit priority still follow order. Checks overlapping hit order and 300 ordinary ordered siblings with zero steady layouts/restyles/arena growth.
Reversed overlapping rows y=160..190 and y=190..220 also match exactly; the first uses `order` and the second source sorting.
Hit priority follows the same order at x=95, y=175. A reversed wrapping column exercises `align-content` and physical gaps.
The top square is red before frame 3, then blue. Rows y=100..130 and y=130..160 must match exactly: runtime-ordered and
source-sorted generated painting of component slots/text/borders under clipping, opacity and z-index. Source/Tab order stays
unchanged. Ordinary ordered subtrees paint linearly. Each canvas-containing ordered child replays generated source slots;
a large sibling list of custom canvases can cost quadratic time.
