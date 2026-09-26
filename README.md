# J3

Ahead-of-time game language. `j3c` parses J3, typechecks it, and prints LLVM IR for the host `clang` to link. C is an optional backend (`--emit c`). The compiler is a Rust crate with no crates.io dependencies and no libLLVM.

## Requirements

- Rust (edition 2021) and Cargo
- `clang` on `PATH`
- SDL2 for the example games. Image, mixer, and ttf are vendored under `examples/dungeon/lib` and picked up by rpath. The 3D examples also need OpenGL 3.3 (`libGL`).

## Compiler

From `j3c/`:

```
cargo build --release
cargo test
```

The binary is `j3c/target/release/j3c`. Put it on `PATH`, or call it by that path. The standard library is compiled into that binary. `J3_STDLIB` (the parent of the `j3/` modules) replaces the embedded copy.

## Examples

```
j3c run examples/bounce
j3c check examples/dungeon
j3c build examples/island
```

`run` and `build` with no path use `./j3.toml`. `build` writes `target/<name>` in the package directory and sets the process working directory to the package root (`J3_ROOT`), so assets load from there.

| Example | What it is |
|---------|------------|
| `examples/bounce` | Paddle and ball |
| `examples/dungeon` | Tile crawler: scenes, UI, save, particles, looping music |
| `examples/gallery` | UI gallery: widgets, controls, layout, style, inventory |
| `examples/defense` | Tower defense. Drag the map, scroll to zoom |
| `examples/island` | Voxel island: GL mesh, shadow map, orbit camera |
| `examples/minecraft` | First person. Break, place, jump, corner shadows |
| `examples/leaves` | First person. Pick up leaves, spend them on reach and grab speed |
| `examples/canvas` | Headless procedural sprite and tile PNG generation |
| `examples/canvas_preview` | Preview generated assets and in-memory texture uploads |
| `examples/raytrace` | Interactive glass, metal, and plastic raytracing in a room |
| `examples/ocean` | Procedural ocean waves with glass and metal reflections |
| `examples/forest` | Autumn forest with CC0 tree assets, godrays, glass and metal |

## New project

```
j3c new mygame
cd mygame
j3c check
j3c run
```

`j3c new mygame --app` scaffolds an SDL window. `j3c guide` starts a project. `j3c doc lang` is the language, `j3c doc` is the standard library, and `j3c doc j3ui` is UI component syntax. `j3c help manifest` lists `j3.toml` keys.

## Programmatic graphics

`j3.gfx.canvas` draws into an RGBA image without opening a window. Create a `Canvas` in `Mode.Pixel` for crisp sprites and tiles or `Mode.Smooth` for antialiased shapes. Draw shapes, gradients, noise, and other canvases; then call `savePng` to export or `j3.game.textureFromCanvas` to display it. Upload static art once and use `updateTextureFromCanvas` for a changed rectangle. Run `j3c doc j3.gfx.canvas` for the API and `j3c run examples/canvas` for a headless sample that writes `lantern.png` and `stone.png` into that example directory.

Build art from a readable silhouette, a restrained palette, consistent lighting, and a few deliberate details. The canvas sample uses separate light and dark layers and seeded variation. Check generated assets at their intended display size before adding texture noise.

## OpenGL rendering

`j3.gfx` provides the mesh renderer. Optional `j3.gfx.urp.raytrace` adds a fullscreen room and sphere path tracer, with `j3.gfx.urp.forest` for compositing `j3.gfx` meshes with volumetric light and glass/metal spheres. All need `App.useGl()` and OpenGL 3.3. Use `j3c doc j3.gfx.urp.raytrace` or `j3c doc j3.gfx.urp.forest` for the module API; the full pass order and GLSL requirements are in [`j3c/stdlib/j3/gfx/urp/README.md`](j3c/stdlib/j3/gfx/urp/README.md). Samples are `examples/raytrace`, `examples/ocean`, and `examples/forest`.

## Editor

`editors/vscode` is the Cursor / VS Code extension: highlighting, `j3c lsp` (completion, imports, signature help, outline), `j3.toml` keys, and run / build / format. Set `j3.j3cPath` when `j3c` is not on `PATH`.

## Layout

| Path | What |
|------|------|
| `j3c/` | Compiler. Map in `j3c/AGENTS.md` |
| `j3c/stdlib/j3/` | Standard library, written in J3 |
| `speculative/` | Acceptance programs (`01`–`08` compile and run; `09` typechecks `j3.game`; `10_j3ui.j3` typechecks `@j3ui`) |
| `examples/` | Playable games |
| `editors/vscode/` | Editor extension |
| `tools/j3mesh_import.py` | Mesh importer |
| `roadmap.md` | What ships and what is still open |
| `AGENTS.md` | Working notes for this repo |
