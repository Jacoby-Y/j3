//! Text for `j3c guide` and `j3c help manifest`. The editor schema in
//! `editors/vscode/toml.js` matches these keys.

pub const GUIDE: &str = "\
j3c guide — start a J3 project

The compiler is `j3c` on PATH. A new directory does not contain the
language. The standard library is inside the `j3c` binary. `$J3_STDLIB`
replaces that copy with a directory when it is set.

    j3c new mygame           hello: j3.toml + src/main.j3 + AGENTS.md
    j3c new mygame --app     SDL window (links SDL2)
    cd mygame
    j3c check                typecheck, then missing libraries and loaded files
    j3c run
    j3c build                writes target/<name>

`j3c new` with no name writes into the current directory. The module
name comes from that directory. `my-game` and `My Game` become `my_game`.

Layout
    j3.toml                  [package] name, entry (default src/main.j3)
    src/main.j3              module <name>; and fn main()
    src/other.j3             use <name>.other.Name
    AGENTS.md                how to work in this project

One file is one module. `use j3.io.printLn` brings in one name.
`use j3.memory.{ Arena, List }` brings in several.

    j3c doc                  list stdlib modules
    j3c doc j3.app           public items and their /// comments
    j3c doc j3.app create    one item
    j3c doc j3.gfx.canvas        headless sprite and texture drawing
    j3c doc j3.gfx.urp.raytrace  fullscreen room and sphere renderer
    j3c doc j3.gfx.urp.forest    mesh-aware light and glass composite
    j3c doc lang             language
    j3c doc j3ui             UI components
    j3c doc editor           3D scene editor, scene files, behaviors
    j3c help manifest        j3.toml keys
    j3c fmt --write          rewrite src/
    j3c check --json         one JSON object (ok, or message/path/line)

`j3c check` does not run the game. It fails when a linked library is
missing, or when `tryLoadFont` / `loadFont` / `tryLoadTexture` /
`loadTexture` / `tryLoadSound` / `loadSound` / `tryLoadMusic` /
`loadMusic` / `setVolume` points at a library or file that is not there.
`j3c run` uses that same check and does not launch when it fails.

`run` and `build` with no path use ./j3.toml. They stop if it is
missing. A project run uses the package root as the working directory
and sets J3_ROOT, so assets are read from that directory.

Larger games live in the compiler repo, not in the new project:
    examples/bounce         2D App, paddle and ball
    examples/dungeon        tiles, UI, save, audio
    examples/island         GL mesh and a shadow map
    examples/minecraft      first person, break and place
    examples/leaves         3D yard, click, upgrades
    examples/defense        tower defense, waves and HUD
    examples/gallery        UI: widgets, controls, layout, style, inventory
    examples/canvas         procedural sprite and tile PNGs
    examples/canvas_preview SDL preview of generated PNGs and live pixels
    examples/raytrace       OpenGL room raytracer
    examples/ocean          custom-shader wave scene
    examples/forest         mesh, canopy shadows, and volumetric light

Programmatic art: `j3.gfx.canvas.Canvas.create(arena, w, h, Mode.Pixel)`
or `Mode.Smooth`, then draw shapes, gradients, or noise. `savePng` exports
without SDL. In a game, `j3.game.textureFromCanvas` uploads once;
`updateTextureFromCanvas` updates a changed rectangle. Keep the arena
alive while drawing; destroy uploaded textures when done.

`j3c lsp` is the language server. The editor extension spawns it.
Set j3.j3cPath when `j3c` is not on PATH.

3D scenes without the editor window (for agents and scripts):
    j3c -e scene list        objects by name path; add/set/delete/check too
    j3c -e scene ground Box  place objects: ground, ontop, scatter
    j3c -e render --view iso the scene as the game draws it, to a PNG
    j3c run --frames 120 --screenshot shot.png --headless
    j3c -e gen               run assets/gen/*.j3 mesh generators (j3.gfx.meshgen)
    j3c -e mcp               the same as MCP tools for an agent; with the
                             editor open, edits land there as undo steps
";

pub const MANIFEST: &str = "\
j3.toml

[package]
    name     binary name (required). `build` writes target/<name>
    entry    file that contains main. Default src/main.j3

[emit]
    backend  llvm (default) or c
    opt      0, 1, 2, 3, s, or z. LLVM default is 2
    debug    debug info. Default true, unless [cc] fast is set
    fast     strip bounds-check aborts, same as --fast
    target   one triple, or an array. Empty means the host.
             `run` uses the host triple. `build` emits each one
    cpu      CPU passed to clang, such as native
    clang    clang executable. Default clang
    flags    extra flags for clang. An array of strings

[link]
    libs     library names without -l (SDL2 links as -lSDL2)
    search   library search directories (-L)
    files    extra object files or archives
    flags    extra linker flags, such as an rpath

[cc]
    std      C standard. Default c11. Used when the backend is C
    opt      optimization. [emit] opt inherits this when [emit] is omitted
    fast     strip bounds-check aborts

CLI flags override the file: --emit llvm|c, --opt N, --fast.
No [emit] section inherits [cc] opt.

Environment
    J3_STDLIB   stdlib directory (the parent of j3/). Unset means the
                copy compiled into this j3c binary
    J3_EDITOR_SRC  editor frontend `src/` directory for `j3c editor`, instead of the
                copy compiled into this j3c binary
    J3_ROOT     set by `run` to the package root. The process cwd is
                that directory too
    J3C_COLOR   set to color rustc-shaped diagnostics
";
