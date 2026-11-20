# Installing J3

`j3c` is one binary: the compiler, the stdlib, the docs, the editor and the MCP server. It compiles through `clang`, and games that open a window load SDL2 (plus SDL2_ttf, SDL2_image, SDL2_mixer) and OpenGL 3.3 at runtime.

After installing, run `j3c doctor`. It tells you what's missing.

| Platform | Status |
|---|---|
| Linux x86-64, arm64 | Main target. Prebuilt binaries. |
| Android (arm64, x86_64) | Build target: `j3c` on Linux builds an APK. |
| Windows 10 1903+ / 11 x64 | Prebuilt `j3c.exe`, cross-built on Linux and tried under wine; not yet run on real hardware. |
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

## From source

Needs `clang` and a seed `j3c`: an installed one (see above), else Rust (with Cargo, [rustup.rs](https://rustup.rs)) once, to build the old Rust compiler as the seed.

```sh
git clone https://github.com/Jacoby-Y/j3
cd j3
./build.sh   # with no j3c installed: J3_SEED=$(j3c-bootstrap/tools/stage0.sh rust-final) ./build.sh
```

`j3c` is written in J3 (`j3c-bootstrap/`). `build.sh` builds `j3c` with a seed (the installed `j3c`, a pinned release, or `J3_SEED`; `j3c-bootstrap/tools/stage0.sh rust-final` builds the last Rust `j3c` from the git tag `rust-final` with Cargo into `~/.cache/j3/stage0/` and prints its path), has that build itself, and installs the result, with the `j3c-composer` tool, to `$J3_BIN_DIR`, else `~/.bin` if it exists, else `~/.local/bin`.

## Windows

### Install script

In PowerShell (no admin rights needed):

```powershell
irm https://github.com/Jacoby-Y/j3/releases/latest/download/install.ps1 | iex
```

It downloads the latest `j3c-<version>-x86_64-windows.zip`, checks it against the release's `SHA256SUMS`, unpacks it to `%LOCALAPPDATA%\Programs\j3c`, adds that folder to your user `PATH` if it is not there, and runs `j3c setup`. Then open a new terminal:

```powershell
j3c new hello --app
cd hello
j3c run
```

A given release, or another folder: `& ([scriptblock]::Create((irm https://github.com/Jacoby-Y/j3/releases/latest/download/install.ps1))) -Version 0.1.1 -InstallDir D:\tools\j3c` (or set `J3_VERSION` / `J3_INSTALL_DIR` before the one-liner).

### Scoop

```powershell
scoop install https://raw.githubusercontent.com/Jacoby-Y/j3/main/packaging/scoop/j3c.json
```

Scoop runs `j3c setup` after installing.

### By hand

Download `j3c-<version>-x86_64-windows.zip` from [Releases](https://github.com/Jacoby-Y/j3/releases) (check it: `Get-FileHash j3c-*.zip` against `SHA256SUMS`), unzip it anywhere, put the `j3c` folder on your `PATH`, then run `j3c setup`.

### SmartScreen

`j3c.exe` is not signed yet, so the first run may show "Windows protected your PC". Click **More info**, then **Run anyway**. Defender can also make the first builds with a freshly downloaded `clang.exe` slow.

### What `j3c setup` downloads

Into `%LOCALAPPDATA%\j3` (or `J3_CACHE`), each archive checked against a sha256 pinned in `j3c` before it is unpacked. Run it again any time: with everything present it prints `up to date`.

| Folder | What | Size on disk |
|---|---|---|
| `toolchain\llvm-mingw-<ver>\` | llvm-mingw: `clang`, `lld`, `llvm-windres` and the x86_64 mingw sysroot (other architectures are deleted after unpacking) | about 530 MB |
| `sdl2-mingw\` | SDL2, SDL2_image, SDL2_ttf, SDL2_mixer mingw kits, plus `dll\` (what runs and `dist` copy beside your game) | about 80 MB |

`j3c` finds them with no `PATH` changes. `j3c doctor` shows which clang it picked (`J3_CLANG`, then the llvm-mingw from `setup`, then `clang` on `PATH`) and what is missing. `j3c add` of a git dependency needs [Git for Windows](https://git-scm.com/download/win).

Not on a Windows host yet: parallel builds and their cache, package tools (`j3c pkg`, `[dist_outputs]` from packages), and the LSP, MCP server and editor (stubs on every host for now).

### Uninstall

Delete `%LOCALAPPDATA%\Programs\j3c` (the exe) and `%LOCALAPPDATA%\j3` (toolchain, SDL kit, caches), and remove the `PATH` entry (Settings, "Edit environment variables for your account"). Scoop: `scoop uninstall j3c`, then delete `%LOCALAPPDATA%\j3`.

### Windows games from Linux

From Linux you can build a Windows exe of your game:

```sh
j3c setup   # once: the SDL2 mingw kit
j3c build --target x86_64-pc-windows-gnu --release   # writes target/<name>.exe; no [emit] target needed
```

This needs a mingw `clang` (and `x86_64-w64-mingw32-windres` for the icon and manifest). Networking (`j3.net.*`) compiles there and returns errors. Progress: [`plans/02-windows.md`](plans/02-windows.md), [`plans/08-windows-j3c.md`](plans/08-windows-j3c.md).

## macOS

No prebuilt binary. Building from source should work but has not been tried on a real Mac, and the macOS stdlib layer (`sys_macos.j3`) is partial: networking and `j3.proc` return errors. You'd need Xcode's command line tools (`clang`) and SDL2 from Homebrew (`brew install sdl2 sdl2_ttf sdl2_image sdl2_mixer`). Progress: [`plans/03-macos.md`](plans/03-macos.md).

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
