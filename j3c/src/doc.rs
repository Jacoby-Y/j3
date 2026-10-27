//! `j3c doc` — public stdlib surface from the same parser. No second frontend.

use std::path::{Path, PathBuf};

use crate::compile::scene_scripts;
use crate::parse::syntax::{
    keyword_names, ATTRIBUTES, BUILTIN_TYPES, INTRINSICS, J3UI_CALL, J3UI_END, J3UI_IF, J3UI_POP,
};
use crate::parse::{self, EnumItem, Expr, FieldItem, File, FnItem, Ident, Item, StructItem, Type, TypeKind};
use crate::project::{self, Input, Manifest};
use crate::span::{FileId, Span};
use crate::stdlib::stdlib_dir;

const HIDDEN: &[&str] = &["game_headless.j3", "sys_windows.j3", "sys_macos.j3", "netsys.j3", "netsys_windows.j3", "netsys_macos.j3", "threadsys.j3", "threadsys_windows.j3"];

/// Public so the runtime or editor can reach them, but not what a game calls.
const INTERNAL: &[(&str, &str)] = &[
    ("j3.scene3d.editor.", "editor internals"),
    ("j3.script", "VM behind `.j3s` scripts; games use `@script` (`j3c doc lang`)"),
    ("j3.gfx.urp.shaders", "GLSL for j3.gfx.urp.raytrace"),
];

const USAGE: &str = "\
usage: j3c doc [topic | module [item]] [--short]
    j3c doc                    tasks, then every stdlib module
    j3c doc j3.app             public items and /// comments
    j3c doc j3.app --short     names only, grouped by type
    j3c doc j3.app create      one item (and where it is defined)
    j3c doc j3.app App.create  one member of one type
    j3c doc app                same as j3.app
    j3c doc drawText           an item by name, when only one module has it
    j3c doc search <word>      modules, public names, then comments with the word
    j3c doc recipes [name]     short complete programs: app2d, scene3d, canvas, script, ...
    j3c doc scripts/door.j3s   a script: props, shared, state, public calls
    j3c doc j3.scene3d.scripts the module made from this project's scenes (run in a project)
    j3c doc mygame.world       a module of the project in this directory
    j3c doc lang               language
    j3c doc j3ui               `.j3ui` components (markup + CSS, compiled by j3c)
    j3c doc j3ui-immediate     `@j3ui` functions over the immediate-mode `j3.ui`
    j3c doc css                the CSS subset `.j3ui` supports, and how it differs from a browser
    j3c doc testing            run, capture, and diff headless (no window); goldens; HTML snapshot
    j3c doc editor             3D scene editor, scene files, scripts
    j3c doc urp                URP render passes (pass order, GL lifetime)
    j3c doc threads            threads and worker pools: rules, safe modules, mistakes
    j3c doc i18n               translated text: locale files, t/tf, plurals, numbers, RTL, j3c check
    j3c doc ship               release builds, zips, and .deb installers with a menu entry and icon";

pub fn cmd(rest: &[String]) -> i32 {
    let mut args: Vec<&str> = Vec::new();
    let mut short = false;
    for a in rest {
        if a == "--help" || a == "-h" {
            println!("{USAGE}");
            return 0;
        }
        if a == "--short" || a == "-s" {
            short = true;
            continue;
        }
        if a.starts_with('-') {
            eprintln!("j3c: unknown flag `{a}`");
            return 1;
        }
        args.push(a.as_str());
    }
    let text = match args.as_slice() {
        [] => index(),
        ["lang"] => Ok(lang_page()),
        ["j3ui"] => Ok(J3UI_PAGE.to_string()),
        ["j3ui-immediate"] | ["@j3ui"] => Ok(j3ui_page()),
        ["css"] => Ok(CSS_PAGE.to_string()),
        ["testing"] | ["test"] | ["headless"] | ["golden"] | ["goldens"] | ["imgdiff"] => Ok(TESTING_PAGE.to_string()),
        ["editor"] => Ok(editor_page()),
        ["urp"] => Ok(urp_page()),
        ["threads"] | ["thread"] => Ok(THREADS_PAGE.to_string()),
        ["i18n"] | ["locale"] | ["locales"] | ["translation"] | ["l10n"] => Ok(I18N_PAGE.to_string()),
        ["ship"] | ["dist"] | ["deb"] | ["package"] | ["packaging"] | ["release"] => Ok(SHIP_PAGE.to_string()),
        ["recipes"] | ["recipe"] => Ok(crate::helptext::recipe_index()),
        ["recipes", name] | ["recipe", name] => crate::helptext::recipe(name),
        ["search", word] => search(word),
        [query] if query.ends_with(".j3s") => script_doc(query),
        [query] => lookup(query, None, short),
        [query, item] => lookup(query, Some(item), short),
        _ => Err(USAGE.to_string()),
    };
    match text {
        Ok(s) => {
            print!("{s}");
            if !s.ends_with('\n') {
                println!();
            }
            0
        }
        Err(e) => {
            eprintln!("j3c: {e}");
            1
        }
    }
}

fn script_doc(query: &str) -> Result<String, String> {
    let path = PathBuf::from(query);
    let src =
        std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let module = ["script".to_string(), "doc".to_string()];
    let file = parse::parse_script(&src, FileId(1), &module)
        .map_err(|e| parse::format_error(query, &src, &e))?;
    let mut out = format!("script {query}\n\n");
    let mut state = String::new();
    let mut calls = String::new();
    for item in &file.items {
        match item {
            Item::Props(b) => {
                out.push_str("props: set by whoever makes an instance; read-only inside the script\n");
                push_script_fields(&mut out, &b.fields, &src, true);
            }
            Item::Shared(b) => {
                out.push_str("shared: one value for every instance; only the script reads or writes it\n");
                push_script_fields(&mut out, &b.fields, &src, false);
            }
            Item::State(s) => {
                state.push_str(&format!("    {}", s.name.name));
                if let Some(ty) = &s.ty {
                    state.push_str(&format!(": {}", ty_str(ty)));
                }
                state.push_str(&format!(" = {}\n", expr_text(&src, &s.value)));
            }
            Item::Fn(f) if f.vis_pub => push_fn(&mut calls, f, None, "    "),
            _ => {}
        }
    }
    if !state.is_empty() {
        out.push_str("state: each instance's own; private to the script\n");
        out.push_str(&state);
        out.push('\n');
    }
    out.push_str("public calls: `instance.name(...)` from J3 (outside code can't read fields)\n");
    if calls.is_empty() {
        out.push_str("    (none)\n");
    }
    out.push_str(&calls);
    Ok(out)
}

fn push_script_fields(out: &mut String, fields: &[FieldItem], src: &str, props: bool) {
    for field in fields {
        out.push_str(&format!("    {}: {}", field.name.name, ty_str(&field.ty)));
        match &field.default {
            Some(e) => out.push_str(&format!(" = {}", expr_text(src, e))),
            None if props => out.push_str(" (required)"),
            None => {}
        }
        out.push('\n');
        push_doc(out, &field.doc, "        ");
    }
    out.push('\n');
}

/// `j3c doc urp`: the URP guide, embedded so it matches this j3c build.
fn urp_page() -> String {
    format!(
        "`j3c doc urp` — URP render passes

In an editor scene, URP is a scene setting (`j3c -e scene set @scene urp=true`) and
`scene3d.draw` traces it with no extra calls (`j3c doc editor`, URP). The modules below are
separate renderers that a game drives itself.

{}",
        include_str!("../stdlib/j3/gfx/urp/README.md")
    )
}

fn names(list: &[&str]) -> String {
    list.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ")
}

fn lang_page() -> String {
    let keywords: Vec<&str> = keyword_names().collect();
    format!(
        "`j3c doc lang` — the language

`int` is i64. `float` is f64. Gameplay math uses `f32`. A number with no context is `int`, or `f64` when it has a dot. `let` is mutable. A local's type is inferred when you omit it.

A number literal takes the other operand's type: with `far: f32`, `2 * far` and `far * 0.5` are `f32`. A variable never converts: with `n: int`, `far * n` is a type error; write `far * f32(n)`. `f32(x)`, `int(x)`, and friends convert explicitly.

`const NAME = value` is a module constant. The value is an int expression (`+ - * / %`), a float, a bool (`true`, `false`, or `!` of one), or a string. `-1.5` is the float it looks like.
`const NAME: [N]T = [a, b, c];` is a read-only table in static data (ints, floats, bools, strings, structs of those; nested `[2][2]int` works on the LLVM backend only, like any nested array global). It needs its type; a wrong length is a compile error. `NAME[i]` is bounds-checked, `NAME.len`, `for x in NAME`, `NAME[a..b]` and passing it as a `[]T` all work. `NAME[i] = v`, `NAME[i].f = v`, `+=` and `&NAME[i]` are compile errors. A `[]T` made from it is not read-only: a callee that writes through the slice changes the table (it is ordinary module data), so keep such callees read-only. It is private to its module (not importable with `use`).

One file is one module. `use j3.io.printLn` brings in one name. `use j3.memory.{{ Arena, List }}` brings in several. `pub` is what another module can name. A module can be spread over several files: the files in the folder beside `X.j3` (`X/*.j3`) that say the same `module` line are more of it and share its names and `use`s (put the `use`s in `X.j3`).

`if` and `while` take no parentheses. Braces are required. An `if` used as a value needs a final `else`; `else if` chains work (`let x = if a {{ 1 }} else if b {{ 2 }} else {{ 3 }};`), and only the taken arm runs. `&&` and `||` stop at the first decisive operand.

`for i in 0..n` stops before `n`. `for i in 0..=n` includes `n`. `for n in xs` walks a slice or an array. The index name ends when the loop ends.

`match` lists every variant. Write `Enum.Name`, not a bare name. `_` is the rest. `defer` runs at the end of its block, including on `return`. `break` and `continue` stay in the loop.

A backtick string `hello {{name}}` fills the hole. A struct in a hole needs `#[Debug]`.

`+=` `-=` `*=` write the place once (no `/=` or `%=` yet). `%` is the remainder and truncates like C: `-7 % 3 == -1`; `math.wrapi(-7, 3) == 2` wraps an index. Integer overflow wraps. Divide or `%` by zero aborts. `--fast` strips that abort and the bounds-check aborts.

`operator+` is a method. `this` is the receiver. `.` follows a pointer, and works on any value, not just a variable: `f().x`, `(a + b).x`, `xs[a..b].len`, `make().inner.field` (the value is held in a temporary, so you cannot assign to it). A method `this: &T` takes the address at the call. A field may end with `= value`. A struct literal that omits the field uses that value.

Methods live inside the struct body; there is no `fn Type.name(` outside it:

    struct State {{
        n: int;
        ys: [3]int;
        fn add(this: &State, k: int) {{ this.n += k; }}
    }}

`s.add(2)` calls it. A fixed-array field has no `= default` (a field default must be a literal), so write it in every struct literal: `State {{ n: 0, ys: [0, 0, 0] }}`. `List<T>` (`j3.memory`) has `push`, `remove(i)` (keeps order), and `swapRemove(i)` (O(1), moves the last element in); its `items` and `len` fields are public, and `list.items[0..list.len]` is the live slice.

`?` unwraps a `#[TryCatch(Ok, Err)]` enum: the failure variant returns from the function, the success variant continues. `j3.option.Option` and `j3.result.Result` are that attribute. The compiler does not special-case the names.

`#[singleton]` is one process value. Its initializer is a constant, and it cannot be a call. Add `#[thread]` for one value per thread (`j3c doc threads`).

`[N]T` is a stack array. `[]T` is a pointer and a length. `xs[i]` checks the bound. `xs[1..3]` and `xs[:]` are views. `string` is a read-only byte view. Mutate bytes with `j3c doc j3.text`.

`use position: Vec2` on a struct embeds those fields. A statement `use transform` names them in this block, and assigning one writes the field.

Threads are `j3.thread`: hand a worker a struct only it writes, then read the result after a signal (`j3c doc recipes threads`, `j3c doc threads`). `#[thread]` and `@atomic*` are the language side.

`use lib \"SDL2\" as sdl {{ fn SDL_Init(flags: u32): i32; }}` declares C functions. `@syscall`, `@sizeof`, and `@slice` are compiler calls. `@atomicLoad(p)`, `@atomicStore(p, v)`, `@atomicAdd(p, v)` (returns the old value) and `@atomicCas(p, old, new)` (returns whether it swapped) work on an `int` through a pointer. `@j3ui` is `j3c doc j3ui`.

No traits, capturing closures, GC, or borrow checker.

Scripts
    A `.j3s` file is a script: J3 with the same checker, and no `module` line (its path names it). Each value of a script is one instance with its own props and state.
    `props {{ hinge: NodeId; openAngle: f32 = 90.0; }}` is what the importer passes in; props are read-only in the script, and one with no default is required.
    `shared {{ opened: int = 0; }}` is one value every instance reads and writes. A top-level `let` is per-instance state. Every default and initializer is a literal. A script may also declare `struct` and `enum`; those names share the script's namespace, and the script type's name is taken.
    Only `pub fn`s are callable from outside. Outside code can't read an instance's fields or copy one; pass `&Door`.
    `const Door = @script(\"scripts/door.j3s\");` (path from the project root) gives the type; `Door {{ hinge: h }}` makes one, and `door.toggle()` calls it. A script can hold other scripts the same way.
    In a scene, `script=scripts/door.j3s` on an object's line attaches one and `param.<prop>=value` beside it sets its props. `use j3.scene3d.scripts;` then `scripts.start(&scene)` and `scripts.update(&scene, dt)` run every attached script's `pub fn start(scene: &Scene)` and `pub fn update(scene: &Scene, dt: f32)`; `scene.script::<Door>(node)` finds one. A `node: NodeId` prop is the object the script is on.
    `j3c run` runs scripts on a bytecode VM: editing a script re-runs without compiling the program, and while it runs a code edit swaps in. `j3c build` compiles every script in. `j3c dump-bc scripts/door.j3s` shows the bytecode.
    `j3c run path/to/tool.j3s` runs that file once: it makes one instance and calls `pub fn main()`. Arguments after the path are `j3.sys.arg` (`arg(0)` is the program). A scene script is still attached with `script=` on an object, not run this way.

Types
    {types}

Keywords
    {keywords}

Attributes
    {attrs}

Intrinsics
    {intrinsics}

The standard library is `j3c doc`. The window loop is `j3c doc j3.app`. Draw, input, and audio are `j3c doc j3.game`. Generate RGBA assets without a window with `j3c doc j3.gfx.canvas`.
",
        types = names(BUILTIN_TYPES),
        keywords = names(&keywords),
        attrs = names(ATTRIBUTES),
        intrinsics = names(INTRINSICS),
    )
}

const J3UI_PAGE: &str = "\
`.j3ui` components: Svelte-like markup, J3 script, and CSS, compiled by `j3c` to J3 and painted
with `j3.gfx.paint`. `j3c doc recipes components` is a complete project; `j3c new mygame
--template ui` writes it; `examples/idle-breakout` is the large one. The older immediate-mode
`@j3ui` functions are `j3c doc j3ui-immediate`.

Files. `[ui] dir = \"ui\"`, `theme = \"ui/theme.css\"` in j3.toml. `ui/` is scanned recursively
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
        const NAMES: [3]string = [\"a\", \"b\", \"c\"];   // shared, read-only (see below)
        fn helper(): int { ... }    // a plain fn is a derived value
    </script>
    <div class=\"x\" class:on={open} on:click={open = !open}>{state.clicks}</div>
    <style> .x { display: flex; gap: 8px; } </style>

- Everything inside `{}` and the script is real J3 (checked, AOT). Errors point at the `.j3ui` line.
- Handlers are statements, not closures: `on:click={state.buy(k)}`, loop variables are in scope.
  A handler may span lines (`on:click={` newline statements newline `}`); past two statements write a `fn` in the script and call it.
- Prop defaults: `props { gap: int = 8; note: string = \"\"; }`. A use site may leave out a prop that has a default; one without
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
  `scale-from={0.5}`, `slide-axis=\"x\"` (default `\"y\"`, the height), `transition-easing=\"ease|linear|ease-in|ease-out|ease-in-out|bounce|spring\"` (default linear for fade, ease-out for the rest). fly and scale also fade;
  slide shrinks the box's own size in layout (height, or width with `slide-axis=\"x\"`) while its content stays put and is clipped, so siblings glide: the box and its parents are laid out every frame while a slide
  is in flight and not at all after (fade, fly and scale are paint-only, hit testing follows, no relayout). In a flex container the item's main size is scaled, so a `flex-grow` item that slides gets the scaled share; grid tracks do not follow a slide.
  A box removed mid-enter leaves from how far it got. Toggling the `{#if}` (or re-adding the keyed row) while its old copy is still leaving brings that copy back from how much of it is shown, so fast toggling never piles up ghosts.
  `anim-delay={ms}` (adds to the CSS animation-delay, to stagger an `{#each}`).
  `animate:flip={ms}` (default 200) on the row element of a keyed, non-`virtual` `{#each}` (`<li animate:flip={200}>`, also under an `{#if}` that is the row): when a sort, filter, insert or remove
  moves the row, it is drawn from its old place (hit testing follows) and glides to the new one; `animate-easing=\"ease|linear|ease-in|ease-out|ease-in-out|bounce|spring\"` picks the curve
  (default `ease-out`). A flight interrupted by another move continues from where the row is drawn. A window resize, a scroll, and a row's first appearance do not flip; a `virtual` list and
  a component tag are compile errors. Easing is one of the seven presets (not `cubic-bezier()`/`steps()`). A popover or drag anchored to a flipping row uses its layout (final) position. Rows without it pay nothing; a flip needs no relayout, only repaint. Attributes: `x={expr}` or `x=\"a {b}\"`.
- Children (`<slot>`): a component with `<slot />` takes the markup written inside its tag; `<slot name=\"x\">fallback</slot>`
  is a named slot and its children show when nothing fills it. A direct child with `slot=\"x\"` goes to that slot, the rest to the
  default one.
      Card.j3ui:  <section>{title}<slot name=\"actions\" /><slot /><div><slot name=\"footer\">Thanks</slot></div></section>
      user:       <Card title=\"Audio\"><button slot=\"actions\" on:click={reset()}>Reset</button><p>{vol}</p></Card>
  Slot content is compiled in the user's scope (its props, `let`s, `fn`s, `{#each}` variables, `emit`, `bind:`) and styled by the
  user's `<style>`; the Card's scoped CSS reaches its own elements, not the slotted ones. Its state (`{#if}`, child components)
  belongs to the user, but when the Card stops rendering the slot (it is inside the Card's own `{#if}`, say) the content is destroyed
  (`onDestroy` runs) and starts fresh when the slot comes back, as in Svelte. An `{#each}` item is read again from the list for the
  slot (the list expression runs once more). A slot may be passed on (`<Card><slot /></Card>`). Errors: `slot=\"x\"` where the
  component has no `<slot name=\"x\">`, children for a component with no `<slot>`, two default slots in one component. `{#if $slots.footer}` (also
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
- Scrolling from code (`d` is the Dom in a handler, `speculative/53_ui_scroll`). `d.byId(\"x\")` is the node of the first element with static
  `id=\"x\"` (-1 for none; a walk of the tree, call it from a handler, not every frame); dynamic `id={..}` is not found. `d.scrollIntoView(node, block, smooth)` scrolls every scroll box
  around `node`, nearest first, by the least that is needed (`block` is `dom.SC_START`, `SC_CENTER`, `SC_END` or `SC_NEAREST` on y; x is
  nearest, `d.scrollIntoViewXY(node, block, inline, smooth)` sets both), counting the boxes' `scroll-padding` and the node's `scroll-margin`;
  a snapping axis lands on a snap position. `d.scrollTo(box, x, y, smooth)` sets one box's offsets (clamped, snapped). `smooth` glides over ~300
  ms and is instant under reduced motion. `d.scrollToKey(box, \"key\", block)` scrolls a `virtual` list to the row with that key even
  when it is not built (false for an unknown box or key); with measured rows it aims again each frame until the row is measured. The
  focus moved by a key (Tab, arrows) is scrolled into view (nearest) on the next frame.
- Virtual lists. `<div class=\"list\" virtual row-height={26}>{#each rows as r, i (r.id)}...{/each}</div>` keeps only the rows
  near the viewport alive (100k rows are about 60 nodes) and pads the box so the scroll bar is true. The element is the scroll
  box (give it a height and `overflow-y: auto`), holds exactly one keyed `{#each}` over a list (no `0..n`); `i` and the key are
  the real index and key. It sets `padding-top`/`padding-bottom` and `style:` slots 6 and 7 of the box, so use a wrapper for
  your own padding there. Four rows of overscan. No slot content inside it.
  Heights: `row-height={26}` is the pixel height every row must have (set the same in CSS, no `gap`, margin or border between
  rows) and is the fast path. Without it, or `row-height=\"auto\"`, rows may differ (wrapped text, images): each row is
  measured after layout (distance to the next row's top, so margin and `gap` count) and cached by key; a row not seen yet
  counts as the mean of the measured ones, `est-height={40}` before any. The total height corrects as rows are measured and is
  an estimate far from where you have scrolled. Lookup is O(log n), a steady frame does no layout and allocates nothing; a
  frame where the list changed (its length or keys) formats every key once, O(rows).
  Scroll anchoring: when rows above the first visible one change height or are inserted or removed (older chat messages
  loading), the scroll offset moves by the difference so the visible rows stay put. After a long jump into unmeasured rows the
  first frame can be a few px off, until the rows around the view are measured.
  `stick-to-bottom`: while the view is at the end an append keeps it at the end; scrolling up releases it, reaching the end
  again re-engages it (it starts at the end). `bind:at-end={state.atEnd}` (a `bool` place) is written every frame with whether
  the view is at the end, for a \"jump to latest\" button; setting it to true (`<button on:click={state.atEnd = true}>`)
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
  `<td title={f.size()}>{f.size()}</td>`, `class=\"{f.kind()}\"`, a keyed `{#each .. (f.key(i))}` key, `style:` text. The text node,
  `title`, `aria-label`, `placeholder`, `class` words, `style:` values and keys compare bytes and keep their own copy, so a
  steady frame does no work (`J3_STEADY`) and a changed string updates. The view lives until the next frame starts (`App` rewinds the
  scratch arena at the top of every frame; a loop without `App` calls `j3.memory.rewindScratch()` itself, once per frame, before
  `render`). `value=` copies it too. Do not store it in a struct or keep it from an event handler, and do not give it to `bind:`.
- Elements: div span p h1 h2 h3 h4 h5 h6 small header nav main section label button input textarea select option dl dt dd canvas dialog aside img svg path ul ol li a hr strong em b i pre code br progress meter details summary glyph.
- `strong`/`b` are bold, `em`/`i` italic, `pre` keeps whitespace, `hr` is a 1px rule; lists have no markers (add `li::before { content: \"• \" }`), links are `<a on:click={...}>` (no `href` navigation).
- Dynamic style. `class=\"slot r{rarity} {extra}\"`: words with no `{}` are static, the rest are re-read when their text changes
  (a word added this way and also by `class:name` is dropped when the text changes; 8 classes per element). `style:prop`
  is an inline value that beats every rule and transitions like one (`transition: width 200ms` on the bar): `style:width={pct * 100.0}`
  (a number: px for lengths, a plain number for `opacity` `flex` `z-index`), or text through the CSS parser,
  `style:width=\"{pct * 100.0}%\"`, `style:background=\"{color}\"` (`={\"...\"}` with a quoted string also means text; any other `={expr}` is a number).
  Shorthands (`padding` `margin` `border` ...) take text. An empty string clears the override. Any property of `j3c doc css`
  except `animation`, `transition`, grid tracks and placement (`grid-*`) and `--vars`; an unknown name is a compile error, a bad value a warning in
  `css.warns`. At most 8 `style:` per element; on the same property the later one wins. `style:left` / `style:top` offset
  from the containing block like the CSS; the `left={x}` / `top={y}` attributes keep their own meaning (viewport point for
  popups and panels: the box is kept on screen and `position-try-fallbacks` flips about it). Nothing is recomputed on a frame
  where the values did not change. A text value that is a gradient allocates paint data each time it changes.
- Class and style on a component. `<kit.Card class=\"wide {x}\" class:on={cond} style:width={w}>` puts them on the component's root
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
- Attributes: `class`, `class:name={cond}`, `style:prop`, `on:click` `on:mousedown` `on:mouseup` `on:mousemove`
  `on:wheel` `on:contextmenu` `on:close` `animate:flip` `animate-easing` (keyed row element, see Blocks) `draggable` `drag-data` `drag-id` `drag-ghost` `on:dragstart` `on:dragend` `on:dragenter` `on:dragleave` `on:dragover` `on:drop` (item drag and drop, below) `popover` `bind:tooltip` `closedby` `title` `left` `top` `panel` `panel-handle` `panel-resize` `menuitem` `tabindex` (`\"0\"`, `\"-1\"`, a positive integer or `={int}`) `id=\"x\"` (static name, for `#x` selectors) `role` `aria-label` (any element); `dialog` takes `modal` and `autoselect`; `label` takes `for=\"id\"` (a static string, below); `button` takes `disabled` and `default`; `input` takes `type` (checkbox, range, color, text, search, password; any other is a text box). A click on a `label`'s text (or `for=\"x\"` naming the control with `id=\"x\"`, found anywhere in the window, first one in tree order when ids repeat, with a one-time entry in `css.warns`) activates the first control inside it: a checkbox toggles, a button is clicked, a text box, range or select takes focus (a select does not open). The label's own `on:click` fires as well. Not when the click lands on a control already, on a disabled control, or after a drag. The label's text is the control's accessible name. Browsers also hover the control with the label; here only the label is `:hover`
  `min` `max` `step` `value` `high` `checked` `name` `placeholder` `maxlength` `disabled`, `bind:value` / `bind:high` / `bind:checked` / `bind:group`
  and `on:input` / `on:change` / `on:submit`; `select` takes `multiple` `value` `placeholder` `disabled` `bind:value` `on:input`;
  `option` takes `value` `disabled`; `button` takes `disabled`; `textarea` is a void tag (`<textarea bind:value={s} rows=\"4\" />`) taking `rows` `maxlength` `placeholder` `disabled` `rich` `triggers` `bind:value` `bind:spans` `on:input` `on:submit` `on:trigger`; `img` takes `src` `alt` `sprite` `frame` `fps`; `svg` takes
  `viewBox`; `path` takes `d` `fill` `stroke` `stroke-width`. Anything else is a compile error naming the accepted list.
- `<br>` (void, `<br>` or `<br/>`) forces a line break in inline content whatever `white-space` is; two in a row leave an empty line, a trailing one adds none, and static-selection copy gets a newline. It cannot go inside a text box (`<input>` and `<textarea>` take no content).
- `<progress value={v} max={m}>` and `<meter value min max low high optimum>` are controls the runtime paints (inline-block, 160x8 by default; a `width` / `height` resizes them; no children). The element's own `background` and `border-radius` are the track (the
  built-in default is a translucent grey with a 4px radius, any rule beats it); the fill sits inside the border, rounded like the track. A progress fills `value / max` (`max` defaults to 1) with `accent-color`; with no `value` (or a negative one) it is indeterminate: a bar
  sweeps the track every 1.4 s, and sits still in the middle under `prefers-reduced-motion`. A meter fills `(value - min) / (max - min)` in one of three colors the way HTML picks them: the value's region (below `low`, between, above `high`; defaults `min` and `max`)
  against the `optimum`'s (default the middle): same region good, next one ok, far one bad. Theme them with custom properties on `:root` (`--j3-meter-good: #3fb950; --j3-meter-ok: #d29922; --j3-meter-bad: #f85149`), or scoped: `.hud { --j3-meter-bad: #ff00aa; }` applies to the meters inside `.hud` (and `meter.x { }`). a11y roles `progressbar` and `meter` with `min` `max` `now`.
- `<details open={bool}>` with `bind:open={place}` (follows clicks) and `on:toggle={..}` (`open` in the handler is the new state; it fires once for a click or key, and once, a frame later, when the program changes `open`, not for the first value), whose first child is a `<summary>`. A click on the summary (or Enter / Space on it: it is focusable, role `button`, `expanded` state) flips
  the box; a closed `<details>` shows only its `<summary>` (the other children stay mounted, so a component or text box inside keeps its state, but are `display: none`). `:open` matches an open `<details>`. `open={expr}` is applied when the value changes,
  so a click is not undone by the next update; use `bind:open` to follow it. There is no marker: draw one with `summary::before { content: \"▸ \" }` and `details:open > summary::before { content: \"▾ \" }`. A button or input inside the summary takes its own click.
- Input widgets (`j3.ui.inputs`, `j3.ui.dropdown`; page 3 of `examples/big_ui_test`). They have no browser look: style them
  (`input[type=\"text\"]`, `select`, `:focus`, `accent-color`). `bind:value` writes a `string` for text, search, color and
  select, an `f32` for a range (the `type` must be a literal). A text box is one line with a caret, drag and Shift+arrow
  selection, Home/End, Ctrl+arrows (words), Ctrl+A/C/X/V (any length), Ctrl+Z undo and Ctrl+Y / Ctrl+Shift+Z redo (typing and deleting runs are one step each, a paste, cut, caret move or selection edit starts a new one, a new edit clears redo, the last 100 steps or 4 MB of text are kept per box, each step being a copy of the text; a value set by the program is not a step and history is kept, so undo may bring older text back), Backspace/Delete, UTF-8 safe, placeholder; `search` adds a clear
  button and Escape clears; Enter fires `on:submit`. A double click selects a word, a triple click the line (all of a one-line box).
  `maxlength=\"n\"` caps typing and pasting at n code points (a value set by the program is not cut). `type=\"password\"` draws
  one bullet per code point and neither copies nor cuts (the bound string is the real text). `<textarea>` is an `input` of type `textarea` underneath: a `textarea { }` rule, an `input { }` rule and `input[type=\"textarea\"]` all match it. It wraps at spaces, Enter
  inserts a newline (Ctrl+Enter fires `on:submit`), Up/Down keep their column, PageUp/PageDown, Home/End go to the line (Ctrl: the text),
  the wheel and the caret scroll its own view; `rows` (default 3) sets its height, a `height` overrides it. IME: the composition
  string is drawn underlined at the caret and the IME is told where the caret is (window pixels; not corrected for `setVirtualSize`).
  `<textarea rich bind:value={s} bind:spans={sp}>` keeps styled spans beside its text: `sp` is a `[]Span` (`use j3.ui.spans.Span;`; `start` and `len` in bytes of the UTF-8
  text, `kind` `SPAN_BOLD` `SPAN_ITALIC` `SPAN_LINK` `SPAN_MENTION` `SPAN_EMOJI`, `data` the id, emoji name or URL). Ctrl+B and Ctrl+I toggle bold and italic on the selection
  (each an undo step; undo restores spans). A mention or emoji is an atomic token: its text is the chip label, the caret skips it, Backspace and Delete remove it whole,
  a selection edge or click inside it snaps outward, and it wraps as one box (a chip: label plus 4 px each side on an `accent-color` tint; links are `accent-color` and
  underlined, italic has no slanted font and is drawn dimmed). Typing at the end of a bold or italic span extends it, typing at its start or after a link or token does not.
  `Dom.insertToken(id, lo, hi, label, kind, data)` swaps a text range (the typed `@ja`) for a token in one undo step, `Dom.caretRect(id)` is the focused box's caret in window px and
  `Dom.caret(id)` its byte. Replacing the text from the program drops the old spans, so set the new ones in the same update (attribute order does not matter); a program that just echoes `sp` back never brings back stale spans. Copy and cut take plain text (labels
  included), paste inserts plain text. A box without `rich` keeps no spans and pays nothing.
  Data grid (spreadsheet): a fixed `row-height` `virtual` box with `tabindex=\"0\"`, `grid={g.sized(rows.len)}` and `on:edit={..}`, `g: &Grid` (`j3.ui.gridstate`, `Grid.new(mem, cols, rowH, colW)`). Rows are
  `<div style:width={g.total}>` of `{#each 0..g.cols as j}<div class:active={g.on(i, j)} style:width={g.w(j)}>..</div>`; `g.widths` is the one column source for the header and every row (`g.setWidth(j, px)`).
  A header outside the box follows `style:margin-left={0.0 - g.sx}`. The grid box has focus (one roving active cell `g.row`/`g.col`, click picks it): arrows, Ctrl+arrows (to the edge, or along runs of
  cells when `g.hasFilled = true; g.filled = fn(row, col): bool`), Tab/Shift+Tab (wrap to the next/previous row), Home/End (Ctrl: first/last cell), PageUp/PageDown, scrolled into view on both axes. Enter, F2 or typing starts the editor:
  `{#if g.editAt(i, j)}<input type=\"text\" bind:value={g.draft} />{:else}..{/if}`. `on:edit` sees `kind` (0 begin, 1 commit), `row`, `col`, `text` (a view of the editor's buffer, like `bind:value` and `on:trigger`: copy it). On begin write
  `g.draft = g.initial(cellText)` (the typed character replaces the text); on commit write the value. Enter commits and moves down, Tab right, a click elsewhere commits, Escape cancels; undo works in the box. The editor's row is kept alive while it has focus (the `virtual` focus rule), so scrolling away does not end the edit; if its row disappears (the list shrank) the text is committed. Several grids may share a screen (state lives in each `Grid`; keys go to the one with focus). Fixed row height; no border or padding on the box; keep `{}` strings out of functions (return a view of a `Text`); the view height is the box height without borders (and without its padding when it is `content-box`). Ranges: Shift+arrows, Shift+Ctrl+arrow, Shift+Home/End/PageUp/PageDown, Shift+click and dragging (auto-scrolls past the box) extend a range from the anchor (`g.ar`, `g.ac`) to the active cell, a plain move collapses it, Ctrl+A selects all; style with `class:sel={g.inRange(i, j)}` (false for a single cell; `g.r0()..g.r1()`, `g.c0()..g.c1()`). Copy: `g.cells(fn(row, col): string)` (a module function) lets Ctrl+C put the range (or active cell) on the clipboard as TSV (cells with tab, newline or quote are quoted, quotes doubled); Ctrl+X also fires a clear. Ctrl+V parses the clipboard TSV and fires ONE `on:edit` with `kind == 2` (paste/clear): `row`/`col` is the top left, `g.pasteRows` x `g.pasteCols` cells (clipped to the grid) are read with `g.pasteCell(i, j)`; one field pasted over a range fills it; the pasted block becomes the selection. `g.queuePaste(tsv)` does the same from outside a frame. A direct write to `g.row`/`g.col` collapses the range (use `g.goto`). Copy and paste are capped at 4 MiB (a cut field ends at a character boundary) and 200000 cells. Frozen columns: `g.freeze(n)`; mark the first `n` cells of each row and header with `class:frozen={g.fz(j)}` and `style:left={g.bx(j)}` (body) / `{g.hx(j)}` (header), give the row and header row `style:padding-left={g.fw()}`, and style `.frozen` as `position: absolute; top: 0; z-index: 2` with a background (the row, and `.head`, `position: relative`): they stay while the rest scroll under, costing the visible rows only when the scroll changes. The header row stays out of the box and follows the scroll (`margin-left`); there is no general `position: sticky` on `left`. Resize: put a thin `position: absolute; right: -4px; width: 8px; z-index: 3` child in each header cell with `on:mousedown={g.grab(j)}`; dragging sets `g.widths` through `setWidth` (header and rows read `g.w(j)`/`g.total`), never below `MIN_W` (24 px), commits an open editor first, keeps the selection and the grid's focus, and on release fires `on:edit` with `kind == 3`: `col` is the column (read `g.w(col)` to save it); an `on:edit` handler must test `kind == 2` for paste. `g.colAtView(x)` is the hit test with frozen columns. `speculative/26_ui_grid`.
  Caret popup (mentions, emoji): `on:trigger={..}` on a text box reports the word at the caret that starts with a trigger character (`triggers=\"@:\"`, a literal; default `@` and `:`) at the start of the text or after whitespace:
  `trigger` is `@ja` (empty when it ended: the caret left it, the box lost focus, a selection, or Escape), `tlo` and `thi` its byte range for `Dom.insertToken`. It fires on each change. Show the list as `{#if open}<div popover=\"caret\">..buttons..</div>{/if}`
  in the same wrapper as the box: the popover hangs below the caret line (above when there is no room), follows wraps and scrolling, and takes no focus (a click on an entry keeps the caret). It anchors at `Dom.caretRect(id)`, worked out from the box's layout and current text (so it does not trail a keystroke); that works in a `<textarea>` and a one-line `<input>` (type text or search), not a password box (it hangs under the wrapper). While it is open with at least one focusable entry (`button` or `menuitem`) Up and Down move the active entry (the `:hover` look), Enter and Tab click it (so a rich textarea inserts no newline while the list has entries: Escape closes the list, then Enter does)
  (put the work in the entry's `on:click`), Escape dismisses it (`on:close`, and the same trigger stays quiet until it ends); typing and the other keys go to the box.
  Ancestor scroll boxes do not scroll to a focused box. The bound string is a view of the box's buffer: copy it if you keep it.
  A range takes arrow keys (Shift: ten steps), Home/End; `bind:high` (or `high`) makes it a two-thumb range slider. `select`
  opens a list of its `<option>`s (value = `value` attribute, else the text) above everything else, scrolls past its
  `max-height` (8 rows by default), flips above near the window bottom; with `multiple` the bound string is the comma list of picked values in pick order.
  `<input type=\"radio\" name=\"size\" value=\"m\" bind:group={size} />` is a radio button, 16x16 and round (`accent-color`, `:checked`, `:disabled`, `:focus-visible`; the rest of
  the box is yours, as for a checkbox; there is no `appearance`). Radios with one `name` are a group: checking one unchecks the others. The group is the
  radios of that name under the nearest ancestor with `role=\"radiogroup\"`, or the whole page when there is none (there is no `<form>`), so two
  groups with the same name in different `radiogroup` containers (or component instances that each have one) do not meet; a radio with no `name` is its
  own group. **In an `{#each}` list of components, every row's radios of one `name` are ONE group unless each row's element has `role=\"radiogroup\"`** (put it on the row's root); a group that spans keyed rows is reported once in the console warnings. A radio whose `name` changes joins the new group and takes the check from it if it has one. `bind:group={x}`: when the user checks this radio `x` becomes its `value`, and it is checked while `x` equals its `value`; put it on every radio of
  the group. A quoted `value=\"m\"` (no `{}` holes: `x` keeps it) makes `x` a `string`; `value={2}` makes it an `int`; `value={name}` is an int unless `x` is a `let` declared `string`
  (a mismatch with a declared `let` is a compile error). `x` changed by code checks the matching radio and unchecks the rest (all of them when no radio has that value), with no `on:change`; `x` always wins, so a handler that writes `x` back to its old value undoes the user's pick one frame later.
  Without `bind:group`: `checked` (once; the user's later choice stays), `checked={cond}` (applied when `cond` changes) or `bind:checked={flag}` (written for every radio that flipped, the one
  that lost the check too). `on:change={..}` runs only for the radio the user just checked, not for a click on the checked one, a code change or the one that lost the check.
  Up / Left and Down / Right on a focused radio move to the previous / next of its group (wrapping, skipping disabled ones) and check it; Space (or Enter) checks the
  focused one; a click or a click on its `<label>` checks it. A disabled radio can not be checked. Arrow keys on a focused radio belong to its group, even inside a `focus-group=\"arrows\"` (elsewhere in it they still move focus), and never scroll a box. Tab and Shift+Tab stop once per group, on the checked radio (else the
  first enabled one, also after the checked one is removed); the pad reaches every radio, and A checks it. Role `radio` with `checked`, `group=\"name\"` in the a11y dump, and a checked event for each radio that flipped.
  `type=\"color\"` binds `#rrggbb` and opens a saturation/value square and hue bar. Tab and Shift+Tab move focus; Enter or
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
    or Tab, focus goes to its first control and back on close; Escape asks to close (`closedby=\"any\"` also on a press outside).
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
    `heading` (`h1`-`h6`), `list` (`ul`, `ol`), `listitem`, `link`, `separator` (`hr`), `nav`, `main`, `header`, `aside`, `section`, `img`, `p`) or from `role=\"list\"`. Name: `aria-label` (an `<img>`
    `alt` counts), else the text of a button, menu entry, option or heading, else for a control the text of the `<label>` that
    wraps it, then `placeholder`, then `title`. An icon-only button needs `aria-label`; a control beside a label needs `<label for=\"id\">` or wrapping. `display: none` and `visibility: hidden` elements are left out (a `display: none` one takes its content with it) and cannot take focus; `a11y.language()` is the document language (`j3.i18n.locale()`, not in `dump`, so goldens keep their text). Keyboard:
    Tab / Shift+Tab go over buttons, inputs, selects, `menuitem` and `tabindex=\"0\"` elements in document order, except that elements with a positive `tabindex` come first in ascending order (ties in document order), as in HTML, and Shift+Tab goes the other way; both wrap; inside a modal dialog, or a `focus-group=\"trap\"` holding the focus, the order is computed within it; from an element Tab skips (`tabindex=\"-1\"`) it goes on to the next after it, and with no focus it starts at the first (Shift+Tab: the last) (`tabindex=\"-1\"` is focusable by click and code but skipped by Tab and the pad; the pad moves geometrically and ignores `tabindex` values; a clickable `<div>` needs `tabindex=\"0\"` and a `role` to be reachable); Enter / Space click a button, entry or
    checkbox and open a select; arrows move a slider and a select; a modal dialog traps Tab. `domhtml.dumpA11y(d, path)`
    writes the tree (`role \"name\" states actions=focus,click,set`, a password box shows `secret`, never its value) for goldens;
    `a11y.perform(d, id, A_FOCUS, A_CLICK or A_SET, num, text)` does an action between frames (what a platform bridge calls;
    a focus from it shows `:focus-visible`).
    More state: `aria-selected` (`=\"true\"`, `={cond}`), `aria-live=\"polite|assertive|off\"`, `aria-describedby=\"id other\"` (static
    ids; `a11y.describedBy`), `<details>` has `expanded`, `<progress>`/`<meter>` are named like a control (`aria-label`, a wrapping
    `<label>`, `<label for>`, `title`); a `<label for=\"x\">` names only `x`, even around another control. For a bridge: AT ids are
    node handles (`d.handle(id)`; every `a11y` call takes one, a gone node answers empty); `a11y.bounds` is the absolute rect (scroll,
    transform and flip applied); `a11y.text(d, id, &info)` gives a text box's value, caret and selection (byte offsets; false
    for `type=password`); after `a11y.enable(d)`, `a11y.events` (focus, checked, expanded, selected, value, children, name; filled only
    by frames where something changed) and `a11y.liveChanges` (the text of changed live regions, polite or assertive; a region's first
    frame is silent) are drained once per frame; `a11y.inbox`/`post`/`drain` carry an action from another thread and wake an idle
    loop (`Dom.requestFrame`, `Dom.wantsFrame`; `App` draws every frame today). Everything but `post` and `requestFrame` is main-thread.
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
    focused the first move takes the first focusable. A is Enter on the focused element (button click, checkbox toggle, `tabindex=\"0\"` summary toggles
    its details, select opens), B is Escape (closes a select list, popover or modal, which gives focus back to what had it), LB / RB set
    `d.padShoulder` (-1 / 1, for one frame: switch your tab), LT / RT scroll the focused element's scroll box a page (else the first scroll box), the right
    stick scrolls it. The pad makes focus `:focus-visible` (`d.setKeyboard(true)`) and the ring goes when the mouse moves; a move reveals the new
    focus in its scroll boxes (instant). Inside an open select or popover menu, and Left/Right on a slider, the directions are the arrow keys; a text
    box takes the pad direction as a focus move (a pad has no caret keys): `d.padText` is the box the pad just focused, for an on-screen keyboard
    (none is built in; without one the field takes keyboard input only). Attributes (static strings): `focus-group=\"wrap trap arrows\"` on a container
    (`wrap`: a move that leaves it comes round to the far side, level with the focus; `trap`: moves stay inside; `arrows`: the keyboard arrows move
    focus too, except from a text box, slider, select or open menu, which keep theirs), and `nav-up|down|left|right=\"id\"` on an element or a group
    (that direction goes to the element with that `id`; an id that is not on the page is ignored with one `css.warns` entry). Tab order stays document order.
    Tests and games feed pads with `d.setPad(padin.parse(\"Down A\"))` (names `Up Down Left Right A B LB RB LT RT SUp SDown SLeft SRight RUp RDown`; it
    holds until set again, `d.usePad(false)` polls `j3.game` again) or `testkit.padPress(d, f, at, \"Down\")` / `padHold`. `j3.game` polls pads 0 and 1 merged (any pad's button counts, the larger push wins an axis, so whichever is used drives the one focus) and
    drops an unplugged pad's handle (`game.padConnected(i)`); `PadRaw { attached: false }` means no pad: nothing is read, a held direction stops, and with none
    plugged in a frame costs two pointer checks. `padnav.usingPad(d)` says the last input was the pad (until a key or a mouse move); `<glyph>` (below) builds on it. Not built: action sets (plan 017 item 5; the `PadRaw` given to
    `d.setPad` is the seam it feeds), Steam Input (plan 025).
- `<glyph action=\"confirm\" />` is an input prompt that shows the button for an action on the device used last and switches live (a pad button or stick: pad; a key, a mouse move or press: keyboard). `Press <glyph action=\"confirm\" /> to start` reads \"Enter\" in a keycap, \"A\" in a round pill once the pad is touched. Built-in `action`s (what `padnav` acts on): `confirm` Enter / A, `back` Esc / B, `prev` Shift+Tab / LB, `next` Tab / RB, `up` `down` `left` `right` arrows / D-pad, `page-up` `page-down` PgUp PgDn / LT RT, `scroll` Wheel / right stick. A game's own action: `key=\"Space\"` and/or `pad=\"x\"` (pad names `a b x y` = bottom, right, left, top face button; `lb rb lt rt up down left right start select ls rs`), alone or beside `action`; each is text or `={expr}`, explicit ones beat the tables, and an action with no button on the device shown falls back to the other device's label. The box is filled by the runtime (no children) and is inline-block, so it sits in a text line and `vertical-align: middle` works. Style it with `glyph`, `glyph[type=key]` (keyboard) and `glyph[type=pad]` (pad); the tag defaults (a bordered, rounded, translucent box at 0.85em, a pill for a pad) lose to any author rule. Pad labels follow `d.setGlyphStyle(\"xbox\" | \"playstation\" | \"nintendo\" | \"generic\")`: xbox A B X Y LB RB LT RT; playstation ✕ ○ □ △ L1 R1 L2 R2 (a glyph whose font lacks those symbols, checked once it has a style, says Cross Circle Square Triangle instead); nintendo B A Y X L R ZL ZR (A and B swapped by position). `start` and `select` are Menu View (xbox), Options Share (playstation), + - (nintendo), Start Select (generic); `ls` and `rs` LS RS (R3 L3 on playstation). `d.setGlyphBinding(action, key, pad)` replaces what an action shows (\"\" keeps the built-in for that device): the seam for a rebinding screen. Accessible name: role img named by its label (\"Enter\", \"A button\"). Any key press, a mouse move or press, or a pad input switches the device. A device switch restyles and re-lays out only the glyph boxes; a steady frame allocates nothing. Drawn boxes only: no image atlas for glyphs.
    `title=\"text\"` on any element is a tooltip (`.j3-tooltip`; a newline starts a new line: `title={\"a\\nb\"}`) after the pointer rests
    `--j3-tooltip-delay` (default 500ms, set on `:root`; a bad value warns and keeps the last good one). Titles wrap at
    320px or the viewport by default; set `.j3-tooltip { width: 180px }` to change it. `white-space: pre-line` preserves
    newlines and `overflow-wrap: break-word` breaks long words. A tooltip is passive and can appear beside an open select or color panel. `on:contextmenu={.. mouse.x ..}`
    runs on a right press or Shift+F10 on a focused owner (`mouse` is viewport px; keyboard requests use its bottom-left).
    Render a conditional child `<div popover=\"context\" on:close={open = false}>` inside the owner, with
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
- Panels (`j3.ui.panels`, `panelmath`; page 12 of `examples/big_ui_test`). `<div class=\"win\" panel>` (a `position: absolute` box
  in a `position: relative` parent, or `position: fixed`) is a draggable window: a descendant with `panel-handle` drags it (a
  button or input inside the handle keeps its click), a descendant with `panel-resize` resizes it from its bottom-right corner.
  It stays inside its workspace (the parent's box, or the window), never goes below its CSS `min-width` / `min-height` (80 by
  36 otherwise) and is pulled back in when the workspace shrinks (one frame later). `panel=\"dock\"` also snaps it: a drag
  that ends within 28 px of the workspace's left, right or bottom edge shows that zone while dragging and docks there (full
  height, or full width at the bottom, keeping the floating size); dragging a docked handle floats it, and a grip on a docked
  panel resizes along the dock axis. The state lives with the element (a branch that removes the panel forgets it), and
  `d.panelDock(id)` reads the zone. Bring-to-front is not done: later siblings and `z-index` decide the order.
- Item drag and drop (`j3.ui.drag`; `speculative/35_ui_drag`, `j3c/stdlib/tests/drag_test.j3`): inventory, hotbar and equip slots.
  `draggable` (or `draggable={bool}`) marks a source; a press that moves 4 px starts the drag (a press that does not is an
  ordinary click, and a drag is never followed by a click). `drag-data={string}` and `drag-id={int}` on the source are the payload.
  Handlers (run in markup order within a frame; compare a `data` string with `same(data, \"x\")`, `use j3.text.same;` in the script): `on:dragstart` and `on:dragend` on the source (`dragend` always fires, dropped or
  not, and has `dropped: bool`; it comes the frame AFTER the release, so `drop` is always seen first wherever the elements are in the markup: move items in `drop`, tidy up in `dragend`); `on:dragenter` `on:dragover` `on:dragleave` `on:drop` on the element under the pointer: the innermost
  element at or above it with any of those four, so a slot with children is one target (`dragover` fires every frame it stays,
  `dragenter` / `dragleave` when it changes, and `dragleave` also on Escape; no `dragleave` before a `drop`). Handlers can read
  `data` (the `drag-data` string), `dragId` (the `drag-id` int) and `mouse` (local to the element; not in dragstart / dragend).
  A target accepts a drop by having `on:drop`; `on:dragover={if dragId == 3 { accept = false; }}` rejects for as long as
  the pointer stays on it (no `:drop-target`, no `drop`). Escape (or the window losing focus) cancels (`dragend` with `dropped` false); releasing anywhere
  else is `dragend` only. If the page removes the source mid-drag (or in `drop`, so its `{#if}` is gone the next frame) the drag ends and no `dragend` is
  delivered, since its element is gone: do the cleanup in `drop`. A removed target just stops being one. Style with CSS only: `:dragging` matches the source for the whole drag, `:drop-target` the element
  with `on:drop` that the pointer is over. Ghost: a copy of the source follows the pointer at 60% opacity above everything
  (not clipped; the source keeps its own `:dragging` look); `drag-ghost=\"none\"` skips it for a game that draws its own.
  Text boxes never start a drag, and static text selection can fight a draggable that holds text: set `user-select: none`
  on it. Only the pointer drags, one at a time: no files, no OS drag and drop, no touch gestures. Tests: `testkit.dragText(d, f, 3,
  \"Sword\", \"Slot 2\")` (also `drag` by node, `dragTag`, `dragTo` a window point), frames 3..8, Escape with `simKeys` on frame 7.
- Sticky headers (`j3.ui.sticky`): `position: sticky; top: Npx` keeps a box under the top edge of its nearest scrolling
  ancestor until its parent ends. Give it a `z-index` so it paints and takes the mouse above the content that slides under it.
- Responsive layout: `@media screen and (max-width: 900px) { .grid { grid-template-columns: repeat(2, 1fr); } }` (px
  `min-` / `max-` `width` / `height` joined by `and`, with `not` and `,` / `or` lists) is re-tested when the window size changes; styles restyle only when a
  condition flips (and transition like any style change). A collapsible sidebar is a `width` `transition` on a class.
- Images and shapes (`j3.ui.media`, `mediastyle`, `imgfit`, `j3.gfx.nineslice`, `clip`, `path`; page 13 of
  `examples/big_ui_test`). `<img src=\"assets/a.png\">` loads once per path (a missing file warns once and draws nothing),
  takes its natural size or keeps its aspect from one of width/height, and honors `object-fit` / `object-position`,
  `border-radius`, transforms and opacity. `<img sprite=\"4,2\" frame={n}>` shows one cell of a 4x2 sheet; `fps=\"8\"` plays it
  (negative plays backwards). `<svg viewBox=\"0 0 24 24\"><path d=\"M2 2 L22 12 Z\" fill=\"currentColor\"/></svg>` draws SVG path
  data (`M L H V Q T C S A Z`, relative forms, holes by winding; `fill` / `stroke` take `currentColor`, `accent` (the `accent-color`), a color, `none`); no groups, SVG transforms, gradients or external `.svg`.
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
  Children are a HUD over the game. A button on top swallows the click.

CSS (scoped per component, plus the theme). Selectors `*` tag `.class` `tag.class` descendant, child `>`,
`:hover :active :focus :focus-visible :focus-within :disabled :first-child :last-child :nth-child(An+B) :not(...)` `:has(> img, .x, + p)` (limited) `:checked :open :enabled :empty :placeholder-shown :only-child :nth-last-child() :first-of-type :last-of-type :only-of-type :nth-of-type() :nth-last-of-type()` `:dragging` `:drop-target` (item drag and drop) `[type=\"range\"]` `:root`; `var(--x, fallback)`; flex, grid
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
as text call `j3.ui.domhtml.dumpHtml(&root.dom, \"target/ui-snapshot.html\")` (`use j3.ui.domhtml.dumpHtml;`) after
`root.render(...)` (static HTML + CSS to diff or open in a browser, with checked, disabled and entered values). `root.dom.setFrameMs(16.0)` fixes the UI clock.

Inspect a running UI (devtools): `root.dom.setInspector(true)`, or F12 with `J3_INSPECT=1`; click pins an element and a panel shows its
box, computed style and matched rules with `file:line`. `J3_INSPECT_DUMP=#id:30` prints that text at frame 30 (`j3c doc testing`).

Cost. Layout and styles run only when something changed (an idle frame is about 40 us for 1000
nodes). Text and shapes are batched: a clip, a canvas, or a texture change ends a batch.
";

fn j3ui_page() -> String {
    format!(
        r#"`@j3ui` — UI components (immediate mode; the retained, CSS-styled kind is `j3c doc j3ui`)

`@j3ui name(ui: &Ui, ...) {{ }}` is a function. The first parameter is the `Ui` from `Ui.begin`. The body cannot see locals from another function. Call it like a function. A component inside another is that call. Each widget's arguments are `j3c doc j3.ui`.

```
@j3ui shopRow(ui: &Ui, w: &World, kind: int, name: string, cost: int, maxed: bool) {{
    row() {{
        label(`{{name}}   {{cost}} leaves`);
        button("buy") {{
            buy(w, kind);
        }}
    }}
}}

@j3ui shop(ui: &Ui, w: &World) {{
    panel(280, 248) {{
        label("shop");
        shopRow(ui, w, 0, "grab speed", costSpeed(w.speed), w.speed >= 5);
    }}
}}

fn draw(w: &World) {{
    let ui = Ui.begin(w.font, 16, 188);
    shop(ui, w);
}}
```

A bare widget name gets the receiver (`label` becomes `ui.label`). These names are the ones the parser accepts:

- {end_names} — the call, the body, then `end`
- {pop_names} — the call, the body, then `popId`
- {if_names} — `if ui.button(...) {{ body }}`. The body runs on the click
- {call_names} — the call, then `;`. A `{{ }}` on these is an error

`buttonOff` draws a button that never clicks. `toggle`, `check`, `slider`, `range`, and `stepper` write through a pointer and return true when the value changes. `field` edits a `Text` buffer. Tab and shift-tab move focus across `button` and `field`. Focused buttons use the theme's hot color and activate on Enter or Space. `drag` stays true while the pointer holds it; `dragDelta()` is the motion since the press. `worldLabel` and `worldLabel3` draw at a world point and do not advance the cursor. `if`, `for`, `let`, and other calls stay J3.

ASCII text is drawn from a glyph atlas, so a changing digit does not allocate a new string texture. `scroll(id, height)` skips rows that sit fully outside the window. `j3c doc j3.game drawText` describes the cache.
"#,
        end_names = names(J3UI_END),
        pop_names = names(J3UI_POP),
        if_names = names(J3UI_IF),
        call_names = names(J3UI_CALL),
    )
}

/// The editor's user guide, embedded so it matches this j3c build.
fn editor_page() -> String {
    format!(
        "`j3c doc editor` — 3D scene editor

For agents editing an editor project (not the editor itself):
- Project-owned: `j3.editor.toml` (main scene), `scenes/*.j3scene`, `assets/`, `scripts/*.j3s` (attached to objects; `j3c doc lang` has a Scripts section).
- Generated, don't hand-edit: everything under `target/` (`target/j3-editor/`, `target/j3s/` bytecode).
- An object's script is `script=scripts/door.j3s` on its line; its props are `param.<prop>=value` beside it. `j3c check` reports a wrong prop type or a missing required prop against the scene line. `j3c -e script-props scripts/door.j3s` lists what the inspector edits.
- Edit scenes with `j3c -e scene` (by object name, `Parent/Child` path, or `#id`), not by hand: `j3c -e scene --help`.
- Look at what you built: `j3c -e render [--view iso|top|front] [--focus NAME] [--out F.png]`, and
  `j3c run --frames 120 --screenshot F.png --headless` for the running game.
- Place without guessing heights: `j3c -e scene ground|ontop|scatter`.
- Meshes from code: `assets/gen/<name>.j3` with `j3.gfx.meshgen`, made by `j3c -e gen` (`j3c doc j3.gfx.meshgen`).
- Reusable scenes can declare props: `j3c -e scene set @scene prop.speed=float:1`, then
  `set <copy> prop.speed=2`; code reads `scene3d.propNumber(&scene, scene3d.copyOf(&scene, id), \"speed\", 1)`.
- `j3c -e mcp` serves all of this as MCP tools; with the editor open, edits go to it live (one undo step each, unsaved until saved); with it closed they write the file and `undo` puts it back. `j3c -e mcp --tools` lists every tool's arguments.
- Heavy work off the frame thread (meshing, generation, decoding): `j3c doc recipes threads`, rules, safe modules, and mistakes in `j3c doc threads`. GL, SDL, and `j3.gfx`/`j3.game`/`j3.scene3d` state are main-thread only.
- Check loop, and what each step catches: `j3c doc recipes check`. Scene loading and drawing from code: `j3c doc recipes scene3d`.
- Scene files are line-based `key=value` saves. The editor picks up outside edits within a second (merging them into unsaved work), but `j3c -e mcp` edits it directly and is safer while it's open.
- Runtime API for scenes, fields, lights, and materials: `j3c doc j3.scene3d`. Build/run: `j3c check`, `j3c run`.
- Build errors from Play land in `target/j3-editor/build.log`.
- Changing the editor itself: `j3c/editor/src/` (window, one module per file; `main.j3` is the entry), `j3c/src/editor.rs` (host, project setup), `j3c/stdlib/j3/scene3d.j3` (format); `J3_EDITOR_SRC=path/to/editor/src` builds from disk.

{}",
        include_str!("../editor/README.md")
    )
}


const TESTING_PAGE: &str = r#"Testing and looking, headless
=============================
Agents never open a window. A visible window blocks the session and nobody is watching it.
Run the real program with no display, save a frame, and read the PNG.

    j3c check                      types, libraries, files that load calls name. Does not run.
    j3c run --frames 120 --screenshot shot.png --headless
                                   the project for 120 frames on SDL's offscreen video and silent
                                   audio; saves the last frame (relative to where you ran j3c);
                                   prints a J3STATS line. Wrap it: `timeout 60 j3c run ...`.
    j3c build && SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy timeout 20 ./target/<name>
                                   a built binary has no --headless flag: set both variables on
                                   the same command line, or a window opens.

`j3c run --hot --headless` keeps running and, when a `.j3`/`.j3ui` file changes, rebuilds and restarts the program
(a failed build keeps the old one). `J3_HOT_QUIT` / `J3_HOT_WIN` / `J3_HOT_STATE` are file paths it sets for the
handshake; tests print from the `App.hot` load hook to see state survive (`j3c doc j3ui`, Dev loop).

Without `--headless` or those variables a run opens a real window. Any game run needs one of them.

`--frames N` (and `J3_CAPTURE=...:N`) runs exactly N frames: the game's `draw` is called N times and
`j3.game.frameCount()` reaches N. The `J3STATS frames=` line counts N minus 3, because the first three
frames are warm-up and left out of its averages.

Environment (read by `j3.game` / `j3.app`, so any App supports them; see `j3c doc j3.app`)

    J3_FRAMES=N, J3_SCREENSHOT=f.png   what `--frames` and `--screenshot` set
    J3_CAPTURE=out.png:N               save frame N (1-based, before it is shown), then quit
    J3_FIXED_DT=0.016                  every frame's dt is that many seconds, so runs repeat

Goldens (checked-in PNGs that must keep matching):

    SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy J3_FIXED_DT=0.016 J3_CAPTURE=out.png:30 \
        timeout 20 ./target/<name>
    j3c -e imgdiff golden/a.png out.png [--tol N] [--max N] [--out diff.png]

Another window shape: `game.setVirtualSize(1920, 1080)` letterboxes everything to that size, so a
capture at a 1400x700 or 1000x900 window shows black bars (`J3_CAPTURE` and `screenshot` are the
whole window, bars included, in real pixels). `game.screenRect()` is the drawn area. With the size off,
output is unchanged. `big_ui_test` reads `UI_VIRTUAL=1920x1080` with `UI_SIZE=WxH`.

Window modes (`game.setWindowMode` borderless/fullscreen, monitors, vsync) do nothing useful under the
offscreen driver. Test the settings logic with `stdlib/tests/display_test.j3` (`j3.game.display`), and the
look by hand on a real display.

`imgdiff` prints how many pixels differ by more than `tol` per channel (default 0), exits 0 when that
is at most `max` (default 0), 1 when more or when the sizes differ, 2 when it cannot read a file.
`--out` writes a diff image (changed pixels red). Update a golden only for a change you meant, and
look at the new PNG first.

`j3c golden <dir> [name...] [--bless]` does the whole loop for a project from `<dir>/golden.toml`: per
`[[state]]` it runs `j3c run . --headless --frames N` (offscreen SDL, dummy audio, `J3_FIXED_DT=0.016`,
`J3_CAPTURE=target/golden/<name>.png:<frame>`), compares with `golden/<name>.png`, and prints `PASS name` or
`FAIL name (why)`; exit 1 on any FAIL. A missing golden is a FAIL; `--bless` copies the captures over the goldens
(named states only, else all). Names run just those states. Diff images and run logs land in `target/golden/`.

    [defaults]                    # for every state: frame (30), tol (per channel), max (pixels),
    frame = 30                    # frames (default frame+2), timeout (seconds, 300)
    [state.settings]              # golden/settings.png; any of those keys may be set here too
    text = "target/ui-a11y.txt"   # optional: this file is also compared with golden/settings.txt
    [state.settings.env]          # env for this run; the app picks the state from it
    APP_GOLDEN = "settings"
    [check.steady]                # a run with no capture; PASS at rc 0 and no output line starting FAIL
    frames = 200                  # omit it to let the program end itself (a self-test)
    [check.steady.env]
    J3_STEADY = "60:120"

The file is the same small TOML subset as `j3.toml` (sections, `key = value`); an unknown key, a bad number or a
repeated section is an error `golden.toml:LINE: ...` (exit 2). Each run gets its own process group and is killed
at the timeout (FAIL).

Checks run only without names and without `--bless`. `examples/idle-breakout/golden.toml` is the plain pattern,
`examples/desk-app/golden.toml` adds text goldens and checks.

Make a frame repeatable: fixed dt (above), fixed seeds, and a state the game sets up itself, for
example `j3.game.env("MYGAME_STATE")` in `boot` choosing a tab or a seeded world. For `.j3ui` also call
`root.dom.setFrameMs(16.0)` so fades and transitions land the same each run.

Steady frame (an idle UI should cost nothing). `J3_STEADY=a:b` (1-based frames, a < b) is read by
`j3.app`: at the end of frame a it notes the layouts, restyles and `App.mem` use, and at the end of
frame b it prints one line and exits (rc 1 when any is non-zero, else 0):

    steady layouts=0 restyles=0 arena_growth=0

    J3_STEADY=60:120 j3c run --headless --frames 200       (a built binary: offscreen SDL, same variable)

Layouts and restyles count boxes laid out and styles recomputed in every `.j3ui` root, so an idle
frame leaves both unchanged; `arena_growth` growing means something allocates per frame. Warm up
first (the first frames lay everything out and a few frames after a state change still settle), and
set a fixed state before measuring. A malformed value prints a warning and is ignored. `examples/desk-app/golden.toml`
uses it as a `[[check]]`; `speculative/29_ui_steady` shows both outcomes (`STEADY_CHURN=1` changes a value every frame: rc 1).

UI inspector (`j3.ui.inspect`, the devtools of a `.j3ui` app). `root.dom.setInspector(true)` turns it on; with `J3_INSPECT=1` F12
toggles it as well (`j3c run` sets that for you, `J3_INSPECT=0` turns it off; a built binary needs it set by hand). The element under the pointer gets margin / border / padding / content bands and a `tag.class#id  W x H` label;
a click pins an element (Escape unpins, Up / Down walk to the parent / first child, Left / Right to the siblings) and a side panel
shows its box, computed style (non-default), inline `style:` overrides first (they win over every rule), matched rules in cascade
order with `file:line` in the `.j3ui` or CSS file (overridden declarations are dimmed and struck through), and custom properties; the
wheel over the panel scrolls it. While on, the app gets no clicks, wheel or keys, and its hover, active state and cursor stay frozen as
they were when the inspector came on. For
agents, `J3_INSPECT_DUMP=<selector>:<frame>` (`#id`, `.class`, a tag, or a node number; frame 1-based) prints the same text to stdout
at that frame, with no window changes needed, and `inspect.text(&root.dom, id)` returns it (valid until the next call). Off, it costs one bool check a frame.

`.j3ui` debugging

    j3c dump-ui ui                 the J3 generated from the components (errors point at the .j3ui)
    use j3.ui.domhtml.dumpHtml;   dumpHtml(&root.dom, "target/ui-snapshot.html")
                                   after `root.render(...)` (it runs `layout()` itself; on a Dom you
                                   drive by hand, after `d.layout()`): the node tree as static HTML + CSS,
                                   with checked, disabled, selected and typed values (never a password's),
                                   readable as text, diffable, or openable in a browser
    root.dom.simInput(x, y, down, wheel)
                                   fake mouse input from the next frame; a click is one frame with
                                   `down` then one without. Drive it from your own test hook.
    root.dom.simKeys("text", "S+Left C+a Backspace")
                                   fake keyboard for the next frame: typed text and key names (Left Right
                                   Up Down Home End Backspace Delete Enter Escape Tab Space a c v x, `S+`
                                   Shift, `C+` Ctrl). `examples/big_ui_test/src/sim.j3` scripts both from `UI_SIM`.
    use j3.ui.testkit;            a script of fake input, one call per frame from your draw hook (`f` = your frame number):
                                   testkit.clickText(d, f, 3, "Size")     press on frames 3..5, check on 6
                                   testkit.clickTextIn(d, f, 8, "dialog", "Rename")   text inside a tag
                                   testkit.clickTag(d, f, 12, "button", 2)   the 3rd <button>
                                   testkit.typeInto(d, f, 18, "input", 0, "ro")   click on 18..20, type on 21
                                   testkit.simKeys(d, f, 30, "", "C+a Backspace")   several key names in one frame
                                   testkit.dragText(d, f, 3, "Sword", "Slot 2")   drag an item onto an element, frames 3..8 (dragTag, drag, dragTo)
                                   testkit.hasText(d, "Saved"), countText, textNode, tagNode
                                   text matches a whole text node (first in tree order; not display:none, not a part of
                                   a split `Rename {name}`). A target that is missing, hidden, size 0 or off screen
                                   prints `testkit: frame N: no text "X"`, sends no input and counts once in
                                   `testkit.misses()` (`resetMisses()` between suites): fail your test on it. Examples: `examples/desk-app/src/selftest.j3`
                                   (DESK_TEST=1), `speculative/14_ui_slots`, `j3c/stdlib/tests/testkit_test.j3`.
    hot reload                     only under `j3c run` (not headless captures): style edits apply in
                                   about 250 ms; `j3c run --hot` also rebuilds and restarts on markup, script and
                                   .j3 edits (`app.hot(save, load)` keeps state). CSS warnings print at start.

Scenes only (no game code): `j3c -e render --view iso --out shot.png`. More: `j3c doc recipes check`.
Speed: the J3STATS line has fps, frame, draws; measure release builds (`j3c build --release`).
"#;

const CSS_PAGE: &str = r#"CSS for .j3ui
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
            `(prefers-color-scheme: dark | light)` (the program's settings, see Preferences below; no other features, no parenthesised
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
            A text-size slider is one line: `style:font-size={px}` on the root (see idle-breakout, golden `scaled`).
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
      Bloom's Gaussian spread is fixed at 12 physical pixels. Each group lazily keeps four RGBA8 images at the
      active paint-surface size. Base cost is one FX pass plus compositing; blur adds two passes, bloom three,
      custom GLSL one, and 3D projection one. Blur/bloom also regenerate mipmaps; groups copy their backdrop.
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
                                           `display: inline-block` is not a value here (the parser warns `invalid value for 'display'`); a bare number
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
"#;

const TASKS: &str = "\
Start from the task. `j3c doc recipes <name>` prints a complete, checked program.
    2D game in a window              recipes app2d     then j3.app, j3.game, j3.ui, `j3c doc j3ui-immediate`
    menus, HUDs, panels styled with CSS recipes components then `j3c doc j3ui`
    3D game from editor scenes       recipes scene3d   then j3.scene3d, j3.gfx, `j3c doc editor`
    first-person 3D starter          `j3c new <name> --template fps`, then j3.scene3d.fpwalk
    sprites and textures, no window  recipes canvas    then j3.gfx.canvas
    shapes, text, HUDs, debug draw   recipes paint     then j3.gfx.paint
    video settings (mode, resolution, vsync)  recipes display  then j3.game.display
    per-object logic in .j3s         recipes script    then `j3c doc lang` (Scripts), j3.scene3d.scripts
    meshes built in code             recipes meshgen   then j3.gfx.meshgen
    heavy work off the frame thread  recipes threads   then `j3c doc threads`, j3.thread
    call a web API (https, JSON)     recipes http      then j3.net.http, j3.data.json, examples/openrouter-chat
    run another program (curl, git)  j3.proc           (`proc.run(argv, stdin, &arena)`: exit code, stdout, stderr)
    materials and shader effects     recipes material  then j3.scene3d.fx, `j3c doc editor`
    traced mirrors, glass, shadows   recipes urp       then `j3c doc urp`
    check and look at what you built recipes check
    run, screenshot, golden-diff     `j3c doc testing` (headless: never open a window)
    CSS support for .j3ui            `j3c doc css`
    ship it: zip, .deb, icon         `j3c doc ship`
    saves, assets, ECS, physics      j3.data.save, j3.data.assets, j3.game.ecs, j3.game.physics
Find a name without knowing its module: `j3c doc search <word>`, or `j3c doc <name>`.
A module's names on one screen: `j3c doc <module> --short`.

";

/// `j3c doc ship`: release builds and `[dist] build_outputs` (`src/dist.rs`, `src/dist/deb.rs`).
const SHIP_PAGE: &str = r#"Shipping a game: release builds, zips, .deb

`j3c build --release` makes the small binary (opt s, no -g, stripped; checks stay on) in
target/<name> (target/<triple>/<name> per [emit] target). Dev builds are for the loop, not
for sending. `j3c size` shows what is in a dev build.

Packages: list them in j3.toml and every release build writes them to target/dist/:

    [package]
    name = "mygame"
    version = "0.1.0"                    # default 0.1.0
    description = "A small arcade game"  # one line; default "<name>, made with j3"

    [dist]
    build_outputs = ["zip", "deb"]
    icon = "assets/icon.png"             # .png (square 256 or 512) or .svg

- zip: target/dist/<name>-<os>-<arch>.zip, one per target. A <name>/ folder holds the
  program, the include list, and for Windows targets the DLLs the .exe imports (found via
  [link] search, its ../bin, or ~/.cache/j3/sdl2-mingw/dll). Needs `zip`.
- deb: target/dist/<pkg>_<version>_<arch>.deb, Linux targets only (others print a skip).
  Built with `tar` and no dpkg, so any Linux or macOS host makes one. `<pkg>` is the name
  lowercased with _ as -. Installs:
      /opt/<name>/                         the zip's folder
      /usr/bin/<name>                      launcher (below)
      /usr/share/applications/<name>.desktop   menu entry
      /usr/share/icons/hicolor/<N>x<N>/apps/<name>.png and /usr/share/pixmaps/<name>.png
  Install with `sudo apt install ./mygame_0.1.0_amd64.deb`; remove with `sudo apt remove mygame`.
- `zip = true` (older projects) is the same as build_outputs = ["zip"].

Working directory: games read assets and write saves relative to it (`j3c run` uses the
package root). /opt is read-only, so the launcher makes ~/.local/share/<name>
($XDG_DATA_HOME), links every installed file into it, and runs the program from there:
relative reads find the data and saves land in the player's folder. It also sets the SDL
window class, so the taskbar groups the window under the menu icon.

More [dist] keys:
    include      files and folders shipped beside the program. Default: whichever of
                 assets, scripts, scenes, locales exist
    title        menu name. Default: the package name title-cased (my_game -> My Game)
    categories   .desktop Categories. Default Game;
    maintainer   Name <email>. Default $DEBFULLNAME/$DEBEMAIL, else git config
    depends      extra Debian packages
Depends are read from the libraries the program loads (`objdump -p`, else [link] libs):
SDL2, SDL2_image, SDL2_mixer, SDL2_ttf, GL, libc. Any other library prints a warning;
add its package to `depends`.

Check a package without installing it: `dpkg-deb --info F.deb`, `dpkg-deb --contents F.deb`,
`desktop-file-validate`. Every key: `j3c help manifest`.
"#;

/// `j3c doc i18n`: translated text.
const I18N_PAGE: &str = r#"Translated text (j3.i18n)
=========================

One file per language, `locales/<locale>.txt`, one `key = text` per line. `#` starts a comment line, `\n` `\t` `\\`
are escapes, a later duplicate key wins, UTF-8 is raw. Keep keys sorted by feature so diffs stay small:

    # locales/en.txt
    app.title = Shop
    shop.buy = Buy {0} for {price}
    shop.items.one = {count} item
    shop.items.other = {count} items

j3.toml (all optional):

    [i18n]
    dir = "locales"      # folder of <locale>.txt
    default = "en"       # locale at startup, and the one `j3c check` compares keys against
    fallback = "en"      # tried when a key is missing

Use (in J3 or in `.j3ui` markup, where `{...}` is an expression):

    use j3.i18n.{ t, tf, tf2, num, real, named };
    <h1>{t("app.title")}</h1>
    <p>{tf("shop.items", num(n))}</p>                      <!-- 1 item / 2 items -->
    <p>{tf2("shop.buy", num(3), named("price", real(2.5)))}</p>

- `t(key)` is the text, or the key itself when no locale has it (one stderr warning per key; `i18n.quiet(true)` mutes it,
  `i18n.missing()` counts them). Lookup order: the current locale (`pt-BR`), its language (`pt`), the fallback locale.
- `tf`, `tf2`, `tf3`, `tfn(key, []Arg)` fill `{0}` `{1}` and `{name}` (`named("name", arg)`); `{{` and `}}` are literal braces.
  Arguments are `num(int)`, `real(f32)`, `text(string)`. Numbers print the way the locale writes them (`1,234.5`,
  `1.234,5`, `1 234,5`). The result is on the frame scratch arena, valid until the next frame; text nodes keep their own
  copy and compare bytes, so a steady frame lays out nothing.
- Plurals: the number named `count`, else the first argument when it is a number, picks `key.zero|one|two|few|many|other`
  (CLDR rules for en de nl sv es it fr pt ru uk pl ar ja zh ko vi th id; unknown languages follow English), then
  `key.other`, then `key`. `{count}` always means that number. `i18n.plural("ru", 3, false)` gives the category.
  Not implemented: the compact `many` of es/fr/it/pt, pt-PT's rule, locale-specific digits (Arabic-Indic).
- `i18n.setLocale("de")` switches at runtime; markup builds every frame, so the next frame shows the new text (nothing
  is cached; `i18n.version()` changes for code that does cache). Saving the choice is the app's own settings:
  `j3.data.save` the locale string and `setLocale` it at startup (`j3c doc j3.data.save`).
- `i18n.rtl()` is true for ar, he, fa, ur: flip your layout with it (a class on the root, `row-reverse`). The text itself
  is shaped and ordered by the renderer (`j3c doc css`, Differences); there is no `direction` property.
- Loading happens at the first call. `[i18n]` from j3.toml is baked into the build (compile-time constants), so a built
  binary needs no j3.toml. `locales/` is looked for in the working directory, then beside the program, so a zip
  unpacked anywhere works when run by path (`[dist]` ships `locales/` by default). `init(dir, default, fallback)`
  sets all three explicitly.
- Live edits: call `i18n.reloadIfChanged()` every few frames in a dev build and saved locale files show without a
  restart (it reads at most three small files; the app picks the rate, nothing calls it for you); `j3c run --hot`
  restarts on code edits only.

Checking: `j3c check` warns `file:line:col: message [i18n-missing]` for a literal key of `t`/`tf`/`tf2`/`tf3`/`tfn` in
`.j3` and `.j3ui` files that the default locale lacks (a `key.one`/`key.other` entry counts for `key`), and for keys
in other locales that the default lacks. Keys built at run time are not seen. `j3c i18n extract [dir]` lists the keys
used in code (`key<TAB>file:line:col`). The keys come from the compiler's lexer over `.j3` files and the
`<script>` and `{...}` spans of `.j3ui`, so strings and comments are handled. `j3c check --json` lists the
warnings in an `i18n` array beside `css`, and the MCP `check` tool reports them as warnings; `--strict` fails on them.
Plural rules and number formats live in `j3.i18nplural` (`j3c doc j3.i18nplural`). Example test: `j3c/stdlib/tests/i18n_test.j3`; golden: `speculative/57_ui_i18n`.
"#;

/// `j3c doc threads`: the long-form guide. `j3.thread`'s own header stays short because the
/// module index prints it.
const THREADS_PAGE: &str = "\
Threads in J3 (Linux) — move heavy work off the frame thread
============================================================

The one idea: HAND WORK OVER, DO NOT SHARE IT. Give a worker a struct only it writes, wait
for its result, then read it. There is no memory model beyond that and the compiler does not
check races, so the rules below are the whole safety net.

Start from a program: `j3c doc recipes threads` (a pool builds chunks, a frame-style loop feeds
it and collects results without blocking). Every primitive is in `speculative/13_threads.j3`.
Names and signatures: `j3c doc j3.thread`.


Pick a tool
-----------
    run one thing in the background   spawn(&t, name, f, arg), later join(&t)
    many jobs on a few workers        Pool.create(&arena, workers, queueCap), then submit / trySubmit
    the same work over 0..n           pool.parallelFor(n, f, ctx)   (f writes out[i]; it waits)
    results back to a frame loop      a second Queue<T>: workers push, main tryPops
    a counter or cancel flag          @atomicAdd / @atomicLoad / @atomicStore / @atomicCas on an int
    anything bigger that is shared    a Mutex around it (better: hand it over instead)
    one value per thread              #[singleton] #[thread] let X: T;   (memory.scratch is one)


The frame thread must never block
---------------------------------
Use these in a 60 Hz loop:   pool.trySubmit (false: queue full, keep the job, retry next frame),
                             queue.tryPop (a result or nothing), queue.tryPush, queue.count,
                             pool.pendingJobs, @atomicLoad.
These can wait on other threads, so keep them to loaders, tools, and shutdown:
                             submit, wait, parallelFor, push, pop, join, lock.
Size every Queue for the worst case. A full results queue blocks the worker in push until main
drains it; a full job queue is what trySubmit reports.


The rules
---------
1. Share by handing off. A worker owns the memory it was given until it reports back. The owner
   touches that memory again only after the signal (wait, pop, join).
2. Ordering: everything a thread wrote before unlock, signal, push, or an atomic store is visible
   to another thread after the matching lock, wait, pop, or atomic load. Nothing else is ordered.
   Two cases callers rely on: everything written before spawn or submit is visible to that job
   when it starts, and everything a job wrote is visible once join, pool.wait, or parallelFor
   returns.
   Never spin on a plain flag: the compiler may hoist the load. Use @atomicLoad or a Queue.
3. A job is a plain fn(ptr) plus ONE ptr to a struct the spawner owns and keeps alive until the
   job has finished. There are no capturing closures. Put what the job needs in that struct: its
   input, the slice it alone writes, an index, a &Queue to send the result to.
4. One Arena per thread. Two threads allocating from one Arena race. A worker allocates from its
   own arena or from memory.scratch, which is per thread and which a pool worker rewinds after
   every job, so scratch memory never outlives the job: copy results into the struct main gave
   you before the job returns.
5. Window, GL, SDL, audio, and file dialogs are main-thread only, and so is all state in j3.game,
   j3.gfx (including j3.gfx.paint), j3.scene3d, j3.ui, and j3.app. Workers compute; main uploads and draws.
6. Races are yours to avoid. Default for data the owner keeps editing: main COPIES the region into a
   worker-owned buffer when it dispatches, and a version stamp only decides if the result is stale
   (see \"Shared data that keeps changing\" below). Reading it live is allowed only when the worker
   survives torn data.
7. Scheduling is nondeterministic: jobs start in order and finish in any order. Anything that
   feeds saves, replays, or netcode reduces results in a fixed order. parallelFor writing out[i]
   and then summing in index order is deterministic.
8. A Thread, Mutex, Cond, and Queue must not move or be copied once in use. Keep one in a let (or
   an arena slot, as Pool does) and pass &.
9. A panic or crash on any thread ends the whole process. A panic prints [thread name] first, and
   every thread has its own crash stack, so a stack overflow in a worker still prints a trace
   (without the thread name). Scripts (.j3s) never run on a worker.


Which modules are safe on a worker
----------------------------------
No module state, and they call nothing main-thread only (checked by reading each module's globals
and imports). They keep state in the values you pass in, so two threads can use two different
values at once, never one value at once:
    j3.math  j3.rand  j3.text (its cstr scratch is per thread)  j3.io (lines from two threads can
    interleave)  j3.option  j3.result  j3.gfx.voxel  j3.gfx.noise  j3.gfx.mesh  j3.gfx.bvh
    j3.gfx.canvas (on an arena you own)  j3.game.path  .space  .collide  .tile  .anim  .ecs
    .physics  j3.data.save  j3.data.res  j3.data.json
Made for workers, since every call blocks: j3.net.tcp  .dns  .tls (loads libssl once under a lock;
its lastError is per thread)  .http  j3.proc (`recipes http`).
Inside j3.gfx only these touch no state: emitFace, emitFaceAo (mesh building), boxInClip, rayAabb
(geometry), plainSurface, fxWidth. Everything else in j3.gfx (draw, upload, lights, shadows,
effects) needs the GL context.
Main thread only: j3.game, j3.gfx (except the list above), j3.scene3d, j3.ui, j3.app,
j3.data.assets (it loads through j3.game).


Common mistakes
---------------
- Passing &local to a job when the function returns before the job runs. The struct must outlive
  the job: put it in an arena, an array, or a let that outlives the wait.
- Reading a buffer while a worker still writes it (\"it is mostly done\"). Wait for the signal.
- Calling wait, submit, or parallelFor from inside a job. A worker waiting on its own pool, or
  filling its queue, can deadlock it.
- A results Queue smaller than the jobs in flight, so workers block in push.
- Destroying the arena that holds a Pool before pool.shutdown(): the workers are still running.
- Two threads calling parallelFor on one Pool at once. One call at a time.
- Assuming jobs finish in the order they were submitted.
- Calling gfx or game from a job \"just to upload\". Hand the data back and upload on main.
- Using one Arena (or one Canvas, one voxel grid) from two threads.


Shared data that keeps changing
-------------------------------
Workers read a big grid while the frame thread edits it. Snapshot, stamp, compare, re-dispatch:

    // main, when it dispatches chunk c
    job.version = version[c];                 // what the grid looked like for this job
    copy(job.cells, grid, region(c));         // worker-owned copy: the edit cannot tear it
    pool.trySubmit(build, job as ptr);        // false: keep it, retry next frame
    // main, on every edit that touches chunk c
    version[c] += 1;
    dirty[c] = true;                          // dispatch c again when a job slot is free
    // main, when a result arrives
    if r.version != version[c] { dirty[c] = true; }   // stale: drop it, do not upload
    else { upload(r); }

Only main writes version[] and dirty[], so they need no atomics. The recipe does exactly this
with current[chunk]. To skip the copy and read the live grid, the worker must clamp every index,
never panic on an odd value, and read only ints and bytes; a bounds-check abort in a debug build
kills the process, and discarding the result afterwards does not help.


Program globals
---------------
- A #[singleton] written once at startup, before the first spawn or submit, and never again is
  safe to read from any worker (spawn and submit order the write before the job).
- Anything written after workers exist needs @atomic* (an int) or a Mutex. @atomicAdd(&COUNT, 1)
  works on a global.
- Do not initialise a global lazily (\"create on first use\") from workers without a lock: two
  threads both see it empty. Create it before the pool starts. memory.scratch gets away with it
  only because each thread has its own copy.
- A #[thread] global is one copy per thread, and a new thread's copy starts at the initializer,
  not at main's current value. Set it inside the job if the job needs it.


Cancel and priority
-------------------
There is no job priority and no cancel. Keep few jobs in flight and submit the urgent ones first.
To cancel, put an int flag in the job's struct, set it with @atomicStore(&flag, 1), and have the
job check @atomicLoad(&flag) between steps and return early. For results that went stale, stamp
each job with a version and have main drop results whose version is no longer current; the recipe
does this.


Language side
-------------
#[thread] on a module let gives every thread its own copy (a worker's copy starts at the
initializer). @atomicLoad(p), @atomicStore(p, v), @atomicAdd(p, v) (returns the old value), and
@atomicCas(p, old, new) (returns whether it swapped) work on an int through a pointer and are
sequentially consistent. Atomics are not supported in .j3s scripts.
";

pub fn index() -> Result<String, String> {
    let mut out = format!("j3c doc — from this j3c build ({})\n\n", crate::stdlib::provenance());
    out.push_str(TASKS);
    out.push_str("stdlib modules (`j3c doc <module>`):\n\n");
    for path in stdlib_files()? {
        let m = load_stdlib(&path)?;
        out.push_str(&m.key);
        if let Some((_, why)) = INTERNAL.iter().find(|(k, _)| m.key.starts_with(k)) {
            out.push_str(&format!(" — internal: {why}"));
        }
        out.push('\n');
        // First paragraph only; `j3c doc <module>` prints the rest.
        let head: String = m.file.doc.lines().take_while(|l| !l.trim().is_empty()).map(|l| format!("{l}\n")).collect();
        push_doc(&mut out, &head, "    ");
        out.push('\n');
    }
    out.push_str("also:\n\n");
    out.push_str("    j3c doc lang      language\n");
    out.push_str("    j3c doc j3ui      .j3ui components (markup + CSS)\n");
    out.push_str("    j3c doc j3ui-immediate  @j3ui immediate-mode UI functions\n");
    out.push_str("    j3c doc editor    3D scene editor\n");
    out.push_str("    j3c doc urp       URP render passes\n");
    out.push_str("    j3c doc threads   threads and worker pools\n");
    out.push_str("    j3c doc i18n      translated text and locales\n");
    out.push_str("    j3c doc recipes   complete programs by task\n");
    out.push_str("    j3c doc --help    every kind of query\n");
    Ok(out)
}

/// The documented stdlib files, `j3/game` before `j3/game/anim`.
fn stdlib_files() -> Result<Vec<PathBuf>, String> {
    let mut files = read_dir_j3(&stdlib_dir().join("j3"))?;
    files.sort_by_key(|p| p.with_extension(""));
    files.retain(|p| {
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        !HIDDEN.contains(&name) && !is_part(p)
    });
    Ok(files)
}

/// True for `ui/dom/tree.j3` when it says `module j3.ui.dom;`: a part of `ui/dom.j3`, documented there.
fn is_part(path: &Path) -> bool {
    let Some(main) = path.parent().map(|d| d.with_extension("j3")) else { return false };
    if main == *path || !(crate::stdlib::embedded(&main).is_some() || main.is_file()) {
        return false;
    }
    let (Ok(a), Ok(b)) = (read_source(path), read_source(&main)) else { return false };
    module_line(&a).is_some() && module_line(&a) == module_line(&b)
}

/// A parsed module and where its source came from.
struct Module {
    key: String,
    file: File,
    src: String,
    /// Where `at` lines point; empty for generated source.
    origin: String,
    /// Part files appended to `src`: (offset where each starts, its origin).
    parts: Vec<(usize, String)>,
    embedded: bool,
    /// `j3.scene3d.scripts`: the scene attachments it was made from.
    scan: Option<scene_scripts::Scan>,
}

fn load_stdlib(path: &Path) -> Result<Module, String> {
    let embedded = crate::stdlib::embedded(path).is_some();
    let origin_of = |p: &Path| p.strip_prefix(stdlib_dir()).unwrap_or(p).display().to_string();
    let (src, parts) = with_parts(path, read_source(path)?, &origin_of)?;
    let file = parse_file(path, &src)?;
    Ok(Module { key: file.module_key(), file, src, origin: origin_of(path), parts, embedded, scan: None })
}

/// The `module` line of a source file, if it has one.
fn module_line(src: &str) -> Option<&str> {
    src.lines().find(|l| l.starts_with("module "))
}

/// `src` with the files of the folder beside `path` that name the same module appended (the module is
/// spread over `X.j3` + `X/*.j3`). Each part's `module` line is blanked so the text parses as one
/// file, and its start offset and origin are returned for `at` lines.
fn with_parts(path: &Path, mut src: String, origin_of: &dyn Fn(&Path) -> String) -> Result<(String, Vec<(usize, String)>), String> {
    let main = module_line(&src).map(str::to_string);
    let mut parts = Vec::new();
    for part in crate::stdlib::parts(&path.with_extension("")) {
        let text = read_source(&part)?;
        if module_line(&text).map(str::to_string) != main {
            continue;
        }
        src.push('\n');
        parts.push((src.len(), origin_of(&part)));
        for line in text.split_inclusive('\n') {
            if line.starts_with("module ") {
                src.push_str(&" ".repeat(line.trim_end_matches('\n').len()));
                src.push_str(&line[line.trim_end_matches('\n').len()..]);
            } else {
                src.push_str(line);
            }
        }
    }
    Ok((src, parts))
}

fn here() -> Option<Manifest> {
    match project::resolve_cwd() {
        Ok(Input::Project(m)) => Some(m),
        _ => None,
    }
}

/// `j3.scene3d.scripts`, made the way the compiler makes it: from this project's scenes.
fn scripts_module() -> Result<Module, String> {
    let m = here().ok_or_else(|| {
        format!(
            "`{}` is made from a project's scenes, so run this in a project directory (one with j3.toml)\n  = help: `j3c doc lang` (Scripts) explains it",
            scene_scripts::MODULE
        )
    })?;
    scripts_at(&m.root)
}

fn scripts_at(root: &Path) -> Result<Module, String> {
    let scan = scene_scripts::scan(root);
    let src = scene_scripts::registry(root, &scan)?;
    let file = parse::parse(&src, FileId(1))
        .map_err(|e| parse::format_error(scene_scripts::LABEL, &src, &e))?;
    Ok(Module {
        key: scene_scripts::MODULE.to_string(),
        file,
        src,
        origin: String::new(),
        parts: Vec::new(),
        embedded: false,
        scan: Some(scan),
    })
}

/// A module by its dotted name: the generated scripts module, the stdlib (`j3.` optional),
/// then the project in this directory.
fn open_module(parts: &[&str]) -> Result<Option<Module>, String> {
    let key = parts.join(".");
    if key == scene_scripts::MODULE || key == "scene3d.scripts" {
        return scripts_module().map(Some);
    }
    let std_parts = if parts[0] == "j3" { &parts[1..] } else { parts };
    if !std_parts.is_empty() {
        let path = stdlib_dir().join("j3").join(format!("{}.j3", std_parts.join("/")));
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !HIDDEN.contains(&name) && (crate::stdlib::embedded(&path).is_some() || path.is_file()) {
            return load_stdlib(&path).map(Some);
        }
    }
    if parts[0] == "j3" {
        return Ok(None);
    }
    let Some(m) = here() else { return Ok(None) };
    let owned: Vec<String> = parts.iter().map(|s| s.to_string()).collect();
    let mut tries = vec![crate::compile::join_mod(&m.src_dir(), &owned)];
    if owned.len() > 1 && owned[0] == m.name {
        tries.push(crate::compile::join_mod(&m.src_dir(), &owned[1..]));
    }
    for path in tries {
        if path.is_file() {
            let src = std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
            let origin_of = |p: &Path| p.strip_prefix(&m.root).unwrap_or(p).display().to_string();
            let (src, parts) = with_parts(&path, src, &origin_of)?;
            let file = parse_file(&path, &src)?;
            return Ok(Some(Module { key: file.module_key(), file, src, origin: origin_of(&path), parts, embedded: false, scan: None }));
        }
    }
    Ok(None)
}

/// The longest module prefix of `query`, and the rest as an item.
fn resolve(query: &str) -> Result<(Module, Option<String>), String> {
    let parts: Vec<&str> = query.trim().split('.').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return Err("missing module\n  = help: `j3c doc`".to_string());
    }
    for end in (1..=parts.len()).rev() {
        if let Some(m) = open_module(&parts[..end])? {
            let item = (end < parts.len()).then(|| parts[end..].join("."));
            return Ok((m, item));
        }
    }
    Err(format!("no module `{}`\n  = help: `j3c doc` lists them; `j3c doc search <word>` finds a name", query.trim()))
}

fn lookup(query: &str, item: Option<&str>, short: bool) -> Result<String, String> {
    let (m, rest) = match resolve(query) {
        Ok(found) => found,
        Err(e) if item.is_none() => return by_name(query.trim()).ok_or(e)?,
        Err(e) => return Err(e),
    };
    let item = item.map(str::to_string).or(rest);
    if short && item.is_none() {
        return Ok(short_view(&m));
    }
    render_module(&m, item.as_deref())
}

/// `j3c doc drawText`: the module that has it, or the choices when several do.
fn by_name(name: &str) -> Option<Result<String, String>> {
    let mut hits: Vec<(String, String)> = Vec::new();
    for path in stdlib_files().ok()? {
        let Ok(m) = load_stdlib(&path) else { continue };
        for s in symbols(&m.file) {
            if s.qual == name || s.qual.ends_with(&format!(".{name}")) {
                hits.push((m.key.clone(), s.qual));
            }
        }
    }
    match hits.as_slice() {
        [] => None,
        [(module, qual)] => Some(lookup(module, Some(qual), false)),
        _ => {
            let mut out = format!("`{name}` is in more than one place:\n");
            for (module, qual) in &hits {
                out.push_str(&format!("    j3c doc {module} {qual}\n"));
            }
            Some(Ok(out))
        }
    }
}

fn render_module(m: &Module, item: Option<&str>) -> Result<String, String> {
    let filter = Filter::parse(item);
    if let Filter::Name(name) = filter {
        let quals: Vec<String> = symbols(&m.file)
            .into_iter()
            .filter(|s| s.qual == name || s.qual.ends_with(&format!(".{name}")))
            .map(|s| s.qual)
            .collect();
        if quals.len() > 1 {
            let mut out = format!("`{name}` names more than one item in {}:\n", m.key);
            for q in quals {
                out.push_str(&format!("    j3c doc {} {q}\n", m.key));
            }
            return Ok(out);
        }
    }
    let cx = Cx { src: &m.src, origin: &m.origin, parts: &m.parts, embedded: m.embedded, filter };
    let mut out = format!("module {}\n", m.key);
    if m.scan.is_some() {
        out.push_str("    Made by j3c from this project's scenes when a program uses it; never written to disk.\n");
    }
    if matches!(filter, Filter::All) {
        push_doc(&mut out, &m.file.doc, "    ");
    }
    out.push('\n');
    let mut hits = 0;
    for it in &m.file.items {
        hits += render_item(&mut out, &cx, it);
    }
    if let (Some(scan), Filter::All) = (&m.scan, filter) {
        out.push_str(&attached(scan));
    }
    if item.is_some() && hits == 0 {
        return Err(format!(
            "no public item `{}` in {}\n  = help: `j3c doc {} --short` lists its names",
            item.unwrap_or(""),
            m.key,
            m.key
        ));
    }
    Ok(out)
}

/// The scripts module's source: which scene objects carry which script.
fn attached(scan: &scene_scripts::Scan) -> String {
    let scripts = scan.scripts();
    if scripts.is_empty() {
        return "No scene attaches a script yet (`script=scripts/x.j3s` on an object line).\n".to_string();
    }
    let mut out = String::from(
        "attached scripts (one instance per object; find one with\n    `const Spin = @script(\"scripts/spin.j3s\");` then `scene.script::<Spin>(node)`)\n",
    );
    for script in scripts {
        out.push_str(&format!("    {script}\n"));
        for a in scan.attachments.iter().filter(|a| a.script == script) {
            out.push_str(&format!("        {}:{}  object #{}\n", a.scene, a.line, a.id));
        }
    }
    out
}

#[derive(Clone, Copy)]
enum Filter<'a> {
    All,
    /// A top-level item, a type, or any member with this name.
    Name(&'a str),
    /// `Type.member`.
    Member(&'a str, &'a str),
}

impl<'a> Filter<'a> {
    fn parse(item: Option<&'a str>) -> Filter<'a> {
        match item {
            None => Filter::All,
            Some(s) => match s.split_once('.') {
                Some((t, m)) => Filter::Member(t, m),
                None => Filter::Name(s),
            },
        }
    }

    fn top(self, name: &str) -> bool {
        match self {
            Filter::All => true,
            Filter::Name(n) => n == name,
            Filter::Member(..) => false,
        }
    }

    /// Whether a type shows all of itself, and whether it shows `member`.
    fn whole(self, ty: &str) -> bool {
        matches!(self, Filter::All) || matches!(self, Filter::Name(n) if n == ty)
    }

    fn member(self, ty: &str, member: &str) -> bool {
        self.whole(ty)
            || matches!(self, Filter::Name(n) if n == member)
            || matches!(self, Filter::Member(t, m) if t == ty && m == member)
    }
}

/// What rendering needs besides the item: the source for defaults and `at` lines.
struct Cx<'a> {
    src: &'a str,
    origin: &'a str,
    parts: &'a [(usize, String)],
    embedded: bool,
    filter: Filter<'a>,
}

impl Cx<'_> {
    /// `at file:line` under an item, only when one item was asked for.
    fn at(&self, out: &mut String, span: Span, indent: &str) {
        if matches!(self.filter, Filter::All) || self.origin.is_empty() {
            return;
        }
        let end = (span.start as usize).min(self.src.len());
        let (from, origin) = match self.parts.iter().rev().find(|(start, _)| *start <= end) {
            Some((start, origin)) => (*start, origin.as_str()),
            None => (0, self.origin),
        };
        let line = self.src[from..end].matches('\n').count() + 1;
        let note = if self.embedded { " (embedded in j3c)" } else { "" };
        out.push_str(&format!("{indent}at {origin}:{line}{note}\n"));
    }
}

/// A literal's text as written, or `...` when it is long.
fn expr_text(src: &str, e: &Expr) -> String {
    let text = src.get(e.span.start as usize..e.span.end as usize).unwrap_or("").trim();
    if text.is_empty() || text.len() > 60 || text.contains('\n') {
        "...".to_string()
    } else {
        text.to_string()
    }
}

/// Generated helpers (`find__scripts_door`) are public to the compiler, not to readers.
fn shown(name: &str) -> bool {
    !name.contains("__")
}

fn render_item(out: &mut String, cx: &Cx, item: &Item) -> usize {
    match item {
        Item::Fn(f) if f.vis_pub && shown(&f.name.name) && cx.filter.top(&f.name.name) => {
            push_fn(out, f, None, "");
            cx.at(out, f.span, "    ");
            1
        }
        Item::Const(c) if c.vis_pub && shown(&c.name.name) && cx.filter.top(&c.name.name) => {
            out.push_str("const ");
            out.push_str(&c.name.name);
            if let Some(ty) = &c.ty {
                out.push_str(": ");
                out.push_str(&ty_str(ty));
            }
            out.push_str(&format!(" = {}\n", expr_text(cx.src, &c.value)));
            push_doc(out, &c.doc, "    ");
            cx.at(out, c.span, "    ");
            out.push('\n');
            1
        }
        Item::Struct(s) if s.vis_pub => render_struct(out, cx, s),
        Item::Enum(e) if e.vis_pub => render_enum(out, cx, e),
        _ => 0,
    }
}

fn render_struct(out: &mut String, cx: &Cx, s: &StructItem) -> usize {
    let name = s.name.name.as_str();
    let whole = cx.filter.whole(name);
    let fields: Vec<&FieldItem> = s.fields.iter().filter(|f| cx.filter.member(name, &f.name.name)).collect();
    let methods: Vec<&FnItem> = s
        .methods
        .iter()
        .filter(|m| cx.filter.member(name, &m.name.name))
        .collect();
    if !whole && fields.is_empty() && methods.is_empty() {
        return 0;
    }
    let owner = format!("{}{}", name, type_params(&s.type_params));
    out.push_str("struct ");
    out.push_str(&owner);
    out.push('\n');
    if whole {
        push_doc(out, &s.doc, "    ");
        cx.at(out, s.span, "    ");
    }
    for f in &fields {
        out.push_str("    ");
        if f.embedded {
            out.push_str("use ");
        }
        out.push_str(&f.name.name);
        out.push_str(": ");
        out.push_str(&ty_str(&f.ty));
        if let Some(d) = &f.default {
            out.push_str(&format!(" = {}", expr_text(cx.src, d)));
        }
        out.push('\n');
        push_doc(out, &f.doc, "        ");
        if !whole {
            cx.at(out, f.span, "        ");
        }
    }
    for m in &methods {
        push_fn(out, m, Some(&owner), "    ");
        if !whole {
            cx.at(out, m.span, "        ");
        }
    }
    out.push('\n');
    1 + fields.len() + methods.len()
}

fn render_enum(out: &mut String, cx: &Cx, e: &EnumItem) -> usize {
    let name = e.name.name.as_str();
    let whole = cx.filter.whole(name);
    let variants: Vec<_> = e.variants.iter().filter(|v| cx.filter.member(name, &v.name.name)).collect();
    if !whole && variants.is_empty() {
        return 0;
    }
    out.push_str("enum ");
    out.push_str(name);
    out.push_str(&type_params(&e.type_params));
    out.push('\n');
    if whole {
        push_doc(out, &e.doc, "    ");
        cx.at(out, e.span, "    ");
    }
    for v in &variants {
        out.push_str("    ");
        out.push_str(&v.name.name);
        if let Some(ty) = &v.payload {
            out.push('(');
            out.push_str(&ty_str(ty));
            out.push(')');
        }
        out.push('\n');
        push_doc(out, &v.doc, "        ");
        if !whole {
            cx.at(out, v.span, "        ");
        }
    }
    out.push('\n');
    1 + variants.len()
}

/// One public name: `drawText`, `Texture`, `Texture.destroy`, `Key.Space`.
struct Symbol {
    qual: String,
    doc: String,
}

fn symbols(file: &File) -> Vec<Symbol> {
    let mut out = Vec::new();
    let mut add = |qual: String, doc: &str| out.push(Symbol { qual, doc: doc.to_string() });
    for item in &file.items {
        match item {
            Item::Fn(f) if f.vis_pub && shown(&f.name.name) => add(f.name.name.clone(), &f.doc),
            Item::Const(c) if c.vis_pub && shown(&c.name.name) => add(c.name.name.clone(), &c.doc),
            Item::Struct(s) if s.vis_pub => {
                add(s.name.name.clone(), &s.doc);
                for f in &s.fields {
                    add(format!("{}.{}", s.name.name, f.name.name), &f.doc);
                }
                for m in &s.methods {
                    add(format!("{}.{}", s.name.name, m.name.name), &m.doc);
                }
            }
            Item::Enum(e) if e.vis_pub => {
                add(e.name.name.clone(), &e.doc);
                for v in &e.variants {
                    add(format!("{}.{}", e.name.name, v.name.name), &v.doc);
                }
            }
            _ => {}
        }
    }
    out
}

/// `j3c doc search <word>`: modules about it, public names containing it, then names whose
/// comment mentions it.
fn search(word: &str) -> Result<String, String> {
    let w = word.to_lowercase();
    let (mut modules, mut names, mut docs) = (Vec::new(), Vec::new(), Vec::new());
    for path in stdlib_files()? {
        let m = load_stdlib(&path)?;
        let internal = INTERNAL.iter().any(|(k, _)| m.key.starts_with(k));
        if m.key.to_lowercase().contains(&w) || m.file.doc.to_lowercase().contains(&w) {
            let first = m.file.doc.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
            let cut: String = first.chars().take(70).collect();
            modules.push((internal, format!("    j3c doc {}  — {cut}\n", m.key)));
        }
        for s in symbols(&m.file) {
            let first = s.doc.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
            let mut line = format!("    j3c doc {} {}", m.key, s.qual);
            if !first.is_empty() {
                let cut: String = first.chars().take(70).collect();
                line.push_str(&format!("  — {cut}"));
            }
            line.push('\n');
            let name = s.qual.rsplit('.').next().unwrap_or("").to_lowercase();
            if name.contains(&w) {
                names.push((internal, line));
            } else if s.doc.to_lowercase().contains(&w) {
                docs.push((internal, line));
            }
        }
    }
    // Game APIs first; internals last.
    modules.sort_by_key(|(internal, _)| *internal);
    names.sort_by_key(|(internal, _)| *internal);
    docs.sort_by_key(|(internal, _)| *internal);
    const MAX: usize = 60;
    let mut out = String::new();
    for (title, list) in [("modules about", &modules), ("names containing", &names), ("comments mentioning", &docs)] {
        if list.is_empty() {
            continue;
        }
        out.push_str(&format!("{title} `{word}`:\n"));
        for (_, line) in list.iter().take(MAX) {
            out.push_str(line);
        }
        if list.len() > MAX {
            out.push_str(&format!("    ... {} more; use a longer word\n", list.len() - MAX));
        }
        out.push('\n');
    }
    // Guide pages and recipes, so a task word finds the page that teaches it.
    let mut pages = Vec::new();
    let guides: [(&str, String); 10] = [
        ("j3ui", J3UI_PAGE.to_string()),
        ("j3ui-immediate", j3ui_page()),
        ("css", CSS_PAGE.to_string()),
        ("testing", TESTING_PAGE.to_string()),
        ("lang", lang_page()),
        ("editor", editor_page()),
        ("urp", urp_page()),
        ("threads", THREADS_PAGE.to_string()),
        ("i18n", I18N_PAGE.to_string()),
        ("ship", SHIP_PAGE.to_string()),
    ];
    for (name, text) in &guides {
        if text.to_lowercase().contains(&w) {
            pages.push(format!("    j3c doc {name}\n"));
        }
    }
    for r in crate::helptext::recipes() {
        if r.name.contains(&w) || r.what.to_lowercase().contains(&w) || r.text.to_lowercase().contains(&w) {
            pages.push(format!("    j3c doc recipes {}  — {}\n", r.name, r.what));
        }
    }
    if !pages.is_empty() {
        out.push_str(&format!("guide pages and recipes mentioning `{word}`:\n"));
        pages.iter().for_each(|l| out.push_str(l));
        out.push('\n');
    }
    if out.is_empty() {
        return Err(format!("no public name or comment mentions `{word}`\n  = help: `j3c doc` lists modules by task"));
    }
    Ok(out)
}

/// `j3c doc <module> --short`: every public name on a screen or so.
fn short_view(m: &Module) -> String {
    let mut out = format!("module {}", m.key);
    if let Some(first) = m.file.doc.lines().map(str::trim).find(|l| !l.is_empty()) {
        out.push_str(&format!(" — {first}"));
    }
    out.push_str("\n\n");
    let mut fns = Vec::new();
    let mut consts = Vec::new();
    let mut types = String::new();
    for item in &m.file.items {
        match item {
            Item::Fn(f) if f.vis_pub && shown(&f.name.name) => fns.push(f.name.name.clone()),
            Item::Const(c) if c.vis_pub && shown(&c.name.name) => consts.push(c.name.name.clone()),
            Item::Struct(s) if s.vis_pub => {
                let mut names: Vec<String> = s.fields.iter().map(|f| f.name.name.clone()).collect();
                names.extend(s.methods.iter().map(|f| format!("{}()", f.name.name)));
                types.push_str(&wrap(&format!("struct {}", s.name.name), &names));
            }
            Item::Enum(e) if e.vis_pub => {
                let names: Vec<String> = e.variants.iter().map(|v| v.name.name.clone()).collect();
                types.push_str(&wrap(&format!("enum {}", e.name.name), &names));
            }
            _ => {}
        }
    }
    if !fns.is_empty() {
        out.push_str(&wrap("fn", &fns));
    }
    if !consts.is_empty() {
        out.push_str(&wrap("const", &consts));
    }
    out.push_str(&types);
    out.push_str(&format!(
        "\nsignatures and comments: `j3c doc {0}`; one name: `j3c doc {0} <name>`\n",
        m.key
    ));
    out
}

/// `label: a b c` wrapped at 100 columns under a hanging indent.
fn wrap(label: &str, names: &[String]) -> String {
    let mut out = format!("{label}:");
    let mut col = out.len();
    for n in names {
        if col + 1 + n.len() > 100 {
            out.push_str("\n   ");
            col = 3;
        }
        out.push(' ');
        out.push_str(n);
        col += 1 + n.len();
    }
    out.push('\n');
    out
}

fn read_source(path: &Path) -> Result<String, String> {
    if let Some(src) = crate::stdlib::embedded(path) {
        return Ok(src.to_string());
    }
    std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))
}

fn read_dir_j3(dir: &Path) -> Result<Vec<PathBuf>, String> {
    if !crate::stdlib::using_override() {
        let mut out: Vec<PathBuf> = crate::stdlib::files().map(|(name, _)| dir.join(name)).collect();
        out.sort();
        return Ok(out);
    }
    let mut out = Vec::new();
    collect_j3(dir, &mut out)?;
    Ok(out)
}

fn collect_j3(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let ents = std::fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
    for ent in ents.flatten() {
        let path = ent.path();
        if path.is_dir() {
            collect_j3(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("j3") {
            out.push(path);
        }
    }
    Ok(())
}

fn parse_file(path: &Path, src: &str) -> Result<File, String> {
    parse::parse(src, FileId(1)).map_err(|e| parse::format_error(&path.display().to_string(), src, &e))
}

fn push_fn(out: &mut String, f: &FnItem, owner: Option<&str>, indent: &str) {
    out.push_str(indent);
    out.push_str("fn ");
    if let Some(owner) = owner {
        out.push_str(owner);
        out.push('.');
    }
    out.push_str(&f.name.name);
    out.push_str(&type_params(&f.type_params));
    out.push('(');
    for (i, p) in f.params.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&p.name.name);
        out.push_str(": ");
        out.push_str(&ty_str(&p.ty));
    }
    out.push(')');
    if let Some(ret) = &f.ret {
        out.push_str(": ");
        out.push_str(&ty_str(ret));
    }
    out.push('\n');
    let doc_indent = format!("{indent}    ");
    push_doc(out, &f.doc, &doc_indent);
}

fn type_params(ps: &[Ident]) -> String {
    if ps.is_empty() {
        return String::new();
    }
    let mut s = String::from("<");
    for (i, p) in ps.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&p.name);
    }
    s.push('>');
    s
}

fn ty_str(t: &Type) -> String {
    match &t.kind {
        TypeKind::Named(n) => n.clone(),
        TypeKind::Generic { name, args } => {
            let args: Vec<String> = args.iter().map(ty_str).collect();
            format!("{name}<{}>", args.join(", "))
        }
        TypeKind::Ref(inner) => format!("&{}", ty_str(inner)),
        TypeKind::Array { len, elem } => format!("[{len}]{}", ty_str(elem)),
        TypeKind::Slice(elem) => format!("[]{}", ty_str(elem)),
        TypeKind::Fn { params, ret } => {
            let params: Vec<String> = params.iter().map(ty_str).collect();
            match ret {
                Some(r) => format!("fn({}): {}", params.join(", "), ty_str(r)),
                None => format!("fn({})", params.join(", ")),
            }
        }
    }
}

fn push_doc(out: &mut String, doc: &str, indent: &str) {
    for line in doc.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        out.push_str(indent);
        out.push_str(line);
        out.push('\n');
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn module(query: &str, item: Option<&str>) -> Result<String, String> {
        lookup(query, item, false)
    }

    #[test]
    fn script_doc_lists_props_shared_state_and_calls() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../speculative/11_script/scripts/door.j3s");
        let text = script_doc(&path.display().to_string()).expect("script doc");
        assert!(text.contains("hinge: Node (required)"), "{text}");
        assert!(text.contains("openAngle: f32 = 90.0"), "{text}");
        assert!(text.contains("shared: one value for every instance"), "{text}");
        assert!(text.contains("doorsOpened: int = 0"), "{text}");
        assert!(text.contains("state: each instance's own; private"), "{text}");
        assert!(text.contains("open = false"), "{text}");
        assert!(text.contains("fn toggle()"), "{text}");
        assert!(!text.contains("fn smooth("), "{text}");
    }

    #[test]
    fn index_lists_tasks_modules_and_provenance() {
        let text = index().expect("index");
        assert!(text.contains("stdlib embedded, docs "), "{text}");
        assert!(text.contains("recipes scene3d"), "{text}");
        assert!(text.contains("j3c doc search <word>"), "{text}");
        assert!(text.contains("j3.app\n"), "{text}");
        assert!(text.contains("j3c doc lang"), "{text}");
        assert!(text.contains("Bevy-shaped runner"), "{text}");
        assert!(text.contains("j3.gfx.mesh\n"), "{text}");
        assert!(text.contains("j3.script — internal:"), "{text}");
        assert!(text.contains("j3.scene3d.editor.paste — internal: editor internals"), "{text}");
        assert_eq!(text.matches("j3.game\n").count(), 1, "{text}");
        assert!(text.contains("game_headless.j3 is the test stand-in"), "{text}");
    }

    #[test]
    fn app_create_is_documented_with_its_source() {
        let text = module("j3.app", Some("create")).expect("doc");
        assert!(text.contains("fn App<W>.create("), "{text}");
        assert!(text.contains("Boots"), "{text}");
        assert!(text.contains("at j3/app.j3:"), "{text}");
        assert!(text.contains("(embedded in j3c)"), "{text}");
        assert!(!text.contains("fn App<W>.run("), "{text}");
        assert_eq!(module("j3.app", Some("App.create")).expect("qualified"), text);
        assert_eq!(module("j3.app.App.create", None).expect("dotted"), text);
    }

    #[test]
    fn ambiguous_names_list_qualified_choices() {
        let text = module("j3.math", Some("length")).expect("choices");
        assert!(text.contains("names more than one item"), "{text}");
        assert!(text.contains("j3c doc j3.math Vec3.length"), "{text}");
        assert!(text.contains("j3c doc j3.math Quat.length"), "{text}");
        let one = module("j3.math", Some("Vec3.length")).expect("one");
        assert!(one.contains("fn Vec3.length("), "{one}");
        assert!(!one.contains("Quat"), "{one}");
    }

    #[test]
    fn a_bare_name_finds_its_module() {
        let text = module("textureFromCanvas", None).expect("found");
        assert!(text.contains("module j3.game"), "{text}");
        assert!(text.contains("fn textureFromCanvas("), "{text}");
        assert!(module("noSuchNameAnywhere", None).is_err());
    }

    #[test]
    fn defaults_and_constant_values_show() {
        let text = module("j3.math", Some("PI")).expect("PI");
        assert!(text.contains("const PI: f32 = 3.14159"), "{text}");
    }

    #[test]
    fn short_view_fits_a_screen() {
        let text = lookup("j3.game", None, true).expect("short");
        assert!(text.contains("fn: "), "{text}");
        assert!(text.contains("textureFromCanvas"), "{text}");
        assert!(text.contains("signatures and comments: `j3c doc j3.game`"), "{text}");
        assert!(text.lines().count() < 80, "{} lines\n{text}", text.lines().count());
    }

    #[test]
    fn search_finds_names_then_comments() {
        let text = search("canvas").expect("search");
        assert!(text.contains("j3c doc j3.game textureFromCanvas"), "{text}");
        assert!(search("zzqqxx").is_err());
    }

    #[test]
    fn scripts_module_comes_from_the_project_scenes() {
        let dir = std::env::temp_dir().join(format!("j3-doc-scripts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (path, text) in crate::editor::project_files("demo", false) {
            let path = dir.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let m = scripts_at(&dir).expect("module");
        let text = render_module(&m, None).expect("render");
        assert!(text.contains("module j3.scene3d.scripts"), "{text}");
        assert!(text.contains("fn start(scene: &Scene)"), "{text}");
        assert!(text.contains("fn update(scene: &Scene, dt: f32)"), "{text}");
        assert!(text.contains("scripts/spin.j3s\n        scenes/main.j3scene:"), "{text}");
        assert!(!text.contains("find__"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn testing_and_css_pages_are_found() {
        assert!(TESTING_PAGE.contains("--headless") && TESTING_PAGE.contains("imgdiff"));
        assert!(J3UI_PAGE.contains("j3c doc css") && J3UI_PAGE.contains("j3c doc testing"));
        for w in ["imgdiff", "golden", "headless", "bind"] {
            assert!(search(w).expect(w).contains("j3c doc "), "{w}");
        }
        assert!(search("imgdiff").unwrap().contains("j3c doc testing"));
        assert!(index().unwrap().contains("j3c doc testing"));
        assert!(search("deb").unwrap().contains("j3c doc ship"));
    }

    #[test]
    fn j3ui_pages_link_each_other() {
        assert!(J3UI_PAGE.contains("j3c doc j3ui-immediate"));
        assert!(J3UI_PAGE.contains("recipes components"));
        assert!(j3ui_page().contains("j3c doc j3ui`"));
    }

    #[test]
    fn j3ui_page_lists_every_element() {
        for name in crate::ui::gen::ELEMENTS {
            assert!(J3UI_PAGE.contains(&format!(" {name}")), "`j3c doc j3ui` is missing element {name}");
        }
    }

    #[test]
    fn j3ui_page_lists_the_contract() {
        let text = j3ui_page();
        assert!(text.contains("@j3ui shopRow"), "{text}");
        assert!(text.contains("cannot see locals"), "{text}");
        for name in J3UI_END.iter().chain(J3UI_POP).chain(J3UI_IF).chain(J3UI_CALL) {
            assert!(
                crate::parse::syntax::is_j3ui_widget(name),
                "{name} is not a widget"
            );
            assert!(text.contains(&format!("`{name}`")), "page missing {name}\n{text}");
        }
    }

    #[test]
    fn lang_page_lists_shipped_forms() {
        let text = lang_page();
        assert!(text.contains("0..n"), "{text}");
        assert!(text.contains("0..=n"), "{text}");
        assert!(text.contains("TryCatch"), "{text}");
        assert!(text.contains("`j3ui`"), "{text}");
        assert!(text.contains("j3c doc j3.app"), "{text}");
        assert!(text.contains("`match`"), "{text}");
        assert!(text.contains("@script"), "{text}");
        assert!(text.contains("props {"), "{text}");
        assert!(text.contains("f32(n)"), "{text}");
        assert!(text.contains("swapRemove") && text.contains("inside the struct body"), "{text}");
    }

    #[test]
    fn missing_item_errors() {
        let err = module("j3.app", Some("nope")).unwrap_err();
        assert!(err.contains("nope"), "{err}");
        let err = module("j3.app", Some("App.nope")).unwrap_err();
        assert!(err.contains("App.nope"), "{err}");
    }

    #[test]
    fn threads_guide_is_reachable_and_complete() {
        let page = THREADS_PAGE;
        for needle in ["Shared data that keeps changing", "Program globals", "visible to that job", "HAND WORK OVER", "trySubmit", "main-thread only", "Common mistakes", "emitFaceAo", "@atomicCas"] {
            assert!(page.contains(needle), "{needle}");
        }
        let idx = index().unwrap();
        assert!(idx.contains("j3c doc threads"), "{idx}");
        assert!(idx.contains("j3.thread\n"), "j3.thread is listed");
    }

    /// Every recipe's files make a program that `j3c check` accepts.
    #[test]
    fn recipes_check() {
        for r in crate::helptext::recipes() {
            if r.files.is_empty() {
                continue;
            }
            let dir = std::env::temp_dir().join(format!("j3-recipe-{}-{}", r.name, std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            for (path, text) in &r.files {
                let path = dir.join(path);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, text).unwrap();
            }
            let targets: Vec<PathBuf> = if dir.join("j3.toml").is_file() {
                vec![dir.clone()]
            } else {
                r.files.iter().filter(|(p, _)| p.ends_with(".j3")).map(|(p, _)| dir.join(p)).collect()
            };
            for t in targets {
                let code = crate::cli::run(["check".into(), t.display().to_string()]);
                assert_eq!(code, 0, "recipe {} does not check", r.name);
            }
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
