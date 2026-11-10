# 12_ui_runtime: the codegen target of `.j3ui` (plan 022 stage 05)

The shape `j3c` generates from `ui/*.j3ui` (plan 022 stages 05 and 06). Stage 05 wrote it by hand; since stage 06 the project compiles the `.j3ui` files itself (`[ui]` in `j3.toml`) and this folder is the test project: `src/main.j3` is the test host, `golden.png` is the frame the hand-written version drew (the generated one matches it pixel for pixel). `j3c dump-ui speculative/12_ui_runtime` prints the generated module, `j3c dump-ui ui/Name.j3ui` one component. `ui/Sidebar.j3ui` is cut down to three tabs; the other components are verbatim copies from the Idle Breakout game (not in this repository).

Run (always headless): `j3c build speculative/12_ui_runtime`, then in this directory with `SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy`: `timeout 10 target/idle test` (scripted mouse, PASS/FAIL), `timeout 10 target/idle steady 1000`, `J3_CAPTURE=out.png:30 timeout 10 target/idle golden` (compare with `golden.png` by `j3c -e imgdiff`).

## One module

All components are one module, `<package>.ui` (`idle.ui`): `use idle.ui.{ Game, GameProps };`. Per component `Name`: `pub const SCOPE_Name: int` (unique, 0 is the theme), `const CSS_Name: string`, `pub struct NameProps`, `pub struct NameEvents`, `pub struct Name`. Shared: `const THEME: string` (the `[ui] theme` file), `struct Branch { v: int; }` when an `{#if}` that holds components sits inside an `{#each}`. The module's `use` lines are the union of every component's `<script>` `use`, one `use path.Name;` per name. A component that no other component contains is a root: it also owns `css` and `dom` and has `render`.

## A component is a struct with three functions

```
Name.new(mem: &Arena): Name                         root: also Css.new + parse(theme) + Name.styles + Dom.new
Name.styles(css: &Css)                              parse(css, CSS_Name, "ui/Name.j3ui", SCOPE_Name); then each distinct child type's .styles
Name.render(this, size: Vec2, p: NameProps)         root only: beginFrame(size); inputs.begin(d); update; d.layout(); paint; inputs.overlay(d); endFrame
Name.update(this, d: &Dom, p: NameProps): NameEvents
Name.paint(this, d: &Dom)
```
`NameEvents` has one `bool` per event (**not** bit flags, J3 has no `|` or `&` on ints).

## Names inside `Name`

- `mem`, `ready` (first `update` runs `init`), `p: &NameProps` (last props, a one-element arena slot written by `*this.p = p`; read by `paint` and by script `fn`s).
- `tag_<tag>: int` per distinct tag, `cls_<class>: int` per distinct class: interned once in `init` (`d.tag`, `d.css.intern`). Never intern per frame (a linear search).
- Instance `let x` is a field `x`, read and written as `this.x`: the generator inserts `this.` before a free identifier token (found with the real lexer) that names an instance `let` or script `fn`, unless it follows `.`/`::`, is followed by `:`, or is a local. A `let` needs a type (`let x: T = ..`) unless its initialiser is a literal.
- `emit x` in a handler becomes `ev.x = true;`. A local (`let`, `for`, an `{#each}` variable) with the name of a prop, `let`, or `fn` is a compile error.
- Props: `update` and `paint` start with `let sim = p.sim;` (or `this.p.sim` in `paint`) for each prop the body uses, so handler and expression text is copied verbatim. Props are read-only.
- Script `fn f(..)` becomes a method `this.f(..)`; it re-binds the props it reads from `this.p`. Calls get `this.`. `f().x` works when `f` returns a reference (stage 06 made the checker spill the pointer).
- Child components: `cN: Child`, numbered in source order; inside an `{#each}` they are `cN: List<Child>` (`while this.cN.len <= i { this.cN.push(Child.new(this.mem)); }`, and `cN.len = n` after the loop). A `List<int>` field fails to check in a module that imports `j3.ui.css`, so per-iteration branch state is `List<Branch>` with a local `struct Branch { v: int; }`.
- `{#each}` iteration count `cntN: int` (set in `update`, read by `paint` so the paint walk never re-evaluates expressions).
- `{#if}` that holds child components or each-lists: `ifN: int` = branch taken last frame (-1 none; per-iteration list when inside an each). When it changes, the children of the branch entered are reset: `this.cK = Child.new(this.mem)` (Svelte destroying them). An `{#if}` with only elements keeps no state.

## Slots (the `d.open` first argument)

`d.open(slot, tag, SCOPE)` reuses the next child only when slot, tag and scope match. Local slots are `0..4095` and only matter among siblings of one DOM parent: every `open` starts a fresh numbering for its children.
- Constructs under one parent are numbered 0, 1, 2 in source order: an element 1; a child component 1; an `{#each}` 1; an `{#if}` chain one per element in all its branches (each branch element gets its own number).
- A child component at number `c`: `let s = d.enter(c); ...child.update(d, props); d.leave(s);` (child slots are offset, not renumbered; up to 4 nested enters per element).
- An `{#each}` at number `c`: `let s = d.enter(c);` then the body's top-level construct at offset `j` uses `i * K + j`, K = the body's total width (so up to `4096 / K` iterations); a component in the body does `d.enter(i * K + j)`; `d.leave(s)` after the loop.
- Text children take no slot (`d.text(s)`, `d.text(\`...{x}...\`)`; templates are on the scratch arena).
- Local names: element ids are `n1, n2...` in source order, enter tokens `sN`, events `eN`, if branches `bN`, loop counts `mN`.

## Update walk

```
let nK = d.open(SLOT, this.tag_div, SCOPE);
d.clsId(nK, this.cls_row);                       class="row"
d.clsIfId(nK, this.cls_locked, !this.canBuy());  class:locked={...}
d.attrDisabled(nK, gold < cost);                 disabled={...}      (also attrType, attrMin/Max/Step)
if d.clicked(nK) { <handler statements> }        on:click={...}; `emit buy` is `ev.buy = true;`
d.fade(nK, 150.0);                               transition:fade={150}
... children ...
d.close();
```
`let ev = NameEvents { buy: false };` first, `return ev;` last. A parent: `let eK = this.cK.update(d, ChildProps { ... }); if eK.buy { <on:buy body> }` (after `d.leave`). `bind:checked={x}` on a checkbox: `if d.clicked(n) { x = !x; } d.attrChecked(n, x);`. `bind:value={x}` on a range: `if d.changed(n) { x = d.value(n); } d.attrValue(n, x);` (`f32`; `bind:high` adds `d.highValue` / `d.attrHigh` and `d.attrMultiple(n, true)` for the second thumb); on a text, search, color input or a `select` it is a string: `if d.changed(n) { x = d.strValue(n); } d.attrText(n, x);` (the type comes from the literal `type`). `on:input` is `if d.changed(n)`, `on:submit` is `if d.submitted(n)`; `value`, `placeholder`, `multiple` are `attrText`, `attrPlaceholder`, `attrMultiple`. `j3.ui.inputs` (keys, text boxes, sliders, Tab) and `j3.ui.dropdown` (select and color popups) are what `inputs.begin` / `inputs.overlay` run. `<canvas>`: `on:mousedown` is `if d.mouseDown(n) { let mouse = d.localMouse(n); <body> }` (also `mouseUp`, `mouseMove`, `wheelAt`); `size` is `d.size(n)`; one canvas per component (else suffix the locals).

## Paint walk

Driven by the Dom's children, so conditions are not evaluated twice:
```
let nK = d.paintNext(SLOT);                      -1 when that node is not in the tree (branch off, each exhausted)
if nK >= 0 { d.paintBox(nK); ...children...; d.paintEnd(nK); }
let s = d.enter(c); this.cK.paint(d); d.leave(s);        a child component, with the same `c` as in update
```
`paintNext` paints text children and fading ghosts it passes; `paintEnd` paints trailing ones. A `<canvas>`: `d.paintBox(n); if d.beginCanvas(n) { let size = d.size(n); <on:draw body>; d.endCanvas(n); } ...children (HUD)...; d.paintEnd(n);`. Paint of an inactive `{#if}` branch is harmless: its `paintNext` calls return -1.

## Frame (root)

`d.beginFrame(size)` (clock, mouse and keyboard snapshot, hit target from the last layout, hover/active/click/wheel) then `inputs.begin(d)` (focus, typing, drags, popups) then `update`, `d.layout()` (removes unvisited nodes, restyle, layout, rewinds the paint walk), `paint`, `inputs.overlay(d)` (the open popup, above the page), `d.endFrame()` (drops finished ghosts). Fixed clock for tests: `d.setFrameMs(16.0)`; synthetic mouse: `d.simInput(x, y, down, wheel)`; synthetic keys for the next frame: `d.simKeys("typed text", "S+Left C+a Backspace")`.
