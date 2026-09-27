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

const HIDDEN: &[&str] = &["game_headless.j3", "sys_windows.j3", "sys_macos.j3"];

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
    j3c doc j3ui               `@j3ui` component syntax
    j3c doc editor             3D scene editor, scene files, scripts
    j3c doc urp                URP render passes (pass order, GL lifetime)";

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
        ["j3ui"] | ["@j3ui"] => Ok(j3ui_page()),
        ["editor"] => Ok(editor_page()),
        ["urp"] => Ok(urp_page()),
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

One file is one module. `use j3.io.printLn` brings in one name. `use j3.memory.{{ Arena, List }}` brings in several. `pub` is what another module can name.

`if` and `while` take no parentheses. Braces are required. An `if` used as a value needs `else`. `&&` and `||` stop at the first decisive operand.

`for i in 0..n` stops before `n`. `for i in 0..=n` includes `n`. `for n in xs` walks a slice or an array. The index name ends when the loop ends.

`match` lists every variant. Write `Enum.Name`, not a bare name. `_` is the rest. `defer` runs at the end of its block, including on `return`. `break` and `continue` stay in the loop.

A backtick string `hello {{name}}` fills the hole. A struct in a hole needs `#[Debug]`.

`+=` `-=` `*=` write the place once (no `/=` or `%=` yet). `%` is the remainder and truncates like C: `-7 % 3 == -1`; `math.wrapi(-7, 3) == 2` wraps an index. Integer overflow wraps. Divide or `%` by zero aborts. `--fast` strips that abort and the bounds-check aborts.

`operator+` is a method. `this` is the receiver. `.` follows a pointer. A method `this: &T` takes the address at the call. A field may end with `= value`. A struct literal that omits the field uses that value.

`?` unwraps a `#[TryCatch(Ok, Err)]` enum: the failure variant returns from the function, the success variant continues. `j3.option.Option` and `j3.result.Result` are that attribute. The compiler does not special-case the names.

`#[singleton]` is one process value. Its initializer is a constant, and it cannot be a call.

`[N]T` is a stack array. `[]T` is a pointer and a length. `xs[i]` checks the bound. `xs[1..3]` and `xs[:]` are views. `string` is a read-only byte view. Mutate bytes with `j3c doc j3.text`.

`use position: Vec2` on a struct embeds those fields. A statement `use transform` names them in this block, and assigning one writes the field.

`use lib \"SDL2\" as sdl {{ fn SDL_Init(flags: u32): i32; }}` declares C functions. `@syscall`, `@sizeof`, and `@slice` are compiler calls. `@j3ui` is `j3c doc j3ui`.

No traits, capturing closures, GC, or borrow checker.

Scripts
    A `.j3s` file is a script: J3 with the same checker, and no `module` line (its path names it). Each value of a script is one instance with its own props and state.
    `props {{ hinge: NodeId; openAngle: f32 = 90.0; }}` is what the importer passes in; props are read-only in the script, and one with no default is required.
    `shared {{ opened: int = 0; }}` is one value every instance reads and writes. A top-level `let` is per-instance state. Every default and initializer is a literal. A script may also declare `struct` and `enum`; those names share the script's namespace, and the script type's name is taken.
    Only `pub fn`s are callable from outside. Outside code can't read an instance's fields or copy one; pass `&Door`.
    `const Door = @script(\"scripts/door.j3s\");` (path from the project root) gives the type; `Door {{ hinge: h }}` makes one, and `door.toggle()` calls it. A script can hold other scripts the same way.
    In a scene, `node.<i>.script s=scripts/door.j3s` attaches one to an object and `param.<id>.<prop>` lines set its props. `use j3.scene3d.scripts;` then `scripts.start(&scene)` and `scripts.update(&scene, dt)` run every attached script's `pub fn start(scene: &Scene)` and `pub fn update(scene: &Scene, dt: f32)`; `scene.script::<Door>(node)` finds one. A `node: NodeId` prop is the object the script is on.
    `j3c run` runs scripts on a bytecode VM: editing a script re-runs without compiling the program, and while it runs a code edit swaps in. `j3c build` compiles every script in. `j3c dump-bc scripts/door.j3s` shows the bytecode.
    `j3c run path/to/tool.j3s` runs that file once: it makes one instance and calls `pub fn main()`. Arguments after the path are `j3.sys.arg` (`arg(0)` is the program). A scene script is still attached with `node.<i>.script`, not run this way.

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

fn j3ui_page() -> String {
    format!(
        r#"`@j3ui` — UI components

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
- An object's script is `node.<i>.script s=scripts/door.j3s`; its props are `param.<id>.<prop>` lines. `j3c check` reports a wrong prop type or a missing required prop against the scene line. `j3c -e script-props scripts/door.j3s` lists what the inspector edits.
- Edit scenes with `j3c -e scene` (by object name, `Parent/Child` path, or `#id`), not by hand: `j3c -e scene --help`.
- Look at what you built: `j3c -e render [--view iso|top|front] [--focus NAME] [--out F.png]`, and
  `j3c run --frames 120 --screenshot F.png --headless` for the running game.
- Place without guessing heights: `j3c -e scene ground|ontop|scatter`.
- Meshes from code: `assets/gen/<name>.j3` with `j3.gfx.meshgen`, made by `j3c -e gen` (`j3c doc j3.gfx.meshgen`).
- Reusable scenes can declare props: `j3c -e scene set @scene prop.speed=float:1`, then
  `set <copy> prop.speed=2`; code reads `scene3d.propNumber(&scene, scene3d.copyOf(&scene, id), \"speed\", 1)`.
- `j3c -e mcp` serves all of this as MCP tools; with the editor open, edits go to it live (one undo step each, unsaved until saved); with it closed they write the file and `undo` puts it back. `j3c -e mcp --tools` lists every tool's arguments.
- Check loop, and what each step catches: `j3c doc recipes check`. Scene loading and drawing from code: `j3c doc recipes scene3d`.
- Scene files are line-based `key=value` saves. The editor picks up outside edits within a second (merging them into unsaved work), but `j3c -e mcp` edits it directly and is safer while it's open.
- Runtime API for scenes, fields, lights, and materials: `j3c doc j3.scene3d`. Build/run: `j3c check`, `j3c run`.
- Build errors from Play land in `target/j3-editor/build.log`.
- Changing the editor itself: `j3c/editor/src/` (window, one module per file; `main.j3` is the entry), `j3c/src/editor.rs` (host, project setup), `j3c/stdlib/j3/scene3d.j3` (format); `J3_EDITOR_SRC=path/to/editor/src` builds from disk.

{}",
        include_str!("../editor/README.md")
    )
}


const TASKS: &str = "\
Start from the task. `j3c doc recipes <name>` prints a complete, checked program.
    2D game in a window              recipes app2d     then j3.app, j3.game, j3.ui, `j3c doc j3ui`
    3D game from editor scenes       recipes scene3d   then j3.scene3d, j3.gfx, `j3c doc editor`
    first-person 3D starter          `j3c new <name> --template fps`, then j3.scene3d.fpwalk
    sprites and textures, no window  recipes canvas    then j3.gfx.canvas
    per-object logic in .j3s         recipes script    then `j3c doc lang` (Scripts), j3.scene3d.scripts
    meshes built in code             recipes meshgen   then j3.gfx.meshgen
    materials and shader effects     recipes material  then j3.scene3d.fx, `j3c doc editor`
    traced mirrors, glass, shadows   recipes urp       then `j3c doc urp`
    check and look at what you built recipes check
    saves, assets, ECS, physics      j3.data.save, j3.data.assets, j3.game.ecs, j3.game.physics
Find a name without knowing its module: `j3c doc search <word>`, or `j3c doc <name>`.
A module's names on one screen: `j3c doc <module> --short`.

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
        push_doc(&mut out, &m.file.doc, "    ");
        out.push('\n');
    }
    out.push_str("also:\n\n");
    out.push_str("    j3c doc lang      language\n");
    out.push_str("    j3c doc j3ui      UI components\n");
    out.push_str("    j3c doc editor    3D scene editor\n");
    out.push_str("    j3c doc urp       URP render passes\n");
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
        !HIDDEN.contains(&name)
    });
    Ok(files)
}

/// A parsed module and where its source came from.
struct Module {
    key: String,
    file: File,
    src: String,
    /// Where `at` lines point; empty for generated source.
    origin: String,
    embedded: bool,
    /// `j3.scene3d.scripts`: the scene attachments it was made from.
    scan: Option<scene_scripts::Scan>,
}

fn load_stdlib(path: &Path) -> Result<Module, String> {
    let src = read_source(path)?;
    let file = parse_file(path, &src)?;
    let embedded = crate::stdlib::embedded(path).is_some();
    let origin = if embedded { path.strip_prefix(stdlib_dir()).unwrap_or(path) } else { path };
    let origin = origin.display().to_string();
    Ok(Module { key: file.module_key(), file, src, origin, embedded, scan: None })
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
            let file = parse_file(&path, &src)?;
            let origin = path.strip_prefix(&m.root).unwrap_or(&path).display().to_string();
            return Ok(Some(Module { key: file.module_key(), file, src, origin, embedded: false, scan: None }));
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
    let cx = Cx { src: &m.src, origin: &m.origin, embedded: m.embedded, filter };
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
        return "No scene attaches a script yet (`node.<i>.script s=scripts/x.j3s`).\n".to_string();
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
        let line = self.src[..end].matches('\n').count() + 1;
        let note = if self.embedded { " (embedded in j3c)" } else { "" };
        out.push_str(&format!("{indent}at {}:{line}{note}\n", self.origin));
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
    }

    #[test]
    fn missing_item_errors() {
        let err = module("j3.app", Some("nope")).unwrap_err();
        assert!(err.contains("nope"), "{err}");
        let err = module("j3.app", Some("App.nope")).unwrap_err();
        assert!(err.contains("App.nope"), "{err}");
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
