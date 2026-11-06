Shipping a game: release builds, zips, .deb

`j3c build --release` makes the small binary (opt s, no -g, stripped; checks stay on) in
target/<name> (target/<triple>/<name> per [emit] target). Dev builds are for the loop, not
for sending. `j3c size` shows what is in a dev build.

Dev-only code (cheats, debug keys, extra logging): `use j3.build;` then `if build.DEV { ... }`
(or `build.RELEASE`). Both are compile-time consts: `--release` (build or run) gives
RELEASE = true, DEV = false; `j3c run` and plain `j3c build` give the opposite.
`build.VERSION` and `build.NAME` are `[package] version` and `name` from `j3.toml`, so an
about box or title shows the version you ship without a second copy of it.

Startup splash: `[splash] enabled = true` in j3.toml (or `app.splash()`) shows the J3 voxel
logo as soon as the window opens, then your cards, while the `startup` systems run:

    [splash]
    enabled = true
    cards = ["assets/studio.png"]        # after the logo; app.splashCard(path) adds more
    card_time = 1.8                      # seconds per card
    card_size = 0.35                     # fraction of the window a card fits in
    theme = "dark"                       # or "light"
    # logo = false                       # cards only
    # skippable = false                  # ignore key / click / tap

Load in a `startup`, not in `boot` (boot runs before the window exists), and call
`splash.tick()` (`use j3.game.splash;`) between long steps so the logo keeps moving. Only
release builds show it; see it in the dev loop with `j3c run --splash` (works even with
[splash] off, to preview). Skipped under --frames/--screenshot/--headless, J3_CAPTURE, a hot
restart, and J3_NO_SPLASH=1.

App icon: `[dist] icon` is the menu icon in packages and, when it is a PNG, the window icon
of an App. Unset, both are the J3 logo.

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
  [link] libs names, and SDL2_image/mixer/ttf for any SDL game (j3.game loads them at runtime,
  so they are not imports), found via [link] search, its
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
- steam: target/dist/steam/content/<os>/ (the zip's folder plus libsteam_api.so or
  steam_api64.dll from the Steamworks SDK) and scripts/app_build_<appid>.vdf. Keys:
      [steam]
      app_id = "480"
      sdk = "~/steamworks_sdk"          # unzipped SDK root
      depot_linux = "481"
      depot_windows = "482"
      branch = "beta"                   # optional; sets the build live there
  Linux and Windows x86_64 only. j3c never uploads; scripts/README.txt has the
  `steamcmd +login <user> +run_app_build <vdf> +quit` line. Setup, the sniper runtime choice,
  the Deck and store checklists: `j3c doc steam`.
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
    section      .deb Section. Default games (a terminal or editor: x11, utils, ...)
    postinst     .deb maintainer scripts, project-relative; copied into the control
    prerm        archive with mode 0755. A terminal registers itself there:
                 update-alternatives --install /usr/bin/x-terminal-emulator \
                     x-terminal-emulator /usr/bin/<name> 40   (postinst)
                 update-alternatives --remove x-terminal-emulator /usr/bin/<name>  (prerm)

[dist.actions]: right-click entries of the menu icon / dock (`.desktop` Actions), in order.
    [dist.actions]
    new-island = { name = "New Island", exec = "--new-island" }
  The id is letters, digits and -; exec is the arguments the program is run with.
A window class of the app's own (a terminal's --class): App.windowClass(name) before run
(game.setWindowClass); it wins over the launcher's.
Depends are read from the libraries the program loads (`objdump -p`, else [link] libs):
SDL2, SDL2_image, SDL2_mixer, SDL2_ttf, GL, libc. Any other library prints a warning;
add its package to `depends`.

Check a package without installing it: `dpkg-deb --info F.deb`, `dpkg-deb --contents F.deb`,
`desktop-file-validate`. Every key: `j3c help manifest`.
