# Plan 08: Ship `j3c.exe`: the compiler on a Windows host, easy to install

**Status (2026-11-20): chunks 1–9 landed, unverified on GitHub and on hardware.** `j3c.exe` cross-builds
(`tools/release/package.sh --windows` → `dist/j3c-<ver>-x86_64-windows.zip` + `SHA256SUMS`) and, under wine, runs
`version` (same string as the Linux build), `doc`, `new`, `check`, `fmt`. `j3c setup`, `install.ps1`, Scoop, the
release and CI workflows are written. Not yet seen: a release run (tag push), the `windows` CI job green, winget
(skeleton only, submit after the second Windows release), and chunk 10 (real hardware). Still off on a Windows host:
the fork-based paths (`cache/jobs` parallel builds, `pkgtool`, dist package tools). This plan took over plan 02
chunks 12–14.

## Goal

A Windows 10 (1903+) / 11 x64 user, with nothing installed, runs one command and then:

```powershell
irm https://github.com/Jacoby-Y/j3/releases/latest/download/install.ps1 | iex   # or: scoop install j3c
j3c new hello --app
cd hello
j3c run --headless --frames 30 --screenshot s.png
```

works. That means:

1. Releases carry `j3c-<ver>-x86_64-windows.zip` (one `j3c.exe` plus licenses and README) in `SHA256SUMS`, cross-built on
   the Linux release runner.
2. `j3c setup` downloads a pinned llvm-mingw and the SDL2 mingw kit into `%LOCALAPPDATA%\j3\`, checks their hashes, and
   `j3c` finds them with no PATH edits. `j3c doctor` says what is missing and the one command that fixes it.
3. `check`, `build`, `run` (console and SDL apps), `--headless`, `doc`, `fmt`, `new`, `add`/`fetch` (git deps), `golden`,
   `size` and `[dist] build_outputs = ["zip"]` work on the Windows host.
4. CI: a Linux job builds the exe and smoke-tests it under wine, and a `windows-latest` job installs the zip the way a user
   would and builds and runs a console program and an SDL app.

Out of scope: the LSP server, MCP server and `j3c editor`, which are stubs on every host until
[plan 06](06-deferred-tools.md). Once ported they pick up this plan's path fixes. Also out: `j3c-composer` (`package.sh`
doesn't ship it on Linux either), Windows on arm64, an MSI/Inno installer, and Authenticode signing (see Later).

## Current state

Checked by reading and grepping `j3c-bootstrap/src` and `j3c/stdlib` on 2026-11-19, with the uncommitted
`plan11-port` work in the tree. Nothing in this section was run on Windows or under wine. "Read" means grep or reading only.

### What already exists

- **Cross builds of games**: `[emit] target = "x86_64-pc-windows-gnu"` → clang `--target`, mingw link flags
  (`toolchain/link.j3`: `--disable-dynamicbase`, the new `[emit] windows_subsystem = "gui"` → `--subsystem,windows`, and
  `[dist] icon` PNG → `.ico` → `windres` → linked `.icon.o` via `toolchain/winres.j3`). `windowsLib` maps `-lGL` to
  `opengl32` (`toolchain/target.j3:187`). Plan 02 "Current state" covers the wine runs. Those were done with the Rust j3c,
  and the J3 port copies its logic.
- **Stdlib on Windows** (uncommitted diff): `j3.proc` on `CreateProcessW` with `waitAny` (`proc_windows.j3:194`, built on
  `winsys.waitAnyOf`), `j3.io` fs plus `path` absolute via `GetFullPathNameA` (`io/path.j3:194`), Windows `dir`/`fs`
  parts. j3c's own parallel clang jobs need `proc.waitAny`, so this is the piece that makes a Windows `j3c` possible at all.
- **Release pipeline**: `tools/release/package.sh` makes the Linux tarball, `.deb` and `SHA256SUMS`. `release.yml` runs
  it on `ubuntu-22.04` and `ubuntu-22.04-arm`, and `install.sh` unpacks the tarball. Nothing for Windows.
- `j3c size` already appends `.exe` (`cmd/size.j3:103`), but only there.

### What breaks on a Windows host (Read)

1. **Host triple and datalayout.** `backend/llvm.j3:92` `tripleOf` returns `x86_64-pc-linux-gnu` when `[emit] target`
   is empty. `:447` always writes the x86-64 ELF datalayout. A j3c.exe has to pick `sys_windows.j3` for an empty
   target on its own host while the IR says Linux. Check what `target.j3:19` (`TargetOs` from the triple) does with `""`
   today. This is the same bug
   [plan 03](03-macos.md) describes, still unfixed in the port.
2. **No `--target` flag.** Nothing in `j3c-bootstrap/src/driver` or `cmd/build.j3` reads one, so cross-building j3c itself
   means editing `j3c-bootstrap/j3.toml`.
3. **`.exe` paths.** `Manifest.outPathFor` (`project/manifest.j3:53`) returns `target/<bin>`. mingw writes
   `target/<bin>.exe`, so `run`, `hot`, `golden`, `dist` and the editor launcher look for a file that isn't there.
4. **`HOME` everywhere.** cmd/PowerShell don't set it. The sites: `dist.j3:265`, `project/config.j3:61,67`,
   `project/deps/git.j3:13` (package cache `~/.cache/j3/pkgs`), `project/manifest/parse.j3:192` (`~` expansion),
   `toolchain/target.j3:157` (`mingwSdlLibs`, the SDL kit), `toolchain/android.j3:62,70,177`, `dist/apk.j3:92,306`.
   `project/config.j3` already has a branch, so check what it does with `APPDATA`.
5. **Embedded stdlib keys.** `Loader.embedded` (`load/stdlib.j3`) strips `<stdlib>/j3/` from a loader path and looks the
   rest up in `stdIndex`, whose keys are the `@embedDir` names with `/`. If `path.join` on Windows writes `\`, every nested
   module (`gfx/canvas.j3`) misses and falls back to disk, and on a user's machine there is no disk copy. This was confirmed
   in the Rust j3c under wine (plan 02). Unknown for the port: check what `j3.io.path.join` emits on Windows.
6. **`--headless`** sets `SDL_VIDEODRIVER=offscreen` (`cmd/run.j3:103`, `cmd/golden.j3:373`, `scene/ops.j3:424`). The
   Windows SDL2 offscreen driver has no GL, so headless frames and screenshots can't work. Plan 02 decision 10 has the fix.
7. **Host tools Windows lacks.** `zip` (8 spawn sites: `dist.j3`, `dist/apk.j3`), `sh` (2: `cmd/testcompiler.j3:280`,
   which also uses `timeout`, plus one more), `strip` in `package.sh` (Linux side, fine). `tar` and `curl.exe` ship with
   Windows 10 1803+. `git` is the user's problem (`j3c doctor` row).
8. **Finding clang.** `EmitOpts.clang` defaults to `"clang"` (`toolchain.j3:122`), resolved on `PATH`. llvm-mingw's clang
   targets mingw by default and brings its own sysroot, lld and `llvm-windres`. Nothing tells j3c where llvm-mingw is.
9. **The exe's own resources.** `j3c.exe` will call `*A` Windows APIs (`GetFullPathNameA` and others in
   `sys_windows`). Without a manifest carrying `<activeCodePage>UTF-8</activeCodePage>`, a user named José or a project
   under `Documents\Café` breaks (plan 02 bug 3, fix verified there under wine). `winres.j3` only does icons today.

## Design decisions

1. **The zip ships only `j3c.exe`. The toolchain comes from `j3c setup`.** Bundling llvm-mingw would make a 150 MB+
   download that has to be re-released for every llvm-mingw bump. Both `install.ps1` and Scoop run `j3c setup` at the end,
   so the user still types one command.
2. **`j3c setup` pins exact versions and hashes inside j3c** (a small table in `toolchain/setup.j3`: URL, sha256, folder
   name). It downloads with `curl.exe -fL` and unpacks with `tar.exe -xf` (zip support comes from bsdtar), both in-box on
   Windows 10 1803+, and checks the sha256 with `j3.hash.sha256` before unpacking. It doesn't use `j3.net.http` in process:
   TLS on Windows would need OpenSSL DLLs (plan 02 decision 9). Writes go to `<cacheRoot>\toolchain\llvm-mingw-<ver>\` and
   `<cacheRoot>\sdl2-mingw\`. Unpacking goes into a `.partial` folder that gets renamed at the end, so a cut-off download is
   never half-used. It is idempotent: a second run with everything present prints "up to date". After unpacking it deletes
   the i686/armv7/aarch64 sysroots. The x86_64-only tree is about half the size.
   On Linux, `j3c setup` fetches only the SDL kit (replacing `tools/fetch_sdl2_mingw.sh`, which stays as a one-line wrapper).
3. **Clang lookup order**: `J3_CLANG` env, then `<cacheRoot>\toolchain\llvm-mingw-*\bin\clang.exe` (highest version),
   then `clang` on `PATH`. One function, `toolchain.findClang`. `j3c doctor` prints which one won. A missing clang is one
   error line: `no clang: run \`j3c setup\``.
4. **Host triple**: an empty `[emit] target` resolves once, at manifest load, to the host triple from `sys.PLATFORM` and
   `sys.ARCH`, which are compile-time in the j3c binary itself: `x86_64-pc-windows-gnu`, `x86_64-pc-linux-gnu`,
   `aarch64-unknown-linux-gnu`, and macOS per plan 03. Everything downstream sees a real triple. Drop the `target datalayout`
   line, since clang fills the right one (plan 03 verified this). Do it in this plan, because Windows needs it first. Plan 03
   reuses it.
5. **One `cacheRoot()`** in `project/config.j3` (or wherever `config` already decides): `J3_CACHE`, then
   `XDG_CACHE_HOME/j3`, then `HOME/.cache/j3`, then `LOCALAPPDATA\j3`, then `USERPROFILE\AppData\Local\j3`. Every `HOME`
   site in "breaks" item 4 calls it, except the `~` expansion, which uses `homeDir()` = `HOME`, else `USERPROFILE`.
6. **One exe path.** `outPathFor` appends `.exe` when the resolved triple is Windows, and j3c passes that exact path to
   clang with `-o`. Every caller already goes through `outPath`/`outPathFor`, so the scattered patches (`size.j3:103` etc.)
   become calls.
7. **Paths compare with `/`.** Before any lookup keyed on embedded names (`Loader.embedded`, `stdIndex`, editor
   `@embedDir`), normalise `\` to `/`. One helper, a unit test with a backslashed path that runs on every host.
8. **Headless on Windows = hidden real window** (plan 02 decision 10): `--headless` sets `J3_HIDDEN=1` and
   `SDL_AUDIODRIVER=dummy` instead of `SDL_VIDEODRIVER=offscreen`, and `game/window.j3` `initGl` adds
   `SDL_WINDOW_HIDDEN` under `J3_HIDDEN`. If hardware gives blank reads from a hidden default framebuffer, render into an
   FBO when hidden (its own chunk, only if needed).
9. **No `zip` → `tar.exe -a -cf out.zip -C dir name`** behind one `dist.zipDir` helper, used on every host when `zip`
   is missing. `cmd/testcompiler.j3` stays Linux only (it is the compiler's own test runner) and says so when run elsewhere.
10. **`j3c.exe` is built by `j3c`, cross, on Linux**: stage 2 (`j3c-bootstrap/target/j3c`) runs
    `j3c build --release --target x86_64-pc-windows-gnu j3c-bootstrap`. Stage 0 (Rust) is not involved. The Windows exe
    can't build itself in CI until the `windows-latest` job exists. Once it does, that job also rebuilds j3c with the
    Windows j3c.exe and compares `version` (the same agreement check `build.sh` does).
11. **Resources for every Windows exe**, j3c.exe included: one `.rc` with the UTF-8/DPI/longPath manifest (plan 02
    decision 3), the icon (j3c.exe gets `logo.svg` rendered to PNG at 256, committed as `j3c-bootstrap/assets/j3c.png`),
    and VERSIONINFO from `[package]`. Keep `windres` (`llvm-windres` comes with llvm-mingw, `x86_64-w64-mingw32-windres` with
    the Linux mingw-w64 binutils) for now. Plan 02 decision 4 (j3c writes the `.rsrc` COFF itself) is the follow-up, not a
    blocker. With no windres the build warns, like it does for icons today.
12. **Install channels, in order**: `install.ps1` (attached to each release, mirrors `install.sh`: download the zip, check
    sha256 against `SHA256SUMS`, unpack to `%LOCALAPPDATA%\Programs\j3c\`, add it to the user `PATH` through
    `[Environment]::SetEnvironmentVariable(..., 'User')`, run `j3c setup`). Then a Scoop manifest in our own bucket
    (`packaging/scoop/j3c.json`, `bin: j3c.exe`, `post_install: j3c setup`, `checkver`/`autoupdate` off GitHub releases).
    winget once a few releases exist (it wants a stable URL history). No MSI.

## Chunks

Run list (orchestrated 2026-11-20 with /orchestrate-opus on branch `plan08-windows-j3c`; plan chunks merged into run chunks):
- [x] R1. Host triple, datalayout, `--target`, exe path, `cacheRoot`, embedded names with backslashes (plan 1, 2, 3)
- [x] R2. `j3c setup`, clang lookup, doctor (plan 4)
- [x] R3. Host tools and headless (plan 5)
- [x] R4. Build `j3c.exe` and its resources (plan 6)
- [x] R5. Release artifacts, Windows CI job, Scoop + docs (plan 7, 8, 9; CI green and winget submission need GitHub)
- Plan 10 stays with the developer.

Each chunk ships alone. Test under wine on Linux unless the chunk says otherwise: `WINEDEBUG=-all`, `timeout`, stdout to
a file, `wineserver -k` afterwards (plan 02 "Testing without the hardware"). Copy test projects **outside the repo**
(scratchpad) so the on-disk stdlib fallback can't hide chunk 3 bugs. Mid-work, rebuild with
`J3_STDLIB=$PWD/j3c/stdlib j3c build --opt 2 j3c-bootstrap` (or let `tests/diff.sh`/`unit.sh` rebuild `target/j3c`). Run `./build.sh` only at the end, after `tests/unit.sh` and the
diff gate pass.

### 1. Host triple, datalayout, `--target` (decisions 4)

- Files: `backend/llvm.j3` (`tripleOf`, datalayout at `:447`), `project/manifest.j3` / `manifest/parse.j3` (resolve
  empty target to the host triple at load), `toolchain/target.j3` (`TargetOs` from the resolved triple), `cmd/build.j3`,
  `cmd/run.j3`, `cmd/check.j3`, `cmd/args.j3` (`--target T` replaces `[emit] target` for this run; `--host` keeps its
  meaning), `text/` help strings, `INSTALL.md:75` (it already documents `--target`).
- Done when `j3c dump-llvm` of hello on Linux prints the host triple and no datalayout line, the Linux build output is
  unchanged (`tests/diff.sh`), and `j3c build --target x86_64-pc-windows-gnu .` writes a PE32+ (`file`).
- Coordinate with plan 03 chunk 1: mark it done there, or point it here.

### 2. Exe path and `cacheRoot` (decisions 5, 6)

- Files: `project/manifest.j3` (`outPathFor` + `.exe`), every `outPath` caller that patches the name by hand
  (`cmd/size.j3:103`, `grep -rn '\.exe' j3c-bootstrap/src`), `project/config.j3` (`cacheRoot`, `homeDir`), each `HOME`
  site from "breaks" item 4.
- Done when a Windows-target project on Linux: `j3c run` with the target set runs it under wine (plan 02 decision 11, if
  not already done there) or at least prints the right `target/x.exe` path. A unit test feeds `cacheRoot` an env with only
  `LOCALAPPDATA` set and gets `...\j3`. `grep -rn 'env("HOME")' j3c-bootstrap/src` hits only `homeDir`.

### 3. Embedded names with backslashes (decision 7)

- Files: `load/stdlib.j3` (`embedded`, `stdIndex` lookups), the editor frontend's `@embedDir` lookup
  (`j3c-bootstrap/src/editor/frontend.j3`), one unit test in `tests/unit.sh`'s set.
- First check what `j3.io.path.join` / `stripPrefix` return under `sys.PLATFORM_WINDOWS`. If they never emit `\`, the
  chunk shrinks to the unit test plus a note.
- Done when `wine j3c.exe check` passes on a copy of `examples/3d-editor-scene` in the scratchpad (needs chunk 6's exe; until
  then the unit test).

### 4. `j3c setup`, clang lookup, doctor (decisions 1, 2, 3)

- Files: new `toolchain/setup.j3` (pin table, download, sha256, unpack, prune, `.partial` rename), new `cmd/setup.j3` +
  dispatch + help, `toolchain.j3` (`findClang`, used where `EmitOpts.clang` is set), `toolchain/target.j3`
  (`mingwSdlLibs` reads `cacheRoot()`), `cmd/doctor.j3` (rows: clang found and which, its target triple via
  `clang -dumpmachine`, SDL kit present, cache writable, `git` on PATH, `tar`/`curl` present), `tools/fetch_sdl2_mingw.sh`
  → `exec j3c setup --sdl-only`.
- Pins to pick when implementing: the latest llvm-mingw `ucrt-x86_64` zip release and the SDL2 / image / ttf / mixer
  mingw dev kits that `fetch_sdl2_mingw.sh` uses today. Record the version and sha256 in the table with the date.
- Done when, on Linux, `j3c setup --sdl-only` fills a fresh `J3_CACHE` dir that a Windows-target game links against, a
  corrupted download (flip a byte in the cached archive) is refused and redownloaded, and a second run prints up to date.
  The llvm-mingw path gets tested for real in chunk 8.

### 5. Host tools and headless (decisions 8, 9)

- Files: `dist.j3`, `dist/apk.j3` (`zipDir` helper with the `tar -a` fallback), `cmd/run.j3:103`,
  `cmd/golden.j3:373`, `scene/ops.j3:424` (Windows host → `J3_HIDDEN=1`), `j3c/stdlib/j3/game/window.j3` (`initGl` hidden),
  `cmd/testcompiler.j3` (clean "Linux only" error elsewhere), `cmd/golden.j3` (child timeout through `j3.proc`, not
  `sh`/`timeout`, on Windows).
- Done when, on Linux with `zip` hidden from `PATH`, `[dist] build_outputs = ["zip"]` still writes a valid zip
  (`unzip -t`), and a Windows-target app built with `J3_HIDDEN=1` under `xvfb-run wine` saves a `J3_CAPTURE` frame that
  matches the Linux headless frame (`j3c -e imgdiff`).

### 6. Build `j3c.exe` and its resources (decisions 10, 11)

- Files: `toolchain/winres.j3` (grows from icon-only to `.rc` with manifest + icon + VERSIONINFO), `toolchain/link.j3`
  (always link the resource object for Windows targets, dev and release), `j3c-bootstrap/assets/j3c.png`,
  `j3c-bootstrap/j3.toml` (`[dist] icon`), `j3c/stdlib/j3/sys_windows.j3` (`SetConsoleOutputCP(65001)` /
  `SetConsoleCP(65001)` in start, if plan 02 chunk 4 hasn't already).
- `j3c-bootstrap/target/j3c build --release --target x86_64-pc-windows-gnu j3c-bootstrap` gives `j3c.exe`. Fix what
  breaks at link time: Linux-only stdlib reached from j3c. `grep` for `sys.fork`, `/proc/`, `/dev/` in `j3c-bootstrap/src`.
- Done when, under wine from a scratchpad copy: `j3c.exe version --verbose`, `doc search window`, `new hello`, `check`
  of `hello` and of `examples/3d-editor-scene`, and `fmt` all work. A project path with `é` in it checks. `objdump -p`
  shows the `.rsrc` section with the manifest (`x86_64-w64-mingw32-objdump -s -j .rsrc j3c.exe | grep activeCodePage`).
  The icon shows in a wine Explorer listing (optional).
- Under wine, `build` needs a Windows clang inside the prefix. Skip it here; it gets real coverage in chunk 8.

### 7. Release artifacts

- Files: `tools/release/package.sh` (`--windows`: builds as in chunk 6 with the version file, strips with
  `x86_64-w64-mingw32-strip` or skips if `--release` already strips, zips `j3c/j3c.exe` + `LICENSE-*` + `README.md` +
  `THIRD_PARTY.md` into `dist/j3c-<ver>-x86_64-windows.zip`, adds it to `SHA256SUMS`), new `tools/release/install.ps1`,
  `.github/workflows/release.yml` (the x86_64 job also installs `clang mingw-w64 binutils-mingw-w64-x86-64` and runs
  `package.sh --windows`. It uploads `install.ps1` as a release asset.)
- `install.ps1`: `$ErrorActionPreference='Stop'`, latest or `-Version`, `Invoke-WebRequest` the zip and `SHA256SUMS`,
  `Get-FileHash` compare, `Expand-Archive` to `%LOCALAPPDATA%\Programs\j3c`, add to the user PATH only if missing,
  `& j3c.exe setup`, then print `open a new terminal, then: j3c new hello --app`. No admin rights.
- Done when a tag push on a fork (or `act`) produces the zip, the checksum line, and `install.ps1` as release assets,
  and `unzip -l` shows the expected files.
- Landed (R5): `package.sh --windows` (cross-build with the version file; `J3_PACKAGE_VERSION` puts the same version
  in the VERSIONINFO; fails when the resources warning appears), `install.ps1` (`-Version`, `-InstallDir`, `-ZipPath`;
  registry user PATH kept as `REG_EXPAND_SZ`), `release.yml` x86_64 job. Ran locally end to end; not run on GitHub;
  `install.ps1` never parsed by PowerShell here (no `pwsh`).

### 8. Windows CI job

- Files: `.github/workflows/ci.yml` (new `windows` job on `windows-latest`, needs the Linux job's `j3c.exe` artifact),
  `tools/ci/windows-smoke.ps1`.
- Steps: run `install.ps1` against the artifact (a `-ZipPath` parameter for CI), `j3c doctor`, then
  `j3c new hello && cd hello && j3c run` (console, compare stdout), `speculative/01`–`07` build and run against their Linux
  stdout, `j3c new app --app && j3c run --headless --frames 5`. The runner has no GPU and its `opengl32.dll` is GDI GL
  1.1, so put mesa-dist-win's `opengl32.dll` beside the exe for the GL step (pin + hash, like `setup`). Cache
  `%LOCALAPPDATA%\j3\toolchain`.
- Then add the self-build: the Windows `j3c.exe` builds `j3c-bootstrap` and the result's `version` matches (decision 10).
- Done when the job is green on GitHub. Tick plan 01 §7 Windows and plan 02 chunk 13 (the j3c part).
- Landed (R5): the Linux job uploads `j3c-windows` (zip + `SHA256SUMS`); the `windows` job (LF checkout, cache of
  `toolchain` + `sdl2-mingw` keyed on `setup.j3`) runs `tools/ci/windows-smoke.ps1`. Mesa pinned to mesa-dist-win
  26.2.7 release-mingw (GitHub's asset digest). `j3c run` on Windows now puts `<sdl2-mingw>\dll` first on the child's
  PATH (`toolchain.runExe`). Never run: the first push decides; likely first fixes are in the mesa DLL step and the
  self-build.

### 9. Scoop, docs, winget

- Files: `packaging/scoop/j3c.json` (+ a bucket repo or `packaging/scoop/` used as one by URL), `packaging/README.md`,
  `INSTALL.md` "Windows" (install.ps1 one-liner, Scoop, manual zip + `j3c setup`, SmartScreen note: unsigned exes
  show "Windows protected your PC" → More info → Run anyway; what `j3c setup` downloads and where; uninstall = delete
  `%LOCALAPPDATA%\Programs\j3c` and `%LOCALAPPDATA%\j3`), `README.md` support line, `j3c doc` pages that say "Linux only"
  about j3c itself (`text/doc/pages/`, `grep -rn -i 'linux only\|linux-only'`), `roadmap.md` status, `plans/01` §6
  Windows box, `plans/02` chunks 12–14 → "see plan 08". Add the Windows host to the untested-features list in the README
  until chunk 10 is done.
- winget: `packaging/winget/` manifest (portable installer type pointing at the zip, `Commands: j3c`), submitted after the
  second Windows release.
- Done when a fresh Windows VM gets from zero to the Goal's four commands by following `INSTALL.md` only. Who runs the VM:
  the developer (see chunk 10).
- Landed (R5): `packaging/scoop/j3c.json` (version/url/hash filled by `checkver -u` after the first Windows release),
  `packaging/winget/` skeleton, `packaging/README.md`, `INSTALL.md` Windows, README, `roadmap.md`, plan 01 §6, plan 02
  chunk 13. No `j3c doc` page called j3c itself Linux only; `j3c doc ship` gained `J3_PACKAGE_VERSION`.

### 10. First real-hardware run (developer)

Not something an agent can do. A checklist for the person with a Windows box:

- `install.ps1` from a non-admin PowerShell. SmartScreen and Defender behaviour on the exe and on llvm-mingw's
  `clang.exe` (Defender sometimes slows first runs of fresh compilers; note the build time).
- The Goal commands. Then a real (non-headless) `j3c run` of the app template and `examples/desk-app`: GPU driver GL,
  DPI at 150%, audio.
- `j3c run --headless --screenshot` on a real driver: if the frame is blank, open the FBO follow-up (decision 8).
- A username or project path with non-ASCII letters, and a project path over 260 characters.
- `j3c add` of a git dependency (needs Git for Windows).
- Write what happened into this plan's Status, with dates. Fixes become chunks here or in plan 02.

## Later (not chunks yet)

- **Signing**: Authenticode (an OV cert or Azure Trusted Signing) removes the SmartScreen prompt. It costs money and the
  key needs a home in CI. Decide after people ask.
- **`.rsrc` written by j3c** (plan 02 decision 4): drops the `windres` need for game developers on Linux without mingw
  binutils.
- **arm64 Windows**: llvm-mingw has it, but the SDL mingw kits ship no arm64 libs.
- **ASLR** on with lld (plan 02 chunk 15). It applies to j3c.exe too.
- **VS Code extension**: spawn `j3c.exe` and use the Windows path for `j3c lsp`, after plan 06 ports the LSP.
- **`j3c self-update`**, once there's more than one release a month.
