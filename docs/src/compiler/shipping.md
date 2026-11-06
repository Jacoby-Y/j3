# Shipping a Game

## Release build

```text
j3c build --release
```

This writes the small, stripped binary to `target/<name>` and, if you asked for any, packages to `target/dist/`.

## Packages

List what you want in `j3.toml`:

```toml
[package]
name = "mygame"
version = "0.1.0"
description = "A small arcade game"

[dist]
build_outputs = ["zip", "deb"]
icon = "assets/icon.png"
```

### zip

`target/dist/<name>-<os>-<arch>.zip` holds a `<name>/` folder with the program and everything in `[dist] include` (by default whichever of `assets`, `scripts`, `scenes`, and `locales` exist). Windows builds also get the DLLs the program needs.

### deb

`target/dist/<pkg>_<version>_<arch>.deb`, for Linux targets. It is built with `tar`, so you don't need `dpkg` to make one. It installs:

| Path | What |
|---|---|
| `/opt/<name>/` | The program and its files |
| `/usr/bin/<name>` | A launcher |
| `/usr/share/applications/<name>.desktop` | The menu entry |
| `/usr/share/icons/...` | The icon |

Because `/opt` is read-only, the launcher runs the game from `~/.local/share/<name>`, so relative asset paths still work and saves land in the player's folder. Package dependencies are worked out from the libraries the program loads.

```text
sudo apt install ./target/dist/mygame_0.1.0_amd64.deb
```

### apk

An Android package for phones and emulators. See `j3c doc android`.

### steam

Steam depot folders and a `steamcmd` build script. See [Shipping on Steam](steam.md).

## Assets and the working directory

Games read assets and write saves relative to the working directory. `j3c run` uses the package root, and the packages above arrange the same, so `"assets/player.png"` works everywhere.
