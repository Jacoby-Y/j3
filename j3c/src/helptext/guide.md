j3c guide — start a J3 project

The compiler is `j3c` on PATH. A new directory does not contain the
language. The standard library is inside the `j3c` binary. `$J3_STDLIB`
replaces that copy with a directory when it is set.

    j3c new mygame           hello: j3.toml + src/main.j3 + AGENTS.md
    j3c new mygame --app     SDL window (links SDL2)
    j3c new mygame --template ui   .j3ui components window (`j3c doc j3ui`)
    cd mygame
    j3c check                typecheck, then missing libraries and loaded files
    j3c run --frames 60 --screenshot shot.png --headless   run with no window, save a frame
    j3c build                writes target/<name>
    j3c build --release      small binary, plus [dist] build_outputs: zip, .deb (`j3c doc ship`);
                             `j3.build.DEV` / `RELEASE` tell code which build it is

Agents: never open a window. A bare `j3c run` of an --app, ui, or 3D project opens one.
Always add `--headless` (or `SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy` on the
same command for a built binary), read the PNG, and wrap long runs in `timeout`.
`j3c doc testing` has captures, goldens, and `j3c -e imgdiff`.

`j3c new` with no name writes into the current directory. The module
name comes from that directory. `my-game` and `My Game` become `my_game`.

Layout
    j3.toml                  [package] name, entry (default src/main.j3)
    src/main.j3              module <name>; and fn main()
    src/other.j3             use <name>.other.Name
    AGENTS.md                how to work in this project

One file is one module. `use j3.io.printLn` brings in one name.
`use j3.memory.{ Arena, List }` brings in several.

Scripts are `.j3s` files anywhere in the project (`scripts/door.j3s`): J3
with `props { ... }`, per-instance `let` state, and `pub fn`s. Bind one
with `const Door = @script("scripts/door.j3s");` and make instances with
`Door { hinge: h }`. `j3c run` runs scripts on a VM, so a script edit
re-runs without compiling the program; `j3c build` compiles them in.
`j3c run path/to/tool.j3s` runs that file's `pub fn main` once.
`j3c doc lang` has the rules; `j3c doc recipes script` is a small program.

    j3c doc                  tasks, then every stdlib module
    j3c doc recipes          complete programs that check: 2D app, 3D scene,
                             components (.j3ui), canvas art, script, meshgen, material, URP
    j3c doc search texture   modules, names, then comments with a word
    j3c doc j3.game --short  a module's names on one screen
    j3c doc j3.app           public items and their /// comments
    j3c doc j3.app create    one item (`App.create` works too)
    j3c doc j3.gfx.canvas        headless sprite and texture drawing
    j3c doc j3.gfx.urp.raytrace  fullscreen room and sphere renderer
    j3c doc j3.gfx.urp.forest    mesh-aware light and glass composite
    j3c doc lang             language
    j3c doc j3ui             .j3ui components (markup + CSS); j3ui-immediate is the older @j3ui
    j3c doc css              the CSS subset .j3ui supports, and differences from a browser
    j3c doc testing          headless run, screenshot, goldens, imgdiff, HTML snapshot
    j3c doc editor           3D scene editor, scene files, scripts
    j3c doc ship             release builds, zips, .deb with menu entry and icon
    j3c help manifest        j3.toml keys
    j3c fmt --write          rewrite src/
    j3c check --json         one JSON object (ok, or message/path/line)
    j3c check --strict       exit 1 on CSS lint warnings too (`j3c doc css`, Lint)

`j3c check` does not run the game. It fails when a linked library is
missing, or when `tryLoadFont` / `loadFont` / `tryLoadTexture` /
`loadTexture` / `tryLoadSound` / `loadSound` / `tryLoadMusic` /
`loadMusic` / `setVolume` points at a library or file that is not there.
`j3c run` uses that same check and does not launch when it fails.

`run` and `build` with no path use ./j3.toml. They stop if it is
missing. A project run uses the package root as the working directory
and sets J3_ROOT, so assets are read from that directory.

Everything above ships inside j3c. Larger games live in the j3 source
repository (not installed with j3c), under examples/, if you have it:
    examples/fps-shooter    first-person shooter: biome scenes, bosses, HUD, save, perks
    examples/3d-editor-scene smallest editor project: two scenes, a .j3s script
    examples/editor_village scenes placing scenes (houses in a village)
    examples/gallery        UI: widgets, controls, layout, style, inventory
    examples/scripted       .j3s scripts: props, state, shared, instances
    examples/idle-breakout  .j3ui UI + canvas game, goldens (golden.toml), bench
    examples/industry       first-person voxel automation: mine, belts, furnaces
    examples/blocks-adventure  network-first voxel adventure (DESIGN.md)
    examples/arcane-bash    3D scene game with editor scenes and tools
    examples/mini-voxel     small voxel world (PERF.md)
    tools/bench2d           2D draw benchmark and golden (README.md)

Programmatic art: `j3.gfx.canvas.Canvas.create(arena, w, h, Mode.Pixel)`
or `Mode.Smooth`, then draw shapes, gradients, or noise. `savePng` exports
without SDL. In a game, `j3.game.textureFromCanvas` uploads once;
`updateTextureFromCanvas` updates a changed rectangle. Keep the arena
alive while drawing; destroy uploaded textures when done.

`j3c lsp` is the language server. The editor extension spawns it.
Set j3.j3cPath when `j3c` is not on PATH.

3D scenes without the editor window (for agents and scripts):
    j3c -e scene list        objects by name path; add/set/delete/check/stats/diff too
    j3c -e scene ring|wall|stairs|spiral ...  build shapes; ground, ontop, scatter place
    j3c -e render --view iso the scene as the game draws it, to a PNG
    j3c -e render --at 0,2,5 --yaw 90   look from any spot (--views for several, --hide to see in)
    j3c run --frames 120 --screenshot shot.png --headless
    j3c -e gen               run assets/gen/*.j3 mesh generators (j3.gfx.meshgen)
    j3c -e mcp               the same as MCP tools for an agent; with the
                             editor open, edits land there as undo steps
    j3c -e mcp --tools       each MCP tool's arguments and CLI twin

Check your work (`j3c doc recipes check` says what each step catches):
    j3c check  →  j3c -e scene check --all  →  j3c -e render  →
    j3c run --frames 120 --screenshot shot.png --headless

Docs that disagree with the compiler? `j3c version --verbose` prints this
build's docs id (the `j3c doc` header shows it too) and which binary ran.
A different id, or a binary you didn't expect on PATH, means other docs:
rebuild or call the j3c you meant.
