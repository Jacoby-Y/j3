# Stage 03: `j3.ui.css`, the CSS subset

## Done

**Shipped**
- `j3c/stdlib/j3/ui/css.j3` (`use j3.ui.css.{ Css, parse, computeStyle, ... }`, embedded in `src/stdlib/mod.rs`, along with `gfx/paint.j3`, which stage 02 had left out). No GL; a `Css` may be used on any thread that owns it. Module doc holds the supported subset, cascade, the Svelte scoping rule, and the table of starting values.
- API: `Css.new(mem: &Arena): Css`; `parse(css: &Css, src: string, file: string, scope: int): Sheet` (appends rules, custom props, `@font-face`s, warnings); `computeStyle(css: &Css, nodes: []MatchNode, id: int, parentStyle: int): int` (style id, -1 parent for the root); `css.get(style, P_*): Value`; `css.intern(name): int`, `css.name(id)`; `css.reset()` (hot reload: reset, parse again, recompute), `css.clearCache()`; `propId(name): int`; `css.warns: List<Warning{file,line,col,msg}>`, `css.fonts: List<FontFace{family,path}>` (path resolved against the css file's dir, `..` collapsed), `css.rules` (`Rule.spec`, `.order`).
- `MatchNode { tag, classes: []int, parent, prevSibling, flags (FL_HOVER|FL_ACTIVE|FL_DISABLED|FL_FIRST_CHILD), type, scope }`; stage 04 keeps a slice of them indexed by node id. Tags, classes and the `type` attribute value are `css.intern(...)` ids.
- `Value { kind, f, i, c }`; kinds `V_PX V_PCT V_AUTO V_NONE V_NUM V_COLOR V_KW V_TRANS V_SCALE V_FR V_STR V_VAR V_INHERIT V_CURRENT`. Colors are `c = r*16777216 + g*65536 + b*256 + a`. Keywords are `i` = index into the property's list (`DISPLAY_*`, `POS_*`, `DIR_*`, `JUSTIFY_*`, `ALIGN_*`, `BOX_*`, `OVERFLOW_*`, `TEXT_*`).
- 57 longhand ids `P_*` (`NPROPS`); a computed style is `NPROPS` values. Shorthands expand at parse time to longhands: sides are top, right, bottom, left at `P_PADDING_TOP+0..3`, `P_MARGIN_TOP`, `P_BORDER_TOP_WIDTH`, `P_BORDER_TOP_COLOR`; corners tl, tr, br, bl at `P_RADIUS_TL`; `gap` is `P_GAP` (column) + `P_ROW_GAP`; `flex: n` is grow in `P_FLEX`; `font-weight` is a number (bold 700); `grid-template-columns` is up to 4 tracks at `P_GRID_COL0+0..3` plus `P_GRID_REPEAT`; `transition` is up to 4 `V_TRANS` at `P_TRANS0+0..3` (`i` first longhand id, `c` how many longhands, `f` ms).
- `stdlib/tests/css_test.j3`: 78 checks plus timings. Run from the repo root: `j3c run j3c/stdlib/tests/css_test.j3` (add `--opt 2` or `--release` for timings).

**Numbers** (Intel Core Ultra 7 270K Plus; median of 51 reps per run, 3 runs; 994 nodes in BallCard-grid shape; 13 distinct styles)

| | target | dev (opt 0) | opt 2 | `--release` |
|---|---|---|---|---|
| parse theme.css + 9 style blocks | <= 200 us | 286 us | 96 to 164 us | 66 to 100 us |
| computeStyle, cold cache | <= 300 us | 209 us | 51 to 53 us | 44 to 45 us |
| computeStyle, cache hit | <= 50 us | 170 us | 43 to 44 us | 36 us |

Dev builds miss the parse target; release meets all. Parse timing is noisy run to run.

**Proof** (final run): `rules=59 decls=295 customs=15 fonts=1`, zero warnings on theme.css and all nine style blocks; `css_test ok`, `checks passed: 78`. `j3c check` of a file with `use j3.ui.css;` prints `checked chk.j3`; `j3c doc j3.ui.css --short` lists the API. `cargo test --lib stdlib::` passes (embedded files match the tree).

**Deviations**
- One `Css` holds every sheet (`parse(&css, ...)` appends and returns a small `Sheet`), instead of `parse(...): Sheet` owning its rules. The cascade and the cache need the flat rule list, and hot reload is `css.reset()` plus `parse` again.
- `computeStyle` takes the `Css` and returns a style id, the cache and values live in `Css`. `var()` custom properties are typed at parse time (one token: color, px, %, number), not re-parsed per use.
- A selector list becomes one `Rule` per selector (specificity differs per selector).
- Warning position is the property name (declaration start), also for bad values; a bad selector warns at its first token.
- Registered `gfx/paint.j3` in `src/stdlib/mod.rs` (stage 02 left it out; it only worked because of the disk fallback).

**Gaps** (`ponytail:` where marked in code)
- Names are interned by linear search. Selector compound: max 4 classes; rule: max 8 selectors; max 96 tokens per value or selector; max 128 matched rules per node; cache is 4096 slots, cleared when half full (old style ids stay valid).
- `auto` is accepted on every length property. Identifiers are case sensitive. Unknown `var()` leaves the previous value (no "invalid at computed time" reset). `!important`, `>` `+` `~`, ids, pseudo-elements, other at-rules warn and drop the declaration or rule.
- `parse` clears the style cache; ids computed before it are stale.
- Arena memory from `reset` + reparse is not reclaimed (rewind the arena yourself).

**Next stage needs to know**
- J3 gotchas hit: no char literals (use codes), no `|` on ints or bools, a local name can't be reused anywhere in a function (even in sibling blocks), `x.get(..).field` isn't a place (copy to a local first), `string == string` needs `j3.text.same`, no `const` arrays.
- Initial `display` is `inline` (CSS initial, no UA styles): stage 04 decides block vs inline for elements with no `display`. `V_CURRENT` (currentcolor, the initial border color) and `V_AUTO` (`line-height: normal`, `accent-color`) are for the consumer to resolve.
- Compute parents before children; pass the parent's style id. Hover/active changes: update `flags`, recompute that node and its descendants.
- Custom properties are only accepted in a `:root` rule; `:root` matches the node with `parent < 0`.
