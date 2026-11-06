# The j3c Command

```text
j3c <command> [file.j3 | project]
```

A path can be a `.j3` file, a `j3.toml`, or a directory containing one. With no path, `j3c` uses `./j3.toml`.

## Projects

| Command | Does |
|---|---|
| `j3c new <name>` | New console project (`--app` for a window, `--template fps` or `--template ui`) |
| `j3c guide` | How to start a project |
| `j3c check` | Type-check without building. `--json` prints one JSON object. |
| `j3c run` | Build and run. `--file f.j3` runs one file inside a project. |
| `j3c run tool.j3s [args]` | Run a script's `pub fn main` once |
| `j3c build` | Build `target/<name>` |
| `j3c size` | What is inside the binary: sections, code per module, biggest functions |

## Build flags

These work with `run` and `build`, and override [`j3.toml`](manifest.md).

| Flag | Effect |
|---|---|
| `--release` | Smallest binary: opt `s`, no debug info, stripped. Checks stay on. `build` also writes [packages](shipping.md). |
| `--fast` | Removes bounds-check aborts and debug info |
| `--opt N` | `0`, `1`, `2`, `3`, `s`, or `z` |
| `--emit llvm\|c` | Pick the backend |

## Running windowed programs headless

| Flag | Effect |
|---|---|
| `--headless` | No window and no sound (SDL offscreen) |
| `--frames N` | Stop after `N` frames |
| `--screenshot f.png` | Save the last frame |
| `--hot` | Rebuild and restart when a source file changes |

See [Testing Headless](testing.md).

## Code tools

| Command | Does |
|---|---|
| `j3c fmt [path]` | Print formatted source. `--write` rewrites files, `--check` fails if any would change. |
| `j3c lsp` | Language server over stdio |
| `j3c doc ...` | Stdlib docs, guides, and recipes (see below) |

## Documentation

| Command | Shows |
|---|---|
| `j3c doc` | Tasks and the list of stdlib modules |
| `j3c doc <module>` | A module's public items (`--short` for one screen) |
| `j3c doc search <word>` | Public names containing a word |
| `j3c doc lang` | Language summary |
| `j3c doc recipes [name]` | Complete example programs |
| `j3c doc j3ui`, `css`, `testing`, `ship`, `threads`, `android` | Guides |
| `j3c help manifest` | Every `j3.toml` key |

## Looking inside

| Command | Prints |
|---|---|
| `j3c dump-ir` | The checked IR |
| `j3c dump-llvm` | LLVM IR |
| `j3c dump-c` | C source |
| `j3c dump-ui <file.j3ui>` | J3 generated from a UI component |
| `j3c dump-bc <file.j3s>` | Script bytecode |
| `j3c dump-syntax` | A TextMate grammar for editors |

## Editor and assets

| Command | Does |
|---|---|
| `j3c editor [project]` | Open the 3D scene editor |
| `j3c -e scene <op>` | List, get, add, set, delete, or check scene objects |
| `j3c -e render` | Draw a scene to a PNG without a window |
| `j3c -e gen` | Run mesh generators in `assets/gen/` that changed |
| `j3c -e mcp` | MCP server, so AI agents can edit and render scenes |
| `j3c golden <dir>` | Headless golden-image tests |
| `j3c -e imgdiff a.png b.png` | Pixel diff two images |

## Version

`j3c version --verbose` prints the build, the stdlib source, and the binary path, which helps spot a stale `j3c` on your `PATH`.
