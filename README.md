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

The binary is `j3c/target/release/j3c`. Put it on `PATH`, or call it by that path. The standard library ships next to the compiler in `j3c/stdlib/j3/`. Override it with `J3_STDLIB` (the parent of the `j3/` modules).

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
| `examples/gallery` | UI gallery |
| `examples/defense` | Tower defense. Drag the map, scroll to zoom |
| `examples/island` | Voxel island: GL mesh, shadow map, orbit camera |
| `examples/minecraft` | First person. Break, place, jump, corner shadows |
| `examples/leaves` | First person. Pick up leaves, spend them on reach and grab speed |

## New project

```
j3c new mygame
cd mygame
j3c check
j3c run
```

`j3c new mygame --app` scaffolds an SDL window. `j3c guide` and `j3c doc` describe the layout and the standard library. `j3c help manifest` lists `j3.toml` keys.

## Editor

`editors/vscode` is the Cursor / VS Code extension: highlighting, `j3c lsp` (completion, imports, signature help, outline), `j3.toml` keys, and run / build / format. Set `j3.j3cPath` when `j3c` is not on `PATH`.

## Layout

| Path | What |
|------|------|
| `j3c/` | Compiler. Map in `j3c/AGENTS.md` |
| `j3c/stdlib/j3/` | Standard library, written in J3 |
| `speculative/` | Acceptance programs (`01`–`08` compile; `09` typechecks `j3.game`) |
| `examples/` | Playable games |
| `editors/vscode/` | Editor extension |
| `tools/j3mesh_import.py` | Mesh importer |
| `roadmap.md` | What ships and what is still open |
| `AGENTS.md` | Working notes for this repo |
