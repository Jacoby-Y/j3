Shipping a game: release builds, zips, .deb

`j3c build --release` makes the small binary (opt s, no -g, stripped; checks stay on) in
target/<name> (target/<triple>/<name> per [emit] target). Dev builds are for the loop, not
for sending. `j3c size` shows what is in a dev build.

Dev-only code (cheats, debug keys, extra logging): `use j3.build;` then `if build.DEV { ... }`
(or `build.RELEASE`). Both are compile-time consts: `--release` (build or run) gives
RELEASE = true, DEV = false; `j3c run` and plain `j3c build` give the opposite.

Packages: list them in j3.toml and every release build writes them to target/dist/:

    [package]
    name = "mygame"
    version = "0.1.0"                    # default 0.1.0
    description = "A small arcade game"  # one line; default "<name>, made with j3"

    [dist]
    build_outputs = ["zip", "deb"]
    icon = "assets/icon.png"             # .png (square 256 or 512) or .svg

- zip: target/dist/<name>-<os>-<arch>.zip, one per target. A <name>/ folder holds the
  program, the include list, and for Windows targets the DLLs the .exe imports plus those
  [link] libs names (SDL2_ttf/image/mixer load at runtime), found via [link] search, its
  ../bin, or ~/.cache/j3/sdl2-mingw/dll. A Windows target links against the SDL libs in
  ~/.cache/j3/sdl2-mingw on its own. Needs `zip`.
  Smoke test a Windows build without a window: a program that loads assets and prints runs
  under `wine <name>.exe`. Windowed frames cannot be captured there: the Windows SDL
  offscreen driver has no renderer or GL, so SDL_VIDEODRIVER=offscreen exits (or hangs).
- deb: target/dist/<pkg>_<version>_<arch>.deb, Linux targets only (others print a skip).
  Built with `tar` and no dpkg, so any Linux or macOS host makes one. `<pkg>` is the name
  lowercased with _ as -. Installs:
      /opt/<name>/                         the zip's folder
      /usr/bin/<name>                      launcher (below)
      /usr/share/applications/<name>.desktop   menu entry
      /usr/share/icons/hicolor/<N>x<N>/apps/<name>.png and /usr/share/pixmaps/<name>.png
  Install with `sudo apt install ./mygame_0.1.0_amd64.deb`; remove with `sudo apt remove mygame`.
- apk: an Android package, debug-signed, for phones and emulators. See `j3c doc android`.
- `zip = true` (older projects) is the same as build_outputs = ["zip"].

Working directory: games read assets and write saves relative to it (`j3c run` uses the
package root). /opt is read-only, so the launcher makes ~/.local/share/<name>
($XDG_DATA_HOME), links every installed file into it, and runs the program from there:
relative reads find the data and saves land in the player's folder. It also sets the SDL
window class, so the taskbar groups the window under the menu icon.

More [dist] keys:
    include      files and folders shipped beside the program. Default: whichever of
                 assets, scripts, scenes, locales exist
    title        menu name. Default: the package name title-cased (my_game -> My Game)
    categories   .desktop Categories. Default Game;
    maintainer   Name <email>. Default $DEBFULLNAME/$DEBEMAIL, else git config
    depends      extra Debian packages
Depends are read from the libraries the program loads (`objdump -p`, else [link] libs):
SDL2, SDL2_image, SDL2_mixer, SDL2_ttf, GL, libc. Any other library prints a warning;
add its package to `depends`.

Check a package without installing it: `dpkg-deb --info F.deb`, `dpkg-deb --contents F.deb`,
`desktop-file-validate`. Every key: `j3c help manifest`.
