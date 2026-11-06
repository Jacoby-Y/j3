# Project Manifest (j3.toml)

Every project has a `j3.toml` at its root. Only `[package] name` is required.

```toml
[package]
name = "mygame"
entry = "src/main.j3"
version = "0.1.0"
description = "A small arcade game"

[emit]
opt = 2

[link]
libs = ["SDL2"]

[dist]
build_outputs = ["zip", "deb"]
icon = "assets/icon.png"
```

## `[package]`

| Key | Meaning | Default |
|---|---|---|
| `name` | Binary name; `build` writes `target/<name>` | required |
| `entry` | File containing `main` | `src/main.j3` |
| `version` | Package version (used for `.deb`) | `0.1.0` |
| `description` | One line for packages and the menu entry | |

## `[emit]`

| Key | Meaning | Default |
|---|---|---|
| `backend` | `llvm` or `c` | `llvm` |
| `opt` | `0`, `1`, `2`, `3`, `s`, `z` | `2` |
| `incremental` | Reuse one object file per module at opt 0 and 1 (opt 2 and up use ThinLTO objects on Linux with lld or mold). `"all"` does it at every level. | `true` |
| `debug` | Debug info | `true` |
| `fast` | Strip bounds-check aborts, like `--fast` | `false` |
| `target` | A target triple or a list of them; empty is the host | host |
| `cpu` | CPU for clang, such as `native` | |
| `clang` | clang executable | `clang` |
| `flags` | Extra clang flags | |

## `[link]`

| Key | Meaning |
|---|---|
| `libs` | Libraries without `-l` (`"SDL2"` links `-lSDL2`) |
| `search` | Library search directories (`-L`) |
| `files` | Extra object files or archives |
| `flags` | Extra linker flags, such as an rpath |

## `[dist]`

What `j3c build --release` packages into `target/dist/`. See [Shipping a Game](shipping.md).

| Key | Meaning |
|---|---|
| `build_outputs` | Any of `"zip"`, `"deb"`, `"apk"` |
| `include` | Files and folders to ship. Default: whichever of `assets`, `scripts`, `scenes`, `locales` exist |
| `icon` | `.png` (square 256 or 512) or `.svg` |
| `title` | Menu name. Default: the name, title-cased |
| `categories` | `.desktop` categories. Default `Game;` |
| `maintainer` | `Name <email>` |
| `depends` | Extra Debian packages |

## `[dependencies]`

Path dependencies: `name = { path = "../dir" }` (or `name = "../dir"`), relative to the `j3.toml` that declares it. If `<dir>` is a package (a `j3.toml` and a `src/`), `use name.a.b;` loads `<dir>/src/a/b.j3`; otherwise `<dir>/a/b.j3`. The file's first line is `module name.a.b;`, the same rule project files follow (`module <package>.a.b;`), so a package's own files work unchanged. A dep's `[package] name` must equal the key and its own `[dependencies]` are used too. Only the modules you import (and what they import) are compiled, for your package's target; the dep's `main`, `[link]` and other settings are not used. Deps are compiled in, so there is nothing to ship; their assets are not located for you. Type errors point at the dep's own `file:line`; manifest errors name the `j3.toml` line: missing folder, a cycle, a key that is `j3`, the package name or a module of `src/`, one name with two paths. Editing a dep file rebuilds what uses it.

```toml
# android/j3.toml: the phone app shares the desktop app's wire protocol
[dependencies]
jzero = { path = ".." }
```

```j3
use jzero.remote.proto.{ Frame, encode, decode };
```

## `[ui]`

| Key | Meaning | Default |
|---|---|---|
| `dir` | Folder of `.j3ui` components; they become the module `<package>.ui` | `ui` |
| `theme` | A global CSS file | |
| `lint` | `false` turns off the CSS lint in `j3c check` | `true` |

## `[i18n]`

| Key | Meaning | Default |
|---|---|---|
| `dir` | Folder of `<locale>.txt` string tables | `locales` |
| `default` | Locale shown first, and the one `check` compares against | `en` |
| `fallback` | Locale used when a key is missing | `en` |

## Environment variables

| Variable | Effect |
|---|---|
| `J3_STDLIB` | Use the stdlib at this path (the parent of `j3/`) instead of the built-in copy |
| `J3_EDITOR_SRC` | Build the editor from this source directory |

Build bookkeeping lives in `target/.j3c/`. Deleting it only costs a full rebuild.
