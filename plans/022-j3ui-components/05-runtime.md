# Stage 05: Paint, input, events, controls, `<canvas>`, and the codegen target

## Done

**Shipped**
- `j3.ui.dom` (`j3c/stdlib/j3/ui/dom.j3`) gained the runtime. Frame: `d.beginFrame(size: Vec2)` (clock, mouse snapshot, hit target from last layout, hover/active/click/wheel), `d.layout()` (ends the build walk, restyle, layout, rewinds paint), `d.endFrame()` (sweeps finished ghosts). The old `layout(w, h)` is now `layoutAt(w, h)`. `setFrameMs(ms)` fixes the clock (else `game.ticks()`); `simInput(x, y, down, wheel)` replaces the real mouse (the test hook).
- Events: `clicked(n)` (press and release on the same node, bubbles to ancestors, disabled target swallows), `mouseDown/mouseUp/mouseMove(n)`, `wheelAt(n)` (target itself only), `localMouse(n)`, `size(n)`, `dragging(n)`, `rangeValue(n)` (min/max/step), `fade(n, ms)`. Hover/active chains set `FL_HOVER`/`FL_ACTIVE` on target and ancestors; wheel scrolls the nearest scrollable ancestor by 48 px, clamped.
- Slots: `enter(c)`/`leave(tok)` offset a nested construct (child component, `{#each}` body); locals are 0..4095, each element resets numbering for its children.
- Paint: `paintNext(slot)`, `paintBox(n)`, `paintEnd(n)`, `beginCanvas(n): bool`/`endCanvas(n)`. Backgrounds, borders (uniform via `roundRectEx`, one-sided as flat strips), 1 to 4 radii, opacity, `scale()` about the center, `overflow-y` clip with scroll, text (bold too), checkbox and range controls (accent-color). Transitions on background, border colors, color, opacity, transform (premultiplied color lerp, retarget from the current value); `transition:fade` enter fade and leave ghost (stays in layout, no input, removed in `endFrame`).
- `j3.gfx.paint`: `textBold`, `pushIdentity`; `j3.game`: `p2TextBold` (+ headless stub).
- `speculative/12_ui_runtime/`: the codegen target (UpgradeRow, Upgrades, Sidebar cut-down, Settings, Game with canvas/HUD/fade overlay, BallCard), `README.md` = the naming scheme, `golden.png`, `embed.py`, test host `src/main.j3` (`test`, `steady [N]`, `golden`).
- `tools/bench2d`: scene `ui N` (BenchGrid + BallCard in the same generated shape).
- Tests: `dom_test.j3` now 172 checks (click, disabled, press/release mismatch, hover transition, wheel, scrolled hit test, ghost, 10,000 idle frames with 0 layouts/restyles/arena growth, slot enter).

**Verify output** (machine: Intel Core Ultra 7 270K Plus, RTX 5060 Ti, driver 580.173.02, offscreen)
- `j3c check speculative/12_ui_runtime`, `tools/bench2d`, `examples/fps-shooter`: pass. `css_test ok (78)`, `dom_test ok (172)`, `cargo test --lib stdlib::` ok.
- `idle test`: 13 PASS, 0 FAIL: UpgradeRow click raises `sim.spent` (parent `on:buy`), disabled button ignores click, canvas click reaches `on:mousedown` with canvas-local coords, HUD click does not reach the canvas, nav button sets `tab`, `bind:checked`, `bind:value` (drag gives 3.5), overlay present, Resume in the canvas/if-block works, ghost kept then removed.
- `idle steady 5000`: layouts 0, style recomputes 0, arena growth 0 bytes. (10,000 frames in dom_test: same.) `golden.png` (looked at once, 1280x760); a fresh capture: `0 of 972800 pixels differ`. `bench2d shapes` golden still `0 of 921600 differ`.
- `bench2d ui 100` (release, median of 3 runs 0.743 to 0.754 ms, 600 draws): 0.75 ms; dev (opt 0) 1.25 ms. `ui 1000`: dev 12.4 ms.

**Deviations**
- Events are a struct of `bool`s (`NameEvents`), not `u64` bits: J3 has no `|`/`&` on ints (plan STOP condition said to do this).
- `d.layout()` as specified; the old `layout(w, h)` was renamed `layoutAt`. `paintBox` returns nothing; `beginCanvas` returns bool. Added `enter/leave` (slots) and `paintNext` (the plan said "driven by the DOM's children" without an API).
- `render` host test hook is `simInput`, not `J3_SCRIPT_INPUT`. `steady` runs 5000 frames in the app (offscreen GL costs 0.9 ms per frame, 10,000 does not fit `timeout 10`).
- No `ui_scale` multiply: `game`/`paint` coordinates are already logical.
- Root `new` builds Css and Dom; every component has `Name.styles(css)`; no `Name.Props` nesting (`NameProps` struct).

**Gaps** (`ponytail:` in code)
- Hit testing ignores `transform` and uses tree order (not CSS z-order). One-sided borders ignore radius. Label click does not forward to its input. `on:draw` inside an `{#each}` cannot see the loop variable (paint does not rebind it). One `<canvas>` per component without suffixing. Per-shape alpha (group opacity needs a layer). Text under `scale()` is blurry.
- `scene3d` inside a canvas: not checked; use `gfx.setViewRect` with `absX/absY/size(n)`.
- Slot ceiling: 4096 local slots per construct (`4096 / K` iterations of an each body), 4 nested `enter`s per element; text children inside `{#if}` next to elements are not numbered.
- A `List<int>` field fails in modules importing `j3.ui.css` (checker quirk); `f().x` is not a place in J3.

**Next stage needs to know**
- `speculative/12_ui_runtime/README.md` is the contract; diff against `src/ui/*.j3`. The generator must hoist `f().field` into locals, bind props to same-named locals, rewrite instance lets to `this.x`, and keep every `{expr}` pure (they run in `update` only).
- Paint cost is dominated by flushes: scissor changes, canvas views and each font-size/bold atlas switch end a batch (`bench2d ui 100` makes 600 draws; the idle-breakout frame about 15, about 0.9 ms of GL submit in offscreen mode). A text atlas shared across sizes or per-instance clip in `j3.gfx.paint` would fix it.
- Scoped CSS: pass the component's `SCOPE` as the third `open` argument for its own elements.
- Not committed: `plans/README.md` row for 022 stays until stage 08.
