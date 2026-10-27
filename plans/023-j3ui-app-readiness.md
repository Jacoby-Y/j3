# Plan 023: `.j3ui` for real desktop apps

Written 2026-10-20 after reviewing `examples/big_ui_test` and `j3c doc j3ui`. Follows plan 022 (`022-j3ui-components/`).
Trimmed 2026-10-20 to the work that is left; the finished stages are in `023-done.md`.

**The original problem.** Desktop apps needed scalable lists, tunable components and fully themed popups.
Stages 1–3 now provide these. Remaining work starts with runtime-chosen components, rich input and data grids,
followed by the ergonomics, CSS, text, accessibility and controller work below.

**Earlier work (old stage numbering).** Stages 0 to 10 and 12 step 1 are finished, and stage 11 is partly done: papercuts, `<slot/>`,
dynamic `class`/`style:`, lifecycle hooks, keyed `{#each}`, event payloads and `provide`/`inject`, `{#match}`,
textarea/password/IME, `flex-wrap`/calc/HiDPI, themed popups and menu keys, font fallback/shaping/bidi, and the
semantic tree (`j3.ui.a11y`). Details and Done notes: `023-done.md`.

**Current progress (2026-10-22).** Stages 1–12, 14 and 15 (items 1–7) are done and reviewed; a final review-only pass ran clean apart from doc fixes. Stages 13 (a11y) and 16 (gamepad) are **deferred: do not implement unless the user asks**. Read [`023-chunks.md`](023-chunks.md) for per-chunk notes.

Each executor: read the whole stage before starting, drift-check its "Current state" against the live files, keep
the J3 rules in `j3c/AGENTS.md` (no crates, no second parser), add tests next to the existing ones
(`j3c/src/ui/tests.rs` for the compiler, `j3c/stdlib/tests/*_test.j3` for the runtime), update `j3c doc j3ui` /
`j3c doc css` for anything users see, and fill in the stage's **Done** notes the way 022's stages do.

## Status

Ranked. Order follows need (chat and spreadsheet first), adjusted for dependencies: the data grid needs virtual
lists, and rich text input needs popups as DOM nodes for the caret popup, so popups sit before it.

| Status | Stage | Title | Effort | Depends on |
|---|---|---|---|---|
| DONE | 1 | Virtualized lists and grids, hash-map keyed reconcile | XL | — |
| DONE | 2 | Component API: prop defaults, `class`/`style` passthrough, `$slots`, `var()` fallback (old 13) | L | — |
| DONE | 3 | Popups as real DOM nodes (old 10 leftovers; context menus, hover cards, custom option content) | L | — |
| DONE | 4 | Dynamic components: `<component this of>` | M | — |
| DONE | 5 | Rich text input: undo/redo, inline spans, caret popup | XL | 3 |
| DONE | 6 | Data grid | XL | 1 (and 3 for editors) |
| DONE | 7 | Markup ergonomics (old 14) | M | — |
| DONE (8a–8c; `j3c golden`) | 8 | Test kit and steady counter (old 15) | M | — |
| DONE | 9 | Frame-scratch text and shutdown hook (old 16) | M | — |
| DONE (table half 6d, dialog half chunk 10) | 10 | Tables and dialog inputs (old 18; the column-track half folds into 6 if that lands first) | M | — |
| DONE (11a, 11b) | 11 | Language papercuts (old 17) | S | — |
| DONE | 12 | Text stack leftovers (old 11): emoji, RTL caret, static selection | L | — |
| DEFERRED (don't implement unless asked) | 13 | A11y gaps (old 12 step 1) and the platform bridge (step 2 is plan 024) | M | — |
| DONE (3 items; rest listed) | 14 | Leftovers | S | — |
| DONE (items 1–7; 8–9 per project need) | 15 | CSS gaps for shipped game UI (units, flex, selectors, inline boxes, text) | L | — |
| DEFERRED (don't implement unless asked) | 16 | Gamepad and focus navigation | M | 13 (focus rules), 017 item 5 (controller lifecycle) |

Stages 15 and 16 were added 2026-10-21 from a review of what blocks a full Steam game with heavy `.j3ui` (Steam work itself: plan 025).

Stages 1, 2, 7 and 11 all touch `gen.rs`; run them one at a time and rebase. Stages 8 to 11 are independent of the
rest and cheap, so they can fill gaps between the big ones.

---

## Stage 1: Virtualized lists and grids

**Why.** `{#each}` builds and lays out every item. A chat history or a 100k-row table cannot work that way. Goal:
100k rows scroll at a steady 0 layouts, with only the visible rows alive.

**Design.**
- A scroll box with `virtual` (name open) takes one `{#each}` child. It keeps a window of rows around the viewport
  plus a few rows of overscan, and puts spacers (or a content height) above and below so the scroll bar is true.
- Fixed height: `virtual row-height={26}`. Row `i` is at `i * h`; the window is an index range.
- Variable height: heights are measured when a row first lays out and cached by key. Unmeasured rows use a running
  average; the total height corrects as rows measure. Cache is dropped when the row's key goes.
- Keyed state: a row that leaves the window loses its component state unless its key is in a `keep` set (the list
  author names it, e.g. the row being edited or a playing video). Kept rows stay alive but unlaid-out off screen.
- Scroll anchoring: when rows above the first visible one change height or are inserted, adjust the scroll offset by
  the difference so the visible content does not jump. `stick-to-bottom`: while the view is at the end, new rows
  keep it at the end; scrolling up releases it; a "jump to latest" check is `scroll.atEnd`.
- Hash map for keyed reconcile: `ui/keyed.j3` matches old and new keys with an O(n^2) scan (the `ponytail:` note).
  Replace it with an open-addressing map keyed by the key bytes (or the typed int fast path from stage 7) so a
  reconcile is O(n). Needed first, since 100k keys make the scan unusable.

**Implementation notes.** Window math lives in a new `ui/virt.j3`; the reconcile in `keyed.j3` gets a "range"
argument so it creates and drops only rows entering and leaving the window. `scroll.j3` already owns offsets and
the wheel; add anchoring there. The generator lowers `virtual` to the same keyed block with a range.

**Tests.** `virt_test.j3`: 100k rows, count live nodes (bounded), scroll to the middle and the end, steady frame
0 layouts, variable heights with anchoring (insert above, no jump), stick-to-bottom with appends, keep set. A bench
in `tools/bench2d` or a speculative demo with 100k chat lines.

**Done when.** A 100k-row list scrolls at 0 steady layouts, a chat demo sticks to the bottom and keeps its place
when older messages load above, and the keyed reconcile is O(n) (checked with 10k keys shuffled).

---

**Done (1a): keyed reconcile is linear.**
- `j3c/stdlib/j3/ui/keyed.j3`: `Keyed.finish` keeps its allocation-free prefix fast path; on a changed frame it builds two open-addressing tables (`otab` over old keys, `ntab` over new keys; hash of the key bytes, linear probing, power-of-two capacity >= 2x rows, kept in the struct and only grown). Old keys go in by index and probe in insertion order, so a repeated key still takes the next unclaimed old row by position (the same-position check stays first). The duplicate-new-key warning uses `ntab` (same text, once per list). The `ponytail:` scan note is gone. API unchanged; no change to `gen.rs`. The hash has no xor (the language has no bitwise ops): `(h + byte + 1) * prime`, then the high half folded in.
- `j3c/stdlib/j3/ui/dom.j3`: new `gfirst` (parallel to `gpar`) is a hint node per row id: set when a node is created inside the row and at `keyEnd` (the last node visited, if in the row), reset to -1 in `groupNew`. `keyBegin` trusts it only if it is a live (not `K_FREE`) child of `parent` still `inGroup` of the row, walks `prev` to the run start, then moves the run as before. -1 (new row) skips the search; a stale hint falls back to today's sibling scan, so it is never wrong, only slower when every node of a row went away.
- Tests: `j3c/stdlib/tests/keyed_test.j3` (steady frames and no arena growth, reverse, insert/remove, all new, empty both ways, duplicates exactly, 10k shuffle against a reference); `shuffle10k` in `speculative/16_ui_lists` (10k rows with marker nodes, node identity per key and order, markers, time). 10k shuffle, dev build: `Keyed.finish` 890 ms to 1.9 ms; whole DOM frame 1209 ms to 41 ms. `speculative/14,15,16,17`, `dom_test` (188), `input_test` (182), `overlay_test` (145), `layout_test` (109), `desk-app` goldens + `DESK_TEST` + `DESK_STEADY` and `cargo test --lib ui::` pass.
- Caveat: timings come from the ms-resolution `ticks` clock when headless. The stdlib is embedded: rebuild `j3c` (or set `J3_STDLIB`) to pick this up.

**Done (1b): fixed-height `virtual` lists.**
- Markup: `<div class="list" virtual row-height={26}>{#each rows as r, i (r.id)}...{/each}</div>`. `gen.rs` `virtual_attrs` checks the element (one child that is a keyed `{#each}` over a list, `row-height` present and numeric, at most 6 `style:` attrs) and hands the node variable and row height to `each`, which lowers to the same keyed code over a window: `d.virtWin(n, len, rh)` gives `lo..hi`, the key loop, reconcile and body loop run `0..hi-lo`, the item is `x[o + i]`, and a user index is the real index (`o + i`). Unkeyed `{#each}`, `0..n` ranges, a `virtual` component tag and slot content inside a virtual each are compile errors (named). `virtual` / `row-height` are in `G::accepted`, so the LSP completes them.
- Runtime: new `ui/virt.j3` (`window(count, rh, scrollY, viewH, over)`, pure, clamps `scrollY` to the real range so a list that shrank still gets `0 <= lo <= hi <= count`; `OVERSCAN = 4`; chunk 1c replaces the `i * rh` positions here). `Dom.virtWin` feeds it the box's `scrollY` and last laid-out `h` (the window `vh` before the first layout). `Dom.virtPad` sets `padding-top` / `padding-bottom` through `styleNum` slots 6 and 7: the spacers are padding on the scroll box, not nodes, so `gap` and the keyed run bookkeeping are untouched, `scrollMax` is true, and an unchanged value dirties nothing (idle frames: 0 layouts, 0 restyles). Cost: the author's own padding on that box is overridden (use a wrapper), and the CSS row height must equal `row-height` with no gap/margin between rows.
- Frame order (found): `render` is `beginFrame` (wheel, scrollbar drag, inertia, `inputEvents`) then `inputs.begin`, `update`, `layout`. So the scroll offset is final before the generated `update` reads it; rows do not lag, and no extra overscan is needed for a frame of wheel travel. Only `n.h` (the viewport height) is one layout old, which matters only on resize. `scrollBy` is clamped by the previous layout's `scrollMax`, which is already the full range because the padding is set every frame.
- Tests: `stdlib/tests/virt_test.j3` (window math: top, middle, end, overscan clamps, empty, shrink under a scrolled box, h = 0, zero row height, 2400 generated windows valid); `speculative/18_ui_virtual` (100k rows: 62 live nodes, `scrollMax` within 1 px of `100000*26 - 200`, middle and end show the right rows in the same frame, a simulated wheel notch, 300 idle frames at 0 layouts / 0 restyles / 0 arena growth, shrink to 10 rows and to 0; 15 PASS); `ui::tests::virtual_list_generates_a_window` (generated lines and the six errors). Regressions: `speculative/14,15,16,17` (12/12/42/6 PASS), `dom_test` 188, `layout_test` 109, `input_test` 182, `overlay_test` 145, `keyed_test`, `cargo test --lib ui::` (40), desk-app goldens.
- Numbers (dev build, headless, ms-resolution not needed: `micros`): one-page scroll frame over 100k rows about 100 us; a one-row scroll lays out 19 boxes.
- Review fixes: `Win` also carries `top` / `bottom` (the spacer px), so the generated code only calls `d.virtWin(n, len, rh)` and `d.virtPad(n, w)` and does no row-height math; 1c changes `virt.window` and these two calls, not the lowering. Nothing is O(len) per frame: the list expression is a slice, `.len` is O(1), keys and rows are built for the window only (10000 frames jumping across 100k rows: arena flat, 0 bytes, about 100 live nodes). More than 6 `style:` on a virtual element is a compile error (tested). A user `padding` shorthand in CSS still gives left/right (the inline longhands override only top and bottom); documented, not warned. Focus: a row that leaves the window frees its nodes and `freeSubtree` clears `Dom.focus` (tested with an `<input>`; scrolling back does not restore it; the 1c `keep` set is the fix). Shrink to 0 while scrolled and grow back from 0 are tested. The 6 restyles seen after a scroll are not the list: any scroll, even inside the same window, restyles 6 nodes in that one frame (scroll bar state in `scrolled()`); rows entering cost about 9 restyles each, once. Sizes: `dom.j3` +24 lines (5665), `gen.rs` 2772, `virt.j3` 34.
- Not built in 1b (done in 1c, below): variable heights and measuring, scroll anchoring, stick-to-bottom, the `keep` set.

**Done (1c): measured heights, scroll anchoring, `stick-to-bottom`, `keep`.**
- Markup: `virtual` without `row-height` (or `row-height="auto"`) measures rows; `est-height={40}` is the guess before any is measured (an error with `row-height`). `stick-to-bottom` (bare), `keep={keys}` (a `[]string` of the `{#each}` key as text), `bind:at-end={place}` (a `bool` written every frame; the author sets it true for "jump to latest"). All in `G::accepted`; checked in `virtual_attrs` (errors named and tested). Fixed `row-height` keeps working and uses the same code with `i * rh` instead of the tree.
- Runtime, three modules. `ui/virt.j3` (pure, `virt_test`): `Tree`, an array segment tree (the language has no bit ops, so not a Fenwick) of measured-height sums and unmeasured-row counts: `prefix(i)`, `find(y)`, `set` O(log n), `remap(from)` / `reset` O(n); an unmeasured row counts as `avg()` (mean of measured, `est` before any, floor 1 px). `windowTree`, and `follow(y, last, shift, total, viewH, stick, want)`, the anchoring and stick rules as one pure function. `ui/vstate.j3`: `VList`, the state the generated component owns (`vl<id>`). `ui/vlist.j3`: the DOM side, `step(d, box, vl, len)` once per frame before the rows are built (measure, remap, follow, window, kept rows, `padding-top/bottom` through slots 6/7) and `finish` after (hide kept rows). `Dom.virtWin/virtPad` are gone (dom.j3 is 20 lines shorter).
- Measuring: at the start of `step`, if `Dom.laid` moved since the last measurement, walk the box's children (a row is a run with the same `Node.gk`) and take row height = next row's top minus this row's top (so margin and `gap` count; the last live row adds the gap seen before), write it to the tree if it differs by 0.05 px. Layout never runs twice per frame: a newly measured row changes the total one frame later and converges (idle frames: 0 layouts, 0 restyles, 0 arena growth). Heights are not re-measured for rows that are not alive, so a width change is corrected as rows scroll back in (anchoring hides it).
- Key table: variable heights, `keep`, anchoring and focus-keeping need row index by key, so a virtual list keeps a second `Keyed` (`vl.all`) over every row. The generated code rebuilds it only when `vl.stale(len)` (the length changed, or `verify` found the window's keys disagree with the table, which catches a same-length edit one frame late): one loop formatting every key, O(rows) once per change, never on a steady frame. `Keyed.finish` then gives `from[new] = old`, which carries the heights (`Tree.remap`), the anchor row and the focused row to their new indices. `Keyed` got `find(key)` (reuses the hash table of the last changed `finish`) and `sameAs`. Append fast path: when the list only grew (`vl.grew`), the first and last old rows are formatted into a scratch `Keyed` and compared with the table (`probeOk`); if they match, only the new keys are `Keyed.push`ed (hash table rehashed, no formatting, when it passes half full) and `Tree.append` adds unmeasured rows, so an append is amortised O(new rows). Chat demo, 1 append to 100000 rows, dev build: 17.7 ms before, 81 us after (asserted < 5 ms). Rows between first and last are not checked; a same-length or interior edit is caught by the window `verify` and takes the full path one frame later. Prepend (older messages) still takes the full path: every index shifts, so the table, hash table and tree are rebuilt anyway; it is rare and one-off.
- Anchoring: `step` keeps the row that holds the viewport top (index and its top px, `anchor` / `anchorTop`); next frame the scroll offset gets `+ (prefix(anchor) - anchorTop)`, so rows measured, inserted or removed above never move what you see, wheel and drag are added on top because they change `scrollY` before `update`. Test: 100 older messages loaded above, the visible message stays within 1 px (it was exact), 40 small scroll steps in unmeasured space shift by exactly the step. After a long jump into unmeasured rows the first frame can be a few px off (rows above the view are laid out real but the tree still has the estimate); the next frame is exact.
- `stick-to-bottom`: `follow` pins to the end while stuck, releases when the offset is more than 1 px above `min(last offset, end)` (a taller box lowers the end and is not a release), re-engages within 1 px of the end; it starts stuck. Stuck ignores the anchor shift, so anchoring and sticking do not fight. `bind:at-end` is `vl.atEnd`; setting it true makes the next `step` jump to the end (and stick if the attribute is on).
- `keep`: keys resolve through `vl.all.find`, rows outside `lo..hi` join the window as sorted extras (`vl.at(i)` maps the i-th live row to a list index, used by the key loop and the body), so the keyed reconcile keeps their nodes. After the rows are built `finish` sets `display: none` on the kept rows' top-level elements through a dedicated style slot (compare-before-set, so a steady frame restyles nothing); they are alive but not laid out or painted, and their cost per frame is the update of their body. A key not in the list holds nothing. The focused row is kept automatically (found from `Dom.focus`'s ancestor in the box, remapped like the anchor); blur lets it go. The hide uses `dynattr` slot 8 (`SLOTS` is now 9), past the eight (0..7) that markup can write, so it cannot clash with an author's `style:`. The box's spacers are slots 6 and 7 of the box itself, hence at most 6 `style:` on it.
- Tests: `virt_test` (Tree vs a walk over 1000 rows, find, set, remap with prepend and shrink, windowTree, 17 `follow` cases), `keyed_test` (`find`, `sameAs`), `ui::tests` (generated lines, options, 6 more errors), `speculative/18_ui_virtual` (24 PASS; the focus case now says focus survives and the kept row is hidden), new `speculative/19_ui_chat` (own `assets/`; 100k varied-height wrapped lines; 31 PASS: starts at the end, atEnd, idle 0 layouts/0 restyles/0 arena, 5 appends stay at the end, scroll up releases, appends leave the view alone, jump to latest, anchoring, 100 older messages above within 1 px, `keep`, 10000 random jumps replayed with 0 bytes of arena growth). The first pass of jumps grows the arena about 24 KB (high-water of nodes when a window is full of short rows); the identical replay grows 0. Regressions: `speculative/14,15,16,17` (12/12/42/6), `dom_test` 188, `layout_test` 109, `input_test` 182, `overlay_test` 145, `cargo test --lib ui::` 40, desk-app goldens + DESK_STEADY + DESK_TEST.
- For the data grid (6a): everything y-axis is in `vlist.step` / `vstate.VList` (`pre`, `rowAt`, `lastY`, the child walk in `measure`); virtual columns would be a second `Tree` and the same `follow` on `scrollX`, and `at()` / `ex` already separate "which rows are alive" from "which rows are in the window". Pitch measuring assumes rows stack vertically in block flow.

## Stage 2: Component API (old 13)

**Why.** A kit component cannot be tuned from outside, so authors add props and wrapper divs. (Gripes 7a 1, 2, 3
and 7b 2, 3.)

**Design.**
- `props { gap: int = 8; note: string = ""; }`: a default per prop, used when the use site omits it.
- `class` (and `style:`) on a component tag is merged onto its root element: `<kit.Card class="wide" style:width={w}>`.
  The user's scoped CSS applies to that root. Error when the component has several roots.
- `{#if $slots.footer}` and `{#if $slots.title}`: the owner knows at compile time which slots a site fills, so this
  is a constant per site (extends the `slotSite` chain in `gen.rs`). Removes `count` props and empty `<small>`s.
- `var(--x, 8px)` fallback in `cssvalue.j3`, and `--vars` declared on any selector and inherited (today `:root` only).

**Tests.** `ui/tests.rs` for defaults, passthrough merge, the several-roots error, `$slots`; `css_test.j3` for the
fallback and inheritance.

**Done when.** `big_ui_test`'s `Section` takes an optional `gap`, desk-app's `Details` needs no wrapper div, and
`Table` has no `count` prop.

**Done (2a): prop defaults and `$slots`.**
- Defaults: `props { gap: int = 8; }` parses in `markup::parse_script` (`Prop.default`, the source range after `=`). In `gen.rs` the use site (`component`, next to the required-prop check) sends the default for an omitted prop in the same `Props { .. }` literal, every frame, rewritten with the child's `const` names; props are read-only in the child, so nothing is stored and a changed default recompiles. The rule: a default is a constant (literal, or a `const` of the child); the child's G setup rejects any prop, `let` or `fn` in it (error on the default). Required = no default. Use-site prop assignment is still one loop (`for p in child.sc.props`), the place chunk 2b/4 should extend.
- `$slots.name` / `$slots.default`: lexed as `_slots` (same length, `$` checked in the source) in `rewrite`, which turns it into `this.slotHas_<name>()`. Each component that declares slots gets one generated `fn slotHas_<name>(this): bool { return this.slotSite == 3 || ... }` (sites collected from `Slots.list` by child and `names`; `false` when none). So it is a per-site constant tested at run time against the existing `slotSite`, no new fields and nothing to go stale; a use with no children never sets `slotSite` (0, matches no site). Errors: `$slots.x` for an undeclared slot (lists the declared ones), bare `$slots`. `slotHas_` is a reserved prefix. Caveat for chunk 4: each `{#match}` arm's use is its own site, so it works unchanged; `<slot name="default">` is a compile error (use `<slot />`).
- Migrated: big_ui_test `kit/Section` takes `gap: int = 8` and `note: string = ""` (`style:gap="{gap}px"`; `style:gap={gap}` set only the column gap, silently: `gap` is a shorthand (column and row), and a number only sets one longhand; it is now a compile error that says to write `style:gap="{v}px"`) and Layout's `note=""` is gone; desk-app `Field` draws its `<small>` only for `$slots.hint`. desk-app `Table` keeps `count`: it is about the rows (`{#if count == 0}`), which a use site's markup cannot say (the rows are one `{#each}` in the default slot, filled either way), so `$slots` cannot replace it.
- Tests: `ui::tests` `prop_defaults`, `slots_constant_per_site` (two sites of one child, tags, errors); `speculative/14_ui_slots` `Tag` (2 more PASS lines). Regressions: speculative 14-19, dom/layout/input/overlay tests, `cargo test --lib ui::` 42, desk-app goldens + steady + DESK_TEST all exact, big_ui_test pages 1-16 pixel identical to before (7 and 9 animate and differ run to run). Not done: LSP completion for `$slots` (the hole completion has no `$` trigger; skipped).

**Done (2b): `class` / `style:` on a component tag.**
- New `ui/pass.rs` (scan, single-root check, the hidden fields and the root's calls); `gen.rs` `comp()` fills them in the same `Props { .. }` literal (`pass_args`, after the prop loop: the place chunk 4's per-arm use sites already reach), `elem()` applies them on the root (`pass_apply`). A component gets the hidden `Props` fields `j3pass_scope`, `j3pass_cls`, and `j3pass_k/n/f/s<k>` per passed style slot only if some use passes something (max over its uses; every use then fills them, unused slots are kind 0). `j3pass*` is a reserved name. Errors at the use: root not one element (names the reason), root `style:` + passed over 8 (slot 8 stays the virtual rows' hide), shorthand with a number, `class:x` without a condition.
- Scoping: the root node carries a **second scope** (`MatchNode.scope2` = the parent's scope id, sent as `j3pass_scope`, 0 when the use passes no class). `cssmatch.computeStyle` lets a sheet of that scope match too, but only rules whose rightmost compound names a class (`cls0 >= 0`: a parent `div {}` does not reach into the child), and such a rule gets +1 specificity (beats the child's own rule of the same shape whatever the parse order). The child's own scope is untouched. `domhtml` adds the second `s<scope>` class too.
- Runtime: `d.scopePass`, `d.clsPass` (a second channel of `clsDyn`: `Dyn.ch/cl/cn` are 2-element, `cids` 16, so the root's own holes and the passed words never drop each other's), `d.stylePass(n, k, kind, name, f, s)` over `styleNum`/`styleStr`. Each compares to the last value first: steady frames restyle 0 times, allocate 0 (speculative/20: 1000 keyed rows passing class, `class:` and `style:`).
- Precedence: passed `style:` take slots `own + k`, after the root's own, and later slots win: the user beats the root's own `style:`. A passed class loses to the root's inline `style:` (inline beats rules).
- Tests: `ui::tests` (`class_and_style_pass_to_the_root`, `passthrough_needs_one_element_root_only_when_used`, `passthrough_errors`, speculative 20 compiles), `speculative/20_ui_passthrough` (13 PASS lines: parent CSS reaches the root, hole churn, passed style beats own, 300 idle frames 0 restyles/0 layouts/0 arena growth). Docs: `j3c doc j3ui` "Class and style on a component".
- Migrated: desk-app `Th` width wrappers are gone (`<kit.Th class="c0" ..>`, goldens exact). **Not** migrated: desk-app `Details` keeps its `<div class="det">`: the wrapper was also what kept the card at content height inside the stretching `.split` row; without it the card stretches to the full height (goldens differ). It needs `align-self` (not implemented), then `<kit.Card class="det">` with `align-self: flex-start` works (Details' root is `kit.Card`, a single element root is not required of Details itself because the class goes to Card).
- Notes for chunk 4: a use site is whatever `comp()` generates, so `{#match}` arms get passthrough with no extra work as long as `pass::scan` sees the arm's tag (it walks `If` branches and `Each` bodies). A dynamic `<component this={x}>` must make `scan` record Pass for every candidate type in `of={..}`, because each shares the one literal shape. `cargo test --lib ui::` overflows the default 2 MB test-thread stack in `lsp::ui::tests::nested_components_complete_and_jump` (passes with `RUST_MIN_STACK=4000000`); it still overflowed with the new dom methods and use-site call removed, so it is probably older.

**Done (2c): `var()` fallback and custom properties on any selector.**
- Parse: `var(--x)` / `var(--x, fallback)` is `parseVar` in `cssprop.j3` (a `V_VAR` with the name in `i`, the fallback at `css.fbs[c - 1]`; the fallback is parsed for the property, may be another `var()`). A `--x: value` declaration is an ordinary `Decl` with `prop = NPROPS + interned name` (any selector; `parseCustom`: color, length, number, time as ms, or a `var()`; multi-token colors like `rgb()` now work). The `:root`-only warning and `P.inRoot` are gone. In `@keyframes` a custom decl is inert; `style:--x` stays a compile error (not supported).
- Storage: custom property **sets**. `css.vsEnt` (`Custom {name, v}` entries) with `vso`/`vsn` per set id (set 0 empty); `css.svs[style]` is the set a computed style resolves `var()` against (`css.varsOf`/`setVars`). `cssmatch.buildStyle` (cache miss only): the parent style's set; if a matched rule declares `--x`, `ownVars` copies it and applies the declarations (last wins) and resolves `var()` values against the set itself, in any order (passes until stable; what is left is a cycle, invalid, so the fallback applies); then the style's own property `var()`s resolve against it (`declValue(css, prop, v, parent, vs)`, one more argument; `dynattr.build` passes the base style's set). The style cache key is unchanged and still correct: the set is a function of the parent style plus the matched rules, so a different parent set is a different parent style id. A node whose rules declare no custom property shares its parent's set id (no copy), so `:root` themes cost one set. Swapping a theme var on `:root` or a class is the normal restyle (reset + parse + `Dom.restyle`, or a class change); nothing rebuilds.
- Semantics: undefined or invalid `var()` without fallback, or a value of the wrong type for the property, leaves the property unset (inherited or initial, as before); a custom property declared invalid does not fall back to the parent's value; `--a: var(--a)` is invalid. `customNum` (tooltip delay) now reads the newest plain `:root` rule's decl, so it still needs no node.
- Also fixed: `Dom.restyle` marks every element layout-dirty (style ids restart after `css.reset()`, so an unchanged id with new values never relaid out).
- Tests: `css_test` section 5b (fallback, nested, class var inherited by a grandchild, overridden on a child, sibling subtrees, order, cycles, reset), `dom_test` `testVars` (inherit, fallback, class swap transitions, 1000 steady frames 0 restyles/0 layouts/0 arena, 2000 class toggles stop growing `vsEnt`/`nstyles`, `Dom.restyle` hot reload). Regressions: css/dom/layout/input/overlay/motion tests, speculative 14-20, `cargo test --lib ui::` 47, desk-app and idle-breakout goldens exact, big_ui_test pages identical except Layout (page 5, intended) and the time-animated ones (7, 9, 15, 16 vary run to run).
- Migrated: big_ui_test `kit/Section` has `gap: var(--section-gap, 8px)` in its CSS (the `gap` prop from 2a is gone, nothing passed it); Layout sets `.roomy { --section-gap: 20px }` on its first Section through the passed class. desk-app `Details` keeps its wrapper div until `align-self` lands (Stage 15); that was the only open item of this stage's "Done when" apart from `Table`'s `count` (kept, see 2a), so Stage 2 is DONE with those two noted.
- Later chunks: `@media (prefers-color-scheme)` and theme switching can set vars on `:root` or a theme class (one restyle). Limits: no `style:--x`, no `var()` inside `@keyframes` or gradient stops, custom values are single values (no lists or `calc()`), a `style:` text with a `var()` fallback keeps its fallbacks in a range of `css.fbs` owned by its `Dyn` slot (`fbo`/`fbc`, `dynattr.adoptFallbacks`): a new text that fits is moved there and the pool cut back, so animated text keeps `fbs` flat (`dom_test` `testFallbackText`, 10000 frames).

---

## Stage 3: Popups as real DOM nodes (old 10 leftovers)

**Original gap (resolved by 3a–3d).** The select list, color panel and tooltip were painted by `dropdown.j3` / `overlays.j3` from the computed
style of stand-in elements. A theme could not add `box-shadow` or children, and the immediate code fixed the row
height, the check mark and the scrollbar. The real-node implementation and accepted visual differences are recorded below.

**Design.**
- Build the select list, color panel and tooltip as DOM subtrees with the `.j3-*` class names already in `uastyle`,
  attached to the overlay layer. Delete the stand-in path once they match pixel for pixel.
- Themable: `box-shadow`, row height (from `.j3-option` padding and line height), the check mark (`::before` or a
  child), scrollbar, the 8-row limit (`max-height`), `.j3-option.selected`.
- Custom content: an option may hold visual markup (icon plus text); `bind:tooltip={open}` controls a direct-child
  conditional popover containing custom hover content (the final API replaces the proposed `tooltip={..}` syntax).
- Context menu: `on:contextmenu` opens a `popover` at the pointer; hover card: a popover shown after a hover delay.
  Both reuse the menu key handling in `overlays.menuKeys`.
- Menu type-ahead shared with selects, and wrapping of long tooltip lines.

**Tests.** `overlay_test.j3` themed shadow, selected row, custom option content, context menu at the pointer. Goldens
of a themed select and tooltip (`speculative/21_ui_select` and `speculative/22_ui_popups` now hold them).

**Done when.** A theme can give the select list a shadow and a custom check mark with ordinary CSS, and a context
menu opens on right click with keyboard navigation.

**Done (3a): the select list is real nodes.** `ui/popnode.j3` is the reusable part (root `Node.rt` child of the
owner, `Dom.ptRoot`, `popRoot`/`dropRoot`/`buildIn`/`buildOut`/`paintDetached` in `dom.j3`); `ui/dropdown.j3` builds the rows
(`.j3-select-list`, `.j3-option` with `.selected`/`:hover`/`:disabled`, `.j3-check` an `<svg>` drawn in `accent-color`
(new `accent` paint in `media.j3`), `.j3-label`) from `inputs.begin`, and shrank (no select painting, no row math). The
list takes its select's style scope, so a component's own `<style>` themes it, and real ancestors match. `max-height`
exists now (`P_MAX_HEIGHT`, clamps a box's own height after layout; no `max-width`, flex main size ignores it: chunk
15a finishes it); the 8-row limit is the UA rule `max-height: 216px`. Lists over 40 options build only the viewport's
rows between two `j3-pad` spacers (rows opened with `enter(i / 4096)` + `open(i % 4096)`), so 5000 options are 12 rows
of nodes (the press that opens it is ~3 ms in a dev build, almost all of it counting the options; a steady frame is
O(options) only for that count). Default look vs the old painted list (big_ui_test page 3, three states): 20 to 489 pixels
differ, none by more than 1 per channel except 4 px of check-mark antialiasing (the disabled row's text is an opacity
group, not an alpha-90 color; the check is an svg path). Tests: `overlay_test` (214 checks: nodes appear and go, `.selected`,
keys, themed `box-shadow`/`max-height`/hidden check, wheel, 5000 options, in a modal, owner removed, restyle, 1000 cycles
flat, steady 0/0/0), `input_test` ported, `speculative/21_ui_select` (goldens default/themed/multi/long, `tools/golden.sh`).
Notes for 3b/3c/5c: a popup is `popnode.open` + `build`/`done` from the input step, `place` + `paint` from the overlay
step (a window-less test calls `dropdown.settle`); hover/press do not climb into the owner past the root (`setChain`
stops at `rt`); `kbHover` is how the keyboard highlight uses `:hover` (cleared by the pointer moving); the highlight color
no longer follows `accent-color` (`.j3-option:hover` is a plain color); `ptRoot` is the interactive runtime popup
slot (3b added independent passive `ttRoot`); tags `j3-list`/`j3-option`/`j3-label`/`j3-pad` keep `div`/`span` rules out.

**Done (3b), independent review CLEAN: tooltip and color panel are real nodes.** `Dom.ttRoot` is a second runtime
slot for passive title tooltips, using the same `runtimeRoot`/`buildIn`/`buildOut`/`freeSubtree`/`paintDetached`
mechanism as the input popup in `ptRoot`. It paints above the input popup; clearing the title, leaving/pressing,
or removing the owner drops it independently. Titles follow `rawHit` filtered by `allowedByModal`, because select
and menu keyboard highlighting may keep `hover` on another node. A tooltip can therefore coexist with an open
select or color panel without stealing focus or its root. The 500ms delay and 120ms fade remain.
`overlays.tipInput` builds ordinary text; `.j3-tooltip` has `white-space: pre-line` and `overflow-wrap: break-word`.
Auto width shrink-wraps short titles and caps long ones at 320px or viewport minus 8px; an explicit CSS width
wins (general `max-width` remains 15a). Position is clamped after layout by `settleTip`.

`dropdown.buildPanel` builds `.j3-color-panel` with `.j3-color-sv` (flat hue plus saturation/shade gradient children),
`.j3-color-hue`, marker/handle, footer, preview and text children. CSS sets the box and controls; saturation/value
and hue drags read the actual child rectangles, so scoped width/padding/control-height themes work. Dynamic
colors use a stack hex buffer and reusable style slots, with cached RGBs; the RGB shortcut also checks that
the dynamic text slot is still valid, so restyle/CSS reload reapplies fills once. Steady frames do no layout/restyle/allocation.
Marker coordinates resolve after layout from the actual control and marker sizes, so opening/resize has no stale-frame offset.
The input popup's Enter/Escape/Tab/outside close and modal focus behavior remain. `colorpick.paintPanel` and
the tooltip painting loop, and the unused `uastyle.styleOf`/`lookOf`/`paintBox` stand-in helpers, are removed.

Regression coverage: `overlay_test` covers scoped shadows/dimensions, wrapping including long words/newlines,
restyle/resize/title updates, both coexistence cases, modal focus/Escape priority, close/removal, steady 0/0/0
and 1000 reopen cycles with flat arena use. Self-review also covers first-frame/resized marker centering, outside drag/release,
UTF-8 fragment boundaries, modal-filtered/disabled titles, pointer crossing, and 1000 shown-title text updates without arena growth. Independent-review regression checks assert that SV hue,
hue-handle fill and preview survive direct restyle, CSS reset/reload and subsequent live color changes. `input_test` also checks the real color root and removal on Enter.
`speculative/22_ui_popups/tools/golden.sh` captures six headless default/themed/wrapped/coexisting states;
`speculative/21_ui_select` remains the select regression fixture. Default before/after PNGs rendered with the
snapshotted old stdlib differ at 1832 tooltip pixels (1556 by only 1 channel value; 276 beyond 1) and 544 color
pixels (241 by only 1; 303 beyond 1): normal DOM translucent-border compositing and marker/border antialiasing,
with unchanged default geometry. Both were visually inspected; the new PNGs are the fixture baselines.
Validation: release build, 47 Rust UI tests, overlay_test (272 checks), input_test (183 checks), and all six new
exact popup goldens pass. The four checked-in select PNGs have pre-existing channel rounding drift (max 2):
the snapshotted pre-3b stdlib produces the same mismatch counts. Current vs snapshotted select captures are
compared exactly; checked-in images are also checked with `--tol 2`. Select behavior/cost checks all pass.

For later popup work (5c): input popups still use the single `ptRoot` slot; tooltips have their own passive `ttRoot`, not another
interactive popup stack. `popnode.place` uses `ptNat` and is for `ptRoot`; tooltips use `settleTip` and do not
change it. Runtime tags are `j3-panel`, `j3-color`, and `j3-tip`, keeping ordinary div/span/button rules out.

**Done (3c), independent review CLEAN: anchored context popovers, custom hover cards and menu type-ahead.**
`popover="context"` is a conditional direct child of the element handling `on:contextmenu`. It captures only
this frame's request in that owner's subtree (otherwise a fresh programmatic open anchors at the owner's
bottom-left), uses the existing fixed/clamped overlay layout, focuses its first entry and restores prior focus
on removal. A second right press reanchors without racing outside dismissal. Shift+F10 dispatches the same
handler from the focused owner; `mouse` remains viewport coordinates, with keyboard requests at its bottom-left.
Ordinary click and explicit `popover left/top` behavior remain.

`bind:tooltip={card}` writes a bool on an owner wrapper; `{#if card}<div popover>...markup...</div>{/if}` must
be its direct child. This follows existing boolean/place binding and conditional overlay conventions rather
than adding element references or a content-value type. The owner is a wrapper so the custom markup may
contain buttons/fields without nesting controls. It waits `--j3-tooltip-delay`, takes no focus, retains the card
while pointer or keyboard focus is inside, and gives pointer crossing 300ms grace. Escape/outside close requests,
modal filtering and owner removal are honored; Escape while still over the owner blocks immediate reopening.
Context popovers are excluded from the hover binding's card association. A shown custom card suppresses its
owner's ordinary title; passive title tooltips elsewhere remain independent (`ttRoot`). Only one non-context
child popover per hover binding is supported; unrelated child popovers need separate owner wrappers.

Self-review fixed a reproduced shared-wrapper case: sibling selects now keep typing/arrows instead of entering
the menu; an active IME composition also blocks menu navigation. Compiler checks reject a missing or wrongly
nested hover-card popover shape, alongside the existing place checks. Additional regression checks cover fresh
programmatic anchors, oversized UTF-8 input, held-right-button and bare-F10 behavior, and recycled owners.

Menus share the select prefix helper (`dropdown.prefix`/`startsWith`): one-second timeout, ASCII-insensitive
prefix, exact UTF-8 bytes, repeated ASCII-letter cycling, owner/level/close reset, bounded incoming text.
Menu labels use `aria-label` or a visible text descendant (not concatenated fragments); hidden/disabled/other
submenu-level entries are skipped. Textboxes, sliders and selects retain their own typing/arrows. SDL text
input is enabled when focus enters menu content. No generic popup layer or dependency was added.

Validation: release build; 48 Rust UI tests; overlay_test 272 and input_test 183 unchanged; context_test 51
checks; generated `speculative/23_ui_context` headless fixture; six exact `speculative/22_ui_popups` goldens;
`speculative/21_ui_select` behavior/cost and four checked-in select PNGs at tolerance 2 all pass. The existing
select rounding drift remains unchanged. Custom-card steady frames are 0 layouts/0 restyles/0 arena growth.
Three explicit close/reopen warmups allow the shared free-node pool's text capacities to settle; an identical
three-cycle replay and 1000 further cycles both allocate zero bytes. Logs are in `/tmp/orchestrate/023/chunk-3c`.

Review fix1: programmatic context popovers no longer read a new owner's zero geometry during generation.
A one-shot `ctxPending` fallback resolves in the absolute layout pass after owner layout; current pointer or
keyboard requests clear it immediately. First-render and newly mounted conditional owners anchor and clamp
in their first layout, viewport resize reclamps the captured anchor, and later frames/removal do not reuse
stale requests. Reviewer repro and seven new focused runtime assertions pass.

**Done (3d), independent review CLEAN: visual markup inside options.** `Dom.optionVisuals` mirrors only
visible popup rows under `.j3-label`, keeping logical source nodes/handlers in place. Text and visual
`div/span/p/small/h1/h2`, `img`, `svg/path` support classes, source-scoped ancestor selectors, inline styles,
media and conditional/loop updates. The mirror matches against source ancestry; inherited styles come from
the runtime row. Media is borrowed (explicit `optionSource` ownership); inline declarations live in a
mirror-owned, recycled `Dyn`. `refreshOptionVisuals` runs after generated updates and before layout so a
removed visual child cannot leave stale borrowed media/text for a frame. Large lists retain the existing
bounded row window; all source options are never cloned together.

`optLabel` caches all descendant text in the logical option's reusable buffer, keyed by dirty generation;
closed labels, type-ahead and fallback values use it. Explicit values remain authoritative. The compiler
rejects interactive descendants/components/events/bindings/focus/overlay attributes rather than creating
duplicate controls. Handle selection on the select. Supply spaces between label fragments. Source option
styles do not replace popup-row inheritance; style visual children directly (for example `option .name`).
Long lists still require uniform row heights; custom icons must fit a themed `.j3-option` height. Existing
DOM depth/slot limits apply. Mirrors add no generalized cloning or event-forwarding API.

A required cache fix in `dynattr.build` invalidates descendants only when a reused style destination's
inherited values or custom-property contents actually change (fresh destinations have no cached children).
It reuses `Css.out` after cascade completion; `declValue` does not recurse into a cascade. The original
unconditional invalidation grew the arena by 518,614,920 bytes per 1000 custom inline-font reopen cycles;
the final fixture reports exactly zero growth after warmup, with correct changed-parent and reload styles.

Validation: release build; 49 Rust UI tests; option_test 27 checks (SVG, scoped/multifragment labels, live
class/text/value/conditional updates, descendants selecting, keys/type-ahead/disabled, a11y, resize/media,
reload, owner removal, 300 steady frames at 0/0/0, 1000 reopen cycles at zero arena growth and 5000-option
bounded mirrors); dom_test 202, css_test 117, overlay_test 272, input_test 183 and context_test 51. The new
`speculative/21_ui_select` custom state includes SVG and image icons and styled nested text; it was visually
inspected and repeats exactly. Four prior select states match the snapshotted pre-3d runtime exactly and
checked-in baselines at tolerance 2 (the known rounding drift); generated select behavior/cost checks and
all six popup goldens pass. Independent review verified 49 UI tests, option_test's 27 checks, a focused
33-check probe and the exact custom golden, with visual inspection. Both consecutive 1000-reopen replays
allocated zero bytes; no must-fix or should-fix findings remained. Snapshot audit accounted for 12 edited
existing files and three new files, with no uncopied sources. Logs/snapshots: `/tmp/orchestrate/023/chunk-3d`
(temporary; the durable restart notes are in `023-handoff.md`). Stage 3 and chunks 3a–3d are complete.

---

## Stage 4: Dynamic components

**Why.** A chat has text, image and embed messages; the component is chosen by data. J3 has no traits, so a
runtime choice needs a closed set known at compile time.

**Design.**
```
<component this={msg.kind} of={TextMsg, ImageMsg, Embed} msg={msg}/>
```
- `of` lists the components. `this` is an enum value or an int index into that list (an enum named after the set is
  generated: `kind: Of_TextMsg_ImageMsg_Embed`).
- Lower to a `{#match}` over `this` with one arm per component, each arm a normal use site with the same props.
  Props must type-check against every listed component (a prop one lacks is a compile error naming it).
- State is per arm and dropped on a switch, like `{#if}`. Inside `{#each}` the key covers the arm.
- Slots and `class` passthrough (stage 2) work as for a normal use site.

**Implementation notes.** Parser: one new tag. `gen.rs`: expand to the existing match lowering, no new runtime.

**Tests.** `ui/tests.rs` for the expansion, the missing-prop error and a switch at runtime (state dropped).

**Done when.** A list of mixed message kinds renders each with its own component from one line of markup.

**Done (4), independent review CLEAN: dynamic components.**

Independent Sonnet review (second run) was CLEAN: 55 UI tests, `speculative/24_ui_dynamic` PASS. Checks below are the implementer's
pre-self-review report. Resume with self-review and greater-tier review before marking complete.

- `<component this={msg.kind} of={TextMsg, ImageMsg, Embed} msg={msg}/>` expands into ordinary match/use-site
  branches before containment, provide/inject, slot-site and class/style scans. All candidates validate shared props,
  events, children and passthrough, including candidates never selected at runtime. Dynamic uses can fill named slots.
- The ordered list generates one public `Of_TextMsg_ImageMsg_Embed` enum with `.TextMsg`, `.ImageMsg`, `.Embed`
  variants. Dotted component paths use underscores. The exact enum or `int` is accepted; bool, float, string and
  unrelated enums are rejected at the source tag. Negative/out-of-range indices render nothing, destroying the old arm.
  Duplicate/unknown/noncomponent candidates, malformed selectors and reserved attributes are generation errors;
  `Of_` component-name prefixes are reserved to avoid generated type collisions.
- J3 has no ordinary function overloads, so the proposed enum/int helper overloads cannot compile. Instead the narrow
  compiler operation `@uiChoice(SetEnum, selector, armIndex)` checks exactly those types and a constant in-range arm,
  then emits existing equality IR. No unchecked cast, general overloading, second parser or runtime component system.
  The match scrutinee is evaluated once; branch lifecycle, local-state destruction and keyed moves use existing code.
- `j3c doc j3ui`, completion and generated TextMate grammars describe/highlight the tag. Rust UI checks: **55 passed**.
  `speculative/24_ui_dynamic` checks mixed messages, enum/int switches, invalid indices, fresh state, keyed moves,
  slots, passthrough and single selector evaluation. Headless speculative 14–20 regressions pass. Release build passes.
- Later stages: generated enum names depend on candidate order and normalized component paths; renaming/reordering a set
  changes its public enum. Ambiguous normalized set names are rejected. Ordinary match semantics remain unchanged.

---

## Stage 5: Rich text input

**Why.** A chat box needs undo, mentions and emoji completion; today a text box is plain text with no undo.

**Design.**
- Undo/redo for `<input>` and `<textarea>` (Ctrl+Z, Ctrl+Shift+Z / Ctrl+Y): a bounded stack of (text, caret, anchor)
  in `Edit`; typing in one run merges into one step, a paste, cut or delete boundary starts a new one. `set` from
  a bound value that differs clears nothing but pushes no step.
- Inline spans in an editable box: `<div contenteditable>` is too big. Smaller: an `rich` textarea whose value is
  text plus a span list `(start, len, kind, data)`. Kinds: bold, italic, link, and atomic tokens (mention, custom
  emoji). Atomic means the caret skips it, Backspace deletes it whole, selection covers it whole. Layout reuses
  `area.j3` wrapped lines with per-span font and color; a token is one box of its measured width.
- Caret popup: `popup-anchor="caret"` places a popover (stage 3) at the caret rect, for emoji or mention
  autocomplete. Up/Down/Enter go to the popup while open; typing keeps going to the box. Reports the trigger text
  (`:smi`, `@ja`) through `on:trigger`.

**Tests.** `input_test.j3`: undo merges and boundaries, redo cleared by a new edit, token atomic delete, span
shift on insert before it, popup anchored at the caret after a soft wrap.

**Done when.** A chat box undoes typing, shows `@name` as an atomic chip, and opens a completion list at the caret.

**Done (5b): rich textarea spans.**
- `<textarea rich bind:value={s} bind:spans={sp}>` (`sp: []Span`, `use j3.ui.spans.Span`; `bind:spans` needs `rich`, both are
  textarea-only). `Span { start, len, kind, data }` counts bytes like `Edit.cur`; kinds bold, italic, link, mention, emoji (the last two are
  atomic tokens, `isToken`). The spans live in `Edit.sp` (empty for every other box, so no allocation or cost) and in each `Undo` entry; the
  byte budget counts 32 bytes a span. `spanEdit(l, at, del, ins, inherit)` is the one maintenance function (shift, grow inside, trim, drop
  tokens a delete touches, drop emptied spans); typing at the end of a bold or italic span extends it, never a link or a token.
- Atomic: `Edit.snap` moves a byte out of a token; `moveTo` (nearest edge, or outward from the anchor while extending), `left`/`right`,
  `deleteBack`/`deleteFwd`, `dropSel`, `selectWord`/`selectLine` use it, and `area.layout` never breaks a line inside a token. Layout, hit
  testing, selection and caret use `j3.ui.rich` (new) when spans exist: segments measured bold, a token is one chip box. Italic has no
  slanted font and is drawn dimmed; chip and link use `accent-color` (no per-class CSS yet).
- Ctrl+B / Ctrl+I (`keys.b`, `keys.i`) call `Edit.toggle`. Copy, cut and paste are plain text. `Edit.set` (a program's new text) drops spans;
  `bind:spans` is emitted after every other attribute, so order does not matter. The box hands the program a view of its spans
  (`Edit.spans`); giving that view back changes nothing, so an echoing program never reapplies stale spans. Any other slice is copied.
  Span data strings live in a per-box ping-pong byte buffer compacted when full (spans and undo steps rewritten), so recycling a box
  allocates nothing after warm-up. Span types and the pure span functions are in `j3.ui.spans` (`use j3.ui.spans.Span;`).
- For 5c: `Edit.replaceWithToken` / `Dom.insertToken(id, lo, hi, label, kind, data)` swap a range for a token (one undo step; `changed` fires
  this frame if the box is not updated yet and next frame regardless, `Edit.late`); `Dom.caretRect(id)` (window px, from the last paint),
  `Dom.caret(id)`. `speculative/25_ui_rich` has a golden. Rust UI tests: `rich_textarea_binds_spans`.

**Done (5c): caret popup and `on:trigger`.**
- Markup: the box carries `on:trigger={..}` (and optionally `triggers="@:"`, a literal; default `@` and `:`); the list is a conditional
  `<div popover="caret">` in the same wrapper as the box (like `popover="context"` it is a conditional child of the element that owns it).
  The handler sees `trigger` (`@ja`, empty when ended), `tlo`, `thi` (byte range, for `d.insertToken`). `d` is in scope in handlers, and
  `d.trigNode` is the box while a trigger is active, so an entry's `on:click` can call `d.insertToken(d.trigNode, tlo, thi, label, kind, id)`
  with the range saved by the trigger handler. Rust: `triggers` needs `on:trigger` and a literal; `on:trigger` is accepted on input/textarea only.
- Detection is `inputs.trigger` (end of `inputs.begin`, after `textKeys`/`textMouse`): the focused box with `trigs`, no selection, no IME;
  scan back at most 48 bytes from the caret for a trigger char at the start or after whitespace, none of the bytes inside a token. State is
  on `Dom` (`trigNode/Lo/Hi`, `trigEv/trigChg`: the event of the frame, an end included). Fires when the box, range or text changed. Ends when
  the caret leaves, the box loses focus, a selection exists, or Escape. Only one box has a trigger at a time: on a focus move the old box gets its
  ended event first and the new box's trigger starts the next frame (one event slot).
- Keys: `overlays.caretKeys` runs before `textKeys` while an open `ovCaret` popover belongs to the focused box (the wrapper contains it).
  Up/Down move the active entry among the popover's focusable descendants (wrapping; shown through `kbHover`, so `:hover` styles it; the
  first entry is active on open; a moving mouse takes over), Enter/Tab set `clickNode` to it (put the work in `on:click`) and are consumed;
  this is intended (as in chat apps): while the list has entries a textarea cannot take a newline; Escape closes it, then Enter inserts one;
  nothing else is. Entries need to be focusable (`button` or `menuitem`). Escape (existing `overlays.input`, first) closes it via `on:close`, ends
  the trigger and sets `trigSup`: the same trigger (start offset) stays quiet until it ends. A press on an entry keeps focus in the box
  (`dom.inCaretPop` skips `focusNode`); the click still lands.
- Placement: `Dom.attrCaret` = `attrOverlay` + `left`/`top` from `caretRect` of the box (the active-trigger box, else the focused one, inside
  the wrapper) each build, so it follows wraps and scroll with no layout when nothing moves (`attrLeft/Top` only dirty on change). Flip above
  is in `layoutOneAbs` (`ovCaret`, `cth` = caret height + 4); window clamp is the existing overlay clamp. The caret rect is the previous
  `Dom.caretRect` is now computed from the box's layout and current text/caret (textarea: `area.layout` + `follow` + `wid`; one-line input:
  `field.scroll` + `textWidth`), not the last paint, so the popup is at the caret on its first frame and does not trail a keystroke. A
  password box has no rect (the popup hangs under the wrapper). The fixture checks it equals what the paint drew.
- Review fixes: `dropOverlay` no longer gives focus back for a caret popup (it restores the focus a popup took; with `focusNode(-1)` it
  re-focused the box the frame the popup closed, so a blurred box snapped back: pre-existing overlay behavior, wrong for popups that never took
  focus). Up/Down are consumed only while the popup has an entry (tested with none).
- Not done: aria roles for the list (use `role`/`aria-label` on the popover and entries as for any popover; no `aria-activedescendant`
  link), custom trigger-word charset, `triggers` as an expression, several boxes' triggers at once.
- Tests: `input_test.j3` `testCaret` (+39 checks: start/after space/mid-word, ends on space/caret/focus/Esc, Up/Down/Enter keep the caret,
  Enter inserts a chip and undo restores, click keeps focus, placement after a soft wrap, flip above, idle 0 layouts/restyles);
  Rust `caret_popup_and_trigger`; `speculative/25_ui_rich` types `@ja`, opens the list, Down/Up, Enter inserts `@Jane` (new `golden/popup.png`).

---

## Stage 6: Data grid

**Why.** A spreadsheet needs more than a table. Built on stage 1 (virtual rows and columns).

**Design.**
- Keyboard cell navigation: arrows, Tab/Shift+Tab, Home/End, Ctrl+arrows to the data edge, PageUp/PageDown.
- In-place edit: Enter or typing starts an editor (a text box over the cell, popup editors from stage 3 for
  selects), Enter commits and moves down, Escape cancels.
- Range selection: shift+arrows and drag; copy as TSV, paste TSV into the range (quote cells with tab or newline).
- Frozen header row and first column (`position: sticky` on both axes inside the virtual scroll box).
- Column resize by dragging the header edge; widths shared by head and rows (this is the column-track half of old
  stage 18: use `grid-template-columns` shared through `display: contents` rows, or `<table>`; pick one).

**Tests.** `grid_test.j3`: navigation, edit commit and cancel, TSV round trip with quotes, resize updates body
columns, frozen row stays while scrolling, 100k x 50 cells at 0 steady layouts.

**Done when.** A 100k-row sheet scrolls, edits a cell, copies a range into another app and pastes it back.

**Done (6a): grid cell navigation and in-place edit.**
- Surface: no new element. A `virtual row-height={h}` scroll box (fixed height only) with `tabindex="0"`, `grid={g.sized(rows.len)}` (a `&Grid`, `j3.ui.gridstate`; needs `virtual`) and `on:edit={..}` (`kind` 0 begin / 1 commit, `row`, `col`, `text`; needs `grid`). The app owns the cell data; the rows are ordinary nested markup (`{#each 0..g.cols as j}` inside the keyed row each). `Dom.gridBind/gridOf/isGrid/gridEdit`, `Dom.grids`; several grids per window (`Dom.grids`: `GridRef{node, g, frame}`; events and all state live in each `Grid`: `evFrame evKind evRow evCol evText`).
- Model (`ui/gridstate.j3`, pure): `Grid { rows cols rowH widths total row col editing draft seed init sx viewH viewW reveal hasFilled filled }`. `widths` is the single column source: header and rows read `g.w(j)` / `g.total` through `style:width` (inline widths, so no `grid-template-columns` limit and no `display: contents`; 6c adds drag by `setWidth`). Moves: `goto move tab page jump homeEnd colX colAt rowAt`. Ctrl+arrow: edge when `hasFilled` is false, else spreadsheet runs over `filled(row, col)` (app callback `fn(int, int): bool`, a module function; keep its data in a module `let`).
- Runtime (`ui/grid.j3`): `grid.step(d)` is the first call of `inputs.begin`, so keys it takes (arrows, Tab, Enter, F2, Escape, typed text; `KeyIn.f2` added) never reach the text box or the Tab walk. The box itself holds focus (roving active cell = `g.row/col`, `class:active={g.on(i, j)}`); a press picks the cell from mouse and scroll; `showCell` scrolls both axes (`Dom.scrollBy/scrollByX`) before `update`, so the virtual window follows in the same frame. Typing needs `game.textInput(true)` when the grid box is focused (`inputs.seeFocus`).
- Editor: Enter/F2 or a typed character sets `g.editing` and fires begin; the markup shows `{#if g.editAt(i, j)}<input type="text" bind:value={g.draft} />`. The input appears in that update; the next `step` finds it (the only `<input>` in the box) and focuses it (so one frame of latency), caret at the end; a typed character replaces the text (`g.initial(text)` returns the seed, else a copy of `text`). Enter commits and moves down (Shift up), Tab/Shift+Tab commit and move, Escape cancels, losing focus (a click elsewhere) commits, the editor row leaving the window does not end the edit (the focused row is kept alive by `virtual`; hidden, still typing); if the editor box is freed anyway (the list shrank) the text of a frame ago (`Grid.last`) is committed and focus returns to the box. `Dom.poolEd`: the editor's `Edit` is `reset()` in place when freed (other boxes keep the replace-on-free rule because bound strings may view the old buffer), so 1000 begin/commit and 1000 begin/cancel cycles allocate nothing. The box is created per edit, not pooled. Select/popup editors are not built (deferred to 6c/14; column kind needs a cell anchor for `popnode`).
- Horizontal virtualization was not needed: 100000 x 50 keeps about 1400 live nodes (13 rows x 50 cells + header); a one-row move lays out 7 boxes, 5 moves 35. Steady (idle and while editing): 0 layouts, 0 restyles, 0 bytes. Rows with many more columns would need a second window on `scrollX`.
- Footgun fixed: a `virtual` box that is not `border-box` measured its content height plus the spacer padding (millions of px), so the window was every row. `vlist.viewHeight` (used by `step` and the grid) is the box height without borders, and without padding when `content-box`; no clamp. Tests: `grid_test` (content-box 100k rows stays bounded; a 600 px box taller than the window inside a scrolling parent builds its whole height).
- Pitfalls found: a `string` interpolated inside a function and returned is garbage after the return (return a view of a `Text`); the grid box needs `box-sizing: border-box` (the virtual spacers are padding); `else if` inside an if-expression does not parse.
- Tests: `stdlib/tests/grid_test.j3` (48 checks: model, Ctrl+arrow with and without data, keys, edits, click, steady, 200 edit cycles flat), `speculative/26_ui_grid` (100000 x 50 with `tools/golden.sh`, `golden/sheet.png`), `ui::tests::grid_attribute_binds_the_grid_and_edit_events`.
- For 6b/6c: selection anchor/range belong in `Grid` (shift+arrows are plain moves today: intercept in `grid.step` before `g.move`); the press handler there is the place for drag; copy/paste hook the `k.c/k.v` keys while `f == b`. Frozen header: the header is already outside the box and follows `g.sx`; frozen first column and resize call `g.setWidth` and need `colAt` to stay the hit test.


**Done (6b): range selection, TSV copy and paste.**
- `Grid` keeps `ar/ac` (anchor) beside `row/col` (the active cell, the moving corner). `goto` collapses the anchor unless `g.ext`; `grid.step` sets `ext` for Shift+arrow, Shift+Ctrl+arrow (`jump`), Shift+Home/End and Shift+PageUp/Down, and for Shift+click; a press sets `g.drag`, held-mouse frames extend to the pointer's cell (outside the box the row is outside the window, `reveal` scrolls there, so speed follows distance). Ctrl+A is `selectAll` (active cell moves to the last cell). Query: `hasRange r0 r1 c0 c1 inRange(i, j)`; the app styles with `class:sel={g.inRange(i, j)}` (a class bool, steady cost 0, tested). `Grid.lr/lc` remember where the active cell was last set, so a direct write to `g.row/g.col` collapses the range at the next `goto`/`sized` (use `goto`); `sized` clamps both corners, so shrinking the list keeps a valid range.
- TSV is pure in `ui/tsv.j3` (`field`, `parse` into flat `data/ends/rows`, `cell`, `rowLen`, `maxCols`): quoted fields with tab, newline, `""`, CRLF/LF/CR, trailing newline ignored, ragged rows kept, an unclosed quote runs to the end.
- Copy: the grid cannot read cells, so the app gives `g.cells(fn(int, int): string)` (module fn, like `filled`); `copyText()` builds the TSV (no trailing newline) and `grid.step` calls `game.setClipboard`. Ctrl+X = copy, then a clear paste.
- Paste: ONE `on:edit` event of kind 2 (`EDIT_PASTE`), `row/col` = top left, `g.pasteRows x g.pasteCols` clipped to the grid, `g.pasteCell(i, j)`; one field over a multi-cell range fills it (Excel). A clear is the same event with an empty fill. The pasted block becomes the selection. `grid.paste(d, g, tsv)` (in a frame) and `g.queuePaste(tsv)` (anywhere; fires next step) feed it, since headless `game.clipboard()` is always empty.
- Bounds: copy and paste 4 MiB (`MAX_BYTES`; the field that hits the cap is cut at a UTF-8 boundary and ends the paste, whole rows past the cap are not copied) and 200000 parsed cells (`MAX_CELLS`); the staging buffers are reused, so repeated pastes do not grow the arena beyond the largest.
- Tests: `grid_test` 95 checks (range keys, collapse, drag, auto-scroll, shift-click, Ctrl+A, TSV cases, copy round trip, clipping, fill, caps, one event per paste, idle with a range 0 cost); `speculative/26_ui_grid` range, copy text and paste checks, golden now shows a selected range. The real system clipboard works under offscreen SDL once `game.init` ran (`grid_test` does it): Ctrl+C then Ctrl+V on the keys round-trips (grid_test now 113 checks). `pasteCell` is O(1) (field end offsets); 1000 x 100 pasted and read back in grid_test. Ctrl+A/C/X/V with the editor open go to the editor (tested).
- For 6c: `colAt/colX` stay the hit test (drag uses them); a frozen first column must keep `inRange`/`on` classes working; resize must not start a cell drag (the press handler in `grid.one` picks cells from anywhere in the box, so a header-edge drag outside the box is safe; a resize handle inside the box needs a guard before `g.drag = true`).

**Done (6c): frozen header row and first column(s), column resize.**
- Frozen header row: it sits outside the box and follows `g.sx` by `margin-left`; `g.sx` is written in `grid.step` before the build, so it tracks the scroll in the same frame (tested: header and body cells at the same x).
- Frozen columns: `g.freeze(n)` (or `g.frozen`). No engine change: the first `n` cells (header and body) are `position: absolute; top: 0; z-index: 2` with a background, inside a `position: relative` row and header (`class:frozen={g.fz(j)}`), and the row and header row carry `style:padding-left={g.fw()}` so the other cells start where they would unfrozen. Body cells use `style:left={g.bx(j)}` (`sx + colX(j)`, so they sit at the box edge while the content scrolls under; 0 for non-frozen), header cells `style:left={g.hx(j)}` (the header row is only shifted by `margin-left`, an absolute cell is placed against `.head`). Cost: a horizontal scroll step restyles and lays out the visible rows' frozen cells (about 130 boxes of 1470), never the 100000 rows; idle 0. `g.colAtView(x)` is the hit test (frozen columns where drawn, the rest shifted by the scroll; used by press and drag in `grid.one`); `showCell` leaves frozen columns alone, scrolls a column clear of them (its left bound is `scrollX + g.fw()`), and moving onto a frozen column leaves the scroll alone (it is always visible; Home/Tab-wrap do not scroll back either). `class:sel`/`class:active` work on frozen cells (CSS: `.cell.frozen.sel/.active` set an opaque background).
- General `position: sticky` stays a CSS gap (stage 15): `j3.ui.sticky` handles `top` only, as a paint offset (`Node.sy`); `left` would add `Node.sx` in `absX`, `hitNode` and the paint translate, but the header lives in a clip box whose `scrollX` is 0 (it follows by `margin-left`), so the header would still need the absolute trick; it was not worth it.
- Resize: a thin absolute child at each header cell's right edge (`right: -4px; width: 8px; z-index: 3`) with `on:mousedown={g.grab(j)}`. `grid.step` (`resize`) measures from the press x (`g.px`, the latest press, since the handler runs a frame after the press), sets the width with `setWidth` while the button is down (header and rows read `g.w(j)`/`g.total`, so both update the next build), clamps at `MIN_W` = 24 px, sets `Cursor.ResizeAcross` during the drag (no hover cursor: the runtime has no CSS `cursor`), and on release fires `on:edit` kind 3 (`EDIT_RESIZE`, `col` = the column, `row` = the active row) only if the width changed. A press on the header blurs the box (nothing focusable there); `g.keep` remembers the box had focus before the press and the drag gives it back; the selection is never touched (the header is outside the box, so no cell drag starts). Double click to auto-fit: not built (needs the app's text widths).
- Breaking for 6a/6b apps: the `on:edit` handler's `else` branch used to be the paste; write `else if kind == 2` (kind 3 now exists).
- Tests: `grid_test` 146 checks (frozen cell and header at the edge while scrolled, same-frame header, hit test, scroll cost, resize drag with `widths`/`total`/body and header cell widths/`colAt`, minimum, selection and focus kept, one event, idle 0 layouts/restyles/arena); `speculative/26_ui_grid` (same; goldens `sheet.png` scrolled under the frozen column and `resized.png`, the frozen column beside the resized column 2, via `GRID_PIC=resized`). Also tested: fast click, release outside the window, drag past the minimum and back, press right after focus arrived, resize with an editor open (commits first), resizing the frozen column (same frame), two frozen columns, drag-select across the frozen boundary.
- For 6d: ordinary tables could share column tracks the same way (`style:width` from one `[]f32`); the frozen/resize markup above is the pattern. For 13 (a11y): the grip is a bare div, give it a role and a keyboard resize if wanted. For 15: `position: sticky` `left`, and a CSS `cursor` property.


---

## Stage 7: Markup ergonomics (old 14)

Gripes 7a 5, 6 and 7b 1, 4.
- `{#let f = state.files[id]}` inside any block: a local evaluated once per update (and a `&T` loop variable for
  `{#each}` over `[]T`, so the body can mutate).
- `emit` inside a script `fn` (rewrite it like a handler; the fn runs where `ev` is in scope, or take `ev` as a
  hidden argument).
- `{#each xs as x [pos]}`: say "positional is fine" on a struct list, or a typed int-key fast path in `keyed.j3`
  (shared with stage 1), so a pool keyed by slot index stops formatting keys.

**Done when.** desk-app's row markup reads `f.name` and `f.sizeText`, and `RenameModal` calls one `fn ok()` from
both places.

**Done (7a): `{#let}` and `{#each xs as &x}`.**
- `{#let name = expr}` is a standalone tag (no `{/let}`, like Svelte's `{@const}`): `markup.rs` `LetNode`, `Node::Let`. `gen.rs`
  `let_node` emits `let ltN = expr;` where it stands and pushes a `rename` (user name to `ltN`), so sibling blocks may reuse a
  name and J3's no-shadowing rule never sees two. `seq` truncates `lets`/`rename` on exit, which is the scope: the block's later
  siblings and their descendants. A duplicate name errors at the markup; a name that shadows a prop/let/fn/loop variable gets the
  usual `check_local` error; use outside the scope is J3's "unknown name" mapped back to the markup position. `slot_method`
  recomputes the lets in scope at the use (the slot content is another fn). Keyed-each key code now truncates `rename` instead of
  clearing it, so an outer let survives it. `single_root` ignores lets.
- `as &x` (`EachHead.by_ref`): the item is bound as `&xN[i]` (also in `virtual` lists and slot rebinds); a range loop with `&` is
  an error. The key expression still sees a copy.
- Pre-existing, not changed: two sibling `{#each}` blocks in one component cannot reuse the loop variable name when one is keyed (J3
  "shadowing" error); the docs already say to give each its own name.
- LSP block completion and the TextMate grammar know `let`. desk-app `Browser.j3ui` rows read `f.name` etc. via `{#let f = &state.files[id]}`.
- Review round: `speculative/27_ui_let` also covers a let in slot content that changes, a `{#match}` arm, `&x` in a `virtual` list
  (click mutates the source) and a 38-frame steady check (0 layouts, 0 restyles, 0 arena bytes). A `(key)` sees lets declared before
  the each, not ones in its body (test). `&x` + push/remove on the same list in one update is a documented rule, not guarded. A let
  named like generated code (`c2`, ...) is rejected by the usual reserved-name check.
- Tests: `ui/tests.rs` (`let_blocks_scope_to_their_block`, `let_errors_point_at_the_markup`, `each_by_reference_binds_a_pointer`,
  `speculative_27_let_and_ref_check`); `speculative/27_ui_let` runs headless (lets at top/each/if, `&x` handler mutates the source array).

**Done (7b): `emit` in a script `fn`.**
- Approach: not a hidden `ev` argument. A fn's `emit x(a)` is written `this.sev.x_a = a; this.sev.x = true`, the same `sev` field slot content already used, which `update` merges into the returned events at its end (call sites unchanged, nothing threaded). gen.rs: `Rw.sev`, `Names.emitting` (fns that emit directly or by calling one that does; token scan to a fixpoint before any code is written). `bridge` (the `sev` field and merge) is now on when any fn emits, as well as for slot owners; the `onMount` call moved before the merge so its emits count.
- Errors (with .j3ui spans): an emitting fn called from a markup expression (would emit on every update), `on:draw` or `onDestroy`, and `emit` written in `onDestroy`. Handlers, other fns, `onFrame`, `onMount` are fine. The scan counts only `name(` calls (not `x.name`, `m::name`, `name:` keys); locals can't shadow a fn (existing rule), recursion terminates. desk-app selftest now clicks the dialog's Rename button as well as Enter.
- Unchanged limit: two emits of one event in a frame, the last payload wins (a fn calling `send` twice sends once).
- `RenameModal` has one `fn ok()` for button and `on:submit`. Tests: `emit_in_script_fns`, `speculative_28_emit_fn_check`; `speculative/28_ui_emit_fn` runs headless (onMount, int/string/bool payloads, transitive call, two handlers). LSP needed nothing (`emit` is only a completion).


**Done (7c): int keys, and "positional is fine".**
- Syntax: `{#each pool as p (p.slot : int)}` (the key expression, then `: int`). gen.rs has no types (it writes J3 text), so the hint is explicit and type-annotation shaped; `parse_each_head` strips it (`EachHead.key_int`). Lowering: `kd.addInt(expr)` instead of `` kd.add(`{expr}`) ``; virtual lists use `all.addInt` / `all.pushInt` / `pk.addInt` (`virt_key` takes the flag). `keep` with an int key is a compile error (keep lists text keys). Text keys unchanged.
- `keyed.j3`: `addInt` / `pushInt` store the int as a sign byte plus its magnitude, low byte first, minimal length (2 to 9 bytes; `%` and `/`, no bit ops; `encode` / `decode` are pub), so hashing, `eqCN`, prefix fast path, `sameAs`, `finish` are untouched and shared; `begin` clears `ints` so the duplicate warning decodes and prints the number. Mixed kinds cannot happen (one list = one lowering).
- Measured first: a templated int key already allocated nothing per frame (the template buffer is reused; idle-frame arena growth was 0 with `(it.id)` in `speculative/16`), so it was CPU only. The first 8-byte encoding was slower than text (2.7 vs 2.2 ms dev); with the variable-length one, 10k shuffle `Keyed` release build (`j3c build --release`, best of 3): text `k{i}` 767 us, int with wide/negative values 796 us, plain ids 0..10k 448 us. Dev build: 2.0 / 2.0 / 1.5 ms. Whole-frame 10k shuffle in `speculative/16` is 47 ms either way (the DOM dominates).
- Positional: `{#each xs as x}` over a struct list is correct and cheap when rows only grow or shrink at the end; documented in `j3c doc j3ui` beside keyed each with when to key.
- Tests: `keyed_test` `intKeys` (steady 0 alloc, reverse, insert/remove, empty, bytes that differ only in high bytes, extremes and negatives, duplicates by position, 10k shuffle 2.7 ms, `pushInt`); `ui::tests::int_keys_skip_formatting` (plain, virtual, `keep` error); `speculative/18_ui_virtual` is int-keyed (29 PASS, incl. in-place reorder, scroll away and back; scroll anchoring follows the key, so the view moves after a reorder). The `: int` hint with and without spaces is tested in `ui::tests` and through the LSP (`int_key_hint_is_not_a_diagnostic`); `: integer` is not the hint. `speculative/16_ui_lists` main `rows` each now uses `(it.id : int)` so its reverse/filter/insert/duplicates/1000-row/idle-allocates-nothing/10k-shuffle checks run on the int path (42 PASS).

---

## Stage 8: Test kit and steady counter (old 15)

Gripes 7a 7 and 7b 6.
- `J3_STEADY=a:b` read by `j3.app` (or `root.render`'s caller): print `steady layouts= restyles= arena_growth=`
  between frames a and b and exit 1 when any is non-zero. Delete `DESK_STEADY` and the temporary edits of 7a.
- `j3.ui.testkit` (stdlib, with a test): `clickText(d, frame, f0, text)`, `clickTag`, `typeInto(d, ...)`, `hasText`,
  and `simKeys` accepting several names per frame. desk-app's `selftest.j3` and `speculative/14` to `17` use it.
- `tools/golden.sh` as `j3c golden <dir>` (states from a `golden.toml`) only if a third project copies the script.

**Done (8a): `J3_STEADY=a:b`.**
- `App.run` (`j3/app.j3`) parses the variable once before the loop (malformed prints a warning and is ignored). After the draw systems of frame a it snapshots the counters and `App.mem.used`; after frame b it prints `steady layouts=N restyles=N arena_growth=N`, stores rc 1 when any is non-zero, and calls `requestQuit`. The normal teardown (`game.destroy`, arenas) runs, then `exit(rc)` after the loop. This is the seam for 9a's `App.shutdown`: run it between the loop and `game.destroy`, and the steady rc survives. A run that ends before frame b (`--frames` too small, window closed) prints `steady: run ended at frame N before b` and exits 1.
- Counting is process-wide, so several roots sum: `Dom` calls `memory.countLaid` / `countRestyled` next to its own `d.laid` / `d.restyled`. They live in `j3.memory` ("work counters") because `j3.app` cannot import `j3.ui` (it would link the UI into every app) and both already import `j3.memory`. A singleton `let` cannot be `pub`, hence the accessors; a whole-program release build inlines them (checked with a 300M-iteration loop that folded away), a dev build pays one call per box. `grid_test` still passes (146). Only `App` loops check the variable; a hand-rolled loop does not.
- desk-app: `DESK_STEADY` and its World fields removed; `tools/golden.sh` runs `J3_STEADY=60:120` and fails on the exit code. `j3c doc testing` and the root `AGENTS.md` row updated.
- `ir_goldens_01_through_08_and_samples` fails on `07_memory.ir` before and after the `memory.j3` edit with a byte-identical dump (compared against the snapshot stdlib): the new functions add no mismatch, so whoever regenerates that golden need not account for 8a.
- Test: new `speculative/29_ui_steady` (shell-run; no cargo test, since the existing tests do not run headless apps): idle `J3_STEADY=30:60 j3c run speculative/29_ui_steady --headless --frames 80` prints all zeros, rc 0; `STEADY_CHURN=1` prints `layouts=90`, rc 1; `--frames 40` prints the ended-early line, rc 1.

**Done (8c): `j3c golden <dir>`.**
- New `j3c/src/golden.rs` (wired in `cli.rs`, uses `imgdiff::diff`, a pure function split out of `imgdiff::cmd`). `<dir>/golden.toml`: top-level defaults, `[state.NAME]` (`frame`, `tol`, `max`, `frames`, `timeout`, `[state.NAME.env]`, optional `text` file compared with `golden/<name>.txt`) and `[check.NAME]` (a run with no capture; PASS at rc 0 and no `FAIL` line). Reuses the `j3.toml` reader (`project::toml_doc`, which now also returns section headers and line numbers) with a `[defaults]` / `[state.NAME]` / `[check.NAME]` (+ `.env`) layout instead of a second parser; unknown keys, bad numbers, repeated sections and duplicate names are `golden.toml:LINE:` errors, rc 2. Each run is its own process group, killed at `timeout` (FAIL). Each run is a child `j3c run . --headless [--frames N]` with fixed dt; `--bless` (named or all states) is the only way goldens change; names run one state.
- All six projects converted; each `tools/golden.sh` is a one-line wrapper. desk-app's a11y goldens renamed `<name>.a11y.txt` to `<name>.txt`; its `steady` and `DESK_TEST` runs are checks; 21's `selects` self-test too.
- Findings: the old 22 script was `set -eu` and stopped at `tooltip`; `j3c golden` reports all six as FAIL (2764, 34961, 16460, 1570, 16355, 31816 pixels), unchanged environmental drift, not re-blessed. 21: all five states (including `custom`) drift by at most 2 per channel, so the file sets `tol = 2` for all (the old exact script failed `custom`, 2538 pixels).
- Tests: `golden::tests` (parser, pass/fail decisions). Not cargo-tested: the runs themselves (they need GL).

**Done (8b): `j3.ui.testkit`.**
- New `j3/ui/testkit.j3` (embedded; `embedded_files_match_the_tree` needs only the `FILES` entry). Frame-driven: each call takes the caller's frame `f` and acts only on its own frames. `clickText(d, f, f0, text)`, `clickTextIn(.., within, text)` (a `<dialog>`), `clickTag(.., tag, k)`, `click(.., id)`, `typeInto(d, f, f0, tag, k, text)` (click `f0..f0+2`, type on `f0+3`), `simKeys(d, f, at, typed, names)`, `hasText`, `countText`, `textNode`, `textNodeIn`, `tagNode`. Several keys per frame needed no Dom change: `Dom.simKeys` already takes space-separated names (`"C+a Backspace"`); the new part is the frame gate. Key names stay `C+`/`S+` (no `ctrl+` alias).
- A missing, `display: none`, size-0 or off-screen target prints `testkit: frame N: no text "X"`, sends no input and bumps `testkit.misses()` once (the same failure on the next frame is skipped; `resetMisses()` for suites sharing a program). Text matching is a whole text node, first in tree order, never `display: none` or fading out; text split over nodes is not matched. Scroll-container clipping is not detected. desk-app adds `misses()` to its failures. Before, `click(-1)` clicked at garbage coordinates.
- Migrated: desk-app `selftest.j3` (its `textNode`/`dialogText`/`click`/`clickText` and the two input loops are gone) and `speculative/14`, `17` (their `button`/`hasText`/`click` copies). `15` and `16` had no duplicated input helpers (no clicks), so they are unchanged. PASS counts: 14 = 14, 15 = 12, 16 = 42, 17 = 6, desk-app DESK_TEST = 18, all rc 0 before and after. Test: `j3c/stdlib/tests/testkit_test.j3` (18 checks).
- `j3c golden`: six projects now carry a `tools/golden.sh` (desk-app, idle-breakout, speculative/21, 22, 25, 26), so the bullet's condition holds; left for a later chunk.

---

## Stage 9: Frame-scratch text and shutdown hook (old 16)

Gripes 7b 5, 7.
- Markup expressions may call a fn returning a `string` built in a per-frame scratch arena (`j3.memory.scratch`
  exists; reset in `endFrame`), and the text node compares bytes, so a steady frame still allocates nothing. Add
  `j3.text.fmtBytes`, `fmtInt`.
- `App.shutdown(fn)` (runs on window close and `requestQuit`), and `Dom.timeMs()` (the UI clock in ms, honoring
  `setFrameMs`) for `Toasts`.

**Done (9a).** `j3.text.fmtInt(t, n)` (`1,234,567`, negatives, i64 min) and `fmtBytes(t, n)` (`0 B`, `999 B`, `1.0 KB`,
`1.5 MB`, ... EB; 1024 units like the desk-app list had, one truncated decimal so `1023.9 KB` never becomes `1024.0 KB`).
Both append to a caller `Text` and return a view of just the appended part, so they can write into any arena or scratch
`Text`. `App.shutdown(fn(&W))`: a list run in registration order after the loop and before `game.destroy`, on window
close, `requestQuit`, `--frames` and `J3_STEADY` alike; not on `panic`. `Dom.timeMs()` returns the `now` that animations use
(follows `setFrameMs`). desk-app: sizes use `fmtBytes` (browser/filter/rename/toast goldens blessed: `12 KB` is now
`12.3 KB`, small files show bytes), and `Toasts` run on `dom.timeMs()` set in `draw`. Tests: `text_test` (+22),
`dom_test` timeMs; `speculative/29_ui_steady` prints `shutdown 1/2` once each for `--frames` and `J3_STEADY`.
Left for 9b: markup calling these through frame-scratch strings.

**Done (9b).** Most of it already worked: a text node (`Dom.text`) compares bytes with `storeBytes` and copies into node-owned
`buf` only when different; `title`/`aria-label`/`role` use `keep` (same), `placeholder` `storeBytes`, `class="{..}"` words
(`clsWords`) hash and intern copies, `style:` text is parsed (hash compare, no alias), keyed text keys are copied into
`Keyed.nb` (7c). Aliasing paths: `value=` (`attrText` kept a view, so a scratch fn there dangled and events read `sval` between frames: now copied into `Node.svBuf` via `keep` unless the box is being edited) and `trigs` (literals only). `scratchText` warns once on stderr past 1 MiB used (a loop without `App` that never rewinds). So the only code is `j3.text.scratchText()` (an empty `Text` on `memory.scratch(256K)`, the
arena `App` creates; distinct from `App.mem`, so scratch use is not arena growth). Owner of the rewind stays `App` at frame start,
not `Dom.endFrame`: the Dom is not the only user (draw systems after `render`, `steadyRange`) and a bare `game` loop has no
`endFrame` guarantee; bare loops call `rewindScratch()` themselves (documented). Fixture `speculative/29_ui_steady`: `State.size/tip/cls/key/row`
return scratch strings used in text, `title=`, `class="size {..}"` and a keyed each text key; idle `steady layouts=0 restyles=0
arena_growth=0`; `STEADY_CHURN=1` asserts each frame's text with `testkit.hasText` (arena_growth=0, layouts>0). Docs: `j3c doc j3ui` "Formatted text from a function".

---

## Stage 10: Tables and dialog inputs (old 18)

Gripes 7b 9, 10.
- Column tracks shared between a head and its rows: `grid-template-columns: 1fr 140px 100px` on `.rows > *` with
  `display: contents` rows, or a `<table>` with `<th>`/`<td>`. Pick the smaller, document it in `j3c doc css`. If
  stage 6 ran first, this is already done there; keep only the dialog half.
- A `<dialog modal>` selects the text of its first input on focus (`autoselect` attribute), and Enter clicks the
  `button.primary` (or `default`) when no input handles `on:submit`.

**Done (6d), the table half.** Chose `display: contents` over `<table>`: it reuses the grid solver (one flatten step in
`layoutGrid`/`contentSizes`), a `<table>` would be a second layout mode. `grid-template-columns` takes up to 16 tracks
(`css.MAX_TRACKS`; five or more are one `V_LIST` in `Css.paints`, more warns and drops the declaration). `display: contents`
(`DP_CONTENTS`): `dom.flatFrom`/`flatAdv` replace a contents child of a grid by its children (nested contents too); the
contents node gets an empty box at the grid origin, so grandchildren's x/y are in the grid's frame and paint, hit test,
`:hover` ancestors, focus order, keyed reconcile and scroll culling (`farOff` looks through it) work unchanged. Only a
grid parent flattens it; elsewhere it is a block. Not for `virtual` lists (their spacers would take grid cells). No
background/border/padding on the contents row itself, so `examples/desk-app` (styled, selectable rows) was not migrated.
Tests: `css_test` (N tracks, limit warning), `dom_test` `testContents` (header + 1000 keyed rows, scroll, reorder, remove, idle).

**Done (10), the dialog half.** Markup flags go through one runtime call, `Dom.attrFlag(id, 1|2|3)`: `autoselect` on a
`<dialog modal>`, `on:submit` on an input (gen now emits it so the runtime knows the box handles Enter), `default` on a
`<button>`. `autoselect`: `overlays.enter` focuses the first enabled text box/textarea (`firstField`) and marks it `fSel`;
`inputs.seeFocus` selects all when it loads the box's text (so it works with a `bind:value` filled the frame the dialog
opens, and only for that open-focus). Enter: `inputs.textKeys` on a single-line box without `fSub` calls
`overlays.defaultClick`, which finds the topmost-enclosing modal and sets `d.clickNode` (the keyboard-button path) on its
first `button[default]`, else first `button.primary`; disabled does nothing (no fall-through). Textarea never triggers it;
buttons and checkboxes keep clicking themselves; the caret popup / select list consume Enter before `textKeys` runs.
Not on `<input>` directly (autoselect-on-every-focus is a different feature). desk-app `RenameModal` uses `autoselect`
and no `on:submit` (`golden/rename.png` re-blessed: the name is now selected). Tests: `overlay_test` `testDialogInputs`
(+13), `ui::dialog_default_flags`.

---

## Stage 11: Language papercuts (old 17)

Gripes 7a 4 and 7b 8. Also on the list, bigger and not scheduled: **capturing closures** (a roadmap non-goal for v1; handlers and callbacks stay verbose until then), and the untyped-left-literal rule (`2 * far` fails, `far * 2` works; roadmap has the fix). Core compiler, so small: a module-level `const NAME: [N]T = [..]` (or `static`), `else if`
inside an if-expression, and a field of any rvalue (`xs[a..b].len`; calls are done). Tests next to `compile.rs`
`field_of_a_call_result`.

**Done (11a), array consts.** `const NAME: [N]T = [..];` is a read-only global: `collect_globals` (check/mod.rs) turns an `ArrayLit` const into a `GlobalDef` with `readonly: true` (folded by `fold_global_init`, the `#[singleton]` path; `collect_consts` skips it). Lowering gives its place `Access::read_only_view` (`global_access` in lower.rs), so index, `.len`, `for`, slices and `[]T` params work, and `=`, `+=`, `.f =` and `&` are errors ("cannot assign into const array `X`"). Emitted as static initialized data (`static` C / `global` LLVM, no init code; deliberately not `const`/`constant`, because slices carry no access mode and a write through a `[]T` made from it would segfault in .rodata). Strings were added to `fold_global_init` (static `{ptr,len}` plus a private bytes global), so `[N]string` works, also for `#[singleton]` arrays. Length mismatch reports the span. Limits: module-private (a `pub const` array is not importable, same as singletons), nested `[2][2]int` works only on LLVM (the C backend cannot emit nested array types; same for `#[singleton]` arrays), a callee writing through a `[]T` made from it changes the table (the checker cannot see that). Tests: `array_const_*` in compile.rs, `examples/honesty/array_const.j3`. For 11b: parser seams untouched; the const parse is `parser.rs const_item`, unchanged.

**Done (11b), `else if` in if-expressions and field of any rvalue.** `parser.rs if_expr`: after `else`, an `if` recurses (any depth; the missing-final-`else` error is unchanged; `.j3ui` expressions use the same parser). `lower.rs base_place` now spills every non-place expression (call, slice, binary/operator, cast, if, ...) to a temp, and `Index` uses it too, so `xs[a..b].len`, `(v + w).x`, `make().inner.x`, `make().arr[i]` work. Stores into such a temp (`pt().x = 1`, `+=`) are rejected in `Stmt::Assign`/`AssignOp` via `writes_temporary` ("cannot assign to a temporary"); a store through a `&T` result still works. Not covered: slicing an rvalue array (`f()[a..b]`, `Slice` base still needs a place). Tests: `else_if_expr_*`, `assigning_into_a_temporary_is_an_error` in compile.rs, `examples/honesty/else_if_expr.j3`, one lsp test in `lsp/query.rs`. `j3c doc lang` updated.

---

## Stage 12: Text stack leftovers (old 11)

- **Color emoji.** SDL_ttf draws Noto Color Emoji in color but at a fixed 109 px strike, ignoring the point size,
  which blows the line height. Option A: scale the strike to the font size in both `game.textWidth` and `p2Text`.
  Option B: a sprite atlas keyed by code point (Twemoji style) with a checked license, nothing downloaded without
  one. Try A first; it needs no asset.
- **RTL caret and selection in text boxes.** Text boxes shape and reorder, but caret, selection and hit test use
  logical order. Map caret index to visual x per run, and make selection a set of visual rectangles.
- **Kerning and subpixel text.** Text is SDL_ttf at whole pixels, with synthetic bold, so it looks worse than a browser. Measure whether FreeType direct (kerning, subpixel advance, real bold weights via `@font-face` per weight) is worth it for a shipped game; a real bold file per weight is the cheap first step.
- **Static selection.** (DONE, 12c) Double-click selects a word, Ctrl+A selects all selectable text, shift-extend (`textsel`).

**Done (12a), color emoji (option A).** `game.strikeScale(raw)` (game.j3) detects a fixed-strike bitmap font: a `FONT_SRC` font
taller than 1.6 em whose height does not change when re-opened at twice the size (checked once, `FontSrc.fixed`); the factor is
`size / TTF_FontHeight` (Noto Color Emoji: 16 px gives 1/8). It is applied in `textWidth` (`textWidthPlain`), `textSizeBytes`,
`fontLine` and `fontAscent`, so layout, line height and `p2Text`'s per-run baseline shift (`fontAscent` difference) agree, and in
`bakeColoredBytes` (GL), which box-filters the raster down once (alpha-weighted) before upload; the text cache holds the scaled
texture, so frames do no rescale. Such fonts skip the ASCII atlas. Emoji advance is about 1.06 em, line height = font size, so
a mixed line is as tall as a plain one. SDL (non-GL) renderer path is unscaled, as it already ignores draw scale. Tests:
`text_test` `testEmoji` (skips with a message when Noto Color Emoji is not installed), golden `speculative/30_ui_emoji`.
Option B (sprite atlas) not needed. Emoji in the font chain must be a code point the primary font lacks (DejaVu has a
monochrome U+1F600).

**Done (12b), RTL caret and selection.** `j3.game` helpers next to `fbVisual`: `textBidi`, `textCaretX(font, msg, bold, i)`,
`textIndexAt(font, msg, bold, x)` (-1 when not reordered), `textSelRects(font, msg, bold, lo, hi)` with `selRectX/W(k)` (singleton
buffers, no allocation); each walks the runs in visual order with the same `runWidth` (shaping + chain font) `p2Text` places them by,
and non-RTL text takes the old `textWidth` prefix path unchanged. RTL run: x = run x + run width - prefix width. Boundary rule: the caret
belongs to the run of the preceding character (index 0: first logical run), so a hit test exactly on a shared point is ambiguous.
Used by `field` (`caretX`, `indexAt`, selection, `run`), `area` (`caretAt`, per-line selection rects, `moveV` goal x), `rich`
(`caretX`, segment hit test, `selRects`: one rectangle per run per segment, a token its chip), and `Dom.caretRect`. Arrow keys stay logical
(documented); `textIndexAt` bisects the hit run (O(n log n)); `textWidthPlain` now measures a non-ASCII run in 1000-byte pieces (`cstrBytes` panicked past 1023 bytes); the selected newline is a 4 px stub at the line's logical end; visual movement would need per-line context in `Edit`. Tests: `text_test` `testCaretRtl`, `input_test` `testRtlInput`
(both skip without DejaVu + HarfBuzz), golden `speculative/31_ui_rtl`. 12c can reuse `textSelRects` for word/range highlights of static text.

**Done (12c), static selection.** `textsel.step` counts fast presses on selectable text (`Dom.selClicks/selCt/selCx/selCy`: under 400 ms
and 4 px): 2 selects the word, 3 the block (the owner that holds the line frags, so it spans decorated spans), Shift+press moves the focus end
and keeps the anchor (a plain click without a drag leaves a collapsed anchor, nothing highlighted, so a later Shift+click selects from it;
`textsel.active` is false for it). Word rule is shared and browser-like: `textedit.charClass` (space, ASCII punctuation, word = ASCII
letters/digits/`_` and non-ASCII letters; no-break and Unicode spaces are space, U+2000..206F and CJK punctuation are punctuation) feeds
`wordRange` (double-click, `Edit.selectWord`) and `Edit.wordLeft/wordRight` (Ctrl+arrows: one class run, then spaces); so "foo," selects
"foo", and "," alone selects ",". No existing `input_test` expectation changed. A click on a gap picks a neighbour (the pick is a caret position). Words stay within a text node. Ctrl+A with no text box focused and no popup open selects all selectable text
under the document root, or under the top modal. Grids still take Ctrl+A when focused. `selected()` now separates blocks by owner (a span inside a
paragraph no longer inserts a newline) and keeps one collapsed space dropped at a piece's end when text follows in the same block. Not done:
Shift+arrow extend, drag-by-word after a double click, punctuation-aware words. Tests: new `j3c/stdlib/tests/textsel_test.j3` (23), golden
`speculative/34_ui_inline` state `sel`.

**Tests.** Golden of emoji at two sizes; `text_test` caret x in an Arabic and mixed string; `input_test` double
click and Ctrl+A on static text.

---

## Stage 13: A11y gaps (old 12)

Step 1 (semantic tree) is done. Gaps: `<label for>` with element ids and click-label-to-focus, `tabindex` other
than 0, `aria-selected`, `aria-live`, `aria-describedby`, a radio input and radio groups, `visibility: hidden`.
Players with a screen reader are a minority on Steam, so this stays optional for a first release; the semantic tree alone is enough for a store checkbox. Step 2, the platform bridge (AT-SPI, then UI Automation and NSAccessibility), is `plans/024-a11y-platform-bridge.md`.

**Done (2026-10-25, plan 026 chunk 31; positive `tabindex` and radio groups followed in plan 027 chunks 8-9).** AT ids are `Dom.handle` handles (every `a11y` call takes a handle or plain id; a gone node answers empty). New in `ui/a11y.j3`: `bounds` (absolute rect with scroll, transform, flip, transition scale), `text` (UTF-8 byte offsets; refused for `type=password`), `describedBy`, `language()` (= `i18n.locale()`), `enable`/`events`/`liveChanges` (filled only after `enable`, queue capped at 256), and `Inbox`/`post`/`drain`/`onAction` (the only off-thread entry; `drain` runs actions with the lock released) plus `Dom.requestFrame`/`wantsFrame`. `a11y.inbox` pads the arena to 8 bytes because `Arena.alloc` does not align and a misaligned mutex aborts. Dump shows `selected`, `live=`, `desc=`; `<details>` has `expanded`; `<progress>`/`<meter>` get names; `<label for>` names only its target; `visibility: hidden` is left out; `tabindex="-1"` is skipped by Tab and the pad; an AT focus turns on keyboard mode. `.j3ui` accepts `aria-live`, `aria-describedby` (static ids; dynamic is a compile error) and `aria-selected`. Tests: `a11y_gaps_test`, `a11y_bounds_test`, `a11y_inbox_test`, Rust `aria_state_attributes`. The bridge API for plan 024 is in plan 026's chunk 31 note.

---

## Stage 14: Leftovers

Small open items; do them when a stage touches the area.
- ~~`{#if}` re-entry allocates through `Child.new`.~~ Done (14).
- Event payload types are limited to scalars and strings.
- `provide`/`inject` types are compared as text.
- Assignment to a const array element is not caught. (Done in 11a: compile error with a span.)
- ~~A mouse click at a mid-word soft wrap shows the caret on the next line.~~ Done (14).
- ~~`left=` / `top=` are not available on `style:`.~~ Done (14): they already worked; now covered by tests.
- Column flex never wraps; no `order`, `align-self`, `max-width` (stage 15).
- Native-target check: run the `.j3ui` and `paint` path on Windows and macOS builds (`sys_windows.j3`, `sys_macos.j3`); until plan 013 lands it is untested there.
- A real HiDPI display was never tested (offscreen only), nor fractional X11 scale.
- The textarea is not a real `overflow` box (own scroll).
- `examples/` is git-ignored: decide whether to track `desk-app` and `big_ui_test`.

**Done (14).** Three items. (1) Entering an `{#if}`/`{#match}` branch (and `<component this>`, which lowers to the same nodes)
calls the held child's `j3Reset()` (as a pooled `{#each}` row does) instead of `Child.new(mem)` (`gen.rs` `resets`); `onDestroy`
runs on exit, `onMount` on entry, state returns to its `let` defaults. `speculative/15_ui_lifecycle`: 1000 on/off cycles after
40 warm-up: 15992 bytes of arena growth before, 0 after (small because the test children are small), and a fresh-state check.
(2) `style:left` / `top` / `right` / `bottom` already compiled and ran (numeric longhands, `dynattr.setNum`; the slot rules
are the 8 per element / 6 on a `virtual` one); this adds a ui:: test and a runtime check in `speculative/29_ui_steady` (boxes
placed by `style:left/top` and `right/bottom`, moved on a change, 0 layouts when idle). (3) `area.indexAt` sets the caret
affinity `e.up` (the flag End already used) when the hit is the mid-word soft break, so a click at the end of a wrapped line
keeps the caret on that line; Left/Right/Up/Down/Home/End are unchanged. Static selection (`textsel`) draws no caret and
does not use this path. `input_test` 421.

Note: `speculative/12_ui_runtime` is an interactive demo; run it with `--frames N`
(`j3c run speculative/12_ui_runtime --headless --frames 30`), or it never exits.

---

## Stage 15: CSS gaps for shipped game UI

Source of truth for what is missing is `j3c doc css` ("Differences from a browser"); update it as items land. Ranked by how
soon a heavy game UI hits them. Each item: parser in `ui/cssparse`/`cssvalue`, matching in `cssmatch`, layout in `ui/dom.j3`,
a runtime test in `j3c/stdlib/tests/`, and a golden in `examples/idle-breakout` or `desk-app`.

1. **`max-width` / `max-height`** (and `min-*` if partial). Today the only workaround is `width: min(...)`. Most-asked-for.
2. **`em` / `rem` units.** Lets one `font-size` on `:root` scale a whole UI (accessibility text-size option, UI scale slider).
   Alternative if `em` is hard: a `--ui-scale` that multiplies px at resolve time. Prefer real `rem`.
3. **Flex:** `flex-basis`, `flex-shrink`, `align-self`, `order`, `*-reverse`, `wrap-reverse`, and **wrap on a column**.
   Needed for inventory grids, hotbars, and responsive panels. `order` matters for controller-focus order (stage 16).
4. **Inline boxes.** `span` and `small` have no box: no padding, border, or background. Chat, tooltips with `[Rare]` tags,
   and rich text want them. Needs inline layout that carries a box across line wraps (split into per-line fragments).
5. **Pseudo-elements** (`::before`, `::after` with `content`), and the sibling combinators `+` and `~` and `#id` selectors.
   Decoration (corner ornaments, separators) without extra markup.
6. **`@media (prefers-color-scheme)` and nesting**, plus a `prefers-reduced-motion` feature (accessibility: animations off).
   Skip the browser-only ones. Parenthesised groups in media queries only if a use shows up.
7. **Text:** `white-space: pre` / `pre-wrap`, `line-clamp`, `justify`, `text-overflow` if missing. Item descriptions and logs.
8. **Scrolling:** `scroll-snap` (carousels, level select), independent `overflow-x`/`-y` clipping.
9. **`!important`, `@layer`, floats, container queries.** Won't-do unless a real project needs one; record the decision here.
   Floats and container queries are the expensive ones; a game UI rarely needs either.

**Done (15b): `em` / `rem`.** `V_EM`/`V_REM` (`css.j3`) come from `parseLenU` (cssvalue; used by the ordinary length paths: `C_LEN`, line-height,
border shorthand, grid tracks, custom properties) and are resolved to `V_PX` in `buildStyle` (`cssmatch`) and `dynattr.build`
through `css.resolveUnits`: `em` of the element's computed font-size (the parent's for `font-size`), `rem` of `Css.rootFs`, set
by `computeStyle` for a parentless node (cache hits too) and by `dom.applyStyle` after the root's `style:` override. A root
font-size change gives the root a new style id, so every child restyles; unchanged costs nothing (`layout_test` `testEmRem`
checks `d.laid`). `calc()` has `O_EM` (uses `Value.f` of the `V_CALC`, which `resolveUnits` fills with the element's font-size)
and `O_REM` (reads `rootFs` at layout time). Keyframe stops convert in `motion.chanOf`. Not covered: box-shadow, filters,
gradient stops, positions (px only); a `style:` font-size does not re-resolve em/rem of that same
element's own rule values. idle-breakout now uses a 16px root and `rem` for every font-size/padding/gap (the six old goldens
are pixel-identical) with a new `scaled` golden (root 20px). `font-size: calc()` resolves at cascade time (`csscalc.fontCalc`; `%`/`em` of the parent). `rootFs` comes only from node 0 (popups are children of their owner, never parentless; `overlay_test` `testSelectRem`). Tests: `css_test` (136), `layout_test` (134), `overlay_test` (294).

**Done (15a).** Independent review CLEAN (fourth run).
 Added `max-width` to the existing length parser/property table,
`calcProp` and `animKind`. `dom.limitSize` applies min/max with containing-block bases before width/definite-height
layout and after natural-height measurement; min wins conflicts, content-box limits include padding/border, and the
existing border-box padding overflow used by virtual-list spacers is preserved. Intrinsic widths and atomic inlines
honor limits; grid percentages use their track width (rows remain content-sized); absolute placement uses the clamped
box. Flex hypothetical sizes, wrap decisions and freeze/redistribution honor both bounds; column natural bases use
unclamped content height. `Node.basisW/H` join the layout cache key, so changing percent bases cannot leave stale limits.
Tests cover px/%/calc/em/rem, conflicts, dynamic removal/none, resizing, intrinsic/auto, both flex axes and steady frames.
Desk-app RenameModal now uses responsive width plus `max-width: 340px` (no `width: min(...)` existed in the large examples,
so this replaces its fixed-px workaround). No golden was re-blessed: all twelve desk/idle captures pass tolerance 2;
fresh snapshot-stdlib browser/upgrades captures match the new captures exactly, confirming pre-existing AA drift.

**Done (15c1).** Independent review CLEAN (fourth run).
 Added `flex-basis` (auto and the existing length/unit/calc path),
nonnegative `flex-shrink` (default 1, content-base weighted, zero/fractional factors and min/max freezing), `align-self`
(auto plus align-items values), and stable integer `order` for both axes and wrapped rows. Numeric `flex` keeps its
existing grow/zero-basis behavior unless an explicit basis is set; multi-value shorthand remains unsupported. Separate
visual links preserve source/selector/event/Tab traversal while ordering geometry, hit testing and painting. Ordinary
ordered subtrees use runtime painting; generated slots replay only for canvas-containing children, preserving their
component/slot/keyed locals and callbacks. Large ordered canvas lists retain a documented quadratic replay limit.
Desk-app Details passes `.det` directly to kit.Card with `align-self: flex-start`, removing its sizing wrapper. Tests
cover units, auto, weighted/zero/fractional shrink, box sizing, min/max, per-item alignment, negative/tied/wrapped order,
dynamic removal/resize and idle costs; generated fixture `speculative/32_ui_flex` checks callback order and steady frames.
Reverse directions and column wrapping are covered by 15c2 below; no golden baselines are re-blessed.

**Done (15c2).** Independent review CLEAN (fourth run).
 `row-reverse` and `column-reverse` reverse main-axis geometry;
`wrap-reverse` reverses cross-axis progression and alignment. Source and order-sorted paint/hit priority stay unchanged.
The shared flex line solver wraps columns with a definite height; indefinite columns keep one natural-height line.
Column line widths use intrinsic/explicit widths before align-content distribution and per-line auto-width stretch,
retaining containing-block percentage/calc bases and min/max limits. The largest used main extent preserves scroll range
for oversized items. Row/column gaps remain physical across all directions.
Tests assert asymmetric padding/margins/gaps, both reverse axes, reversed cross-axis self/content alignment, baseline alignment,
order, column stretch and percentage/calc clamps, natural/removed/resized height, empty/zero/oversized items and idle caches.
The generated `speculative/32_ui_flex` fixture adds reversed overlap paint/hit parity and reversed column wrapping while
retaining callback/keyed/slot and steady-frame checks. All twelve desk/idle captures match prechunk captures exactly;
stored captures retain their known <=2 channel rounding drift. No golden baselines were re-blessed.

**Done (15d).** Independent review done (see chunk notes). `white-space: pre | pre-wrap` (spaces, tabs at eight-space stops, trailing
empty line kept; pre never wraps), `text-align: justify` (soft-wrapped lines only; the stretched gap is a real fragment, so
selection and picking use it), `line-clamp: N` (positive integer, not inherited, `style:line-clamp` too; last kept line ends in
an ellipsis; height and copied text stop at the clamp), and `visibility: hidden | visible` (inherited; the box keeps layout but
does not paint, hit-test, focus, activate, select or run canvas callbacks; a `visible` descendant overrides). `text-overflow:
ellipsis` already existed. All share the one inline-run (`Ifc`) used for measuring and painting. Hidden focus/activation checks use
`dom.styleVisible` (visibility only), not `dom.paintVisible`: a virtual list keeps the focused row alive as `display: none` and
`speculative/26_ui_grid` broke when focus was dropped for that (caught and fixed here). Tests: `text_css_test` (38; split out of
`text_test` because both together exceed the 32-font-handle budget, `text_test` now 166), `speculative/32_ui_flex` has a `TextCSS`
component and new `golden.toml` states `before`/`after` (blessed here, no existing golden changed). textarea keeps its own wrapping
(not affected by `white-space`).

**Done (15e).** Independent review done (see chunk notes). Selectors: `#id` (new static `id="x"` attribute, `Dom.attrId`, `MatchNode.id`; specificity 1,0,0),
`a + b` and `a ~ b` (`Step.comb` replaces `child`; `cssmatch.matchAt` backtracks over ancestors and earlier siblings, skipping text nodes and
pseudo-element boxes via `MatchNode.pe`), and `::before` / `::after` (also `:before`). Invalidation extends `markStyle`: with sibling rules loaded
(`Css.sibMode`: 1 for a lone `+`, 2 for `~` or chained `+`) a class/state/attribute change, insertion, removal (`unlinkNode`) or keyed move also
marks the next element (mode 1) or all later elements (mode 2) style-dirty, so one change costs up to the siblings after it; with no sibling rules
nothing changes. Pseudo-elements: `content` is a new property (`P_CONTENT`, string or none). `Rule.pe` rules are skipped for ordinary nodes and
matched against the host for the box. After a host restyles (`resolveNode` -> `syncPseudo`) a ghost-flagged child (`Node.pe`, tag `::before`/`::after`,
one text child) is made or dropped while some such rule matches; `applyStyle` fills its text from `content` and sets `display: none` for none.
Reusing `ghost` gives: skipped by the build walk/keyed removal/`trimTail`, painted from the tree by the existing ghost paths, not hit-testable, not
focusable, not in a11y or testkit; `insertBefore` keeps `::after` last, `fixFirst` ignores the boxes (`:first-child`). Not done: `attr()`, counters,
`content` escapes, `var()` in `content`, inline-box decoration (display defaults to inline until 15f). Tests: `css_test` (parse, specificity, matching),
new `selector_test` (67: insert/remove/reorder, `~`, chained `+`, sibling `:hover`, restyle bound, `#id`, boxes, content toggle, hit, keyed rows,
steady), golden fixture `speculative/33_ui_selectors` (`before`, `after`); `J3_STEADY=60:120` zeros. Review round: rules without `+`/`~` keep the old greedy matcher (`Rule.sib`); leaving fade ghosts are `MatchNode.pe = -2` (not siblings, followers re-marked); scoped/`scope2`, keyed-shuffle `::after`-last and ghost cases tested; on a `virtual` box the boxes flank the rendered window only.

**Done (15f).** Independent review done (see chunk notes). Inline boxes. `display: inline` elements with padding, border, margin, background or shadow are tracked while
their block is laid out (`ifcNode` pushes an `OBox` on `Dom.ob`; `placeItem` adds the left edge (`Ifc.lead`) to the first item and records the
first/last fragment per line (`boxItem`); closing adds the trailing edge). `endLine` (`flushBoxes`, after align/justify/clip positions are final) turns each
line's piece into a rect on the owner's own chain (`Node.box0`, `Dom.boxes`: `Frag` with `kind 2`, `ap` = height, `start`/`end` = first/last piece;
storage reused like `frags`, so steady frames allocate nothing). Fragments (`frag0`) are unchanged: text, selection, justify and ellipsis code never see boxes.
`paintFrags` paints the pieces before the text through `paintBackground` (sliced ends: the box is drawn whole, extended on the open side, clipped to the piece);
`hitNode` asks `hitBox` after the children (innermost piece wins; a `::before`/`::after` piece answers for its host; the block's own rect does not gate it unless it clips). The last word's fit test and min-content include the closing edges of the boxes ending after it (`trailAfter`, `Ifc.tr`). Vertical
padding/border never touch the line height. Empty or line-clamped-away boxes take no room. Not done: see `j3c doc css` ("Differences"): a margin at a line break,
`vertical-align`, `clone`. Tests: new `inline_test` (58); golden fixture `speculative/34_ui_inline` (`boxes`); no existing golden changed.

**Done when.** Items 1 to 3 pass tests and are documented, and one of the large examples drops a `width: min(...)` or
fixed-px workaround for each. Items 4 to 8 are picked per project need.

---

## Stage 16: Gamepad and focus navigation

**Why.** A Steam Deck or controller-first game needs the whole UI usable without a mouse. Today focus is Tab/Shift-Tab and
menu keys (`ui.j3`, `j3.ui.dom` focus order). No D-pad traversal, no pad-driven activation, no on-screen prompts.
Plan 017 item 5 covers the controller lifecycle and input contexts (rebinding, hotplug, action sets); this stage is the
`.j3ui` half and reuses it.

**Design.**
- **Spatial focus:** D-pad / left stick moves focus to the nearest focusable in that direction (by `absX/absY/w/h`, which
  the a11y tree already exposes), honoring modals (`allowedByModal`). Per-container override: `focus-group` (wrap, trap),
  and explicit `nav-up|down|left|right="id"` for odd layouts. Tab order stays DOM order (+ `order` once stage 15 lands).
- **Activation and back:** A = click through the keyboard path (as Enter does), B = close the top modal or popup,
  shoulders = tab switching, triggers = page scroll. Mapped through the action sets of plan 017 item 5, not hard-coded.
- **Focus visuals:** a `:focus-visible` that shows only for keyboard or pad focus, hidden after a mouse move. Input-device
  tracking: last-used device switches the whole UI's glyph set.
- **Prompts:** a `<glyph action="confirm"/>` element that renders the right icon for the active device (key, Xbox, PlayStation,
  Deck) from an atlas the game supplies; Steam Input API glyphs come in plan 025.
- **Scrolling boxes:** right stick scrolls the focused scroll box; focus moving to an off-screen item scrolls it into view.
- **Text entry:** focusing a text input with a pad opens an on-screen keyboard hook (platform call; Steam's floating keyboard
  is plan 025). Without one, the field takes keyboard input only; document this.

**Tests.** `dom` test: a 3x3 grid moves focus correctly in 4 directions and across a gap; a modal traps focus; `domhtml`
dump shows the focused id; fake-input run in desk-app (`DESK_TEST=1`) with pad events.

**Done when.** desk-app is fully usable with a gamepad and the focus ring never shows after a mouse move.

**Done (2026-10-25, plan 026 chunk 30).** `ui/padin.j3` + `padnav.j3`: D-pad/stick spatial focus (repeat 400/120 ms), focus-group wrap/trap/arrows, `nav-*` overrides, A = Enter, B = Escape, shoulders/triggers/right stick scroll, `d.padText` on-screen-keyboard hook, `:focus-visible` for pad, hotplug via `PadRaw.attached` plus `padConnected`, pads 0 and 1 merged; seam `d.setPad`/`usePad`. Tests: `padnav_test` (92 checks), golden `speculative/59`. `<glyph>` element added later in plan 027 chunk 10. Not built: rebinding/action sets (017 item 5), Steam Input; not tried on a real controller.
