# Getting Started

## Installing

You need Rust (with Cargo) and `clang` on your `PATH`. Games that open a window also need SDL2, and 3D ones need OpenGL 3.3.

From the repository root:

```text
cd j3c
cargo build --release
```

The binary is `j3c/target/release/j3c`. Put it on your `PATH` (the repo's `build.sh` builds and installs it to `$J3_BIN_DIR`, else `~/.bin` if that exists, else `~/.local/bin`). Check it with:

```text
j3c version
```

## Your first project

```text
j3c new mygame
cd mygame
j3c check
j3c run
```

`j3c new` writes a `j3.toml` manifest and `src/main.j3`. There are a few starting points:

| Command | What you get |
|---|---|
| `j3c new mygame` | A console "hello" program |
| `j3c new mygame --app` | An SDL window with a game loop |
| `j3c new mygame --template fps` | A first-person 3D game with an editor scene |
| `j3c new mygame --template ui` | A window built from `.j3ui` components |

## The everyday loop

| Command | Does |
|---|---|
| `j3c check` | Parses and type-checks. Fast; produces nothing. |
| `j3c run` | Compiles and runs the project. |
| `j3c build` | Writes the binary to `target/<name>`. |
| `j3c fmt --write` | Formats every `.j3` and `.j3s` file. |

With no path these use `./j3.toml`. You can also point them at a directory, a `j3.toml`, or a single `.j3` file.

## A window

Here is the shape of a small game loop using `j3.game` directly:

```rust
module game_loop;

use j3.game;
use j3.math.Vec2;

fn main() {
    let pos = Vec2 { x: 40, y: 40 };
    let running = true;

    while running {
        let dt = game.poll();        // pump input, get the frame time
        if game.quitRequested() || game.keyDown(game.Key.Escape) {
            running = false;
        }
        pos.x += 60 * dt;

        game.clear();
        game.drawRect(pos, Vec2 { x: 8, y: 8 });
        game.present();
    }
}
```

Bigger games usually use `j3.app`, a runner with startup, update, and draw stages. `j3c doc recipes app2d` prints a complete one.

## Editor support

`editors/vscode/` is a VS Code and Cursor extension: syntax highlighting, completion and diagnostics through `j3c lsp`, and Run/Build/Format commands. Set `j3.j3cPath` if `j3c` is not on your `PATH`.
