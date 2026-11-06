`.j3ui` components: Svelte-like markup, J3 script, and CSS, compiled by `j3c` to J3 and painted
with `j3.gfx.paint`. `j3c doc recipes components` is a complete project; `j3c new mygame
--template ui` writes it; `examples/idle-breakout` is the large one. The older immediate-mode
`@j3ui` functions are `j3c doc j3ui-immediate`.

Files. `[ui] dir = "ui"`, `theme = "ui/theme.css"` in j3.toml. `ui/` is scanned recursively
(hidden entries skipped). `ui/Name.j3ui` is the component `Name` (J3 types `Name`, `NameProps`,
`NameEvents`, in module `<package>.ui`). A folder is a namespace: `ui/game/Bar.j3ui` is `game.Bar`,
`ui/game/hud/Cool.j3ui` is `game.hud.Cool`. Markup always writes the full path from `ui/`, from any
file, with no short form: `<game.Bar hp={hp} on:low={...} />`. Props, events and `bind:` work as on a
bare tag. Dots become underscores in J3 (`game_Bar`, `game_BarProps`, `game_BarEvents`), so a root
mounts as `game_Bar.new(mem)`; two files giving the same J3 name (`Game_Bar.j3ui` and `Game/Bar.j3ui`)
are a compile error naming both. The last segment is Capitalised; folders start with a letter. It is
still one module, and `<style>` stays per file. A capitalised tag is a component (no import). The
component nobody contains is the root: `Root.new(mem)`, then each frame
`root.render(windowSize(), RootProps { ... })`. `j3c dump-ui <dir>` prints the generated J3 (a dir inside a project works; runtime aborts name the `.j3ui` line).

A component:

    <script>
        use mygame.state.State;
        props { state: &State; gap: int = 8; }    // from the parent; `= value` makes one optional
        events { buy; }             // what it can `emit buy`
        let open = false;           // per-instance state
        const NAMES: [3]string = ["a", "b", "c"];   // shared, read-only (see below)
        fn helper(): int { ... }    // a plain fn is a derived value
    </script>
    <div class="x" class:on={open} on:click={open = !open}>{state.clicks}</div>
    <style> .x { display: flex; gap: 8px; } </style>

- Everything inside `{}` and the script is real J3 (checked, AOT). Errors point at the `.j3ui` line.
- Text and attribute values with holes (`<p>{text}</p>`, `title="{n} of {name}"`) have no length limit: a lone hole is the value, and text around holes is joined in a frame-scratch `Text` (one 512-byte template per hole, so only a single number or `#[Debug]` struct must fit). A keyed `{#each}` key is one template.
- Handlers are statements, not closures: `on:click={state.buy(k)}`, loop variables are in scope.
  A handler may span lines (`on:click={` newline statements newline `}`); past two statements write a `fn` in the script and call it.
- Prop defaults: `props { gap: int = 8; note: string = ""; }`. A use site may leave out a prop that has a default; one without
  a default is required. The default is a constant, a literal or one of this component's `const`s; naming a prop, `let` or `fn` is an
  error. It is sent every frame like a given value (props are read-only in the child), so nothing is stored.
- `const NAME: Type = value;` in a script is one read-only value for every instance, used by its plain name in markup and fns.
  A literal (`const MAX = 4;`) is a real J3 const. Anything else, like an array, needs its type written and is filled once at
  startup (J3 consts hold scalars only), so its initializer is plain J3 (it cannot name other consts or props). Writing to it is
  not allowed: assignments to the name are an error (`NAMES[i] = x` is not caught; don't).
  `<Child on:buy={...} />` runs when the child emits `buy`. `bind:value` / `bind:checked` write
  through the place. Blocks: `{#if}{:else if}{:else}{/if}`, `{#let name = expr}`, `{#each xs as x, i}` (slice, array, `0..n`; keys below).
  `{#match page}{:case 0}...{:case 1, 2}...{:else}...{/match}` is an `{#if}` chain on `page == 0`, `page == 1 || page == 2`
  (page is evaluated once; compared with `==`, so ints, enums, bools; a duplicate case is an error; `{:else}` is optional and
  last; only `{:case}`/`{:else}` may sit directly inside). State, components and `onDestroy` behave exactly as in `{#if}`.
  `<component this={msg.kind} of={TextMsg, ImageMsg, Embed} msg={msg}/>` chooses from a closed component list.
  `this` accepts an `int` index (zero-based) or the generated `Of_TextMsg_ImageMsg_Embed` enum, with variants
  `.TextMsg`, `.ImageMsg`, `.Embed` (dotted component names use underscores). List order defines the enum name
  and indices; the same ordered list shares its enum. Other selector types are errors; invalid indices render nothing.
  Shared props and children must be valid for every candidate, including candidates never selected at runtime.
  Slots, events, class/style passthrough and provide/inject work as on ordinary component uses. Switching candidates
  destroys the old arm and mounts fresh state; keyed row moves preserve the selected arm. `this` and `of` are reserved
  selector attributes; generated `Of_` component-name prefix is reserved.
  `transition:fade|fly|slide|scale={ms}` (default 150) on an `{#if}` child or a keyed `{#each}` row plays the effect when the element appears and again, as a ghost that takes no input, when it leaves; `in:<kind>` / `out:<kind>`
  do one direction only (`in:fly={200} out:fade`; one per direction, `transition:` counts for both). Parameters are attributes beside it, shared by both directions: `fly-x={px}` `fly-y={px}` (start offset, default 0 and 20),
  `scale-from={0.5}`, `slide-axis="x"` (default `"y"`, the height), `transition-easing="ease|linear|ease-in|ease-out|ease-in-out|bounce|spring"` (default linear for fade, ease-out for the rest). fly and scale also fade;
  slide shrinks the box's own size in layout (height, or width with `slide-axis="x"`) while its content stays put and is clipped, so siblings glide: the box and its parents are laid out every frame while a slide
  is in flight and not at all after (fade, fly and scale are paint-only, hit testing follows, no relayout). In a flex container the item's main size is scaled, so a `flex-grow` item that slides gets the scaled share; grid tracks do not follow a slide.
  A box removed mid-enter leaves from how far it got. Toggling the `{#if}` (or re-adding the keyed row) while its old copy is still leaving brings that copy back from how much of it is shown, so fast toggling never piles up ghosts.
  `anim-delay={ms}` (adds to the CSS animation-delay, to stagger an `{#each}`).
  `animate:flip={ms}` (default 200) on the row element of a keyed, non-`virtual` `{#each}` (`<li animate:flip={200}>`, also under an `{#if}` that is the row): when a sort, filter, insert or remove
  moves the row, it is drawn from its old place (hit testing follows) and glides to the new one; `animate-easing="ease|linear|ease-in|ease-out|ease-in-out|bounce|spring"` picks the curve
  (default `ease-out`). A flight interrupted by another move continues from where the row is drawn. A window resize, a scroll, and a row's first appearance do not flip; a `virtual` list and
  a component tag are compile errors. Easing is one of the seven presets (not `cubic-bezier()`/`steps()`). A popover or drag anchored to a flipping row uses its layout (final) position. Rows without it pay nothing; a flip needs no relayout, only repaint. Attributes: `x={expr}` or `x="a {b}"`.
- Children (`<slot>`): a component with `<slot />` takes the markup written inside its tag; `<slot name="x">fallback</slot>`
  is a named slot and its children show when nothing fills it. A direct child with `slot="x"` goes to that slot, the rest to the
  default one.
      Card.j3ui:  <section>{title}<slot name="actions" /><slot /><div><slot name="footer">Thanks</slot></div></section>
      user:       <Card title="Audio"><button slot="actions" on:click={reset()}>Reset</button><p>{vol}</p></Card>
  Slot content is compiled in the user's scope (its props, `let`s, `fn`s, `{#each}` variables, `emit`, `bind:`) and styled by the
  user's `<style>`; the Card's scoped CSS reaches its own elements, not the slotted ones. Its state (`{#if}`, child components)
  belongs to the user, but when the Card stops rendering the slot (it is inside the Card's own `{#if}`, say) the content is destroyed
  (`onDestroy` runs) and starts fresh when the slot comes back, as in Svelte. An `{#each}` item is read again from the list for the
  slot (the list expression runs once more). A slot may be passed on (`<Card><slot /></Card>`). Errors: `slot="x"` where the
  component has no `<slot name="x">`, children for a component with no `<slot>`, two default slots in one component. `{#if $slots.footer}` (also
  `$slots.default`, and in any expression) is true when the use site fills that slot, so a Card can skip an empty footer; a use
  with no children makes every slot false, and `$slots.x` for a slot the component does not declare is an error. `:slotted()` is
  not supported. `slot` is reserved as an attribute on a direct
  child of a component.
- Lists. `{#each files as f (f.id)}` is keyed: the part in parentheses is any J3 expression that formats with `{}` (an int or a
  string; it is compared as text) and must differ between rows. Each row keeps its component state, `{#if}` branches, nested lists
  and its DOM nodes (focus, scroll, text caret, transitions) across a sort, filter or insert; a key that disappears destroys its
  row (`onDestroy`), a new key mounts one. A repeated key prints one warning naming the `.j3ui` line and its rows are matched by
  position. Without a key a row is its index (fine for lists that only grow and shrink at the end). Keep the key cheap: it runs
  once per row per frame, and a keyed row's DOM nodes are told apart by the key, not by position, so lists over 4096 rows work.
  Int keys: write `(f.id : int)` (any expression, then `: int`) and the key is compared as 8 raw bytes with nothing formatted per
  row per frame (a pool keyed by slot index, or rows with a numeric id); the same list never mixes the two kinds, and `virtual`
  lists take it too, except with `keep` (which lists text keys). Positional is fine: `{#each pool as p}` over a struct list needs no
  key when rows never move or change identity (they only grow or shrink at the end, or every row is rebuilt from data each frame,
  as in a read-only table); key the list when rows are sorted, filtered, inserted in the middle, or hold state (inputs, focus,
  transitions, child components) that must follow the item.
  Lists nest to any depth, components included, keyed or not; give every `{#each}` loop variable in a component its own name
  (J3 has no shadowing). Slot content inside two nested lists is not supported yet. Teach `{#each items as it}` over `[]Item`
  rather than `0..N` over parallel arrays.
  `{#each xs as &x}` over a `[]T` binds `x` as a pointer to the element (not a copy), so a handler or `bind:` in the body changes
  the source array in place: `{#each state.items as &it}<input bind:value={it.name} />{/each}`. Not for `0..n` ranges.
  The loop holds the slice it started with, so do not push to or remove from that list inside its own rows' handlers: a `List<T>`
  push can move the storage (writes through `&it` then land in the old copy) and a remove shifts the rows under the pointer. Queue
  the change and apply it after the update. A `(key)` sees the `{#let}`s declared before the `{#each}`, not the ones in its body.
  `{#let f = &state.files[id]}` (no closing tag) is a local for the rest of its block, evaluated once per update: the siblings after it and
  their descendants, in an `{#each}` row, an `{#if}`/`{#match}` arm or the top of the markup, and slot content written inside the
  block. A later `{#let}` may use an earlier one; a repeated name (or one that shadows a prop, `let`, `fn` or loop variable) is an error
  at the markup. Use it after the block ends and J3 reports an unknown name at that spot.
  A `List<T>` (`j3.memory`) is not a slice, so loop over its live part: `{#each state.items.items[0..state.items.len] as it (it.id)}`.
  To hand a row to a child, pass its address,
  `<Row item={&it} on:remove={state.remove(event.id)} />`, with `props { item: &Item; }` in `Row.j3ui`; the child edits the
  row in place (`bind:checked={item.done}`). Remove inside the `on:remove` handler, not while looping.
- Scrolling from code (`d` is the Dom in a handler, `speculative/53_ui_scroll`). `d.byId("x")` is the node of the first element with static
  `id="x"` (-1 for none; a walk of the tree, call it from a handler, not every frame); dynamic `id={..}` is not found. `d.scrollIntoView(node, block, smooth)` scrolls every scroll box
  around `node`, nearest first, by the least that is needed (`block` is `dom.SC_START`, `SC_CENTER`, `SC_END` or `SC_NEAREST` on y; x is
  nearest, `d.scrollIntoViewXY(node, block, inline, smooth)` sets both), counting the boxes' `scroll-padding` and the node's `scroll-margin`;
  a snapping axis lands on a snap position. `d.scrollTo(box, x, y, smooth)` sets one box's offsets (clamped, snapped). `smooth` glides over ~300
  ms and is instant under reduced motion. `d.scrollToKey(box, "key", block)` scrolls a `virtual` list to the row with that key even
  when it is not built (false for an unknown box or key); with measured rows it aims again each frame until the row is measured. The
  focus moved by a key (Tab, arrows) is scrolled into view (nearest) on the next frame.
- `bind:scrollX={place}` / `bind:scrollY={place}` (an `f32`) on a scroll box tie its offset to a value every frame: a scroll by the user
  (wheel, scrollbar, inertia) or `d.scrollTo` writes the place, and setting the place from code (a middle-drag pan, a playhead follow)
  moves the box at once, clamped to its range, stopping any glide. Works on `overflow: hidden` too.
- `on:wheel|stop={..}` runs like `on:wheel` and keeps the wheel over the element from scrolling it or any scroll box around it (a
  wheel-zoom canvas inside an `overflow-y` list). Scroll boxes inside the element still take the wheel first.
- Virtual lists. `<div class="list" virtual row-height={26}>{#each rows as r, i (r.id)}...{/each}</div>` keeps only the rows
  near the viewport alive (100k rows are about 60 nodes) and pads the box so the scroll bar is true. The element is the scroll
  box (give it a height and `overflow-y: auto`), holds exactly one keyed `{#each}` over a list (no `0..n`); `i` and the key are
  the real index and key. It sets `padding-top`/`padding-bottom` and `style:` slots 6 and 7 of the box, so use a wrapper for
  your own padding there. Four rows of overscan. No slot content inside it.
  Heights: `row-height={26}` is the pixel height every row must have (set the same in CSS, no `gap`, margin or border between
  rows) and is the fast path. Without it, or `row-height="auto"`, rows may differ (wrapped text, images): each row is
  measured after layout (distance to the next row's top, so margin and `gap` count) and cached by key; a row not seen yet
  counts as the mean of the measured ones, `est-height={40}` before any. The total height corrects as rows are measured and is
  an estimate far from where you have scrolled. Lookup is O(log n), a steady frame does no layout and allocates nothing; a
  frame where the list changed (its length or keys) formats every key once, O(rows).
  Scroll anchoring: when rows above the first visible one change height or are inserted or removed (older chat messages
  loading), the scroll offset moves by the difference so the visible rows stay put. After a long jump into unmeasured rows the
  first frame can be a few px off, until the rows around the view are measured.
  `stick-to-bottom`: while the view is at the end an append keeps it at the end; scrolling up releases it, reaching the end
  again re-engages it (it starts at the end). `bind:at-end={state.atEnd}` (a `bool` place) is written every frame with whether
  the view is at the end, for a "jump to latest" button; setting it to true (`<button on:click={state.atEnd = true}>`)
  scrolls to the end and re-engages the stick. Both work with `row-height` too.
  Kept rows: `keep={state.keepKeys}` (a `[]string` of keys, the `{#each}` key formatted as text) keeps those rows alive when
  they are outside the window, e.g. the row being edited or a playing video. A kept row off screen is `display: none` (state,
  `<input>` text and nodes stay; it is not laid out or painted) through a style slot markup cannot use; a key that is
  not in the list holds nothing. The row with focus is kept automatically. Other rows that scroll out lose their component state.
- Copying a bound string. `bind:value={draft}` (a text box) gives a `string` view that changes with the next keystroke, so
  copy it before you keep it, in a method of a struct that holds `mem: &Arena`:
  `let t = Text.new(this.mem); t.pushStr(draft); this.items.push(Item { label: t.str() });` (`use j3.text.Text;`). Strings that are literals or
  already copied need no copy.
- Formatted text from a function. A `{}` string inside a function is garbage once it returns, so build it in the frame
  scratch arena: `fn size(this: &File): string { let t = scratchText(); return fmtBytes(&t, this.bytes); }`
  (`use j3.text.{ scratchText, fmtBytes, fmtInt };`; `pushStr` and `pushInt` work too) and use it in markup:
  `<td title={f.size()}>{f.size()}</td>`, `class="{f.kind()}"`, a keyed `{#each .. (f.key(i))}` key, `style:` text. The text node,
  `title`, `aria-label`, `placeholder`, `class` words, `style:` values and keys compare bytes and keep their own copy, so a
  steady frame does no work (`J3_STEADY`) and a changed string updates. The view lives until the next frame starts (`App` rewinds the
  scratch arena at the top of every frame; a loop without `App` calls `j3.memory.rewindScratch()` itself, once per frame, before
  `render`). `value=` copies it too. Do not store it in a struct or keep it from an event handler, and do not give it to `bind:`.
- Elements: div span p h1 h2 h3 h4 h5 h6 small header nav main section label button input textarea select option dl dt dd canvas dialog aside img svg path ul ol li a hr strong em b i pre code br progress meter details summary glyph icon.
- `strong`/`b` are bold, `em`/`i` italic, `pre` keeps whitespace, `hr` is a 1px rule; lists have no markers (add `li::before { content: "• " }`), links are `<a on:click={...}>` (no `href` navigation).
- Dynamic style. `class="slot r{rarity} {extra}"`: words with no `{}` are static, the rest are re-read when their text changes
  (a word added this way and also by `class:name` is dropped when the text changes; 8 classes per element). `style:prop`
  is an inline value that beats every rule and transitions like one (`transition: width 200ms` on the bar): `style:width={pct * 100.0}`
  (a number: px for lengths, a plain number for `opacity` `flex` `z-index`), or text through the CSS parser,
  `style:width="{pct * 100.0}%"`, `style:background="{color}"` (`={"..."}` with a quoted string also means text; any other `={expr}` is a number).
  Shorthands (`padding` `margin` `border` ...) take text. An empty string clears the override. Any property of `j3c doc css`
  except `animation`, `transition`, grid tracks and placement (`grid-*`) and `--vars`; an unknown name is a compile error, a bad value a warning in
  `css.warns`. At most 8 `style:` per element; on the same property the later one wins. `style:left` / `style:top` offset
  from the containing block like the CSS; the `left={x}` / `top={y}` attributes keep their own meaning (viewport point for
  popups and panels: the box is kept on screen and `position-try-fallbacks` flips about it). Nothing is recomputed on a frame
  where the values did not change. A text value that is a gradient allocates paint data each time it changes.
- Class and style on a component. `<kit.Card class="wide {x}" class:on={cond} style:width={w}>` puts them on the component's root
  element (same rules as on an element; `style:` values are numbers or text as above). The component must be exactly one element
  (whitespace and comments aside): a root `{#if}` / `{#each}` / `{#match}`, `<slot />`, text or another component is a compile
  error at the use that passes class or style (a component with several roots is fine while nobody passes them). Scoping: your
  scoped rules match the passed class on that root, though the root belongs to the child's scope; only rules whose last compound
  names a class match it (a `div { }` rule of yours does not reach into the child), and one of yours beats the child's own rule
  of the same specificity. The child's own scoped CSS still applies. Precedence: a passed `style:` beats the root's own `style:`
  for the same property (the user wins), and any `style:` beats rules; the root's own `style:` plus the passed ones are at most 8.
  A block-level child is laid out by its parent's rules, so a wrapper div that only sized it is not needed (a flex row's
  `align-items: stretch` acts on the root; use `align-self: flex-start` to keep its natural height). The names `j3pass*` are reserved.
- Lifecycle. Optional script fns found by name: `fn onMount()` (once, at the end of the component's first update, so children
  mount first; state it sets shows in the next build), `fn onFrame(dt: f32)` (every update, before the markup; `dt` is seconds from
  the Dom clock: 0 on the first frame, at most 0.1, exactly the step of `setFrameMs` / `J3_FIXED_DT`; `d.dt()` on the Dom),
  `fn onDestroy()` (children first). Use `onFrame` for per-frame work, not `class:tick={tick()}`. A component inside an `{#if}`
  branch is destroyed when the branch is left and starts fresh (state reset, `onMount` again) when it is entered; an `{#each}`
  that shrinks destroys the dropped items. A hidden branch gets no `onFrame`. Wrong signatures are compile errors; the names
  `destroy` and `mounting` are reserved. The root component is never destroyed.
- `bind:this={place}` and `use:`. `bind:this={el}` (a place holding an `int`, `let el = -1;`; also `rows[i]` inside an `{#each}`) writes the
  element's handle on every build, `d.handle(id)`: the node id plus the node's serial, so an id that was freed and recycled is told
  from the old element. Any Dom method that takes a node accepts a handle or a plain id (`d.byId`, `d.focusNode(el)`,
  `d.size(el)`, `d.scrollIntoView(el, ..)`), and a stale handle is ignored (a getter returns 0, false or an empty string); exceptions: `caretRect`,
  `spansValue`, `gridOf`. `d.alive(h)` asks, `d.node(h)` gives the plain id (-1 when gone). When the element goes away the place is set
  back to -1 (end of that update), if the place is a component `let`, a field of one, or an element of one (`rows[i]`); a place that only
  exists inside the markup (an `{#each}` item, a `{#let}`) keeps its last handle, which the Dom calls reject. Script fns see the Dom
  as `d` (`fn focus() { d.focusNode(el); }`; a user's own `d` is a compile error). `bind:this` on a `<Component>` is a compile error
  (put it on an element inside).
  `use:name={param}` (or `use:name`) on an element calls the script fn `fn name(node: int, param: T)` on the first build after the
  element's first layout (one frame after it appears, so `d.size(node)` is real), `fn name_update(node: int, param: T)` when the
  parameter changes (`T` is a number, bool or string; strings compare by bytes and one buffer per site is kept, so a string that
  changes every frame allocates nothing once it fits), and `fn name_destroy(node: int)` once the element is gone (at the end of that
  update: the handle is already stale) or its component or row is destroyed. A keyed row that only moves is not remounted.
  `_update` and `_destroy` are optional; a missing or mistyped fn is a compile error at the `use:`. Only script fns, not dotted module functions.
- Idle frames. `App` sleeps while the UI is still (`App.idleSkip(false)` turns it off): `Dom.endFrame` reports `Dom.idleMs()` to the loop, which waits in
  `SDL_WaitEventTimeout` instead of updating, drawing and swapping. It wakes on any SDL event (input, resize, expose, focus, drop), on
  `Dom.requestFrame` (any thread; it pushes an SDL user event), on any `j3.thread.Queue` push (a worker's result is drawn on the next
  frame; workers need no UI calls), at `game.wakeAt(ms)`, when the caret next blinks, and on a 250 ms safety tick (40 ms under
  `run --hot`) for timers the UI cannot name (toast expiry, a hover card's delay, style hot reload); a woken frame sees a 16 ms dt. Frames come
  back to back while a transition, animation, inertia, ripple, fade-out, `<progress>`, tooltip, held button or key, the
  inspector or fake input is live, and while a `<canvas>` body's draws change: each frame hashes what the canvas bodies queue
  (`game.p2Watch`), and the loop stays awake until that holds still for 50 ms, so an animation drawn by hand from `game.ticks()` runs
  at full rate with no wake calls and a still canvas sleeps. It sees `j3.gfx.paint` / `game.draw*` calls, not raw GL or `j3.gfx` 3D in a
  canvas, nor a texture whose pixels change under the same draw (call `dom.requestFrame()` for those). It cannot see an animation
  that has not started yet: a timer in app logic ("bot moves at `ticks() >= due`") calls `game.wakeAt(due)`. Any registered system
  (`first`, `add`, `last`, `always`) keeps the loop awake; drawing outside the Dom with no system (3D in `draw`) needs `idleSkip(false)`.
  A draw system that counts frames (autosave after N quiet frames) must call `dom.requestFrame()` until done. Tests and captures never
  sleep: `J3_FRAMES`, `J3_SCREENSHOT`, `J3_CAPTURE`, `J3_FIXED_DT`, `J3_STEADY`, and the `offscreen` / `dummy` video drivers turn it off; `J3_IDLE_SKIP=1`
  forces it on, `=0` off; `J3_IDLE_STATS=1` prints `idle: loops=N presented=N` at exit (`game.framesPresented()`).
- Event payloads. `events { pick(slot: int); rename(name: string, n: int); close; }`, `emit pick(i)` (one argument per field,
  any J3 expression), and the parent writes `<Pad on:pick={state.slot = event.slot} />`: `event` is a struct of the payload in
  that handler only. Field types: int, bool, string, float, f32, f64, i32, u8. A plain event next to payload events is fine. Two
  emits of one event in a frame: the last one wins. `event` is a reserved name. Emits from slot content go to the component that
  wrote the content, payload included.
- `emit` in a script `fn`: `fn ok() { if valid() { emit rename(draft); } }`, then `on:click={ok()}` and `on:submit={ok()}` share it. A fn
  may call another fn that emits. The event reaches the parent at the end of that frame's update (a fn run from `onMount` or
  `onFrame` too). Calling an emitting fn from a markup expression (it runs on every update), `on:draw` or `onDestroy` is a compile error.
- Provide / inject. `provide state;` in a script (a prop or `let` of that component) hands it to every descendant; a descendant
  writes `inject state: &State;` and uses `state` like a prop. Resolved at compile time: each component on the way gets a hidden
  prop that its parent fills, so there is no runtime lookup. Slot content sees the providers of the component that wrote it, not of
  the component that shows it. Errors: nothing above provides the name (the message gives the path from the root), `provide`
  and `inject` disagree on the type (compared as text), a component that has `state` as its own prop or `let` but did not
  `provide` it while a descendant injects it. Inject names cannot be passed by hand on a `<Component>`.
- Bubbling: `on:click`, `on:mousedown`, `on:mouseup` and `on:contextmenu` fire on the element under the pointer AND on every
  ancestor with the same handler (`<div on:mousedown={a = 1}><canvas /></div>` runs for a press on the canvas), each level with
  its own `mouse` (local to it) and `button` in scope. `on:mousedown|stop` / `on:mouseup|stop` / `on:contextmenu|stop` run like the
  plain handler and end the walk there (innermost first: ancestors above a `|stop` see nothing); `on:mousedown|stop={}` only stops (a HUD over a `<canvas>`). All levels run in the same
  frame, in markup order (outer first), so when two levels write the same state the inner one wins; use `|stop` to keep the
  outer one out. `on:mousemove` and `on:wheel` do not bubble (the element itself, or inline text inside it).
- Attributes: `class`, `class:name={cond}`, `style:prop`, `on:click` `on:mousedown` `on:mouseup` (`|stop`, bubbling above) `on:mousemove`
  `on:wheel` (`|stop`) `bind:scrollX` `bind:scrollY` `on:contextmenu` (`|stop`) `on:close` `animate:flip` `animate-easing` (keyed row element, see Blocks) `draggable` `drag-data` `drag-id` `drag-ghost` `on:dragstart` `on:dragend` `on:dragenter` `on:dragleave` `on:dragover` `on:drop` (item drag and drop, below) `popover` `bind:tooltip` `closedby` `title` `left` `top` `panel` `panel-handle` `panel-resize` `window-drag` `menuitem` `tabindex` (`"0"`, `"-1"`, a positive integer or `={int}`) `id="x"` (static name, for `#x` selectors) `role` `aria-label` (any element); `dialog` takes `modal` and `autoselect`; `label` takes `for="id"` (a static string, below); `button` takes `disabled` and `default`; `input` takes `type` (checkbox, range, color, text, search, password; any other is a text box). A click on a `label`'s text (or `for="x"` naming the control with `id="x"`, found anywhere in the window, first one in tree order when ids repeat, with a one-time entry in `css.warns`) activates the first control inside it: a checkbox toggles, a button is clicked, a text box, range or select takes focus (a select does not open). The label's own `on:click` fires as well. Not when the click lands on a control already, on a disabled control, or after a drag. The label's text is the control's accessible name. Browsers also hover the control with the label; here only the label is `:hover`
  `min` `max` `step` `value` `high` `checked` `name` `placeholder` `maxlength` `disabled` `noprimary`, `bind:value` / `bind:high` / `bind:checked` / `bind:group`
  and `on:input` / `on:change` / `on:submit`; `select` takes `multiple` `value` `placeholder` `disabled` `bind:value` `on:input`;
  `option` takes `value` `disabled`; `button` takes `disabled`; `textarea` is a void tag (`<textarea bind:value={s} rows="4" />`) taking `rows` `maxlength` `placeholder` `disabled` `rich` `noprimary` `triggers` `bind:value` `bind:spans` `on:input` `on:submit` `on:trigger`; `img` takes `src` `alt` `sprite` `frame` `fps`; `svg` takes
  `viewBox`; `path` takes `d` `fill` `stroke` `stroke-width`. Anything else is a compile error naming the accepted list.
- `<br>` (void, `<br>` or `<br/>`) forces a line break in inline content whatever `white-space` is; two in a row leave an empty line, a trailing one adds none, and static-selection copy gets a newline. It cannot go inside a text box (`<input>` and `<textarea>` take no content).
- `<progress value={v} max={m}>` and `<meter value min max low high optimum>` are controls the runtime paints (inline-block, 160x8 by default; a `width` / `height` resizes them; no children). The element's own `background` and `border-radius` are the track (the
  built-in default is a translucent grey with a 4px radius, any rule beats it); the fill sits inside the border, rounded like the track. A progress fills `value / max` (`max` defaults to 1) with `accent-color`; with no `value` (or a negative one) it is indeterminate: a bar
  sweeps the track every 1.4 s, and sits still in the middle under `prefers-reduced-motion`. A meter fills `(value - min) / (max - min)` in one of three colors the way HTML picks them: the value's region (below `low`, between, above `high`; defaults `min` and `max`)
  against the `optimum`'s (default the middle): same region good, next one ok, far one bad. Theme them with custom properties on `:root` (`--j3-meter-good: #3fb950; --j3-meter-ok: #d29922; --j3-meter-bad: #f85149`), or scoped: `.hud { --j3-meter-bad: #ff00aa; }` applies to the meters inside `.hud` (and `meter.x { }`). a11y roles `progressbar` and `meter` with `min` `max` `now`.
- `<details open={bool}>` with `bind:open={place}` (follows clicks) and `on:toggle={..}` (`open` in the handler is the new state; it fires once for a click or key, and once, a frame later, when the program changes `open`, not for the first value), whose first child is a `<summary>`. A click on the summary (or Enter / Space on it: it is focusable, role `button`, `expanded` state) flips
  the box; a closed `<details>` shows only its `<summary>` (the other children stay mounted, so a component or text box inside keeps its state, but are `display: none`). `:open` matches an open `<details>`. `open={expr}` is applied when the value changes,
  so a click is not undone by the next update; use `bind:open` to follow it. There is no marker: draw one with `summary::before { content: "▸ " }` and `details:open > summary::before { content: "▾ " }`. A button or input inside the summary takes its own click.
- Input widgets (`j3.ui.inputs`, `j3.ui.dropdown`; page 3 of `examples/big_ui_test`). They have no browser look: style them
  (`input[type="text"]`, `select`, `:focus`, `accent-color`). `bind:value` writes a `string` for text, search, color and
  select, an `f32` for a range (the `type` must be a literal). A text box is one line with a caret, drag and Shift+arrow
  selection, Home/End, Ctrl+arrows (words), Ctrl+A/C/X/V (any length), Ctrl+Z undo and Ctrl+Y / Ctrl+Shift+Z redo (typing and deleting runs are one step each, a paste, cut, caret move or selection edit starts a new one, a new edit clears redo, the last 100 steps or 4 MB of text are kept per box, each step being a copy of the text; a value set by the program is not a step and history is kept, so undo may bring older text back), Backspace/Delete, UTF-8 safe, placeholder; `search` adds a clear
  button and Escape clears; Enter fires `on:submit`. A double click selects a word, a triple click the line (all of a one-line box).
  `maxlength="n"` caps typing and pasting at n code points (a value set by the program is not cut). `type="password"` draws
  one bullet per code point and neither copies nor cuts (the bound string is the real text). PRIMARY selection (X11/Wayland; text boxes and textareas, `rich` too):
  when a selection settles (mouse up after a drag, double or triple click or Shift+click, Ctrl+A, a Shift+caret key) its text goes to PRIMARY
  (`game.setPrimarySelection`), once, not every frame; a password box never sets it. A middle press on an enabled text box focuses it, puts the
  caret at the click and inserts PRIMARY there like Ctrl+V (one undo step, `maxlength`, `on:input`, any length); a disabled box ignores it. Static
  `user-select: text` sets PRIMARY the same way. `noprimary` on a box turns both off for it. `Dom.selection(id)` is the selection as a byte range
  (`.lo`, `.hi`; equal when empty), `Dom.selectedText(id)` its text; `Dom.primary()` / `Dom.setPrimary(s)` read and write PRIMARY, and tests call
  `Dom.simPrimary(s)` (keep it in the Dom; headless has none) with `Dom.simMiddle(down)` for the middle button. `<textarea>` is an `input` of type `textarea` underneath: a `textarea { }` rule, an `input { }` rule and `input[type="textarea"]` all match it. It wraps at spaces, Enter
  inserts a newline (Ctrl+Enter fires `on:submit`), Up/Down keep their column, PageUp/PageDown, Home/End go to the line (Ctrl: the text),
  the wheel and the caret scroll its own view; `rows` (default 3) sets its height, a `height` overrides it. IME: the composition
  string is drawn underlined at the caret and the IME is told where the caret is (window pixels; not corrected for `setVirtualSize`).
  `<textarea rich bind:value={s} bind:spans={sp}>` keeps styled spans beside its text: `sp` is a `[]Span` (`use j3.ui.spans.Span;`; `start` and `len` in bytes of the UTF-8
  text, `kind` `SPAN_BOLD` `SPAN_ITALIC` `SPAN_LINK` `SPAN_MENTION` `SPAN_EMOJI` `SPAN_COLOR`, `data` the id, emoji name, URL, or for `SPAN_COLOR` a `#rgb` / `#rrggbb` / CSS color name: syntax highlight, rebuilt by a tokenizer on change). Ctrl+B and Ctrl+I toggle bold and italic on the selection
  (each an undo step; undo restores spans). A mention or emoji is an atomic token: its text is the chip label, the caret skips it, Backspace and Delete remove it whole,
  a selection edge or click inside it snaps outward, and it wraps as one box (a chip: label plus 4 px each side on an `accent-color` tint; links are `accent-color` and
  underlined, italic has no slanted font and is drawn dimmed). Typing at the end of a bold or italic span extends it, typing at its start or after a link or token does not.
  `Dom.insertToken(id, lo, hi, label, kind, data)` swaps a text range (the typed `@ja`) for a token in one undo step, `Dom.caretRect(id)` is the focused box's caret in window px and
  `Dom.caret(id)` its byte, `Dom.setSelection(id, lo, hi)` selects bytes `lo..hi` and scrolls the caret into view even unfocused (a find match). Replacing the text from the program drops the old spans, so set the new ones in the same update (attribute order does not matter); a program that just echoes `sp` back never brings back stale spans. Copy and cut take plain text (labels
  included), paste inserts plain text. A box without `rich` keeps no spans and pays nothing.
  Data grid (spreadsheet): a fixed `row-height` `virtual` box with `tabindex="0"`, `grid={g.sized(rows.len)}` and `on:edit={..}`, `g: &Grid` (`j3.ui.gridstate`, `Grid.new(mem, cols, rowH, colW)`). Rows are
  `<div style:width={g.total}>` of `{#each 0..g.cols as j}<div class:active={g.on(i, j)} style:width={g.w(j)}>..</div>`; `g.widths` is the one column source for the header and every row (`g.setWidth(j, px)`).
  A header outside the box follows `style:margin-left={0.0 - g.sx}`. The grid box has focus (one roving active cell `g.row`/`g.col`, click picks it): arrows, Ctrl+arrows (to the edge, or along runs of
  cells when `g.hasFilled = true; g.filled = fn(row, col): bool`), Tab/Shift+Tab (wrap to the next/previous row), Home/End (Ctrl: first/last cell), PageUp/PageDown, scrolled into view on both axes. Enter, F2 or typing starts the editor:
  `{#if g.editAt(i, j)}<input type="text" bind:value={g.draft} />{:else}..{/if}`. `on:edit` sees `kind` (0 begin, 1 commit), `row`, `col`, `text` (a view of the editor's buffer, like `bind:value` and `on:trigger`: copy it). On begin write
  `g.draft = g.initial(cellText)` (the typed character replaces the text); on commit write the value. Enter commits and moves down, Tab right, a click elsewhere commits, Escape cancels; undo works in the box. The editor's row is kept alive while it has focus (the `virtual` focus rule), so scrolling away does not end the edit; if its row disappears (the list shrank) the text is committed. Several grids may share a screen (state lives in each `Grid`; keys go to the one with focus). Fixed row height; no border or padding on the box; keep `{}` strings out of functions (return a view of a `Text`); the view height is the box height without borders (and without its padding when it is `content-box`). Ranges: Shift+arrows, Shift+Ctrl+arrow, Shift+Home/End/PageUp/PageDown, Shift+click and dragging (auto-scrolls past the box) extend a range from the anchor (`g.ar`, `g.ac`) to the active cell, a plain move collapses it, Ctrl+A selects all; style with `class:sel={g.inRange(i, j)}` (false for a single cell; `g.r0()..g.r1()`, `g.c0()..g.c1()`). Copy: `g.cells(fn(row, col): string)` (a module function) lets Ctrl+C put the range (or active cell) on the clipboard as TSV (cells with tab, newline or quote are quoted, quotes doubled); Ctrl+X also fires a clear. Ctrl+V parses the clipboard TSV and fires ONE `on:edit` with `kind == 2` (paste/clear): `row`/`col` is the top left, `g.pasteRows` x `g.pasteCols` cells (clipped to the grid) are read with `g.pasteCell(i, j)`; one field pasted over a range fills it; the pasted block becomes the selection. `g.queuePaste(tsv)` does the same from outside a frame. A direct write to `g.row`/`g.col` collapses the range (use `g.goto`). Copy and paste are capped at 4 MiB (a cut field ends at a character boundary) and 200000 cells. Frozen columns: `g.freeze(n)`; mark the first `n` cells of each row and header with `class:frozen={g.fz(j)}` and `style:left={g.bx(j)}` (body) / `{g.hx(j)}` (header), give the row and header row `style:padding-left={g.fw()}`, and style `.frozen` as `position: absolute; top: 0; z-index: 2` with a background (the row, and `.head`, `position: relative`): they stay while the rest scroll under, costing the visible rows only when the scroll changes. The header row stays out of the box and follows the scroll (`margin-left`); there is no general `position: sticky` on `left`. Resize: put a thin `position: absolute; right: -4px; width: 8px; z-index: 3` child in each header cell with `on:mousedown={g.grab(j)}`; dragging sets `g.widths` through `setWidth` (header and rows read `g.w(j)`/`g.total`), never below `MIN_W` (24 px), commits an open editor first, keeps the selection and the grid's focus, and on release fires `on:edit` with `kind == 3`: `col` is the column (read `g.w(col)` to save it); an `on:edit` handler must test `kind == 2` for paste. `g.colAtView(x)` is the hit test with frozen columns. `speculative/26_ui_grid`.
  Caret popup (mentions, emoji): `on:trigger={..}` on a text box reports the word at the caret that starts with a trigger character (`triggers="@:"`, a literal; default `@` and `:`) at the start of the text or after whitespace:
  `trigger` is `@ja` (empty when it ended: the caret left it, the box lost focus, a selection, or Escape), `tlo` and `thi` its byte range for `Dom.insertToken`. It fires on each change. Show the list as `{#if open}<div popover="caret">..buttons..</div>{/if}`
  in the same wrapper as the box: the popover hangs below the caret line (above when there is no room), follows wraps and scrolling, and takes no focus (a click on an entry keeps the caret). It anchors at `Dom.caretRect(id)`, worked out from the box's layout and current text (so it does not trail a keystroke); that works in a `<textarea>` and a one-line `<input>` (type text or search), not a password box (it hangs under the wrapper). While it is open with at least one focusable entry (`button` or `menuitem`) Up and Down move the active entry (the `:hover` look), Enter and Tab click it (so a rich textarea inserts no newline while the list has entries: Escape closes the list, then Enter does)
  (put the work in the entry's `on:click`), Escape dismisses it (`on:close`, and the same trigger stays quiet until it ends); typing and the other keys go to the box.
  Ancestor scroll boxes do not scroll to a focused box. The bound string is a view of the box's buffer: copy it if you keep it.
  A range takes arrow keys (Shift: ten steps), Home/End; `bind:high` (or `high`) makes it a two-thumb range slider. `select`
  opens a list of its `<option>`s (value = `value` attribute, else the text) above everything else, scrolls past its
  `max-height` (8 rows by default), flips above near the window bottom; with `multiple` the bound string is the comma list of picked values in pick order.
  `<input type="radio" name="size" value="m" bind:group={size} />` is a radio button, 16x16 and round (`accent-color`, `:checked`, `:disabled`, `:focus-visible`; the rest of
  the box is yours, as for a checkbox; there is no `appearance`). Radios with one `name` are a group: checking one unchecks the others. The group is the
  radios of that name under the nearest ancestor with `role="radiogroup"`, or the whole page when there is none (there is no `<form>`), so two
  groups with the same name in different `radiogroup` containers (or component instances that each have one) do not meet; a radio with no `name` is its
  own group. **In an `{#each}` list of components, every row's radios of one `name` are ONE group unless each row's element has `role="radiogroup"`** (put it on the row's root); a group that spans keyed rows is reported once in the console warnings. A radio whose `name` changes joins the new group and takes the check from it if it has one. `bind:group={x}`: when the user checks this radio `x` becomes its `value`, and it is checked while `x` equals its `value`; put it on every radio of
  the group. A quoted `value="m"` (no `{}` holes: `x` keeps it) makes `x` a `string`; `value={2}` makes it an `int`; `value={name}` is an int unless `x` is a `let` declared `string`
  (a mismatch with a declared `let` is a compile error). `x` changed by code checks the matching radio and unchecks the rest (all of them when no radio has that value), with no `on:change`; `x` always wins, so a handler that writes `x` back to its old value undoes the user's pick one frame later.
  Without `bind:group`: `checked` (once; the user's later choice stays), `checked={cond}` (applied when `cond` changes) or `bind:checked={flag}` (written for every radio that flipped, the one
  that lost the check too). `on:change={..}` runs only for the radio the user just checked, not for a click on the checked one, a code change or the one that lost the check.
  Up / Left and Down / Right on a focused radio move to the previous / next of its group (wrapping, skipping disabled ones) and check it; Space (or Enter) checks the
  focused one; a click or a click on its `<label>` checks it. A disabled radio can not be checked. Arrow keys on a focused radio belong to its group, even inside a `focus-group="arrows"` (elsewhere in it they still move focus), and never scroll a box. Tab and Shift+Tab stop once per group, on the checked radio (else the
  first enabled one, also after the checked one is removed); the pad reaches every radio, and A checks it. Role `radio` with `checked`, `group="name"` in the a11y dump, and a checked event for each radio that flipped.
  `type="color"` binds `#rrggbb` and opens a saturation/value square and hue bar. Tab and Shift+Tab move focus; Enter or
  Space click a focused button, checkbox or radio. Type letters on a focused select (closed or open) to jump to the option that starts
  with them (case-insensitive for ASCII, the prefix resets after 1 s). The open select list is a real DOM subtree (a child of the select on the overlay layer, `j3.ui.popnode`), so
  ordinary CSS styles it, in theme.css or in the component's own `<style>` (it takes the select's scope): `.j3-select-list` (background,
  border, `border-radius`, `box-shadow`, `padding`, `min-width`, `max-height` (default 216px = 8 rows; longer lists scroll with the
  normal scrollbar), selectors see the select's real ancestors: `.dark .j3-select-list`, `select.wide .j3-select-list`), `.j3-option` (a
  flex row: `height` 24px by default, `height: auto; padding: 8px 12px` for taller rows; `:hover` is the highlighted row, pointer or
  keyboard, `:disabled`, `.selected` the chosen ones), `.j3-check` (an `<svg>` 10 x 8 px drawn in the select's `accent-color`; resize,
  recolor or `display: none` it) and `.j3-label` (the content). Options may contain text plus div/span/p/small/h1/h2, img and svg/path visual markup,
  including conditionals and loops. Popup rows mirror that content without moving the logical source nodes.
  Classes, scoped selectors against the source ancestry (for example `option .name`), `style:` and media updates apply;
  inherited styles come from the popup row. Controls, components, handlers, bindings and focus/overlay attributes on
  visual descendants are compile errors: use `on:input` on the select. The closed label, type-ahead and fallback value
  concatenate all descendant text in document order (supply spaces between fragments); explicit `value` wins.
  Large lists require uniform row heights, including custom content; theme `.j3-option` height to fit icons. A list of more than 40 options builds only the rows near the viewport,
  so 5000 options cost like 40. Color panels and title tooltips are real DOM subtrees too, inheriting their owner's scope and ancestors.
  `.j3-color-panel` and `.j3-tooltip` accept ordinary box styles including shadow, padding and size. Color children are
  `.j3-color-sv` (saturation/value), `.j3-color-hue`, `.j3-color-marker`, `.j3-color-handle`, `.j3-color-preview` and
  `.j3-color-value`; drag hit areas follow their actual boxes. Text boxes: right-to-left text is reordered; caret x, click and
  selection follow the drawn (visual) order, see `j3c doc css` (Complex scripts).
- Scrolling and overlays (`j3.ui.scroll`, `scrollbar`, `overlays`, `toast`; pages 8 and 10 of `examples/big_ui_test`). `overflow: auto`
  (either axis) scrolls with the wheel and gets overlay scrollbars; see `j3c doc css` for `scrollbar-*`, `scroll-behavior`,
  `overscroll-behavior`. Overlays are ordinary elements that you render while they are open (`{#if open}`); the app owns the
  flag and clears it in `on:close`:
    `<dialog modal on:close={open = false}>` is a modal: centered, a dim (`backdrop-color`) behind it, the page takes no mouse
    or Tab, focus goes to its first control and back on close; Escape asks to close (`closedby="any"` also on a press outside).
    `<dialog modal autoselect>` focuses its first text box on open with all its text selected (typing replaces it; only on
    open, not on a later click or Tab). Enter in a text box that has no `on:submit` clicks the dialog's default button: the
    first `<button default>`, else the first `button.primary` (a disabled one does nothing; a textarea keeps Enter as a
    newline; an open select list or caret popup takes Enter first). A box with `on:submit` keeps Enter to itself.
    `<div popover left={x} top={y} on:close={..}>` is a light-dismiss popover (menu, context menu): a press outside or Escape
    asks to close; a press on a control in the popover's parent (the trigger) does not, so a toggle button works. It is
    placed in the window (kept inside it; `left`/`top` take px, else the CSS or the spot in the flow, e.g. `top: 100%` of a
    `position: relative` anchor). Escape closes only the top overlay; a popover over a dialog sits above it.
    Accessibility (`j3.ui.a11y`, computed only when asked, never per frame): every element has a role from its tag (`button`,
    `input` by type: `checkbox` `radio` `slider` `textbox` `searchbox`; `select`: `combobox`/`listbox`; `option`, `dialog`, `h1`/`h2`:
    `heading` (`h1`-`h6`), `list` (`ul`, `ol`), `listitem`, `link`, `separator` (`hr`), `nav`, `main`, `header`, `aside`, `section`, `img`, `p`) or from `role="list"`. Name: `aria-label` (an `<img>`
    `alt` counts), else the text of a button, menu entry, option or heading, else for a control the text of the `<label>` that
    wraps it, then `placeholder`, then `title`. An icon-only button needs `aria-label`; a control beside a label needs `<label for="id">` or wrapping. `display: none` and `visibility: hidden` elements are left out (a `display: none` one takes its content with it) and cannot take focus; `a11y.language()` is the document language (`j3.i18n.locale()`, not in `dump`, so goldens keep their text). Keyboard:
    Tab / Shift+Tab go over buttons, inputs, selects, `menuitem` and `tabindex="0"` elements in document order, except that elements with a positive `tabindex` come first in ascending order (ties in document order), as in HTML, and Shift+Tab goes the other way; both wrap; inside a modal dialog, or a `focus-group="trap"` holding the focus, the order is computed within it; from an element Tab skips (`tabindex="-1"`) it goes on to the next after it, and with no focus it starts at the first (Shift+Tab: the last) (`tabindex="-1"` is focusable by click and code but skipped by Tab and the pad; the pad moves geometrically and ignores `tabindex` values; a clickable `<div>` needs `tabindex="0"` and a `role` to be reachable); Enter / Space click a button, entry or
    checkbox and open a select; arrows move a slider and a select; a modal dialog traps Tab. While a text box, select or grid has focus the UI owns the keyboard: game code's `j3.game.keyDown` / `keyPressed` / `keyReleased` read false (until the frame after it blurs, so the Escape that leaves a box does not reach the game); `keyDownRaw` / `keyPressedRaw` see every key (a screenshot key), and `d.takeKeys = false` turns it off. `domhtml.dumpA11y(d, path)`
    writes the tree (`role "name" states actions=focus,click,set`, a password box shows `secret`, never its value) for goldens;
    `a11y.perform(d, id, A_FOCUS, A_CLICK or A_SET, num, text)` does an action between frames (what a platform bridge calls;
    a focus from it shows `:focus-visible`).
    More state: `aria-selected` (`="true"`, `={cond}`), `aria-live="polite|assertive|off"`, `aria-describedby="id other"` (static
    ids; `a11y.describedBy`), `<details>` has `expanded`, `<progress>`/`<meter>` are named like a control (`aria-label`, a wrapping
    `<label>`, `<label for>`, `title`); a `<label for="x">` names only `x`, even around another control. For a bridge: AT ids are
    node handles (`d.handle(id)`; every `a11y` call takes one, a gone node answers empty); `a11y.bounds` is the absolute rect (scroll,
    transform and flip applied); `a11y.text(d, id, &info)` gives a text box's value, caret and selection (byte offsets; false
    for `type=password`); after `a11y.enable(d)`, `a11y.events` (focus, checked, expanded, selected, value, children, name; filled only
    by frames where something changed) and `a11y.liveChanges` (the text of changed live regions, polite or assertive; a region's first
    frame is silent) are drained once per frame; `a11y.inbox`/`post`/`drain` carry an action from another thread and wake an idle
    loop (`Dom.requestFrame`, `Dom.wantsFrame`; see Idle frames). Everything but `post` and `requestFrame` is main-thread.
    Menu keys: in a `popover` the elements with `menuitem` (a bare attribute, focusable like a button) and every button, input or
    select are the entries. Down or Up on the trigger enters the menu, then Up/Down/Home/End move between entries (wrapping, disabled
    ones skipped), Enter or Space click one, Right on an entry that has a child of class `sub` opens it (it shows through the
    usual `.host:hover > .sub` rule) and focuses its first entry, Left or Escape inside a `.sub` closes that level only, Escape on the
    top level closes the popover and focus returns to what had it before. Style the focused entry with `:focus`. The pointer leaving a
    host toward its `.sub` keeps it open for 300 ms. Text boxes, sliders and selects inside a menu keep their own arrow keys.
    Gamepad and focus navigation (`j3.ui.padnav`, `j3.ui.padin`; each frame in `inputs.begin`, idle cost is reading the pad): the D-pad or left stick
    (half way) moves focus to the nearest focusable element in that direction by box position (`absX/absY/w/h`), one move at once, then
    repeating after 400ms every 120ms. Elements overlapping the focused one on the other axis win, the nearest of them first; then the nearest by
    gap and twice the sideways distance. Only focusable (shown, enabled), modal-allowed elements count, so a modal traps focus; with nothing
    focused the first move takes the first focusable. A is Enter on the focused element (button click, checkbox toggle, `tabindex="0"` summary toggles
    its details, select opens), B is Escape (closes a select list, popover or modal, which gives focus back to what had it), LB / RB set
    `d.padShoulder` (-1 / 1, for one frame: switch your tab), LT / RT scroll the focused element's scroll box a page (else the first scroll box), the right
    stick scrolls it. The pad makes focus `:focus-visible` (`d.setKeyboard(true)`) and the ring goes when the mouse moves; a move reveals the new
    focus in its scroll boxes (instant). Inside an open select or popover menu, and Left/Right on a slider, the directions are the arrow keys; a text
    box takes the pad direction as a focus move (a pad has no caret keys): `d.padText` is the box the pad just focused, for an on-screen keyboard
    (none is built in; without one the field takes keyboard input only). Attributes (static strings): `focus-group="wrap trap arrows"` on a container
    (`wrap`: a move that leaves it comes round to the far side, level with the focus; `trap`: moves stay inside; `arrows`: the keyboard arrows move
    focus too, except from a text box, slider, select or open menu, which keep theirs), and `nav-up|down|left|right="id"` on an element or a group
    (that direction goes to the element with that `id`; an id that is not on the page is ignored with one `css.warns` entry). Tab order stays document order.
    Tests and games feed pads with `d.setPad(padin.parse("Down A"))` (names `Up Down Left Right A B LB RB LT RT SUp SDown SLeft SRight RUp RDown`; it
    holds until set again, `d.usePad(false)` polls `j3.game` again) or `testkit.padPress(d, f, at, "Down")` / `padHold`. `j3.game` polls pads 0 and 1 merged (any pad's button counts, the larger push wins an axis, so whichever is used drives the one focus) and
    drops an unplugged pad's handle (`game.padConnected(i)`); `PadRaw { attached: false }` means no pad: nothing is read, a held direction stops, and with none
    plugged in a frame costs two pointer checks. `padnav.usingPad(d)` says the last input was the pad (until a key or a mouse move); `<glyph>` (below) builds on it. Not built: action sets (plan 017 item 5; the `PadRaw` given to
    `d.setPad` is the seam it feeds), Steam Input (plan 025).
- `<glyph action="confirm" />` is an input prompt that shows the button for an action on the device used last and switches live (a pad button or stick: pad; a key, a mouse move or press: keyboard). `Press <glyph action="confirm" /> to start` reads "Enter" in a keycap, "A" in a round pill once the pad is touched. Built-in `action`s (what `padnav` acts on): `confirm` Enter / A, `back` Esc / B, `prev` Shift+Tab / LB, `next` Tab / RB, `up` `down` `left` `right` arrows / D-pad, `page-up` `page-down` PgUp PgDn / LT RT, `scroll` Wheel / right stick. A game's own action: `key="Space"` and/or `pad="x"` (pad names `a b x y` = bottom, right, left, top face button; `lb rb lt rt up down left right start select ls rs`), alone or beside `action`; each is text or `={expr}`, explicit ones beat the tables, and an action with no button on the device shown falls back to the other device's label. The box is filled by the runtime (no children) and is inline-block, so it sits in a text line and `vertical-align: middle` works. Style it with `glyph`, `glyph[type=key]` (keyboard) and `glyph[type=pad]` (pad); the tag defaults (a bordered, rounded, translucent box at 0.85em, a pill for a pad) lose to any author rule. Pad labels follow `d.setGlyphStyle("xbox" | "playstation" | "nintendo" | "generic")`: xbox A B X Y LB RB LT RT; playstation ✕ ○ □ △ L1 R1 L2 R2 (a glyph whose font lacks those symbols, checked once it has a style, says Cross Circle Square Triangle instead); nintendo B A Y X L R ZL ZR (A and B swapped by position). `start` and `select` are Menu View (xbox), Options Share (playstation), + - (nintendo), Start Select (generic); `ls` and `rs` LS RS (R3 L3 on playstation). `d.setGlyphBinding(action, key, pad)` replaces what an action shows ("" keeps the built-in for that device): the seam for a rebinding screen. Accessible name: role img named by its label ("Enter", "A button"). Any key press, a mouse move or press, or a pad input switches the device. A device switch restyles and re-lays out only the glyph boxes; a steady frame allocates nothing. Drawn boxes only: no image atlas for glyphs.
    `title="text"` on any element is a tooltip (`.j3-tooltip`; a newline starts a new line: `title={"a\nb"}`) after the pointer rests
    `--j3-tooltip-delay` (default 500ms, set on `:root`; a bad value warns and keeps the last good one). Titles wrap at
    320px or the viewport by default; set `.j3-tooltip { width: 180px }` to change it. `white-space: pre-line` preserves
    newlines and `overflow-wrap: break-word` breaks long words. A tooltip is passive and can appear beside an open select or color panel. `on:contextmenu={.. mouse.x ..}`
    runs on a right press or Shift+F10 on a focused owner (`mouse` is viewport px; keyboard requests use its bottom-left).
    Render a conditional child `<div popover="context" on:close={open = false}>` inside the owner, with
    `on:contextmenu={open = true}` on the owner. It captures this frame's request, clamps to the viewport, focuses the
    first entry, and restores the previous focus on close. Ordinary `popover left={x} top={y}` still works.
    For delayed custom tooltip markup, use an owner wrapper (not a button containing other buttons):
    `<div bind:tooltip={card}>{#if card}<div popover><span>Details</span><button>Action</button></div>{/if}</div>`.
    `card` is a writable bool; the card must be a direct conditional child popover. It opens after `--j3-tooltip-delay`
    without taking focus, suppresses the owner's title while shown, and remains open while the pointer or focus is
    inside. Leaving has a 300ms crossing grace. Escape/outside close it; Escape suppresses reopening until the pointer
    leaves. Modal filtering and owner removal close the card. Style its ordinary markup with CSS; default anchoring is
    the owner box, and CSS insets or `left`/`top` can place it below the owner. Nested menus still use
    `.host:hover > .sub { display: block }` and `position-try-fallbacks: flip-inline`.
    Menu type-ahead uses a 1s prefix, ASCII case-insensitive and exact UTF-8 bytes; a repeated ASCII letter cycles.
    Prefixes reset on menu level changes and close. Labels use `aria-label`, else a visible text descendant;
    hidden/disabled entries and other submenu levels are skipped. Text boxes, sliders and selects keep their input.
    Drawers and scrims are `position: fixed` boxes with `transition` on `left`; `pointer-events: none` lets a fading
    scrim through. Toasts: `j3.ui.toast.Toasts` (slots, expiry) rendered in a fixed column with `{#each}`.
- Panels (`j3.ui.panels`, `panelmath`; page 12 of `examples/big_ui_test`). `<div class="win" panel>` (a `position: absolute` box
  in a `position: relative` parent, or `position: fixed`) is a draggable window: a descendant with `panel-handle` drags it (a
  button or input inside the handle keeps its click), a descendant with `panel-resize` resizes it from its bottom-right corner.
  It stays inside its workspace (the parent's box, or the window), never goes below its CSS `min-width` / `min-height` (80 by
  36 otherwise) and is pulled back in when the workspace shrinks (one frame later). `panel="dock"` also snaps it: a drag
  that ends within 28 px of the workspace's left, right or bottom edge shows that zone while dragging and docks there (full
  height, or full width at the bottom, keeping the floating size); dragging a docked handle floats it, and a grip on a docked
  panel resizes along the dock axis. The state lives with the element (a branch that removes the panel forgets it), and
  `d.panelDock(id)` reads the zone. Bring-to-front is not done: later siblings and `z-index` decide the order.
- Client-side title bar (`window-drag`, `j3.ui.windrag`): with `app.undecorated()` (no system title bar), an element with
  `window-drag` moves the window when dragged, through the window manager (`game.setDragRegions`), so Super+drag, edge snap
  and tiling keep working; double-clicking it toggles maximize. Its focusable descendants (`button`, `input`, `select`, or any
  element with `tabindex` / `menuitem`) stay clicks; a plain `<div on:click>` inside it drags instead, so give it `tabindex`.
  The boxes come from the previous frame's layout and ignore clipping. Window calls are in `j3.game`
  (`j3c doc j3.game setDecorated`): `minimizeWindow()`, `setMaximized(!windowMaximized())`, `closeWindow()` (runs `App`
  shutdown callbacks like the WM close button). An undecorated window resizes from an invisible 6 px band on its edges
  (`game.setResizeBorder`). Linux: X11, and Wayland through SDL's Wayland driver (`SDL_VIDEODRIVER=wayland`; SDL2 uses
  XWayland on GNOME by default, where it is X11). Headless these are no-ops.

      <script> use j3.game; </script>
      <header class="bar" window-drag>
          <span class="title">{title()}</span>
          <div class="spacer"></div>
          <button on:click={game.minimizeWindow()}><icon name="dash-lg" /></button>
          <button on:click={game.setMaximized(!game.windowMaximized())}>
              <icon name={if game.windowMaximized() { "fullscreen-exit" } else { "square" }} />
          </button>
          <button on:click={game.closeWindow()}><icon name="x-lg" /></button>
      </header>
- Item drag and drop (`j3.ui.drag`; `speculative/35_ui_drag`, `j3c/stdlib/tests/drag_test.j3`): inventory, hotbar and equip slots.
  `draggable` (or `draggable={bool}`) marks a source; a press that moves 4 px starts the drag (a press that does not is an
  ordinary click, and a drag is never followed by a click). `drag-data={string}` and `drag-id={int}` on the source are the payload.
  Handlers (run in markup order within a frame; compare a `data` string with `same(data, "x")`, `use j3.text.same;` in the script): `on:dragstart` and `on:dragend` on the source (`dragend` always fires, dropped or
  not, and has `dropped: bool`; it comes the frame AFTER the release, so `drop` is always seen first wherever the elements are in the markup: move items in `drop`, tidy up in `dragend`); `on:dragenter` `on:dragover` `on:dragleave` `on:drop` on the element under the pointer: the innermost
  element at or above it with any of those four, so a slot with children is one target (`dragover` fires every frame it stays,
  `dragenter` / `dragleave` when it changes, and `dragleave` also on Escape; no `dragleave` before a `drop`). Handlers can read
  `data` (the `drag-data` string), `dragId` (the `drag-id` int) and `mouse` (local to the element; not in dragstart / dragend).
  A target accepts a drop by having `on:drop`; `on:dragover={if dragId == 3 { accept = false; }}` rejects for as long as
  the pointer stays on it (no `:drop-target`, no `drop`). Escape (or the window losing focus) cancels (`dragend` with `dropped` false); releasing anywhere
  else is `dragend` only. If the page removes the source mid-drag (or in `drop`, so its `{#if}` is gone the next frame) the drag ends and no `dragend` is
  delivered, since its element is gone: do the cleanup in `drop`. A removed target just stops being one. Style with CSS only: `:dragging` matches the source for the whole drag, `:drop-target` the element
  with `on:drop` that the pointer is over. Ghost: a copy of the source follows the pointer at 60% opacity above everything
  (not clipped; the source keeps its own `:dragging` look); `drag-ghost="none"` skips it for a game that draws its own.
  Text boxes never start a drag, and static text selection can fight a draggable that holds text: set `user-select: none`
  on it. Only the pointer drags, one at a time: no files, no OS drag and drop, no touch gestures. Tests: `testkit.dragText(d, f, 3,
  "Sword", "Slot 2")` (also `drag` by node, `dragTag`, `dragTo` a window point), frames 3..8, Escape with `simKeys` on frame 7.
- Sticky headers (`j3.ui.sticky`): `position: sticky; top: Npx` keeps a box under the top edge of its nearest scrolling
  ancestor until its parent ends. Give it a `z-index` so it paints and takes the mouse above the content that slides under it.
- Responsive layout: `@media screen and (max-width: 900px) { .grid { grid-template-columns: repeat(2, 1fr); } }` (px
  `min-` / `max-` `width` / `height` joined by `and`, with `not` and `,` / `or` lists) is re-tested when the window size changes; styles restyle only when a
  condition flips (and transition like any style change). A collapsible sidebar is a `width` `transition` on a class.
- Images and shapes (`j3.ui.media`, `mediastyle`, `imgfit`, `j3.gfx.nineslice`, `clip`, `path`; page 13 of
  `examples/big_ui_test`). `<img src="assets/a.png">` loads once per path (a missing file warns once and draws nothing),
  takes its natural size or keeps its aspect from one of width/height, and honors `object-fit` / `object-position`,
  `border-radius`, transforms and opacity. `<img sprite="4,2" frame={n}>` shows one cell of a 4x2 sheet; `fps="8"` plays it
  (negative plays backwards). `<svg viewBox="0 0 24 24"><path d="M2 2 L22 12 Z" fill="currentColor"/></svg>` draws SVG path
  data (`M L H V Q T C S A Z`, relative forms, holes by winding, or `fill-rule="evenodd"`; `fill` / `stroke` take `currentColor`, `accent` (the `accent-color`), a color, `none`); no groups, SVG transforms, gradients or external `.svg`.
- Icons: `<icon name="trash" />`. Names are Bootstrap Icons ids (icons.getbootstrap.com, v1.11.3, MIT): if you know
  them, just use them; otherwise `j3c doc icons` lists all 2050 and `j3c doc icons <word>` filters (`j3c doc icons arrow`).
  An unknown name is a compile error that suggests close ones. Your own: drop `icons/<name>.svg` in the project (beside
  `j3.toml`); it is read at build time and beats a Bootstrap name (shapes: path, circle, ellipse, rect, polygon, polyline,
  `fill-rule`; no strokes, gradients, `<g transform>`). The box is inline-block, 1em square, `vertical-align: middle`, filled
  with `color`, so `<button><icon name="floppy" /> Save</button>` needs no CSS; size it with `font-size` or `width`/`height`
  (tag defaults lose to any author rule). A computed name takes its choices from the string literals in the expression:
  `<icon name={if muted { "volume-mute" } else { "volume-up" }} />`; only the icons named in the project are compiled in.
  `aria-label` names it for screen readers. No children.
  CSS adds `background-image: url()`, nine-slice `border-image: url(..) 12 fill / 12px`, `clip-path`, `mask-image` and
  `image-tint`: `j3c doc css`.
- Post effects (`j3.gfx.post` behind `j3.gfx.paint`; page 14 of `examples/big_ui_test`). `backdrop-filter: blur()` for frosted
  glass, `filter:` (blur, bloom, grain, scanlines, crt, color functions), `mix-blend-mode`, and the `perspective` / `rotate-x` /
  `rotate-y` plane projection are CSS (`j3c doc css`, Paint). Custom GLSL and render-to-texture are painter calls
  (`paint.shader`, `paint.target`, `beginTarget` / `endTarget`): `j3c doc j3.gfx.paint`. GL only.
- Game HUD patterns (bars with a damage trail, cooldown sweeps, inventory with rarity borders and hover cards, damage
  numbers, quest toasts, a typewriter dialogue box, minimap, world-space nameplates) are plain components on these
  features: `examples/big_ui_test/ui/game/*.j3ui` (page 15), state in `src/gamedemo.j3`.
- Loading components (page 16 of `examples/big_ui_test`, `ui/load/`): `load.Spinner` (a conic-gradient border turned by a
  transform; `tone`, `big`), `load.Dots`, `load.Progress` (a shimmering fill slid by `left`, or a running chunk when `busy`) and
  `load.Skeleton` (grey bars with an animated-gradient sheen). They are plain components, copy them into your project. Wave
  text, animated gradients and border and shadow pulses are on the same page (`j3c doc css`, Motion and Text).
- `<canvas on:draw={...} on:mousedown={...}>`: the game is one box in the layout. Inside `on:draw`
  the viewport is the box (`0,0` is its corner, `size` is in scope); `mouse` is canvas-local.
  `on:mousedown` / `on:mouseup` fire for any button (on any element, bubbling to ancestors), with `button` in scope (0 left, 1 middle, 2 right);
  only the left one focuses, clicks and sets `:active`.
  Children are a HUD over the game. A button on top swallows the click.

CSS (scoped per component, plus the theme). Selectors `*` tag `.class` `tag.class` descendant, child `>`,
`:hover :active :focus :focus-visible :focus-within :disabled :first-child :last-child :nth-child(An+B) :not(...)` `:has(> img, .x, + p)` (limited) `:checked :open :enabled :empty :placeholder-shown :only-child :nth-last-child() :first-of-type :last-of-type :only-of-type :nth-of-type() :nth-last-of-type()` `:dragging` `:drop-target` (item drag and drop) `[type="range"]` `:root`; `var(--x, fallback)`; flex, grid
(`repeat(n, 1fr)`), flex-wrap, absolute, px / % / `vw` `vh` / `calc()` `min()` `max()` `clamp()`, padding and margin, border, radius, opacity, colors,
`transition`, `transform`, `animation` + `@keyframes`, `@font-face`, `@media`, `position: sticky`/`fixed`, gradients, shadows,
`text-shadow`, images, `clip-path`, `mask-image`, `filter`, `backdrop-filter`. A scoped rule beats a global one of the same shape.
No `!important`, floats. No browser default styles; `margin: auto` centers; no margin collapsing. Every property and known difference: `j3c doc css`.

Dev loop and hot reload. Under `j3c run`, edits to a `<style>` block or theme.css show up in the running window
within about 250 ms. A markup, script or `.j3` edit needs a rebuild: `j3c run --hot` does it for you (watches `.j3`, `.j3ui`,
`.j3s`, `j3.toml`; saves within 150 ms are one build). The old program keeps running while it builds; a build error prints
`file:line` and changes nothing, a good build restarts the program with its window position, size and mode kept. Keep app
state too with `app.hot(save, load)` (`fn save(w: &World, s: &Save)` / `fn load(w: &World, s: &Save)`, `j3.data.save`
values; only called in a hot restart, so release builds are untouched); without it the app restarts fresh, and UI state
(focus, scroll, typed text) always restarts. No in-window error banner: read the terminal. It is opt-in (`--hot`) because
a plain `j3c run` streams nothing until exit and has no watcher to maintain; the flag is ignored (with a note) for projects
with `.j3s` scripts, whose own watcher swaps script edits live. A program with its own main loop (no `App.run`) calls
`app.hotPoll()` each frame and leaves when it is true, saving to `app.hotStatePath()`; otherwise it is killed 3 s after a
rebuild. Watched: the project folder (not target/, .git, hidden folders), plus `J3_STDLIB` if set. Ctrl+C or SIGTERM
stops j3c and the program. CSS warnings print at startup as `warning: file:line:col: message`.

Test and debug without a window (agents never open one): `j3c doc testing`. In short:
`j3c run --frames 30 --screenshot shot.png --headless`, then read the PNG; goldens are
`J3_CAPTURE=out.png:N J3_FIXED_DT=0.016` plus `j3c -e imgdiff golden.png out.png`; for the layout
as text call `j3.ui.domhtml.dumpHtml(&root.dom, "target/ui-snapshot.html")` (`use j3.ui.domhtml.dumpHtml;`) after
`root.render(...)` (static HTML + CSS to diff or open in a browser, with checked, disabled and entered values). `root.dom.setFrameMs(16.0)` fixes the UI clock.

Inspect a running UI (devtools): `root.dom.setInspector(true)`, or F12 with `J3_INSPECT=1`; click pins an element and a panel shows its
box, computed style and matched rules with `file:line`. `J3_INSPECT_DUMP=#id:30` prints that text at frame 30 (`j3c doc testing`).

Cost. Layout and styles run only when something changed (an idle frame is about 40 us for 1000
nodes). Text and shapes are batched: a clip, a canvas, or a texture change ends a batch.
