# 03: Cut `.j3ui` CPU per frame

Goal: the `examples/ui_app_compare` stress scene (`STRESS=1`, 1,200 cards, every card changes text and bar width each
frame, 1 in 8 swaps class) spends less CPU per frame. Today on a real GPU it's 9.6 ms CPU / 104 fps; under Xvfb 58.5 ms
(about 15 ms of that is J3's own thread, the rest is llvmpipe). Electron does 22.3 ms under Xvfb.

## Baseline (do this first, again after each step)

```
cd examples/ui_app_compare/j3 && j3c build --release .
STRESS=1 SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy J3_FRAMES=900 timeout 30 ./target/todo | tail -2
STRESS=1 J3_F4=1 SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy J3_FRAMES=900 timeout 30 ./target/todo | grep J3UISTATS
```

Measured 2026-11-11 (F4 on, which slows it to 42 fps, so read shares, not totals):
`build=2.5 style=6.1 layout=7.8 paint=6.8 gpu_paint=0.1`, `text=3469 shapes=3565`.

Then `../stress.py j3` for the README table. Every step must keep `cargo test` (the `dom_test`, `css_test`,
`layout_test`, `motion_test` stdlib tests), `j3c golden examples/desk-app` and the `J3_STEADY=60:120` idle check passing.

## Steps (order = expected win; each is independent and shippable alone)

### 1. Layout boundaries (layout, biggest)
`markDirty` (`j3c/stdlib/j3/ui/dom/treemark.j3:5`) marks the node and every ancestor up to the root dirty. One
changed text in one card makes `.grid` re-run flex-wrap over all 1,200 children, every frame.

- A node is a **layout boundary** when its border box can't depend on its content: definite `width` and `height` in
  px (or %/calc of a definite parent), `overflow` not `visible` with both definite, or `contain: size`/`strict`.
  Compute it in `applyStyle` (`dom/style.j3`) and store it on `Node` (`dom/types.j3`).
- `markDirty` stops climbing after it marks a boundary, and pushes the boundary onto a `Dom.relayout` list.
- `layout` lays out the normal pass (now skipping the clean grid via the existing check at `dom/layout.j3:262`), then
  each listed boundary again with its last `lastW`/`lastH`/`basisW`/`basisH` and its last position.
- Careful: `intrGen` / `skipped` (`dom/layout.j3:5`) is the intrinsic-size cache; a boundary's min/max-content doesn't
  change either, so ancestors keep theirs. Absolute boxes (`layoutabs.j3`) and `snapDirty` still have to follow.
- Test: in `dom_test.j3`, a 100-card fixed-size flex-wrap grid, change one card's text, assert `d.laid` is the card's
  subtree size, not ~100.

### 2. Retained paint (paint)
Every frame re-emits every text run and shape through `j3.gfx.paint`. Cache per node the instances it produced
(the `p2Push` records, `j3.game` `p2batch.j3`) keyed by its style id, size and `dirtyGen`, plus the paint-state
(translate/alpha/clip stack) it was emitted under. A clean node copies its cached run into the batch (one memcpy);
a dirty one rebuilds. Start with plain boxes and text only; anything with effects, transitions, `textwave`,
ripples, canvases or stacking contexts (`dom/stack.j3`) paints as today. Cache memory goes in the Dom arena and is
freed when the node is. Test: second identical frame produces byte-identical batch; goldens unchanged.

### 3. Inline `style:` without re-matching (style)
`styleNum`/`styleStr` (`dom/build.j3:~230`) call `markStyle`, which reruns `computeStyle`
(`cssmatch/cascade.j3:282`) for the node and its subtree. When only the inline value changed (classes, flags and
parent style unchanged), mark a new `inlineDirty` bit instead: `resolveNode` then skips `computeStyle`, keeps
`y.base`, and runs only `inlineStyle` (`dom/style.j3:52`). The subtree restyles only if an inherited property
changed (what `dynattr.build` already reports). Here that's 1,200 fills a frame that no longer match.

### 4. In-place value writes for paint-only props (style)
Properties that never change which rules match or the box size (`background-color`, `color`, `opacity`,
`transform`, `border-color`, `box-shadow` color) don't need a restyle + relayout. For a class change whose old and
new matched rules differ only in such properties, compute the new style but skip `markDirty` (`applyStyle` sets
`changed` today on any new style id). Generalise: `applyStyle` diffs old vs new style on the layout-affecting
props only (one table in `css/props.j3`) and calls `markDirty` only when one differs. Covers the 150 `.c0`–`.c3`
swaps and transitions too.

### 5. Text width cache (layout)
Changed text nodes call `game.textWidth` (`j3c/stdlib/j3/game/textmeasure.j3:9`) each time. Add a small fixed-size
hash cache (font raw, bold, letter/word spacing, bytes up to 32) → width in `j3.game`, cleared when fonts reload.
ASCII already sums advances, so measure first: if it's under 0.3 ms in the stress scene, drop this step.

### 6. Cheaper build (build)
`{#each 0..N}` formats 1,200 strings and walks 4,800 build calls a frame. Check with `j3c dump-ui ui` what the
generated code does per item: interpolation into a stack string, `storeBytes` compare, `clsIfId` per class.
Look for: a per-node "same int as last frame" fast path for `{int}` text (skip formatting), and class lists
compared as interned ids once rather than per class. Smallest win (2.5 ms of 23); do it last.

## Done when
Stress scene CPU per frame on a real GPU roughly halves (target under 5 ms), Xvfb `./stress.py j3` re-run and the
README "Heavy UI" table and notes updated, `cargo test` and goldens pass, `./build.sh` run.
