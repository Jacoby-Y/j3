j3.toml

[package]
    name     binary name (required). `build` writes target/<name>
    entry    file that contains main. Default src/main.j3
    version  package version (the .deb version). Default 0.1.0
    description  one line for the .deb and the menu entry
    j3       oldest j3c a package builds with, `">=0.1"`; checked when another
             project depends on it (`j3c doc packages`)

[lib]
             (no keys) this project is a package for others: no main, entry is
             src/<name>.j3 (what `use <name>;` loads); `run`/`build` refuse it.
             `j3c new NAME --lib` makes one

[emit]
    backend  llvm (default) or c
    opt      0, 1, 2, 3, s, or z. LLVM default is 2
    incremental  at opt 0 and 1, reuse one object per module. Default true.
             "all" does it at every opt (faster rebuilds, no
             cross-module inlining). --release ignores it
    debug    debug info. Default true, unless [cc] fast is set
    fast     strip bounds-check aborts, same as --fast
    target   one triple, or an array. Empty means the host.
             `run` uses the host triple. `build` emits each one,
             to target/<triple>/<name> when there are several
    cpu      CPU passed to clang, such as native
    clang    clang executable. Default clang
    flags    extra flags for clang. An array of strings

[link]
    libs     library names without -l (SDL2 links as -lSDL2)
    search   library search directories (-L); Windows targets add
             ~/.cache/j3/sdl2-mingw's SDL lib dirs themselves
    files    extra object files or archives
    flags    extra linker flags, such as an rpath

[dist]
    build_outputs  what `build --release` writes to target/dist/, per target:
             zip    <name>-<os>-<arch>.zip, a <name>/ folder with the
                    program, the include list, and (Windows) the DLLs the
                    .exe imports or [link] libs names, found via [link] search
                    or ~/.cache/j3/sdl2-mingw/dll
             deb    <pkg>_<version>_<arch>.deb (Linux targets): the folder
                    in /opt/<name>, a /usr/bin/<name> launcher that runs it
                    from ~/.local/share/<name> (so relative saves work), a
                    menu entry, the icon. Depends come from the libraries
                    the program loads. Needs `tar`, not dpkg
             steam  target/dist/steam/: content/<os>/ (the zip folder plus
                    libsteam_api.so or steam_api64.dll from [steam] sdk) and
                    scripts/app_build_<appid>.vdf with the steamcmd line in
                    README.txt. Linux and Windows x86_64 only; never uploads
    zip      true: same as build_outputs = ["zip"]
    include  files and folders to ship. Default: whichever of
             assets, scripts, scenes, locales exist
    icon     .png (square 256 or 512 looks best) or .svg for the menu, and (PNG
             only) the window icon of an App. Default: the J3 logo
    title    menu name. Default: the package name, title-cased
    categories  .desktop Categories. Default Game;
    maintainer  Name <email>. Default $DEBFULLNAME/$DEBEMAIL, else git
    depends  extra .deb packages, after the ones found

[steam]
    app_id   numeric app id. Required for build_outputs "steam". A dev run
             (`j3c run`) sets SteamAppId and SteamGameId from it
    sdk      unzipped Steamworks SDK root (~ and project-relative ok). Dev runs
             set J3_STEAM_LIB to its libsteam_api.so; "steam" copies the libs
    depot_linux, depot_windows  depot ids; each needed when that OS is built
    branch   set the build live on this branch (not "default"). Default: none
    runtime_libs  folder of .so files copied beside the Linux exe (libs built in
             the sniper SDK container); libssl.so.3 and SDL add-ons load from there first
    glibc_max  highest GLIBC the Linux build may need; warns above it. Default 2.31
    desc     build description. Default "<name> <version>"

[splash]
    enabled  true shows a startup splash in App.run: the J3 logo animates as
             soon as the window opens, while the startup systems load. Default
             false (App.splash() turns it on from code too)
    logo     false leaves the J3 logo out (cards only). Default true
    cards    images shown after the logo, in order, e.g. ["assets/studio.png"]
             (App.splashCard adds more). A missing one is skipped
    card_time  seconds per card, fades included. Default 1.8
    card_size  a card fits in this fraction of the window (0.05 to 1). Default 0.35
    theme    dark or light background. Default dark
    skippable  false ignores the key / click / tap that skips a card. Default true
             Release builds only: `j3c run --splash` shows it in dev. Never shown
             under --frames, --screenshot, --headless, J3_CAPTURE, a hot restart,
             or J3_NO_SPLASH=1 (`j3c doc j3.game.splash`)

[dependencies]
    name = { path = "dir" }   another package (or a folder of .j3 modules), relative to
             this j3.toml; `name = "dir"` is the same. `use name.a.b;` loads
             <dir>/src/a/b.j3 when <dir> has a j3.toml and a src/, else <dir>/a/b.j3;
             its first line must be `module name.a.b;` (what the package's own files
             say). The dep's [package] name must equal the key; its [dependencies]
             are used too (relative to it). Only imported modules are compiled, for
             this package's target: not the dep's main, [link], or anything else in
             its j3.toml. Errors point at the dep's file:line. Manifest errors:
             missing path, a cycle, a name that is `j3`, this package, or a module of
             src/, one name with two paths. A dep's assets are not found for you.
             Example: android/j3.toml with `jzero = { path = ".." }`, then
             `use jzero.remote.proto;`. `use name;` alone loads <root>/name.j3
    name = { git = "URL", tag = "v1" }   (or rev = "...", branch = "...", or
             neither for the default branch): cloned with the system git into
             ~/.cache/j3/pkgs (J3_PKG_DIR), then the same as a path dep. j3.lock
             beside j3.toml pins every git dep to a commit; commit it. A local
             path URL is relative to this j3.toml. `j3c fetch`, `j3c update [name]`,
             `j3c add NAME --path DIR | --git URL [--tag T]`. `j3c doc packages`

[ui]
    dir      folder of .j3ui components. Default ui. All of them become
             one module, <package>.ui. `j3c dump-ui FILE` prints the J3
    theme    one global CSS file, read at compile time (scope 0)
    lint     false turns the `j3c check` CSS lint off; lint-off = ["rule", ...]
             skips some (`j3c doc css`, Lint)

[i18n]
    dir      folder of <locale>.txt string tables. Default locales
    default  locale shown first, and the one `j3c check` compares keys
             against. Default en
    fallback locale tried when a key is missing. Default en
             (`j3c doc i18n`; shipped in dist zips and .debs when the folder exists)

Build bookkeeping (stamps, per-module objects, the `run` dev binary)
lives in target/.j3c/. Deleting it only costs a full rebuild.

[cc]
    std      C standard. Default c11. Used when the backend is C
    opt      optimization. [emit] opt inherits this when [emit] is omitted
    fast     strip bounds-check aborts

CLI flags override the file: --emit llvm|c, --opt N, --fast, --release (opt s, no -g, stripped; `j3c size` shows what is in the binary; also writes [dist] build_outputs, `j3c doc ship`).
No [emit] section inherits [cc] opt.

Environment
    J3_STDLIB   stdlib directory (the parent of j3/). Unset means the
                copy compiled into this j3c binary
    J3_EDITOR_SRC  editor frontend `src/` directory for `j3c editor`, instead of the
                copy compiled into this j3c binary
    J3_ROOT     set by `run` to the package root. The process cwd is
                that directory too
    J3C_COLOR   set to color rustc-shaped diagnostics
    J3C_TIMINGS set to print how long each build phase took, to stderr
    J3_FRAMES, J3_SCREENSHOT  set by `run --frames` / `--screenshot`; an App
                stops after that many frames and saves the last one
    J3_CAPTURE  out.png:N saves frame N (1-based), then quits (goldens)
    J3_FIXED_DT seconds per frame instead of measured time, so captures repeat
    SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy
                no window or sound: what `run --headless` sets. Set both on the
                same command when running a built binary. `j3c doc testing`
