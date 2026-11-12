# Installing J3

`j3c` is one binary: the compiler, the stdlib, the docs, the editor and the MCP server. It compiles through `clang`, and games that open a window load SDL2 (plus SDL2_ttf, SDL2_image, SDL2_mixer) and OpenGL 3.3 at runtime.

After installing, run `j3c doctor`. It tells you what's missing.

| Platform | Status |
|---|---|
| Linux x86-64, arm64 | Main target. Prebuilt binaries. |
| Android (arm64, x86_64) | Build target: `j3c` on Linux builds an APK. |
| Windows | In progress. Cross-builds from Linux; the exe has not been run on Windows yet. |
| macOS | In progress. No prebuilt binary; untested. |

## Linux

### Install script

```sh
curl -fsSL https://raw.githubusercontent.com/Jacoby-Y/j3/main/install.sh | sh
```

It downloads the latest release for your CPU, checks it against `SHA256SUMS`, and puts `j3c` in `~/.local/bin`. Options:

| Variable | Effect |
|---|---|
| `J3_VERSION=0.1.0` | Install that release instead of the latest |
| `J3_INSTALL_DIR=DIR` | Install somewhere other than `~/.local/bin` |
| `J3_TARBALL=FILE` | Install a release tarball you already downloaded (checksum from `J3_SHA256` or a `SHA256SUMS` next to it) |

### Packages

- **Debian / Ubuntu:** the `.deb` from [Releases](https://github.com/Jacoby-Y/j3/releases), then `sudo apt install ./<file>.deb`.
- **Arch:** `yay -S j3c-bin` (AUR).
- **Anything else:** the `j3c-<version>-<arch>-linux.tar.gz` from [Releases](https://github.com/Jacoby-Y/j3/releases); `j3c` is in `j3c/bin/`.

### Dependencies

Debian / Ubuntu:

```sh
sudo apt install clang libsdl2-2.0-0 libsdl2-ttf-2.0-0 libsdl2-image-2.0-0 libsdl2-mixer-2.0-0 libgl1
```

Fedora:

```sh
sudo dnf install clang SDL2 SDL2_ttf SDL2_image SDL2_mixer mesa-libGL
```

Arch:

```sh
sudo pacman -S clang sdl2 sdl2_ttf sdl2_image sdl2_mixer libglvnd
```

HTTPS (`j3.net.tls`, `j3.net.http`) also needs OpenSSL 3 (`libssl3` on Debian/Ubuntu). Console programs that don't open a window only need `clang`.

## From source (any platform with Rust)

Needs Rust (with Cargo, [rustup.rs](https://rustup.rs)) and `clang`. The crate has no dependencies.

```sh
git clone https://github.com/Jacoby-Y/j3
cd j3
./build.sh
```

`build.sh` release-builds `j3c` and installs it, with the `j3c-composer` tool, to `$J3_BIN_DIR`, else `~/.bin` if it exists, else `~/.local/bin`. Without the script: `cargo build --release --manifest-path j3c/Cargo.toml`, and the binary is `j3c/target/release/j3c`.

## Windows

There is no Windows build of `j3c` yet. From Linux you can build a Windows exe of your game:

```sh
j3c build --target x86_64-pc-windows-gnu --release
```

This needs a mingw `clang` and SDL2 import libraries; a missing one shows up as a link error. The exe links but has never been run on Windows, so expect bugs. Networking (`j3.net.*`) and `j3.proc` compile there and return errors. Progress: [`plans/04-windows.md`](plans/04-windows.md).

## macOS

No prebuilt binary. Building from source should work but has not been tried on a real Mac, and the macOS stdlib layer (`sys_macos.j3`) is partial: networking and `j3.proc` return errors. You'd need Xcode's command line tools (`clang`) and SDL2 from Homebrew (`brew install sdl2 sdl2_ttf sdl2_image sdl2_mixer`). Progress: [`plans/05-macos.md`](plans/05-macos.md).

## Android

`j3c` on Linux builds APKs for phones (arm64) and the emulator (x86_64). It needs JDK 17, the Android SDK (build-tools 35, platform 35, platform-tools) and NDK r27+, then SDL2 for Android built once from the repo:

```sh
tools/android/build-sdl.sh
```

Add the targets and `build_outputs = ["apk"]` to `j3.toml`, then `j3c build --release` or `j3c -e android run`. The full setup, the `j3.toml` keys and signing are in `j3c doc android`.

## Editor and agent extras

- **VS Code / Cursor:** the extension in [`editors/vscode`](editors/vscode) (highlighting, `j3c lsp`, Run/Build).
- **MCP server for agents:** `claude mcp add -s user j3 -- j3c -e mcp`.

## Check it

```sh
j3c version
j3c doctor
j3c new hello --app && cd hello && j3c run
```
