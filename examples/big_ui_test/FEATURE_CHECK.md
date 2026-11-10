# Game UI Framework — Visual Feature Checklist

## To Do Next (grouped, do each group together)

### A. Paint primitives (gradients, shadows, borders)
Everything that is "a fancier rect fill"; shares one paint path.
- [x] Linear, radial, conic, and multi-stop gradients (section 5)
- [x] Gradient backgrounds, layered backgrounds, gradient borders
- [x] Soft drop shadows, inner shadows, layered shadows, neon glow (section 6)
- [x] Dashed and dotted borders

### B. Text effects
- [x] Text wrapping, truncation, alignment
  (`white-space: nowrap | pre | pre-wrap | pre-line`, `overflow-wrap: break-word`, `text-overflow: ellipsis`, `line-clamp`, left/center/right/`justify` (soft-wrapped lines only). All on the Text page.)
- [x] Text shadows, outlines, glow, gradient text
  (`text-shadow`, `-webkit-text-stroke`, `background-clip: text` with a linear gradient. Limits: outlines grow outward and cannot be hollow; radial/conic text gradients fall back to a plain color; blur is about 70 copies of the text, so use it on labels.)
- [x] Animated text (typewriter, reveal)
  (`j3.ui.textanim`: `typewriter`, `words`, `blink`, `cycle` from markup. Not done: per-glyph fade/wipe reveal, wave.)
- [x] Icons, emojis, and Unicode
  (UTF-8 end to end, `&nbsp;`, a second `@font-face` for symbols and monochrome emoji. `font-family: "Ui", "Symbols"` falls back per glyph; Arabic/Hebrew are shaped and reordered (HarfBuzz). Color emoji (Noto Color Emoji as a third `@font-face`, scaled to the font size) at 16 and 32 px, and RTL/mixed text in an input and a textarea, on the Unicode page. Limits: color emoji need the font at its system path (without it glyphs fall back to "Symbols"); GL only.)

### C. Interaction states and motion
Hover/press/focus plus the animation engine they sit on.
- [x] Hover scale, rotation, glow, shadow
- [x] Click ripple, animated focus indicators
- [x] Slide, scale, rotation animations; custom easing; spring/bounce
- [x] Keyframe and staggered animations; interruptible/reversible
- [x] Animated layout changes
  (CSS `transition`/`@keyframes` over transform, opacity, shadow, colors and px lengths; `:focus`, `ripple:`, `anim-delay`, `bounce`/`spring` easings. Limits: fixed transform order, clips and canvases are not rotated, ripple is clipped to the box rect not its rounded corners, reversed transitions restart at full duration, no Tab key focus, `localMouse` ignores transforms. Tested: `j3c/stdlib/tests/motion_test.j3`, Hover & Motion page with `UI_MOUSE=x,y[,1]`.)

### D. Input widgets
- [x] Text boxes and search fields (cursor, selection)
  (`<input type="text|search" bind:value placeholder on:input on:submit>`: caret, drag and Shift+arrow selection, Home/End, Ctrl+arrows, Ctrl+A/C/X/V, key repeat, UTF-8 safe, long text only measures what is shown; search has a clear button and Escape clears. Limits: single line (no textarea), no double-click word select, no password mask, no maxlength, no IME, clipboard copy cut at 1023 bytes by `game.setClipboard`.)
- [x] Sliders and range sliders
  (`<input type="range" min max step bind:value>`, drag, arrow keys (Shift = 10 steps), Home/End; `bind:high` makes it a two-thumb range slider (non-HTML). Limits: horizontal only, no tick marks.)
- [x] Dropdown and multi-select menus
  (`<select bind:value>` with `<option value disabled>`, and `multiple` (bound string is the comma list in pick order). Popup is drawn last (`j3.ui.popup`, `j3.ui.dropdown`), flips above near the window bottom, 8 rows then scrolls (wheel, arrows, Home/End), Enter/Space/Esc. Limits: popup not CSS-styled, no type-ahead, option value "" falls back to its text, option values are kept as views, no `<optgroup>`.)
- [x] Color pickers
  (`<input type="color" bind:value>` binds `#rrggbb`; swatch plus popup with saturation/value square, hue bar, preview and hex. Limits: no alpha, no hex typing or eyedropper, no keyboard control inside the panel.)
- Also done with D: Tab / Shift+Tab focus order over buttons, inputs and selects; Enter/Space click a focused button or checkbox. Tested: `j3c/stdlib/tests/input_test.j3` (113 checks incl. synthetic mouse/keys through the whole dom path), Buttons & Inputs page with `UI_SIM="6:m=x,y,1;...;14:t=text;16:k=S+Left"` (`src/sim.j3`). Not checked on a real keyboard/window (headless only).

### E. Scrolling and overlays
- [x] Vertical/horizontal scrolling, custom scrollbars, inertia
  (`overflow` / `-x` / `-y` auto|scroll|hidden; wheel, Shift+wheel for x, wheel chaining to the parent box and `overscroll-behavior: contain`; overlay scrollbars you can drag, track paging, `scrollbar-width: auto|thin|none`, `scrollbar-color: thumb track`; `scroll-behavior: smooth` is wheel inertia; boxes far outside a clip box are not painted or hit, so a 4000-row list stays cheap. Limits: either axis not visible clips both, scrollbars overlay the content (take no layout space), no scroll-snap, no touch drag or flick, no keyboard paging, no `overscroll` bounce.)
- [x] Popups, modals, tooltips, hover cards
  (`<dialog modal on:close>`: centered, `backdrop-color` dim, input block, focus in/out, Tab trap, Escape; `popover` elements: light dismiss, trigger press, Escape on the top layer only, nested above a modal; `title="..."` tooltip after 500 ms; hover cards are CSS `.host:hover > .card` (new child combinator). Limits: the app owns the open flag (render with `{#if}`), the tooltip is one unstyled line, a popover is not re-anchored when its page scrolls under it until the next layout, no `<dialog open>` attribute.)
- [x] Slide-out drawers, toasts
  (`position: fixed` boxes painted past ancestor clips; the drawer is a `transition` on `left` with a `pointer-events: none` scrim; toasts are `j3.ui.toast.Toasts` slots rendered in a fixed column with `@keyframes` in and `transition:fade` out. Limits: one flat z-order, a toast leaving does not slide the others up, no swipe to dismiss.)
- [x] Context menus, nested dropdowns
  (`on:contextmenu` with the pointer in window px, `left`/`top` attributes, `position-try-fallbacks: flip-inline|flip-block|flip-both` plus an edge clamp; submenus are `.item:hover > .sub` with a flip. Limits: no keyboard navigation inside menus, no hover-intent delay, `position-try-fallbacks` takes one keyword.)
- Tested: `j3c/stdlib/tests/overlay_test.j3` (117 checks: scroll math, toasts, wheel/chaining/inertia/thumb drag, modal/popover/Escape/focus/Tab, placement, tooltips, removal and resize while open), Scrolling and Overlays pages with `UI_PAGE=8` / `10` and `UI_SIM="5:m=x,y,0;7:m=x,y,1;9:m=x,y,0;12:w=3;14:m=x,y,2"` (`w=n` wheel notches down, third `m` value 2 is the right button; `src/sim.j3`). Not checked on a real window or touch device (headless only).

### F. Layout and panels
- [x] Responsive sizing and alignment
  (`@media screen and (min-width: Npx) and (max-height: Npx) { .. }` px bounds joined by `and`, re-tested only when the window size changes and restyled only when a condition flips (changes transition like any style change); with the existing `%`, `min-`/`max-` sizes, flex `align-*`/`justify-*` and `grid repeat(n, 1fr)`. Layout page, `UI_SIZE=800x560` starts a headless run at another size and `UI_SIM="20:s=600,500"` resizes live. Limits: no `flex-wrap`, no `vw`/`vh`/`clamp()`/`min()`/`max()`, no container queries, no `not`/`or`/comma lists or `em` in `@media`.)
- [x] Collapsible sidebars
  (the Shell nav is a `width`/`padding` `transition` on a `collapsed` class, toggled by the `<` button, and compact below 820 px through `@media`; the Panels page has a second one. Limits: a collapsed box clips its content, it does not reflow it; no `<details>`.)
- [x] Resizable and draggable panels, dockable windows
  (`j3.ui.panels`: `<div panel>` (`panel="dock"` to dock) with a `panel-handle` descendant to drag and a `panel-resize` grip; stays inside its workspace, honors CSS `min-width`/`min-height`, follows a shrinking workspace (one frame late), snaps to the left, right or bottom edge with a preview and floats again when its handle is dragged. Panels page. Limits: no bring-to-front (tree order and `z-index`), no top dock or tabbed/split docking, a docked panel resizes along its dock axis only, state is lost when the element is removed, a panel's parent must be its `position: relative` containing block (or the panel `position: fixed`).)
- [x] Sticky headers
  (`position: sticky; top: Npx` inside any clipping ancestor, held by its parent's end; hit testing follows. Limits: `top` only (no `bottom`/`left`/`right`), needs a `z-index` to paint above the rows sliding under it, computed from the previous frame's layout, nested sticky boxes do not stack.)
- Tested: `j3c/stdlib/tests/layout_test.j3` (81 checks: media parse/viewport, sticky math and hit test, panel math, and drag, clamp, dock, undock, resize and window shrink through the whole dom path with synthetic mouse), Panels page with `UI_PAGE=11` and `UI_SIM="6:m=300,140,1;8:m=330,200,1;10:m=500,330,1;12:m=500,330,0"`. Not checked on a real window or touch device (headless only).

### G. Images and shapes
- [x] PNG/JPEG images, cropping, scaling, tinting
  (`<img src>`: natural size, aspect-preserving width/height, `object-fit: fill|contain|cover|none|scale-down`, `object-position`, rounded corners, transforms and opacity; `image-tint` multiplies image color and transitions on hover. `background-image: url()` supports size, position and repeat. Limits: one image background per box, no image atlas; WebP not verified.)
- [x] Nine-slice panels, sprite sheets
  (`border-image: url(...) 12 fill / 12px`, one to four source slices in image px, optional center fill and one uniform px width override; `<img sprite="4,2" frame={n} fps="8">` selects or animates a sheet cell, including reverse playback. Limits: stretched nine-slice edges (no tile/round), no percentage slices, no per-frame durations.)
- [x] Custom polygons, SVG/vector, masks and clipping paths
  (`clip-path: circle()|ellipse()|inset(... round ...)|polygon()` with px/% coordinates and even-odd polygons; `mask-image` uses linear/radial gradient alpha. Inline `<svg viewBox>` with direct `<path>` children supports `M L H V Q T C S A Z` and relative commands, fill/stroke, `currentColor` and holes by winding. Limits: no external SVG files, groups, SVG transforms or SVG gradients; paths flatten to at most 2048 points, polygons to 60 vertices; one active clip per shape slot and one mask (nested pushes replace rather than intersect, four deep); hit testing uses box bounds.)
- Verified 2026-10-20 with a fresh release compiler: `j3c check examples/big_ui_test`; `j3c/stdlib/tests/image_test.j3` (133 checks), `css_test.j3` (96), `dom_test.j3` (176). Inspected headless Images & Shapes captures (`UI_PAGE=12`) at 1100x700 and 1400x1600, plus `UI_SIM="6:m=850,628,1;8:m=850,628,0;10:m=622,480,0"` (next frame, hover tint) and `UI_SIM="5:m=1000,400,0;6:w=14"` (scroll to clips/masks/SVG). No runtime warnings. Not checked on a real window (headless llvmpipe only).

### H. Post effects
- [x] Gaussian and backdrop blur, glassmorphism
  - [x] Reusable offscreen Gaussian blur foundation and rounded backdrop compositing (`paint.backdropBlur`, GL only).
  - [x] `backdrop-filter: none | blur(<nonnegative px>)` on retained UI boxes; Post Effects page (`UI_PAGE=13`) compares translucent and frosted panels, with enable and 4/8/16 px controls.
  - [x] Content Gaussian blur (`filter: blur(3px)`) of a whole UI subtree, alongside the glass comparison.
  Limits: Gaussian sigma is capped at 64 logical px and framebuffer edges clamp. GL only, with reusable RGBA8 targets; backdrop blur requires a single-sample, window-sized destination.
  First-increment verification 2026-10-20: fresh release compiler, project check, `post_test` (62), `css_test` (96), `dom_test` (176), `image_test` (133), and `post_render_test` (pixel contrast, zero radius, rounded edges, masks/subviews, GL state, custom framebuffer, target resize and teardown). Inspected headless `UI_PAGE=13` captures at 1100x700 (8/16 px and disabled) and 640x700 (scrolled); existing Images & Shapes content matches its pre-change capture exactly. Headless NVIDIA only; real-window resizing is unverified (SDL offscreen keeps its original drawable size, so resize coverage uses a resized user framebuffer).
- [x] Bloom, noise/grain, scanlines/CRT
  (`filter: bloom() grain() scanlines() crt()`; Effects & Color tab has individual presets and a combined chain against the original scene. Bloom uses an LDR threshold; CRT is curvature/vignette, scanlines and grain are separate controls.)
- [x] Color filters, blend modes, custom shaders
  (`brightness`, `contrast`, `saturate`, `grayscale`, `sepia`, `invert`, `hue-rotate`; `mix-blend-mode: normal|multiply|screen|overlay|add`. Shader & Texture tab uses a custom GLSL 330 `effect(uv, color)` pass via `paint.shader`, with `uTex`, `uResolution`, `uTime`.)
- [x] Render-to-texture, 3D perspective transforms
  (`paint.target`, `beginTarget`/`endTarget`, `targetTexture`, `projectTexture`; a cached 320x200 board is reused in two views. Direct `projectTexture` supports translation and positive scale; capture rotated/reflected content as a layer instead. Retained groups use J3 extensions `perspective`, `rotate-x`, `rotate-y`. These project UI planes; layout/hit testing remains unprojected.)
- Demo captures: `UI_PAGE=13 UI_POST_TAB=0|1|2`; tab 1 accepts `UI_POST_FX=0..5` (clean/bloom/grain/scanlines/CRT/combined), `UI_POST_COLOR=none|gray|sepia|invert|hue|bright|contrast|saturate`, `UI_POST_BLEND=normal|multiply|screen|overlay|add`; tab 2 accepts `UI_POST_SHADER=0` / `UI_POST_TILT=0` to disable either effect. Default size is 1100x700; `UI_SIZE=640x700` stacks comparisons and scrolls.
- Group limits: filter functions are unique and use the documented fixed order; a descendant in a different flat `z-index` paint layer is outside its ancestor's effect group. Own overflow clips children before filtering, then blur/bloom may spread locally while ancestor clips/masks constrain the composite. Eight nested group slots (a projected box with its own mask/clip-path uses two), 16 total captures, 64 surface handles including internal targets, and 32 shader handles. Targets are 1..8192 px per side and use RGBA8; this is a UI plane renderer, not a 3D depth scene.
- Costs: each group lazily holds four full paint-surface RGBA8 images and copies its backdrop. One FX pass plus composite; blur adds two passes, bloom three, custom shader one, projection one; blur/bloom also build mipmaps. Bloom spread is fixed at 12 physical px. Cache targets/shaders; explicit release is available and context shutdown releases remaining handles.
- Expanded demo verification 2026-10-20: project check/build against disk stdlib; inspected all three tabs headless at 1100x700 and 640x700. Checked individual bloom/grain/scanlines/CRT and combined presets, sepia/grayscale/hue, screen/multiply/add, cached texture reuse, shader and perspective disabled, and scripted blur/radius/content/shader/perspective clicks with scrolling. No runtime CSS warnings. Fresh embedded-compiler and renderer regression checks are recorded separately below when complete.

### I. Game components (build last, on top of A–G)
- [x] Animated health/resource bars, cooldown indicators
- [x] Inventory grid with rarity borders and tooltips
- [x] Floating damage numbers, toasts for quests
- [x] Dialogue box with text effects
- [x] Minimap, world-space nameplates
  (Game UI page, `UI_PAGE=14`; composed from the earlier features, no engine changes. components live in `ui/game/` (the component `game.Bar` is `ui/game/Bar.j3ui`, written with its full path in markup): `game.Bar` (HP/MP/SP and nameplate bars; fill eases and a pale ghost trails a drop, in the script because a `left` attribute is not transitioned; pulses under 25%), `game.Cooldown` (`<canvas>` clock wipe via `paint.fillShape`, seconds left, ready flash), `game.ItemSlot` + `game.Inventory` (100 cells in a scroll box, rarity border and glow, stack count, CSS hover card that flips at window edges, click two cells to swap), `game.Floaters` (pool of 24 number slots, oldest replaced), `game.QuestToast` (`j3.ui.toast.Toasts`, one notice per quest), `game.Dialogue` (typewriter over a transparent copy so wrapping never jumps, whisper and shout effects, click skips mid-line, answers on the last line), `game.Minimap` (rotating canvas map, rim-clamped dots, compass N), `game.Nameplate` + `game.World` (fake camera in `src/gameworld.j3`; plates exist only for entities in front of and on screen), `game.Hud`, `game.Controls`. State is plain J3 in `src/gamedemo.j3` (shared across page visits), data tables in `src/items.j3` / `src/dialogue.j3`, painting in `src/gameart.j3`.
  Limits: scene is a fixed 560x330 px; bar width is 180 or 80 px (dynamic width is not an attribute, so bars slide a clipped fill); nameplates have three size tiers rather than continuous scaling; the damage-number pool restarts no animation when it recycles a live slot; no drag-and-drop between cells, only click-swap; 120 mobs cost about 3.6 ms per frame headless. Headless only.
  Check: `UI_PAGE=14`, `UI_SIZE=1100x1000`; `UI_SIM` hover `10:m=510,530,0` for a tooltip, `m=995,530,0` for the flipped one, clicks on the dialogue box (600,900), "Burst x30" (902,287), "Take 400" (902,191).)

### K. Plan 023 additions
Shown on the Text, Unicode and Layout pages (extended) and page 17 "New in 023" (`UI_PAGE=16`, `ui/pages/Fresh.j3ui`, `ui/kit/Pinger.j3ui`, `src/fresh.j3`). Each section's label names its feature.
- [x] Text CSS: `white-space: pre`/`pre-wrap`, `line-clamp`, `text-align: justify`, `visibility: hidden` with a `visible` child (Text page)
- [x] Units and sizing: `em`, `rem`, `font-size: calc()`, `max-width` / `min-width` (Text page)
- [x] Flex: `flex-basis`, `flex-shrink`, `align-self`, `order`, `row-reverse`, `column-reverse`, `wrap-reverse`, column wrap (Layout page)
- [x] Selectors: `#id`, `+`, `~`, `::before` / `::after` with `content`, including a content swap on click (New in 023)
- [x] Inline boxes: padded, bordered, rounded `span` pills and one highlight wrapping across lines (New in 023)
- [x] Static text selection: `user-select: text` (Text page; the drag, double-click and Ctrl+A need a real pointer, not captured)
- [x] Color emoji at two sizes; RTL and mixed text in `<input>` and `<textarea>` (Unicode page)
- [x] Markup: `{#let}`, `{#each xs as &x}` mutating in place, int keys `(x.id : int)`, `emit` from a script fn (New in 023)
- [x] `<dialog modal autoselect>` with Enter on `button.primary` (New in 023; checked with `UI_SIM`: click Rename..., type, Enter)
- [x] `display: contents` rows sharing `grid-template-columns` tracks, a small table (New in 023)
- Check: `UI_PAGE=0|2|4|16 J3_STEADY=60:120` all report 0 layouts, 0 restyles, 0 arena growth. No golden.toml (this app has no goldens).

### L. Box and selector additions
Page 18 "Box Styling" (`UI_PAGE=17`, `ui/pages/Styling.j3ui`). Screenshot-checked headless; `cursor` needs a real window.
- [x] `margin: auto`: block `0 auto`, flex `margin-left: auto` and `margin: auto`, absolute `inset: 0; margin: auto`
- [x] `aspect-ratio: 16 / 9` and `1`; `outline` solid, dashed, with `outline-offset` and a radius
- [x] `text-decoration` underline / line-through / overline, inherited into a span; `font-style: italic` (faux slant)
- [x] `:not(.active):hover`, `:nth-child(even)`, `:nth-child(3n+1)`, `:last-child`
- [ ] `cursor: pointer` / `grab` / `default` on hover (real window)

### J. Motion polish
Page 16 (`UI_PAGE=15`); loading components in `ui/load/`; engine in `j3.ui.gradmix`, `j3.ui.textwave`, `j3.ui.motion`.
- [x] Wave text (per-glyph offset animation; finishes "Animated text (typewriter, reveal, wave)")
  (`text-wave: <amplitude> <length> <period>`, a J3 extension: each glyph is lifted by `amplitude * sin(tau * (t / period - x / length))`, inherited, `none` turns it off; shadows, outline and gradient fill follow each glyph. One draw per glyph, ASCII advances from a table and other glyphs measure every frame, so use it on labels. Wave runs on the page clock, so runs on one line stay in step. Tested: `motion_test.j3` (`textwave.lift`, shorthand, inheritance).)
- [x] Animated gradients (transition/keyframe gradient stops, angle and position; also a moving/sheen background)
  (`background-image`, `border-image` and `mask-image` gradients blend in `transition` and `@keyframes` through `j3.ui.gradmix`: same layer count and types, then angle (a conic's `from`), stop positions and premultiplied stop colors blend; a shorter stop list is padded with its last stop; anything else jumps (halfway in keyframes). Sheen = keyframes moving the stops, spinner = conic `from 0deg` to `from 360deg`. Browsers do not interpolate gradients, so this is an extension. Limits: stop colors stay literals (no `var()`), keep stop positions in 0..100% (a negative position reads as unset), a radial/conic `at` position does not exist, so "position" is stop position and `background-position` of a gradient is not animated; moving stops or colors bakes one 256-texel ramp per element per frame and ends the draw batch (angle changes reuse the ramp), 64 ramp rows shared; a blend is a few dozen floats per element, with no parsing or allocation per frame (scratch grows once). Conic masks are not painted, so only linear/radial masks animate. Tested: `motion_test.j3` (transition, retarget, mismatched stops, incompatible types, zero duration, no allocation per frame, keyframes).)
- [x] Animated borders and shadows (keyframed border-color/box-shadow/glow pulses; a rotating conic-gradient border)
  (`border-color` and `box-shadow` lists already blended; the page adds a colour pulse, a glow pulse, a neon border + inner + outer shadow keyframe, and a rotating `border-image: conic-gradient(from 0deg ..) 1` on a rounded box.)
- [x] Loading animations (spinners, progress shimmer/skeleton, dots) as reusable components
  (`ui/load/`: `load.Spinner` (conic border turned by a transform animation, three tones, two sizes), `load.Dots` (staggered hop), `load.Progress` (shimmering fill, or a running chunk when `busy`), `load.Skeleton` (placeholder bars with an animated-gradient sheen). Cost: 60 skeletons (about 180 animated gradients) add about 0.6 ms of frame time when off screen and about 0.4 ms for 90 visible ones headless.)

---


## 1. Text & Typography
- [x] Custom fonts and font weights
- [x] Rich text (mixed colors, sizes, styles)
- [x] Text gradients
- [x] Text shadows, outlines, and glow
- [x] Animated text (typewriter, reveal, wave)  (typewriter and word reveal in B, `text-wave` in J)
- [x] Text wrapping, truncation, alignment
- [x] Text selection and cursor rendering  (inside text boxes in D, RTL-aware; static text: opt-in `user-select: text` with drag, double/triple click, Ctrl+A, Ctrl+C, Text page)
- [x] High-quality anti-aliasing  (shapes, borders, shadows, clips and masks have analytic edge coverage in the paint shader; text is SDL_ttf antialiased at whole pixels. No MSAA, no subpixel/LCD text)
- [x] Icons, emojis, and Unicode

## 2. Buttons & Inputs
- [x] Standard, outlined, and gradient buttons  (Buttons & Inputs page: Standard, Outlined, Gradient with a hover blend)
- [x] Rounded, pill, and custom-shaped buttons  (Pill, and an Arrow button cut with `clip-path: polygon()`)
- [x] Text boxes and search fields
- [x] Checkboxes and radio buttons
- [x] Animated toggle switches
- [x] Sliders and range sliders
- [x] Dropdown and multi-select menus
- [x] Color pickers
- [x] Loading and progress indicators
- [ ] Custom input styling  (inputs, selects and the slider accent take CSS: background, border, radius, `:focus`, `accent-color`; the select list and color popups are not CSS-styled yet)

## 3. Navigation & Menus
- [x] Horizontal and vertical navigation bars
- [x] Collapsible sidebars
- [x] Tabbed interfaces
- [x] Nested dropdown menus
- [x] Context/right-click menus
- [ ] Radial menus
- [x] Scrollable menus  (select popups scroll past 8 rows with wheel/arrows; scroll boxes work inside popovers)
- [x] Animated menu transitions  (popover menus `transition:fade`, hover cards and submenus `@keyframes rise`)
- [x] Breadcrumb navigation
- [ ] Floating navigation panels

## 4. Layout & Containers
- [x] Flexbox-style layouts
- [x] Grid layouts
- [x] Absolute and relative positioning
- [x] Responsive sizing and alignment
- [x] Nested containers
- [x] Rounded and custom-shaped panels  (rounded panels everywhere; `clip-path` polygon/ellipse/inset and masks on any box, Images page)
- [x] Overlapping UI layers
- [x] Clipping and overflow handling
- [x] Resizable and draggable panels
- [x] Dockable windows
- [x] Animated layout changes  (width/height/flex transitions, Hover & Motion page)

## 5. Colors & Gradients
- [x] Solid colors and transparency
- [x] Linear gradients
- [x] Radial gradients
- [x] Conic gradients
- [x] Multi-stop gradients
- [x] Animated gradients  (J)
- [x] Gradient borders
- [x] Gradient text
- [x] Gradient backgrounds
- [x] Layered backgrounds
- [x] Dynamic color transitions

## 6. Borders, Shadows & Shapes
- [x] Rounded corners
- [x] Custom border thickness and colors
- [x] Gradient borders
- [x] Dashed and dotted borders
- [x] Soft drop shadows
- [x] Inner shadows
- [x] Multiple layered shadows
- [x] Neon glow effects
- [x] Custom polygons and shapes  (`clip-path`, G)
- [x] SVG/vector graphics  (inline `<svg><path>`, G)
- [x] Masks and clipping paths  (G)

## 7. Hover & Click Effects
- [x] Hover color changes
- [x] Hover scaling and rotation
- [x] Hover glow and shadows
- [ ] Hover blur effects
- [x] Smooth hover transitions
- [x] Click ripple effects
- [x] Press/depression animations
- [ ] Click particle effects
- [x] Animated focus indicators
- [ ] Custom cursor effects
- [ ] Cursor-following elements
- [ ] Mouse parallax effects

## 8. Animations & Transitions
- [x] Fade in/out
- [x] Slide in/out
- [x] Scale and rotation animations
- [x] Spring and bounce animations
- [x] Custom easing curves
- [x] Keyframe animations
- [x] Staggered animations
- [ ] Timeline-based animations
- [x] Animated borders and shadows  (J)
- [x] Animated gradients  (J)
- [x] Smooth layout transitions  (width, height, flex, margin, padding, left/top transition; Hover & Motion page)
- [x] Loading animations  (J: `ui/load/`)
- [ ] Scroll-triggered animations
- [x] Interruptible/reversible animations
- [ ] GPU-accelerated animations

## 9. Advanced Visual Effects
- [x] Gaussian blur  (H, Post Effects page)
- [x] Backdrop/background blur  (H, Post Effects page)
- [x] Frosted glass / glassmorphism  (H, Post Effects page)
- [x] Bloom  (H, Post Effects page)
- [x] Neon lighting and glow  (H, Post Effects page)
- [ ] Refraction and distortion
- [ ] Chromatic aberration
- [x] Noise and grain effects  (H, Post Effects page)
- [x] Scanlines and CRT effects  (H, Post Effects page)
- [ ] Pixelation
- [x] Color filters and grading  (H, Post Effects page)
- [x] Blend modes  (H, Post Effects page)
- [x] Custom shaders  (H, Post Effects page)
- [ ] Shader-driven transitions
- [x] Animated masks  (J: `mask-image` gradient stops in `@keyframes`, a wipe on the Motion Polish page; linear and radial only)
- [ ] Particle effects
- [ ] Procedural backgrounds
- [x] 3D perspective transforms  (H, Post Effects page)

## 10. Scrolling & Overlays
- [x] Vertical and horizontal scrolling
- [x] Smooth/inertial scrolling
- [x] Custom scrollbars
- [x] Nested scrollable containers
- [ ] Scroll snapping
- [x] Sticky headers and panels
- [ ] Scroll-linked animations
- [x] Popups and modals
- [x] Tooltips and hover cards
- [x] Slide-out drawers
- [x] Floating overlays  (`position: fixed`, popovers)
- [x] Animated notifications/toasts
- [x] Layered transparent overlays  (modal dim, popovers over a modal, `z-index` layers)

## 11. Images, Video & 3D
- [ ] PNG/JPEG/WebP images
- [x] SVG/vector images  (inline `<svg>` paths; no external `.svg`)
- [ ] Animated images
- [x] Sprite-sheet animations  (`<img sprite fps>`)
- [x] Image tinting and masking  (`image-tint`, `mask-image`)
- [x] Image cropping and scaling  (`object-fit`, `object-position`)
- [x] Nine-slice panels
- [ ] Video backgrounds
- [ ] Video inside UI elements
- [ ] Live 3D model previews
- [ ] 3D objects embedded in UI
- [x] Render-to-texture elements  (`paint.target`, Post Effects Shader & Texture tab)
- [ ] Animated icons
- [ ] Image-based particle effects

## 12. Game-Specific Visual Components
- [x] Animated health/resource bars
- [x] Inventory grids
- [x] Item rarity borders and glows
- [x] Detailed item tooltips
- [ ] Skill trees with animated connections
- [ ] Radial weapon menus
- [x] Animated HUD elements
- [x] Floating damage numbers
- [x] World-space UI/nameplates
- [x] Minimap and map overlays
- [x] Animated quest notifications
- [x] Dialogue boxes with text effects
- [ ] Character preview panels
- [x] Cooldown indicators
- [ ] Animated loading screens
- [ ] Cinematic menu transitions
