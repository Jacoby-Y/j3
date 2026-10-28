j3.toml

[package]
    name     binary name (required). `build` writes target/<name>
    entry    file that contains main. Default src/main.j3
    version  package version (the .deb version). Default 0.1.0
    description  one line for the .deb and the menu entry

[emit]
    backend  llvm (default) or c
    opt      0, 1, 2, 3, s, or z. LLVM default is 2
    incremental  at opt 0, reuse one object per module. Default true.
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
    zip      true: same as build_outputs = ["zip"]
    include  files and folders to ship. Default: whichever of
             assets, scripts, scenes, locales exist
    icon     .png (square 256 or 512 looks best) or .svg for the menu
    title    menu name. Default: the package name, title-cased
    categories  .desktop Categories. Default Game;
    maintainer  Name <email>. Default $DEBFULLNAME/$DEBEMAIL, else git
    depends  extra .deb packages, after the ones found

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
