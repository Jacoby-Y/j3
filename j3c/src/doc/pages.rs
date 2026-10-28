use super::*;

// Guide pages that are plain text live in pages/*.md.
pub(super) const TASKS: &str = include_str!("pages/tasks.md");
pub(super) const J3UI_PAGE: &str = include_str!("pages/j3ui.md");
pub(super) const CSS_PAGE: &str = include_str!("pages/css.md");
pub(super) const TESTING_PAGE: &str = include_str!("pages/testing.md");
/// `j3c doc threads`: the long-form guide. `j3.thread`'s own header stays short because the
/// module index prints it.
pub(super) const THREADS_PAGE: &str = include_str!("pages/threads.md");
pub(super) const I18N_PAGE: &str = include_str!("pages/i18n.md");
/// `j3c doc ship`: release builds and `[dist] build_outputs` (`src/dist.rs`, `src/dist/deb.rs`).
pub(super) const SHIP_PAGE: &str = include_str!("pages/ship.md");
/// `j3c doc android`: APK builds, `[android]` keys, running on a device, and the limits (`src/dist/apk.rs`,
/// `src/android_run.rs`, `src/emit/android.rs`, `tools/android/build-sdl.sh`).
pub(super) const ANDROID_PAGE: &str = include_str!("pages/android.md");

/// `j3c doc urp`: the URP guide, embedded so it matches this j3c build.
pub(super) fn urp_page() -> String {
    format!(
        "`j3c doc urp` — URP render passes

In an editor scene, URP is a scene setting (`j3c -e scene set @scene urp=true`) and
`scene3d.draw` traces it with no extra calls (`j3c doc editor`, URP). The modules below are
separate renderers that a game drives itself.

{}",
        include_str!("../../stdlib/j3/gfx/urp/README.md")
    )
}

pub(super) fn names(list: &[&str]) -> String {
    list.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ")
}

pub(super) fn lang_page() -> String {
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

pub(super) fn j3ui_page() -> String {
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
pub(super) fn editor_page() -> String {
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
        include_str!("../../editor/README.md")
    )
}

