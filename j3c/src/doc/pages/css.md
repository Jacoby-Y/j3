CSS for .j3ui
=============
A subset, parsed at startup (`j3.ui.css`) and laid out by `j3.ui.dom`. Scoped per component
(`<style>` in a `.j3ui`) plus one global file (`[ui] theme` in j3.toml). Unknown properties and bad
declarations print `warning: file:line:col: message` at startup and are skipped; a bad selector drops its rule.

Selectors   `*`  tag  `.class`  `tag.class`  descendant (space)  child `>`  `:hover :active :focus :focus-visible :focus-within :disabled :first-child :last-child :dragging :drop-target`
            `:nth-child(odd | even | N | An+B)`  `:not(a, .b, #c, :hover, [type=x])` (one compound per argument: no combinators, no nesting; specificity of the most
            specific argument; `:nth-child` counts element siblings only, and a sibling change restyles those after it)  `[type="range"]`  `:root`  `#id` (an `id="x"` attribute, a static string; nothing looks elements up by id)  next sibling `a + b`
            and later sibling `a ~ b` (text and pseudo-element boxes are not siblings)  `::before` / `::after` (below). (`.a:hover > .b` shows
            a hover card or a submenu with no code: an ancestor's `:hover` matches while the pointer is inside it.)
            Specificity is the usual (ids, classes/pseudo-classes/attributes, tags/pseudo-elements); `li + li { border-top: 1px solid #fff }` is a
            separator list. A sibling rule restyles later siblings when a node changes, enters, leaves or moves: only the next one for a
            lone `+`, every later one for `~` or a chain (cost is the number of siblings after the change; no sibling rules, no cost).
            More: `:checked` (checkbox / radio, follows `bind:checked`: `input:checked + label`), `:open` (an open `<details>`), `:enabled` (= `:not(:disabled)`), `:placeholder-shown`
            (a text box with a `placeholder` and an empty value), `:empty` (no element child and no text but whitespace, Selectors 4: browsers
            still count whitespace; `::before` boxes do not count), `:nth-last-child()` / `:nth-of-type()` / `:nth-last-of-type()` (An+B like
            `:nth-child`; "type" is the tag name), `:only-child` `:first-of-type` `:last-of-type` `:only-of-type`. All of them work in `:not()` (not
            the `:nth-*()` ones) and in nested rules. A rule with one of the structural ones (not `:checked` and friends: those are flags)
            makes every child insert, remove, move, or text change blank-or-not restyle the parent and all its element children (cost is the number of
            children; no such rule, no cost). Not supported: `:indeterminate` `:required` `:optional` `:valid` `:invalid` `:read-only` `:read-write`
            (no such state in the runtime), `<option>` `:checked`.
            `:has(arg, ...)` (limited): each argument is one compound (tag, classes, `#id`, `[type=x]`, state pseudo-classes such as `:checked` `:hover`
            `:focus-visible`) after an optional `>` (child), `+` (next sibling), `~` (later sibling) or nothing (any descendant): `.card:has(> img)`,
            `.row:has(input:checked)`, `label:has(+ input:disabled)`, `.card:has(> img) .title`. No nested `:has()`, no complex argument
            (`:has(.a .b)`), no `:has()` inside `:not()`, one `:has()` per compound: those warn at their position and drop the rule. Specificity is that of
            the most specific argument. Cost: matching searches the children, the subtree (early exit) or the later siblings, only for elements that pass
            the rest of their compound. When a class, flag (`:checked`, `:hover`, ...), `#id`/`[type]` a rule's argument tests changes, or an
            element is inserted, removed or moved, the elements carrying a `:has()` (every ancestor for a descendant argument, the parent for `>`,
            the earlier siblings for `+` / `~`) restyle with their subtree: cost is the depth, plus the restyled subtrees. A change nothing tests
            walks nothing; no `:has()` rule, no cost.
Pseudo      `.card::before { content: "text"; display: block; ... }` (`::after` the same, also the one-colon spelling) makes a box that is the
            first (`::before`) or last (`::after`) child of the matched element, styled by that rule (display, size, padding, background, border,
            color, font, `position: absolute` for corner ornaments). `content` takes one string (write the character itself, no `\` escapes) or
            `none`/`normal` (no box); `attr()` and counters are not supported. The box exists while a rule for it matches and is `display: none`
            while its content is none; it is not hit-testable itself (the pointer over it, over its inline box included, is over the host), not focusable, not in the accessibility tree or the test
            kit's text search, and `:first-child` ignores it. It is a runtime node: no per-frame cost when nothing restyles. `*` and other rules do not
            apply to it, only `x::before` ones, and `display` defaults to inline, which gives it an inline box (padding, border, background, radius: a `[Rare]` pill); `block` makes a real box. On a `virtual` box the boxes sit before / after the rendered window, not at the true ends of the scroll range. A leaving element (`transition:` / `out:`) is not a sibling for `+` / `~`.
Nesting     A style rule may hold style rules and `@media`: `.card { padding: 8px; &:hover { ... }  .title { ... }  > .x { ... }  & + & { ... }
            &.on { ... }  .dark & { ... } @media (max-width: 600px) { padding: 4px; } }`. `&` is the parent selector, anywhere and any number of times;
            a nested selector without `&` is a descendant of the parent, and one starting with `>`, `+` or `~` is relative to it. Lists multiply:
            `a, b { &:hover, .x & {} }` is four rules. Declarations written after a nested rule apply to the parent after it (source order wins).
            Up to 8 levels deep (a ninth warns and is skipped). A bad nested selector warns at its position and drops that nested rule only. Works in
            a component `<style>` (every joined rule is scoped like a flat one) and in theme.css. Each joined selector is an ordinary flat rule, so
            specificity is that of the joined selector (a browser gives `&` the specificity of `:is(parent list)`, the most specific of the list for
            every part). Without `:is()` a parent with combinators (`.a .b`) takes `&` as the first thing (`&:hover`, `&.on > li`) or as the whole last part after a descendant space (`.dark &` is `.dark .a .b`, as in Sass; `.dark > &` and `.x + &` are dropped; it differs
            from a browser's `:is()` only when `.dark`'s element sits inside the parent chain); `& + &` or `.x&` there warns and drops that nested rule. A one-compound parent allows `&` anywhere (`.x&` on `div` is `div.x`). Up to 64 joined selectors per rule.
At-rules    `@font-face { font-family: "Ui"; src: url("../assets/font.ttf"); }` (url relative to the CSS
            file). `@media [screen|all|only] [and] (min-width: 600px) and (max-height: 400px) { rules }`: px bounds of the
            window's width and height joined by `and`, `not` in front, alternatives with `,` or `or`, and `(prefers-reduced-motion: reduce | no-preference)` /
            `(prefers-color-scheme: dark | light)`, `(pointer: coarse | fine)` / `(hover: none | hover)` (the program's settings, see Preferences below; no other features, no parenthesised
            groups); the rules inside apply while it holds. `@media` inside `@media` ANDs the conditions, and inside a style rule (below) it
            applies the declarations to that rule's selector. `@keyframes` and `@property` as below. Others are skipped. Put `font-family` on `:root` or an element to use it.
Inline      `style:width={px}` / `style:color="{c}"` on an element (`j3c doc j3ui`, Dynamic style): beats every rule, transitions like one.
Variables   `--x: value` on any selector (`:root` for a theme, a class or component for a subtree), inherited by
            descendants and read with `var(--x)` or `var(--x, fallback)` (the fallback can hold another `var()`).
            A value is one color, length, number, angle (`90deg`, `0.5turn`), a time (ms/s, read as milliseconds) or a `var()` of another
            variable of the same element. Undefined with no fallback, or a type that does not fit the property, makes
            the declaration not apply (inherited or initial value); a cycle is invalid. A page tunes a component from
            outside: `gap: var(--section-gap, 8px)` in the component, `.page { --section-gap: 16px }` around it.
            `var()` also works inside compound values: gradient stops and positions (`linear-gradient(var(--a), var(--b) 50%)`),
            `box-shadow` / `text-shadow` colors and lengths, `color-mix()` arguments, the `border` / `outline` shorthands, `padding`
            and the other box shorthands. The cascade substitutes the variables and parses the result once per computed style,
            so a theme flip (a class on `:root`, `prefers-color-scheme`) restyles once, then idle frames cost nothing. A
            variable holds a color, length, `%` or number there (a time such as `200ms`, `calc()` and keywords other than `auto` cannot be
            written back, so `transition: color var(--t)` does not apply); an undefined one without fallback, or a typo in the value,
            drops the whole declaration and warns once, at its place in the sheet. Not in `style:` text (a
            warning); in `@keyframes` it is resolved per element (see `animation`), and a custom property cannot itself be a compound value (`--shadow: 0 0 4px red` is invalid; use one variable per color or length).
            Not in `style:` (no `style:--x`). Built-in: `--j3-tooltip-delay` (title and custom hover-card delay).
@property   `@property --p { syntax: '<percentage>'; inherits: false; initial-value: 0%; }` gives a custom property a type, so it can
            transition and animate. `syntax` is one of '<color>' '<length>' '<number>' '<percentage>' '<length-percentage>' '<angle>' '<url>' or '*'
            (anything; it does not animate); '<url>' takes a `url()` and jumps instead of blending ('<image>' is rejected: a gradient cannot be a custom property value); `syntax` and `inherits` are required, `initial-value` too unless the syntax is '*'. A rule
            that lacks one, or has a bad one, is ignored with a positioned warning. A registered property takes only values of its type
            (`--p: red` is dropped, so the inherited or initial value stays; a `var()` that resolves to the wrong type too; `0` is a
            length; an angle needs its unit); `inherits: false` gives children the initial value; unset, a registered property reads as its
            `initial-value`. Then `transition: --p 1s` (or `all`) and `@keyframes { to { --p: 70% } }` blend it, and everything that reads it
            re-resolves every frame: `background: conic-gradient(#48f var(--p), #dde var(--p))` is a progress ring, `conic-gradient(from var(--a), ...)`
            with `--a` going to `360deg` a spinner, `width: var(--w)` or `calc(var(--w) * 2)` a bar. An unregistered property does not animate
            (a transition on it never starts; a reader jumps with the new value). Cost while it runs: the animated element's style is
            recomputed from its rules each frame (nothing is allocated; only declarations that read the property are copied
            to the box; `style:` overrides and `--b: var(--a)` on the element follow), and so are descendants that read an inherited animated property, found by a walk over the subtree per frame
            (a subtree that declares the property itself is skipped); when it ends the cost is zero.
Popups      The built-in sheet (parsed before theme.css) styles `.j3-select-list` (a real element: `box-shadow`, `max-height`,
            scrollbar), `.j3-option` (`:hover`, `:disabled`, `.selected`), `.j3-check`, `.j3-label`, `.j3-color-panel` and
            `.j3-tooltip` (real nodes with wrapping text). Option visual children retain source scopes/classes/selectors
            (for example `option .icon`) inside `.j3-label`; inherited styles come from the popup row.
            Use uniform `.j3-option` heights for long lists. Color panel children `.j3-color-sv`, `.j3-color-hue`,
            `.j3-color-marker`, `.j3-color-handle`, `.j3-color-preview`, `.j3-color-value` accept ordinary CSS too; override them in theme.css or a component's `<style>` (`j3c doc j3ui`, Input widgets).
Units       px  %  ms  s  fr  vw vh vmin vmax (of the window, in logical px)  em  rem.
            `em` is the element's own font-size (for `font-size` itself: the parent's, so `2em` compounds down the tree);
            `rem` is the root's (`:root`, or `style:font-size` on it), 16px when unset. Both work on every ordinary length
            (sizes, insets, margins, padding, gap, border width, radius, line-height as a length, grid tracks), in `calc()`
            (`calc(1rem + 4px)`) and in `var()` (an `em` in a custom property is that of the element that uses it).
            They resolve to px in the cascade, so changing the root font-size restyles and relays out everything that
            uses `rem`/`em` once and costs nothing after; a unitless `line-height` stays a multiplier. Not accepted in
            box-shadow, text-shadow, filters, gradient stops, background/object positions and sizes. In `transform` (`translate*`), `translate` and `transform-origin` they work as plain lengths, also beside `%`, but not inside `calc()`. A `style:` font-size on an element does not re-resolve the `em`/`rem` of that same
            element's own rule (use a class); descendants are fine. Transitions interpolate the resolved px.
            A registered `@property` with syntax '<length>' resolves an `em` against 16px, not the element. A `style:` `transform` text that changes every frame leaks a little memory (a few floats per change): prefer a custom property or a class.
            A text-size slider is one line: `style:font-size={px}` on the root.
Functions   `calc()` with `+ - * /`, `min()`, `max()`, `clamp(lo, v, hi)` on lengths of mixed px, % and viewport units
            (`width: calc(100% - 2 * 16px)`, `padding: clamp(8px, 2vw, 24px)`). `+ -` need spaces and one dimension,
            `*` and `/` need a number on one side (`calc(10px + 5)` and `calc(10px * 2px)` warn at startup). Only on
            width, height, min-width, min-height, max-width, max-height, margin, padding, gap, left/right/top/bottom and font-size (there `%` and
            `em` are of the parent's size; not radius or border width, those warn). A `%` resolves against the containing block like the plain `%`; with no basis
            (an auto-height parent), a divide by zero or an overflow it falls back to `auto`. No transition on them (a change snaps).
            A viewport unit relays out the whole page on every resize.
Cascade     specificity then source order; a scoped rule counts one extra class, so it beats a global
            rule of the same shape. `inherit`, `initial`, `unset`, `revert` and `all: <keyword>` work anywhere (also in
            `style:`; not in `@keyframes`, where they warn). `unset` is `inherit` for the properties below, else `initial`; `revert`
            rolls back to the tag defaults (`strong { font-weight: revert }` is bold), else `unset` (in `style:` always `unset`);
            `revert-layer` is the same as `revert` (no layers). `all` skips custom properties; on a custom property `initial` is the
            registered initial-value or invalid (so a `var()` fallback applies). These inherit: color font-size font-weight
            font-family text-align line-height accent-color white-space overflow-wrap user-select text-shadow text-stroke-*
            letter-spacing word-spacing text-transform text-indent font-variant-numeric font-style text-decoration cursor.

Layout
  display            block (div etc.), inline (span, small), flex, grid, none, contents (see below)
  position           static, relative, absolute (left right top bottom inset, px or %; against the nearest positioned
                     ancestor), fixed (against the window, drawn past the ancestors' `overflow` clips and transforms,
                     ignoring their scroll; an overlay's home), sticky (in flow; `top: Npx` pins it under the top of the nearest
                     clipping ancestor until its parent's end; `bottom`/`left`/`right` are ignored; add a `z-index`)
  position-try-fallbacks  flip-inline | flip-block | flip-both (one keyword): an absolute or fixed box that would leave the
                     window is mirrored across its anchor (the parent's box, or the `left`/`top` point) when that fits
  z-index            integer or auto, on a positioned element or a flex or grid item: it makes a stacking context. Inside a context the order is its
                     own box, negative z-index, in-flow (not positioned) content in tree order, positioned boxes with z-index
                     auto or 0 in tree order, then positive z-index ascending; the mouse takes the topmost painted box. A
                     z-index only competes with the boxes of its own context: a 9999 inside a z-index 1 card stays under
                     a z-index 2 sibling card. The root, `position: fixed`/`sticky`, overlays, `opacity` below 1, a
                     `transform` (any other than the identity), `filter`, `backdrop-filter`, `mix-blend-mode`, `perspective`,
                     `rotate-x/y`, `clip-path`, `mask-image` and `isolation: isolate` also make one. A scrolling box
                     (`overflow: auto | scroll`) is no context: a positioned descendant joins the nearest one, still clipped
                     by the scroller and moving with its scroll offset. One J3 deviation: while a transition or animation of one of the properties above (opacity, transform
                     and its parts, filter, z-index, ...) runs on a box it is a context, a color or background one is not.
                     `order` still orders flex items within the in-flow part.
  isolation          auto | isolate (a stacking context without any other effect)
  flex               flex-direction, `flex: n` (grow n, zero basis unless flex-basis is set), gap, row-gap, justify-content
                     (flex-start flex-end center space-between space-around space-evenly), align-items (stretch flex-start
                     flex-end center baseline), min-* auto rule, `flex-wrap: nowrap | wrap | wrap-reverse` (a definite main size:
                     an item that would overflow starts the next line; `gap` is the column gap, `row-gap` the row gap;
                     `align-content` places the lines: stretch (default) flex-start flex-end center space-between
                     space-around space-evenly; a wrapped row's min-content is its widest item). Columns wrap with a definite
                     height; an indefinite main size stays on one line. row-reverse/column-reverse reverse main geometry;
                     wrap-reverse reverses the cross axis. Neither changes paint, hit priority or source/Tab order.
                     flex-basis: auto (width/height, else content) or any supported length, %, calc, em/rem;
                     an indefinite percentage basis falls back to content. flex-shrink: nonnegative number (default 1),
                     weighted by the content base size; 0 keeps the base size. Min/max freeze and redistribute shares.
                     align-self: auto (parent align-items), stretch flex-start flex-end center baseline.
                     order: integer (default 0), stable for ties; controls layout, paint and hit order, including wrapped
                     rows. Source/selector/event/Tab order is unchanged. Ordinary ordered subtrees paint in linear time;
                     each canvas-containing ordered child replays generated source slots (large ordered canvas lists can
                     cost quadratic time). Numeric flex remains the grow shorthand; multi-value flex is unsupported.
  grid               grid-template-columns / grid-template-rows: tracks `100px`, `25%`, `2em`, `1fr`, `auto`, `minmax(min, max)` (min: length, %, auto;
                     max: length, %, fr, auto), `repeat(n, ...)`, and as the whole value `repeat(auto-fill | auto-fit, <track>...)`: as many repetitions
                     as fit the content width (at least one; auto-fit then collapses the tracks no item uses, so the rest stretch). A repeated
                     track needs a length or % as its min or max: `repeat(auto-fill, minmax(64px, 1fr))` is an inventory. Up to 64 tracks (more
                     warns and drops the declaration; auto-fill stops at 64). `%` and `fr` rows need a definite height (else auto, as in a browser).
                     grid-auto-columns / grid-auto-rows (the sizes of tracks past the template, repeating), grid-auto-flow: row | column [dense],
                     gap and row-gap (also across spans). Items: grid-column / grid-row `<start> [/ <end>]` and `-start` / `-end`, each `auto`, a
                     line number (1 first, -1 the last line of the explicit grid) or `span N`; items fill the next free cell in source order
                     (sparse; `dense` goes back for holes and rescans from the start for each item, so it is quadratic in a huge grid), after the ones with fixed places; `order` is not used. A grid with no column list
                     has one `1fr` column. An `auto` track is its items' max-content (it does not shrink, and does not stretch to fill the
                     width); an item over several tracks adds what they lack evenly to the content-sized ones.
                     `grid-template-areas: "head head" "side main"` names rectangles of cells (`.` is an empty cell; a row of another length
                     or an area that is not one rectangle warns and drops the declaration); they make that many tracks explicit (tracks past the
                     template list are sized by `grid-auto-*`, a grid with areas and no column list has `auto` columns). `grid-area: main`
                     puts an item there; `grid-area: <row-start> / <col-start> / <row-end> / <col-end>` takes lines or `span N`; `grid-column: main`
                     / `grid-row: head / foot` use an area name as a line (its start, or its end as the second value). An unknown name is `auto`.
                     Alignment: `justify-self` / `align-self` (auto stretch start end center, with `flex-start` / `flex-end` the same) and
                     `justify-items` / `align-items` for the container, `place-self` / `place-items` for both (`<align> [<justify>]`). The default is
                     stretch; any other value sizes the item to its content (at most its area) and places it in the area. `margin: auto` on an item
                     centers or pushes it in its area on that axis. `justify-content` / `align-content` (start end center space-between
                     space-around space-evenly) place the tracks when they are smaller than the grid box (a definite width or height); `fr`
                     tracks fill it instead, and with no definite size nothing moves. Not done: named lines (only areas),
                     `min-content` / `max-content` / `fit-content()` as tracks, `grid-template` / `grid` shorthands, `subgrid`, masonry, `style:` on
                     any grid property, auto rows stretching to a taller grid.
  display: contents  the element has no box and its children become items of the parent `display: grid`. A header and every
                     keyed `{#each}` row can be `display: contents` children of one grid, so they share its columns:
                     `.t { display: grid; grid-template-columns: 1fr 140px 100px; } .t > .head, .t > .row { display: contents; }`.
                     A contents element paints, hit-tests and hovers through its children (`.row:hover > .cell` and `.row.sel > .cell` restyle the cells; `.row:focus-within > .cell` works too, the row stays in the tree; a contents element is not a focus target); it has no background,
                     padding or border of its own (style the cells, or use a flex row when the row itself needs a look). Only
                     a grid parent flattens it (a flex or block parent treats it as a block); it is not used inside a `virtual` list.
  box                width height min-width min-height max-width max-height (lengths, %, calc; max defaults to none),
                     padding, margin (1 to 4 values), border,
                     box-sizing
  scrolling          overflow-x / overflow-y / overflow (`x [y]`): visible hidden clip auto scroll. Each axis clips on
                     its own (`overflow-x: clip` cuts left/right and lets content show above and below). As in a browser, a
                     `visible` axis next to `hidden`, `auto` or `scroll` computes to `auto`, and `clip` next to those to
                     `hidden`; `clip` and `visible` together stay as they are. `clip` never scrolls, not even from code.
                     `auto` and `scroll` boxes take the wheel (Shift + wheel, or a box with only x, scrolls
                     sideways) and show scrollbars; `hidden` scrolls only from code. The wheel goes to the innermost
                     box that can still move on that axis and chains outward at its end.
                     overscroll-behavior: auto|contain (contain keeps the wheel in the box), scroll-behavior: auto|smooth
                     (smooth: wheel inertia, one notch coasts 48 px), scrollbar-width: auto|thin|none (10, 6, 0 px),
                     scrollbar-color: <thumb> <track>|auto. Scrollbars overlay the content (no layout space), the thumb
                     drags, a press on the track pages 90%, and children far outside the view are not painted or hit.
  scroll snap        scroll-snap-type: none | x | y | block | inline | both, then `mandatory` or `proximity` (default) on the scroll
                     box; scroll-snap-align: none | start | center | end (block, then inline; one value sets both) and
                     scroll-snap-stop: normal | always on its children; scroll-padding (box, 1 to 4 values) and scroll-margin
                     (child) move the snap edges. After a wheel, scrollbar drag or `d.scrollBy` rests for ~90 ms (or the wheel
                     inertia ends) the box glides (instant under reduced motion) to the nearest snap position on each snapping
                     axis; `proximity` only within a third of the view; `always` stops a gesture at the first such child it
                     reaches. A relayout (resize, changed content, a re-sorted keyed `{#each}`) keeps the box on the element it last snapped to: the same key in a keyed list, the same index in an unkeyed one. Children
                     inside a nested scroll box belong to that box. The frame stays live (`d.isLive()`) while it waits and glides.
  pointer-events     auto | none (the box is never the mouse target; its children still can be)
  overlays           an element with `popover` or a `<dialog modal>` in markup joins the overlay stack (see `j3c doc
                     j3ui`); `backdrop-color` (not CSS: `dialog::backdrop`) is the dim behind a modal, and an overlay
                     with no `z-index` paints at 1000 + its depth in the stack, in the root context above every `z-index` of the
                     page (popups, the `select` list, tooltips and the drag ghost are above those).
Paint
  background (hex, rgb(), rgba(), hsl(), hsla(), transparent, the 148 CSS color names), color,
  border / border-left / border-bottom / border-color,
  border-radius (1 or 4 values), opacity, font-size, font-weight (600+ is bold), font-family, text-align,
  line-height, accent-color, `font: inherit`
  background / background-image: linear-gradient, radial-gradient, conic-gradient (angles in deg turn rad
  grad or `to right`; stops with optional %; several layers, first on top; `background: layers, #color`),
  box-shadow (comma list, `inset`, blur, spread; a bright colour with 0 offset is a glow),
  border-style: solid dashed dotted none (also in `border:`), border-image: a gradient (`border-image:
  linear-gradient(..) 1`, the slice is ignored) or a nine-slice image (`border-image: url(panel.png) 12 fill / 16px round`:
  source, slice (one to four numbers in image px or %, optional `fill`), `/ width` (one to four numbers = times the border
  width, px, % of the box or `auto` = the slice), `/ outset` (numbers = times the border width, or px), repeat (one or two
  words). Longhands: border-image-source, -slice, -width, -outset, -repeat. border-image-repeat: stretch | repeat | round
  | space (horizontal edges first): `repeat` tiles the edge (and `fill` center) slices at their scale (border width over
  slice size), centered, the end tiles cut; `round` rescales them to a whole number; `space` keeps the size and spreads
  gaps (nothing is drawn when one tile does not fit). The corners never tile. A box with a nine-slice border still takes
  its layout from border-width, so give it `border: 16px solid transparent`
  background-image: url(..) and gradients mix as layers (`background-image: url(a.png), linear-gradient(..), url(b.png)`,
  first on top, at most 8 painted, more warn); background-size (px % auto cover contain), background-position and
  background-repeat (repeat no-repeat repeat-x repeat-y space round, or two words) are comma lists that cycle over the layers,
  and the `background` shorthand takes the same layers (`background: url(a.png) center / 32px round, #123`, color last). A custom property can hold a `url()` (`--bg: url(a.png)`, then `background-image: var(--bg)`, in a layer list, in `border-image-source` and the `border-image` and `background` shorthands, and as a `var(--x, url(b.png))` fallback); the path is kept as written and resolved from the project root (the working directory) like every `url()` in a background or border image, whichever file declared the variable (only `@font-face` resolves against its sheet), and the image loads once per path however often a class flip swaps it. `var()` colors inside a layered background work.
  `round` rescales the tile to a whole number per axis, and an `auto` other axis follows to keep the aspect. Size, position and
  repeat apply to url layers; a gradient layer always fills the box. At most 4096 tiles per box (more draws one). Tiles of
  one texture share a draw call (a tiled frame is one draw; each url layer another). Paths are relative to the project root.
  image-rendering: auto | pixelated (inherited): nearest-neighbor sampling for `<img>`, url backgrounds and border images, so
  pixel art stays crisp. The filter is per texture, so one image drawn both ways in a frame flushes the batch at each switch
  object-fit: fill contain cover none scale-down, object-position (px %), on `<img>`
  image-tint: <color> (J3 extension) multiplies into `<img>`, url backgrounds and border images; transitions
  clip-path: inset(.. round ..) | circle() | ellipse() | polygon([evenodd,] ..), px or %; cuts paint, not hit testing
  mask-image: linear-gradient(..) | radial-gradient(..): the gradient's alpha fades the box and its children
  backdrop-filter: none | blur(<nonnegative px>)   GL-only Gaussian blur of the pixels already painted behind
      the box, composited inside its border-radius before its own background and content. Use a translucent
      background for frosted glass. Unitless zero is accepted; multiple filters and other units are unsupported.
      The radius is Gaussian sigma in logical pixels (maximum 64); zero disables the effect. The painter API is
      `paint.backdropBlur(pos, size, corners, radius)` (`j3c doc j3.gfx.paint`).
      This uses two reusable window-sized RGBA8 targets and two full-frame passes per blurred box; use sparingly.
      The destination must be a single-sample color framebuffer at window size. Framebuffer edges clamp;
      clips, masks, transforms and opacity apply to the composite.
  filter: none | <functions>   an offscreen group containing the box and its descendants. Functions have one
      argument and appear at most once, in this order: blur(Npx), bloom(N), grain(N), scanlines(N), crt(N),
      brightness(N), contrast(N), saturate(N), grayscale(N), sepia(N), invert(N), hue-rotate(Ndeg).
      blur is Gaussian sigma (up to 64px); bloom is LDR thresholded light spread; grain is noise; scanlines
      darken alternate rows; crt adds curvature and a vignette. Numeric amounts also accept percentages (100% = 1).
      Bloom/brightness/contrast/saturate range from 0 to 4; grain/scanlines/crt/grayscale/sepia/invert from 0 to 1.
      Color functions use 1 for identity brightness/contrast/saturate and 0 for identity grayscale/sepia/invert/hue.
      Bloom's Gaussian spread is fixed at 12 physical pixels. A group's content draws into a shared target the
      size of the paint surface, but its effect passes and composite cover only what it drew plus twice the
      effect's reach (blur: about 3 sigma; bloom: about 40 px), rounded up to 64 px: a small glow costs about its
      own area. Base cost is one FX pass plus compositing; blur adds two passes, bloom three, custom GLSL one, and
      3D projection one; blur/bloom build only the mips they sample. The backdrop is copied only for a blend mode
      (or a `backdrop-filter`/blend nested inside), and a group that drew nothing costs nothing. Custom GLSL, 3D
      projection, a blend inside another group, a group covering most of the surface, and 3D/raw GL content
      inside a group run at full surface size.
  mix-blend-mode: normal | multiply | screen | overlay | add   composites the group's color with pixels
      already behind it. `add` is a J3 extension.
  perspective: Npx; rotate-x: Ndeg; rotate-y: Ndeg   J3 extensions that project a complete UI group as a plane.
      This is a visual projection; input still uses the original box. It is separate from the 2D `transform`.
  Custom GLSL and render textures use the painter API (`j3c doc j3.gfx.paint`), rather than CSS strings:
      `beginLayer()` / `endLayer(pos, size, Effects { ... })` groups draws; `target(w, h)` returns a cached-size
      handle for `beginTarget(handle)` / `endTarget()`, with `targetTexture(handle)` for drawing it again.
      `projectTexture(texture, pos, size, rotateX, rotateY, perspective)` projects a textured plane.
      That direct API accepts translation and positive scale; 2D rotation/reflection is rejected. Capture rotated
      content with `beginLayer` / `endLayer` instead; retained projected groups can include a 2D transform.
      Target texture pixels are vertically flipped for ordinary `image`; use `projectTexture` with zero rotations
      and distance for an upright flat view. Target textures are borrowed until their target is destroyed.
      `shader(source)` wraps GLSL 330 source defining `vec4 effect(vec2 uv, vec4 color)`; use its handle in
      `Effects.shader`. `color` is straight RGBA; `uTex` stores premultiplied RGBA. UV and `uResolution` span
      the whole captured paint surface, rather than the group's box. `uTime` comes from `Effects.time`. `destroyTarget` and
      `destroyShader` release resources explicitly; the renderer also releases them when its GL context closes.
      `shader` returns zero and prints the GL diagnostic on failure, preserving existing shader handles.
      Targets accept 1..8192 px per side; the 64-surface pool includes internal layer/projection targets. There
      are 32 custom shader slots, 16 total captures and eight nested effect groups. A projected CSS box with
      its own mask/clip-path uses two group slots. A group holds its whole stacking context: descendants with a
      `z-index`, positioned or not, are inside their ancestor's group. Own overflow clips children before filtering;
      blur/bloom can spread outside that box, while ancestor clips/masks constrain the final result.
      Layout and hit testing stay unchanged. Effects are GL-only and use RGBA8 color.
Text
  white-space: normal nowrap pre-line pre pre-wrap
  overflow-wrap (word-wrap): normal break-word anywhere
  text-overflow: clip ellipsis             (the ellipsis needs overflow-y/overflow other than visible, as in a browser)
  pre/pre-wrap preserve spaces and newlines (including an empty final line); tabs advance to eight-space stops.
  pre does not wrap; pre-wrap wraps while preserving spaces. normal/nowrap collapse whitespace; pre-line keeps newlines.
  text-align: left right center justify   justify stretches interword spaces on soft-wrapped lines, not final/explicit-newline lines
  line-clamp: none | <positive integer>    limits a block's inline content (text and spans together) to N lines and natural height; truncated text gets …
                                           no -webkit-line-clamp/display:-webkit-box syntax; block children are separate blocks; inputs and textareas ignore white-space
  text-decoration: none | underline | line-through | overline   inherited (a browser propagates it instead; a child can switch it off here); also `text-decoration-line`.
                                           A bar the width of each text run in the text color; color, style and thickness words are accepted and ignored
  font-style: normal | italic | oblique    inherited; faux italic (the text is sheared 0.2 about its baseline), there is no italic face
  font-variant-numeric: normal | tabular-nums | proportional-nums   inherited; tabular-nums gives every ASCII digit the widest digit's advance, centred in
                                           it, so counters and timers do not jitter (measure, paint, caret, selection, ellipsis and text-wave all agree).
                                           One keyword; lining-nums, oldstyle-nums and slashed-zero parse and do nothing. Non-ASCII digits keep their
                                           advance, and shaped (HarfBuzz) text is only changed at ASCII digits. Digits in a line with non-ASCII text (or in a
                                           fixed-size bitmap font) are drawn one by one, so shaping and kerning do not span a digit
  letter-spacing: normal | <length>        inherited; px, em or rem, may be negative. Extra advance after every code point of the run, the last included, as a
                                           browser adds it; measure, paint, caret, selection, hit test, ellipsis, line-clamp, justify and text-wave agree.
                                           Transitions and animates like any length (`normal` is 0px; the text re-measures each frame while it runs). Not applied to shaped runs (complex scripts, right-to-left): a browser does not space them either.
                                           Non-ASCII text outside the glyph atlas is drawn one code point at a time while it is on
  word-spacing: normal | <length>          inherited; extra advance on each U+0020 (not no-break space), on top of letter-spacing; justify stretches the gaps further
  text-transform: none | uppercase | lowercase | capitalize   inherited; ASCII and the Latin-1 letters only (same-length UTF-8: no German ss, no Greek or Turkish
                                           rules, no case mapping beyond U+00FF: ß, ÿ, Ÿ, µ, ǆ and every other script pass through byte for byte). capitalize raises the first letter after a non-word byte (apostrophes and
                                           digits stay in the word), per text node, so `<b>h</b>ello` capitalizes both parts. Applies to static text only,
                                           not `<input>`/`<textarea>` values, placeholders or `<option>` labels. Ctrl+C of a static selection copies the text as
                                           written (Firefox; Chrome copies the transformed text). The transformed copy is made once per text or style change
  text-indent: <length> | <percentage>     inherited; the first line of a block's inline content starts that far in (% of the block's content width), shortening
                                           that line's break; negative hangs out. Not on inline boxes (only the block's own value counts); there is no
                                           `hanging` / `each-line`. Each inline run between block children is its own first line
  vertical-align: baseline | top | bottom | middle | text-top | text-bottom | sub | super | <length> | <percentage>
                                           not inherited; inline elements (`span`, `::before`/`::after`, ...) and atomic inline boxes (`img`, `button`, `input`,
                                           `canvas`, ...). A length raises (negative lowers) by that many px, a % of the element's own line-height; `sub` /
                                           `super` lower / raise by a fifth / a third of the parent's font size + 1px (Blink); `middle` puts the box's
                                           midpoint a quarter of the parent's font size above its baseline; `text-top` / `text-bottom` line the box up
                                           with the parent's ascent / descent. The line box grows by whatever sticks out, like a browser. `top` /
                                           `bottom` sit against the top / bottom of the line box after the rest is placed and grow it when taller; the outermost `top` /
                                           `bottom` element is one box (its parts of different sizes keep one baseline, a nested raise moves within it).
                                           A shifted element's text, background box, text selection and hover follow it. Snaps in a
                                           transition (no blending). Not on table cells (there are none). An atomic inline box is one of those tags:
                                           `display: inline-block` is not a value here (the parser warns `invalid value 'inline-block' for 'display'`); a bare number
                                           such as `vertical-align: 5` warns the same way and is dropped
  box-decoration-break: slice | clone     not inherited; what a wrapped inline element's line pieces get. `slice` (default) splits one box over the lines;
                                           `clone` gives every piece its own padding, border, radius, shadow and background (see Inline boxes below)
  visibility: visible hidden              inherited; hidden retains layout, suppresses own paint/input/focus/selection;
                                           an explicitly visible descendant can draw and receive input
  text-shadow: x y blur color, ...         several, first on top; blur 0 is one copy, a glow is 0 0 blur color
  -webkit-text-stroke: width color         an outline grown outward by half the width, under the fill
  background-clip: text (-webkit-)         a linear-gradient (or plain color) background shows through the glyphs;
                                           put it on the block that holds the text, with `color: transparent`
  Animated text is plain J3 in the markup: `{typewriter(s, t, 20.0)}`, `words`, `blink` (`j3.ui.textanim`).
  text-wave: <amplitude> <length> <period>   not CSS: wave text. Each glyph is lifted by amplitude * sin(tau * (t / period
                                           - x / length)), so a crest runs right; `text-wave: 6px 90px 1.4s`, `none` turns it
                                           off (longhands text-wave-amplitude, -length, -period; inherited, so set it on the
                                           block). Shadows, outline and gradient fill follow each glyph. One draw per glyph:
                                           ASCII measures from a table, other glyphs call TTF_SizeUTF8 every frame, so use it
                                           on labels (`j3.ui.textwave`).
  font-family: "Ui", "Symbols", sans-serif   a list is a per-glyph fallback chain: the first listed `@font-face` family is the font, a code
                                           point it lacks is drawn (and measured) by the next listed family that has it (up to 4
                                           fallbacks; names with no `@font-face` are skipped; marks, ZWJ and variation selectors stay with
                                           the glyph before). The chain belongs to the primary font at that size, so every use of the
                                           primary shares it. Needs SDL_ttf 2.0.18+, else the first family draws everything.
  Complex scripts: with SDL_ttf 2.20+ built with HarfBuzz (`j3.game.textShaping()`) Arabic, Hebrew, Devanagari, Bengali, Thai, Tamil and
                                           Telugu runs are shaped (joined forms, reordering) and Arabic/Hebrew text is reordered for display
                                           by a UAX #9 subset (one paragraph per text, base direction = first strong letter). Implemented: W1
                                           (Arabic and Hebrew marks), W2-W7 (so `12`, `10:30`, `3,5`, `1.000,50`, `1/2`, `50%`, `$20` stay together
                                           as numbers; Arabic-Indic digits are AN), N1/N2, I1/I2, L2. Not implemented: explicit embeddings,
                                           overrides and isolates (X rules), bracket pairing (N0), LRM/RLM/ALM as strong characters; there is no
                                           `direction` or `unicode-bidi` property. Text with no right-to-left or Arabic-digit character skips the
                                           resolution. More than 1024 runs in one text warns once on stderr and lays the tail out as one run.
                                           Measure and draw use the same
                                           runs. Without HarfBuzz text is laid out glyph by glyph in logical order. Text boxes shape and
                                           reorder too, and the caret, click hit test and selection follow the drawn order: the caret is at the
                                           visual x of its byte (inside an RTL run x falls as the index grows), a click picks the nearest
                                           boundary of the run under the pointer, and a selection is one rectangle per run it crosses
                                           (adjacent ones merged). A caret between a left-to-right and a right-to-left run belongs to the
                                           run of the character before it (index 0: the first run). Left/Right, Backspace and Delete stay
                                           logical (one character toward the text start or end), not visual. A rich textarea reorders inside
                                           each span segment only (a selection is one rectangle per run per segment). Ligature
                                           pairs (Arabic lam-alef) have one glyph, so the caret between them is approximate. The font must have the glyphs (list one in `font-family`).
  Color emoji: put a color bitmap font (Noto Color Emoji) in the `font-family` chain. SDL_ttf renders it in full color at its fixed
                                           strike (109 px); the runtime shrinks that to the font size (line height = `font-size`, advance
                                           about 1.06 em), centered on the first font's baseline, so a line with emoji is as tall as one
                                           without. Scaled once when the text is cached. GL only; no emoji sprite atlas is bundled.
  user-select: text                        opt-in selection of static text (inherited; `auto` and `none` are not selectable): press and drag across
                                           text nodes, Ctrl+C copies (blocks on separate lines). A press on a button, input or select never
                                           starts one; a press elsewhere clears it. Highlight uses accent-color. Double-click selects the word (a run of letters/digits/_, of
                                           punctuation, or of spaces: the text box rule, within one text node), triple-click the block, Shift+click extends from
                                           the last click or selection, Ctrl+A (no text box focused) selects all selectable text (only the top modal's when one is open).
                                           No Shift+arrow extend (`j3.ui.textsel`).
Boxes
  margin: auto                             left/right: a block with a width centers, `margin-left: auto` pushes it right; flex: auto margins on either axis take
                                           the free space (before `justify-content`, and an item with a cross-axis auto margin is not stretched), so a flex item
                                           with `margin: auto` is centered both ways; an absolute box with `left`+`right` (or `top`+`bottom`) centers. Block top/bottom
                                           auto is 0; a grid item with an auto margin is not stretched and takes the free space of its area on that axis
  aspect-ratio: auto | <w> | <w> / <h>     border-box ratio; an auto height follows the width, an auto width follows a length height. Not inherited
  outline: <width> <solid|dashed|dotted|none> <color>   (also outline-width, -style, -color, -offset) a ring outside the border box that takes no layout space and follows the
                                           border radius; painted under the box's children, `outline-offset` may be negative. Initially none
  cursor: auto default pointer text crosshair move not-allowed grab grabbing ew-resize ns-resize col-resize row-resize wait progress help
                                           inherited; the pointer's node picks the system cursor each frame, so `button:hover` / `:disabled` rules work;
                                           `auto`/`default`/`help` leave the arrow (or the text cursor over a field) alone; grab and grabbing show the hand
Motion
  transition: <prop|all> <time> [<easing>] [<delay>], ...   (up to four). Animated: colors, opacity, flex,
      transform, box-shadow, gradients, and px lengths (width, height, margin, padding, left/top, gap, border width, radius);
      length to auto or % jumps. Layout properties re-lay out each frame while they move.
      Also registered custom properties (`@property`, see Variables): `transition: --angle 1s` or `all` blends a typed `--angle`, and every
      `var(--angle)` reader follows each frame.
  transform: 2D functions applied left to right: translate(x[, y]) translateX translateY (px or % of the element's own box),
      rotate(deg|rad|turn|grad), scale(x[, y]) scaleX scaleY, skew(ax[, ay]) skewX skewY, matrix(a, b, c, d, e, f); at most 8
  transform-origin: <x> [<y>]   (left|center|right|top|bottom, px or %; default 50% 50%)
  translate: x [y]   rotate: angle   scale: x [y]   (apply before `transform`, in that order; they transition and animate like `transform`)
      `transform`, `translate`, `rotate` and `scale` transition and animate: a transition or @keyframes blends two transform lists function by function when they have the same functions in the same
      order, else as matrices (translate, rotation, scale, shear); a translate between px and % blends as px plus % of the box. A box whose matrix cannot be
      inverted (`scale(0)`) is not hittable.
  easing: ease linear ease-in ease-out ease-in-out cubic-bezier(a,b,c,d) steps(n[, start|end]) step-start step-end,
      and not CSS: bounce, spring, spring(damping, cycles)
  animation: <name> <duration> <easing> <delay> <count|infinite> <direction> <fill-mode>, ...  (up to 4, comma separated; also the
      longhands animation-name -duration -timing-function -delay -iteration-count -direction -fill-mode, each a comma list that
      repeats from its start when shorter than the names). Animations of other properties run together; for a property
      several animate, the later one in the list wins (no additive composition, and only while it is running or filling).
      Each keeps its own timeline: it restarts only when its name leaves and re-enters the list, so adding or removing a
      neighbour restarts nothing. More than 4 are ignored with a warning.
  @keyframes name { from {..} 40%, 60% {..} to {..} }   a stop may set animation-timing-function for its span.
      A stop value may use `var(--x)`, also inside a compound value (`transform: translate(var(--dx), 0)`): it is resolved
      against each element's custom properties, once per element set, so `to { background: var(--accent) }` pulses toward
      each element's own accent. A shorthand with `var()` inside is not animated (use the longhand). A stop may also set a registered custom
      property (`@property`, see Variables): `to { --angle: 360deg }`.
      `%` mixes with px for `left`, `top`, `width`, margins, padding and the other lengths layout resolves against the
      containing block (the value is a `calc()` that layout resolves each frame), and a `translate` mixes px and % of the box.
  :focus (a press focuses the button or input under it; `dom.focusNode(id)`), :hover, :active
  :focus-visible: the focus came from the keyboard (Tab, arrows in menus, `focusNode` after a key) or is on a text box or textarea; a
      mouse press on a button, checkbox, range or select does not show it (only text boxes and textareas always do), until a key is pressed while it has focus. Put the ring on `:focus-visible`, not `:focus`.
      `d.setKeyboard(true|false)` sets the input modality (keys set it, a press clears it; a gamepad layer calls it), `d.focusVisible()` reads it.
  :focus-within: the element or any descendant has focus (a popover or select list inside it counts); only the old and new
      focus's parent chains restyle, so it is cheap on a deep tree.
  :dragging (the source of an item drag), :drop-target (the `on:drop` element under it): not browser pseudo-classes, see `j3c doc j3ui`
  ripple: <color> [<time>]   not CSS: a click wave from the press point, clipped to the box
  and `transition:fade|fly|slide|scale`, `in:`, `out:` on an {#if} child in markup.
  Gradients animate too (`background-image`, `border-image` with a gradient, `mask-image`), in `transition` and `@keyframes`
      (J3 extension; browsers jump). A list with a `url()` layer never blends (it jumps). Two lists blend when they have the same number of layers of the same types
      (`linear` to `linear`, `conic` to `conic`). Then angle (a conic's `from`), every stop position and every stop colour blend
      (colours premultiplied). With different stop counts the shorter list is padded with copies of its last stop, so new stops
      grow out of the end. Anything else (a linear to a radial, one layer to two, a gradient to `none` or a url) jumps, at the
      start of a transition and halfway through a keyframe span. A sheen is a keyframe moving the stops, a spinner a conic
      `from 0deg` to `from 360deg` (`border-image: conic-gradient(from 0deg, ..) 1` on a rounded box is a spinning border).
      Keep stop positions in 0..100%. Cost: the blend writes into per-element scratch (no parsing, no allocation per frame); an
      angle change reuses the baked colour ramp, but moving stops or colours bakes one ramp per element per frame and ends the
      current draw batch, so animate stops on tens of elements, not hundreds (a ramp is 256 texels; the 64-row ramp cache is
      shared). Off screen elements pay the blend, not the bake (boxes outside a clip are not painted).
  Borders and shadows: `border-color` and `box-shadow` (also lists of several) animate in `transition` and `@keyframes`, so a
      glow pulse is `@keyframes { from { box-shadow: 0 0 4px #a6e3a1 } to { box-shadow: 0 0 22px 5px #a6e3a1 } }` alternating.

Preferences (dark mode, reduced motion)
  A settings toggle calls the setters, the stylesheet reacts with `@media`. In ui/theme.css (listed as `theme` in j3.toml):
      :root { --bg: #f0f0f0; --fg: #202020; }
      @media (prefers-color-scheme: dark) { :root { --bg: #181820; --fg: #e8e8f0; } }
      @media (prefers-reduced-motion: reduce) { .spinner, .pulse { animation: none; } }
  In the game (a `bool` from the saved settings; `dom` is the component's `root.dom`):
      root.dom.setColorScheme(settings.dark);
      root.dom.setReducedMotion(settings.reduceMotion);
  Each call that changes the answer re-tests the `@media` rules and restyles once; idle frames stay free. `speculative/39_ui_prefs` is a golden fixture.

Lint (`j3c check`)
`j3c check <project>` also reads the project's theme.css and every component `<style>` with the real CSS parser (a small cached program, a few ms,
no window) and prints `warning: file:line:col: message [rule]` lines: the parse warnings the app prints at startup, then:
  never-matches              a scoped rule whose last compound (tag, classes, `#id`) matches no element in its component's markup. `<slot>` content
                             is the user's markup; a class passed to a child (`<Kid class="x">`) counts for the parent's rules that name a class;
                             `class="r{n}"` is a pattern; a fully dynamic `class="{x}"` (or `id={x}`) can be anything, so it silences the rule for
                             elements it could style; `:root`, `.j3-*` (popups, tooltips) and `.selected` are never judged; theme.css is not judged.
                             Ancestors in the selector are not checked (they may belong to a parent component).
  ellipsis-without-overflow  `text-overflow` and no `overflow` on the element (no rule that can match it sets one)
  offset-without-position    `top` `left` `right` `bottom` `inset` and no `position` (same test)
  duplicate-decl             one property written twice in a rule (`padding` then `padding-left` is fine)
  undefined-var              `var(--x)` with no fallback and no `--x` declared or `@property` anywhere in the sheets (`--j3-*` are built in)
  broad-has                  `:has()` on a compound with no class or id (`*:has(...)`, `div:has(...)`): every such element is a carrier
  has-before-combinator      `:has()` outside the last compound (`.a:has(.b) .c`): a change restyles the carrier's whole subtree
  style-revert               `style:prop="revert"` (acts as `unset`)
Exit code is unchanged (`j3c check --strict` fails on any warning); `--json` adds a `css` array. Silence one: `/* j3-lint-ignore never-matches */` on the line
before (or the same line; no rule name = every rule). Silence in `j3.toml`: `[ui] lint = false` or `lint-off = ["never-matches", ...]`.

Differences from a browser (design the CSS around these)
- Images: `border-image` slices, width and outset follow CSS, but a `border-image` box takes its layout from `border-width` (not from `border-image-width`), `background-repeat: space` with `background-position` keeps the first tile at the position when one does not fit twice, a gradient layer ignores size, position and repeat, and `background-origin` and `-attachment` are not parsed (the shorthand takes no clip box either). Tile edges snap to whole pixels (no hairline seams). `image-rendering` knows `auto` and `pixelated` only.
- Gamepad focus navigation reads untransformed layout boxes (`absX/absY/w/h`): `transform`, `translate` and `animate:flip` / `transition:fly` offsets are paint only and ignored, so move a thing by layout, not by transform, if the pad must find it where it looks.
- `revert-layer` acts as `revert` (there are no cascade layers); `revert` in `style:` or a `var()` fallback acts as `unset`. A CSS-wide keyword is a whole value (`padding: 1px inherit` warns and is ignored), as in a browser.
- A gamepad move shows `:focus-visible` and a mouse move hides it again (a browser keeps the ring); keyboard focus keeps the browser rule. Pad navigation is geometric, not DOM order (see Gamepad above).
- `prefers-color-scheme` and `prefers-reduced-motion` are answered by the program, never the OS: `dom.setColorScheme(true)` for dark (default light) and
  `dom.setReducedMotion(true)` (default no-preference); `J3_COLOR_SCHEME=dark` / `J3_REDUCED_MOTION=1` set the start value for tests. A change restyles once. Reduced motion also stops
  the built-in motion that is not CSS: `animate:flip` (rows jump), `transition:` / `in:` / `out:` (instant, no leave ghost), `ripple`, and `scroll-behavior: smooth` wheel inertia.
  CSS `transition` and `animation` are NOT stopped, as in a browser: switch them off yourself under `@media (prefers-reduced-motion: reduce)`.
- `(pointer: coarse)` and `(hover: none)` hold together on a touch screen: on by default on Android, off elsewhere; `dom.setTouch(bool)` changes it
  and `J3_POINTER=coarse|fine` sets the start value. It follows the platform, not the last input device used. No `any-pointer` / `any-hover`.
- `letter-spacing` / `word-spacing` / `text-transform` / `text-indent` are in the Text list above: spacing skips shaped (complex-script, right-to-left) runs, `text-transform` covers ASCII and Latin-1 only and not editable text, and copy gives the untransformed text.
- `transition:fly` and `scale` offset or scale the paint like `animate:flip`, so `absX`/`absY` ignore them; `slide` is real layout (in flow) only for block and flex parents. A leaving element keeps its place in layout until it is gone, so siblings close up at the end (except after a `slide`, which closes them gradually).
- `animate:flip` is Svelte's, on a keyed row: it offsets the paint (and hit test) like a `translate`, not a CSS transition, so `transition`/`transform` on the row still work beside it; `virtual` lists are not supported; `absX`/`absY` (popover anchors) ignore the flight offset.
- Whitespace between two elements is dropped (`<glyph /> <glyph />` has no gap, `<b>a</b> <i>b</i>` neither): put text between them (`/`) or write `&nbsp;`; a space inside text next to an element is kept.
- `<glyph>` is not HTML: it is the runtime's input prompt (keycap or pad button drawn from a label table, no image files), chosen by the last-used device; `[type=key]` / `[type=pad]` are its styling hooks.
- Drag and drop is the game's own: `draggable` + `on:drag*` / `on:drop` with a `drag-data` / `drag-id` payload and `:dragging` / `:drop-target` instead of HTML5 `DataTransfer`, `dragover` default prevention or `dropzone`; the drag ghost is a translucent copy of the source, not the browser's drag image.
- `<progress>`, `<meter>`, `<details>`, `<summary>` and `<br>` are the small set above, not the browser's: no pseudo-elements for the fill (it uses `accent-color` or the `--j3-meter-*` custom properties, and the element's `background` is the track), no `<meter>` color keywords, no `[open]` attribute selector (use `:open`), no `name` accordion grouping, no disclosure triangle, and a closed `<details>` hides by `display: none` (no `content-visibility`, no find-in-page).
- `:has()` takes only simple relative arguments (one compound after `>`, `+`, `~` or nothing, comma lists; no nesting, no `.a .b`, not inside `:not()`), and a
  change that could flip it restyles the carrying element's whole subtree, not just the element; a `:has()` with a descendant argument on a big, busy root costs a restyle of everything below it.
- Sizing limits apply to block, atomic inline, flex, grid and positioned boxes; min wins a conflicting max.
  Limits follow box-sizing. Percent heights require a definite containing
  height (grid rows follow the grid's definite height only with `fr` or `%` tracks); ordinary inline spans ignore sizes. Flex redistributes space after clamping.
- No browser default styles: every margin and size is yours. Reset in theme.css: `* { margin: 0; padding: 0; box-sizing: border-box; }`. The only built-in styles are the few in the UA sheet (`j3.ui.uastyle`): `button` centres its text, `strong`/`b` are bold, `em`/`i` italic, `pre` keeps whitespace, `hr` is a 1px rule, `a` has a pointer cursor, `input[type=radio]` is round (`border-radius: 50%`, for its focus ring). Any author rule overrides them, a theme's `*` rule included (the tag defaults rank at specificity 0, so source order decides); a value set on a parent is inherited as usual (`pre` reaches a nested `code`).
- No margin collapsing: adjacent block margins add.
- `vertical-align`: `middle` takes the x-height as half the parent's font size (not the font's real one), `text-top` /
  `text-bottom` use the line-height box rather than the content area, and the property does nothing on blocks, flex or grid items (no table cells either).
- No `<table>`/`<tr>`/`<td>` layout: shared columns are `display: grid` plus `display: contents` rows (above).
- Inline boxes (`span`, `small`, `::before`/`::after`; any `display: inline` element) take padding, border, margin and background,
  one box per line when the element wraps (`box-decoration-break: slice`, the default: the left padding, border and rounded corners only on the first
  piece, the right ones only on the last; `clone`: every piece gets the full padding, border, radius, shadow and its own background, gradient or image
  layers included, and the cloned edges make each piece wider, so lines break earlier. Not inherited; no transition, it snaps; the minimum content width counts one cloned word). Horizontal padding, border and margin take room in the line; vertical padding and border
  paint over the neighbouring lines and never change the line height, as in a browser. A box is hit-testable and `:hover`-able over its
  padding (innermost first), also where it sticks out of its block (vertical padding above the first line). The closing edge
  wraps with the last word (word + edge must fit). Not done: a margin at a line break,
  a box over mixed left-to-right / right-to-left runs is one rectangle from its first to its last fragment on the line (fragments keep their
  logical order; padding sits at the logical start and end), and `background-clip: text`, `opacity` and `transform` on the span.
- `margin: auto` centers a block with a width, flex, grid and absolute boxes.
- Grid: only `grid-template-areas` names lines (`name-start` / `name-end`; no `[line-names]`), an unknown area name is `auto` (a browser makes an implicit line from it), `justify-self` / `align-self` `normal` and `self-start` are not parsed, and an `auto` track does not stretch to a taller grid (use `1fr`, or `justify-content` / `align-content` to place it).
- `button` centres its text both ways (one of the built-in styles above). Checkbox, radio and range inputs are
  16x16, 16x16 and 129x16, a color input 44x24, other inputs and `select` 160x28, unless the CSS sets a size.
  Text, search, color and select inputs have no look of their own: give them a background, border and padding.
- CSS nesting expands to flat rules at parse time (see Nesting above): specificity is per joined selector, not `:is()` of the parent list, and `&` after other parts of a selector needs a one-compound parent.
- Not supported: `!important`, floats, `@layer`, container queries, `<canvas>` children
  other than as an overlay (`position: absolute`).
- Text is SDL_ttf at whole pixels (no kerning or subpixel positions) and bold is synthetic, so text
  differs a little from Chrome; boxes and colours match to 1 px.
- High DPI: layout, `windowSize()` and the mouse are in logical px; drawing is scaled by `game.dpiScale()` and text is
  re-rasterised at the physical size (a second font per face, opened lazily). `app.hiDpi()` (before `run`, with `useGl`) asks
  SDL for the larger drawable and follows a move to another display; `J3_UI_SCALE=1.5` forces a scale (headless runs too:
  the window and screenshot keep their pixels, the layout gets smaller). Letterboxed `setVirtualSize` already fits the physical
  window. The glyph advances come from the physical font, so a line can differ from its 1x width by a pixel or two.
  3D code that needs real pixels uses `game.drawSize()`; offscreen paint targets (`p2SurfaceBegin`) stay unscaled.
- Scrolling and overlays: scroll snap is the subset above (no `scroll-snap-type` on the root page, no keyboard-driven snapping, no `scroll-snap-align` through `transform`), no `overscroll` bounce, no scroll anchoring outside `virtual` lists; the wheel is one number (Shift picks the x axis); a fixed box is not clipped by an ancestor but an
  ancestor's `opacity` still applies; a popover keeps the place it had when it was laid out while the page scrolls under
  it (it is not re-anchored). The paint order of each stacking context is a sorted list built when the tree, a computed style or the
  style sheets change (an idle frame only compares a fingerprint of the tree): painting walks those lists, so a page with many
  `z-index` values costs about what one without. Differences from a browser: a
  running transition of a context-making property makes one (see `z-index` above); a positioned box inside an `overflow` clip is clipped
  even when its containing block is outside it.
- Hit testing follows `transform`; a click on a `<label>`'s text activates its control (below), and the label's own `on:click` still fires once. Rotation and skew turn shapes, text and images
  (text is drawn at its normal size and then transformed, so a big scale is soft). An `overflow` clip under a rotation or skew is the
  turned box itself (a scissor on its bounding box plus an antialiased test in the shader), intersected with every outer clip; a clip on
  one axis (`overflow-x` alone) cuts only the two turned side edges. At most four such turned clips nest (a `filter`/`opacity` group
  counts the clips outside it too): the fifth falls back to its bounding box and prints one warning to stderr. Quarter turns and flips
  stay plain scissors. A rounded `border-radius` does not round an `overflow` clip, turned or not. A `<canvas>` inside a rotated box is not rotated.
  A reversed transition restarts at full duration (no shortening); more than 16 properties in flight on one
  element snap; an animated custom property rebuilds the styles that read it from their rules each frame (`style:` overrides still win; a custom property declared on the same element as a whole `var()`, `--b: var(--a)`, follows, one declared on an ancestor keeps its value); values of `grid-template-*` and other list properties, and `var()` in another keyframe stop, do not follow it, and `@property` syntaxes with several types (`<length> | <color>`, `<integer>`) and `<image>` are not supported; `%` mixes with px in `@keyframes` only (a `transition` between px and `%` jumps, except translate), and not for border widths, radii or `font-size`; `canvas` mouse coordinates ignore transforms.
- Text effects: shadows, glows and outlines draw copies of the text (a blur is about 70 copies at most), so keep them
  to headings and labels. Outlines cannot be hollow (the fill is not erased). `background-clip: text` takes the
  first linear gradient only (radial and conic fall back to the plain background color). Color emoji need a color
  font drawn at a fixed strike SDL_ttf does not scale: use a monochrome symbol font.
- Colours: `rgb()` and `hsl()` take commas or the space form (`rgb(255 0 0 / 50%)`, `rgb(100% 0% 0%)`); names are
  case-insensitive. `color-mix(in srgb, A 30%, B)` follows CSS Color 5 (a missing percentage is 100 minus the other,
  a sum under 100 scales the alpha, premultiplied) but only the `srgb` space (no `oklab`, `srgb-linear`, hue methods). `hsl()` takes a bare number for saturation and lightness too (browsers want `%`; we accept both).
- Gradients: stop colours may be `var()` or `color-mix()` (see Variables); at most 16 stops and 64 distinct
  gradients per program; radial is a centred farthest-corner ellipse and conic is centred (size and `at`
  words are skipped); `to top right` is a plain 45deg. A gradient `border-image` follows `border-radius`
  (browsers drop the radius). Dashes restart on each side, so a corner may not land on a dash.
  Border styles (dashed, dotted) are not transitioned; gradients are (Motion, above).

See also `j3c doc j3ui` (markup), `j3c doc testing` (capture a frame, `dumpHtml`).
