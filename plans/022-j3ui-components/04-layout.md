# Stage 04: `j3.ui.dom`, the retained tree and layout

## Done

**Shipped**
- `j3c/stdlib/j3/ui/dom.j3` (`use j3.ui.dom.{ Dom, Node, Frag }`, embedded in `src/stdlib/mod.rs`). Main thread only; no GL, no window. Module doc holds the supported layout subset.
- `Dom.new(mem: &Arena, css: &Css): Dom`; `d.tag(name): int`; `d.setFallbackFont(path)`.
- Build: `d.begin()`, `d.open(slot, tag, scope): int`, `d.text(s): int`, `d.close()`, `d.end()`. `open` reuses the next child when slot, tag and scope match, drops earlier siblings with a smaller slot (slots increase within a parent), else inserts; `close`/`end` remove the unvisited tail (ids go to a free list). `text` copies only when the bytes changed.
- Attributes: `cls`, `clsId` (pre-interned), `clsIf`, `clsIfId`, `attrType(id, s)`, `attrDisabled`, `attrChecked`, `attrValue`, `attrMin/Max/Step`, `setFlag(id, css.FL_*, on)` (hover/active), `restyle()` (hot reload). All mark dirty only on a real change.
- Run: `d.layoutAt(w, h)` (renamed from `layout` in stage 05; resolves dirty styles, lays out, positions absolutes; a no-op when nothing changed), `d.invalidate()`, `d.scrollBy(id, dy)`, `d.absX/absY(id)` (scroll included), `d.textOf(id)`, `d.count()`; counters `d.laid`, `d.restyled`.
- Results are plain fields on `d.nodes.items[id]` (`Node`): `x y` (relative to the parent's border box), `w h` (border box), `contentH scrollY scrollMax baseline style disp checked value attrMin attrMax attrStep`. Text is `Frag` chains: `node.frag0`, then `d.frags.items[f].next`; a `Frag` is `{node, kind (0 text, 1 inline-block), x, y, w, start, end}` with `x y` relative to the owner block's border box and `y` = top of the font line box (where `drawText` takes its position). The owner is the block that has inline children, or a text node that is itself a flex/grid item.
- `j3.game` (+ stub in `game_headless.j3`): `textWidth(font, []u8, bold): f32`, `fontLine(font)`, `fontAscent(font)`.
- `stdlib/tests/dom_test.j3`: 157 checks plus timings. Run from the repo root: `SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy timeout 120 j3c run --opt 2 j3c/stdlib/tests/dom_test.j3`.

**Numbers** (Intel Core Ultra 7 270K Plus; median of 51 reps, `--release` is `-Os`, three runs identical to 1%; 56 cards = 985 nodes, 125 cards = 2158 nodes; BallCard tree is 17 nodes, so the plan's "125 cards" is 2158 nodes, not 1000)

| | target | release | `--opt 2` | dev (opt 0) |
|---|---|---|---|---|
| full layout, 985 nodes | <= 500 us per 1000 | 212 us | 171 us | 1358 us |
| full layout, 2158 nodes | | 390 us | 315 us | 1880 us |
| steady frame (build + layout, nothing changed), 985 / 2158 nodes | <= 100 us | 45 / 89 us | 24 / 36 us | 120 / 266 us |
| one counter text changes, 985 / 2158 nodes | <= 100 us | 46 / 90 us | 25 / 37 us | 125 / 261 us |

A steady frame runs zero layouts and zero restyles (`d.laid`/`d.restyled` unchanged). A counter change lays out 9 boxes of 985 or 2158. Arena `used` is identical before and after 10,000 changing frames (5834864, 9602398 bytes). Dev builds miss the steady targets at 2158 nodes (the cost there is the build calls and byte compares).

**Proof**: `dom_test ok, checks passed: 157`; `j3c check` of a scratch user of `j3.ui.dom` prints `checked chk.j3`; `j3c doc j3.ui.dom --short` lists the API; `css_test` still passes; `cargo test --lib stdlib::` passes; `cargo test` has the same 11 failures as stage 02 (missing examples, an lsp test, `phase::`); `j3c check examples/fps-shooter` and `tools/bench2d` pass.

**Deviations**
- `textSize` needs a GL or SDL renderer for ASCII (it builds the glyph atlas) and panics headless, so layout measures with the new `j3.game.textWidth`: SDL_ttf only, sum of per-glyph advances (exactly the widths the GL atlas draws with), `TTF_SizeUTF8` per run of non-ASCII bytes. Mixed strings such as `Buy · 5` therefore differ by kerning from a whole-string measure.
- Fonts are cached by (family, size); bold is a draw/measure flag (`TTF_STYLE_BOLD`), not a separate load.
- Text nodes get a real computed style (tag `#text`, inherits from the parent), so every reader works on them.
- One built-in browser style: a `button` centers its text horizontally and vertically (Chrome does; the nav buttons are wider than their labels). Any other UA default is absent, as the README says.
- Replaced sizes (mine, not CSS): checkbox 16x16, range 129x16, other inputs 160x28.
- No margin collapsing, documented in the example README with the other known differences. idle-breakout never puts two non-zero margins together (checked: `.grid` margin-top follows an `h2` with none, the last section's margin-bottom sits in a padded scroll box, flex margins don't collapse).
- Natural size of an `align-items: stretch` flex item is laid out once, then again only if the stretched size differs (column and grid rows); with a definite container height a row item is laid out once at the stretched height.

**Gaps** (`ponytail:` where it matters)
- Inline elements (`span`, `small`) have no box: no padding, border, or background on them; their rect is 0,0,0,0. Inline-block in a line is baseline-aligned by its own baseline.
- Static position of an absolute box is its parent's content origin. `position: relative` offsets (`left/top`) are not applied. Percent padding/margin use the parent width.
- Flex: one line, no wrap, no `*-reverse`, no `align-self`, no `order`, `flex-basis` only via `flex: n` (basis 0) or width/auto. Grid: up to 16 columns, no spans, no row template, `fr` floors use min-content in one pass, rows are auto and not stretched to a definite height. At most 16,384 flex/grid items across nested containers (fixed array); at most 8 classes per node; nesting depth 128.
- Percent `height` needs a definite parent; `margin: auto` is 0; `display: inline` set explicitly on a block tag falls back to the tag default.
- Intrinsic widths are cached by a generation counter; they ignore `max-width` (not in the subset).
- Fragments of a block are reused in place on relayout; the frag list only grows (freed ones are reused).

**Next stage needs to know**
- A checker quirk: a struct field of type `List<int>` in a module that also imports `j3.ui.css` fails with "expected `List`, found `List`" (here worked around with `List<Idx>`). `x.f().field` is not a place, `match` is a keyword (field `mnodes`), `let v = 0.0;` is f64 (write `: f32`), and two literals in one expression (`760.0 - 12.0`) make an f64.
- Paint each element at `absX/absY`, then its text fragments at owner position + `Frag.x/y`; children of an `overflow-y` box shift by `-scrollY` (absY already does). Inline-block fragments (`kind == 1`) are painted as boxes via their node. `Node.style` is a css style id; read colors, radii, borders with `css.get`.
- Hover/active: `setFlag` then `layout`; only that node and its subtree restyle. The id of a restyled node changes only if its computed style did, so transitions can compare `Node.style` before and after.
- Position and sizes are float px; Chrome rounds to device pixels, so snap in paint, not in layout.
- Text keeps `Node.buf/tlen`; `textOf(id)` is the string for `Frag.start..end`.
- Fonts load lazily on first layout of a style (`tryLoadFont`); a missing file leaves `d.fontMissing` > 0 and text measures 0 wide.
