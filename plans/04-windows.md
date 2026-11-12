# Plan 04: Windows: run, fill the stubs, ship

**Status (2026-11-11): TODO.** Windows exes already build and run under wine (console, SDL/GL apps, the editor's
renderer). What's left: a short list of real bugs found under wine, the `proc`/`net`/`instance`/`term` stubs, the
manifest/icon/subsystem, `j3c` on a Windows host, CI, and a first run on real hardware.

## Goal

1. A J3 game built for `x86_64-pc-windows-gnu` runs on Windows 10 (1903+) / 11 x64 the way it runs on Linux:
   window, GL, input, audio, UI text, saves under `%APPDATA%`, UTF-8 paths. It ships as a zip with an icon, no console
   window, and DPI awareness.
2. A game will likely call these stdlib modules, and none of them should hit a "not on this target" stub:
   `monotonicMs`, `j3.proc`, `j3.net.tcp`/`dns`/`tls`/`http`, `j3.instance`, `j3.game.hotkey`, non-text clipboard,
   `j3.term` tty. Drag-out and transparent windows are documented as unsupported.
3. `j3c` itself (check, build, run, `--hot`, doc, lsp, editor, MCP render) runs on a Windows host with llvm-mingw.
4. CI builds every sample for Windows and runs it under wine on Linux. A `windows-latest` job builds and tests
   `j3c.exe`. Releases carry `j3c-<ver>-x86_64-windows.zip`.

## Current state (verified)

Checked on 2026-11-11 with wine 9.0, clang 18.1.3, the Ubuntu mingw-w64 sysroot (gcc 13-win32, `ld -m i386pep`, no
lld), the SDL2 2.30.9 / image 2.8.2 / ttf 2.22.0 / mixer 2.8.0 mingw kits in `~/.cache/j3/sdl2-mingw` (from
`tools/fetch_sdl2_mingw.sh`), and the installed `j3c 0.1.0` (docs id 389221fa). All scratch builds were in the
session scratchpad. "V:" says how a claim was checked. "Read" means only from reading the code. "Unverified" means not checked.

### Building and running Windows exes (from Linux)

- **Only `[emit] target` sets the target.** `j3c build --target x86_64-pc-windows-gnu .` ignores the flag without a
  word and builds a Linux ELF (V: `file target/whello`). `INSTALL.md:75` and `roadmap.md:147` both document that flag.
  `j3c size` is the only command that takes `--target` (`src/cli/build.rs:12`).
- **Console hello**: PE32+ console exe (275 KB dev build) that imports `KERNEL32.dll`, `msvcrt.dll`, `USER32.dll`. Under wine it
  prints `windows` for `sys.PLATFORM_NAME`, and `arg(0)`, `env("USERPROFILE")` and `exeDir()` come back correct (V: ran it).
- **SDL apps**: the `j3c new --app` template, `examples/desk-app` and `examples/3d-editor-scene` build for Windows
  (dev, opt 1). With the four SDL DLLs beside the exe, each runs under
  `xvfb-run -a wine X.exe` with `J3_CAPTURE=shot.png:N J3_FIXED_DT=0.016`. Text, UI, 3D, shadows and sky all draw correctly,
  through `gl=llvmpipe`. The app template's frame matches the Linux `--headless` frame exactly:
  `j3c -e imgdiff` reports `0 of 230400 pixels differ` (V: ran all three, looked at the PNGs). No window shows up on the
  desktop because Xvfb is a virtual display.
- **Editor**: `j3c/editor/src` builds for Windows (`j3_editor.exe`, 6 MB, warnings only). Under `xvfb-run wine`,
  `J3_EDITOR_MODE=render J3_EDITOR_SCENE=scenes/main.j3scene` writes a correct `render.png` from a hidden GL window
  (V: ran it, looked at it).
- **Samples**: `speculative/01`–`07` and `13_threads` print the same stdout under wine as `j3c run` on Linux
  (V: diff). `08_game_loop` without `SDL2.dll` beside it exits with wine rc 53 (DLL not found) and prints nothing.
- **Stdlib tests under wine** (V: built each `stdlib/tests/X.j3` for Windows and ran it from the repo root under
  `xvfb-run wine`): `cmp`, `map`, `list`, `option`, `path`, `png`, `i18n`, `arena_align`, `j2_vm`, `j2_limits`,
  `j2_gc`, `term_grid`, `term_ansi` pass. `json_test` fails (see bugs). `dir_test` and `proc_test` fail on the
  `j3.proc` stub. All three then **hang** until the timeout (see the panic box). The batch timeout cut off `net_test` and
  `instance_test`, which hit stubs. The tests not listed were not run.
- **Release + zip**: `build_outputs = ["zip", "deb"]` with `j3c build --release` writes
  `target/dist/wapp-windows-x86_64.zip`. It holds `wapp.exe` (220 KB), `SDL2.dll`, `SDL2_image.dll`, `SDL2_mixer.dll` and
  `SDL2_ttf.dll`, and the deb step prints a skip (V: `unzip -l`). The DLLs come from `src/dist.rs:dlls`
  (objdump imports, plus the three add-ons that `addons.j3` loads at runtime).
- **Existing tests**: `cargo test --lib windows` runs 4 tests and all pass. Among them are `windows_io_runs_under_wine`
  (`src/compile/tests/targets.rs:156`, which runs `stdlib/tests/win_io_test.j3` for `\`, `C:` and mixed separators) and
  `windows_hello_uses_kernel32` (V: ran). `tools/ci/stdlib-tests.sh` skips `win_io_test`.
- **Link**: clang `--target=x86_64-pc-windows-gnu` drives the host `/usr/bin/ld -m i386pep` (V: `clang -v`).
  `src/emit/host/link.rs:50` always adds `-Wl,--disable-dynamicbase -Wl,--disable-high-entropy-va`. The comment says
  mingw ld leaves the TLS directory (`#[thread]` globals) without base relocations. So ASLR is off and
  DllCharacteristics is `0x100` (NX only) (V: objdump). Release flags are `-Wl,-s -Wl,--no-insert-timestamp`
  (`src/emit/host/target.rs:67`). Unwind tables stay in.
- **Crash filter**: a bad write in a dev build writes `crash.log` to the cwd. It holds the exception code
  `c0000005`, the module base, the fault offset and the stack words that point into the exe (V: under wine,
  `sys_windows.j3:crashFilter`). Nothing goes to stderr and nothing gets symbolized. The Linux debug crash handler
  (`src/emit/llvm_crash.rs`, `supported()`) is x86-64 Linux only.
- **Subsystem**: every exe is `Windows CUI` (console). Adding `-Wl,--subsystem,windows` through `[link] flags` gives
  `Windows GUI`, and stdout still reaches a redirect (V: objdump + wine).

### Bugs found under wine (all V: a probe program `wprobe` or a stdlib test)

1. `sys.monotonicMs()` always returns 0 (`sys_windows.j3:272`). That breaks `instance.request` timeouts
   (`instance.j3:222`) and `term/screen.j3:300` timing.
2. `O_RDWR` opens read-only: `openMode` only tests `O_WRONLY`, so `write` on an `O_RDWR` fd returns -1.
3. **UTF-8 paths**: `io.writeFile("café.txt")` creates `cafÃ©.txt`, because `CreateFileA` reads the bytes in the ANSI code
   page. Every `*A` call does the same (`GetEnvironmentVariableA`, so `%APPDATA%` for a user named José, plus
   `FindFirstFileA`, `GetModuleFileNameA`, and the msvcrt `argv`). **Fix verified**: a manifest with
   `<activeCodePage>UTF-8</activeCodePage>`, turned into an object by `x86_64-w64-mingw32-windres` and linked through
   `[link] files`, makes the same program create `café.txt` (V: wine 9).
4. `json_test` fails at "number 5 writes 1.5e-07" because msvcrt `snprintf("%.*g")` prints a 3-digit exponent. The cause is
   `data/json/write.j3:48`, and `strtod` in json/j2 has the same msvcrt exposure. `libmingwex.a` has `__mingw_snprintf`
   and `__mingw_strtod` (V: `nm`).
5. **A panic can hang forever.** When stdout/stderr are redirected and there is no console, `showPanicBox`
   (`sys_windows.j3`) opens a modal `MessageBoxA`. `GetConsoleProcessList` returns 1 or fewer there. `json_test` and `dir_test` sat
   until the 90 s timeout. This is a hazard for CI and for any tool that pipes a game's output.

### Stubs and Linux-only code (Read, except where it says V)

- `sys_windows.j3`: `fork`, `execve`, `pipe`, `dup2`, `ioctl`, `setsid`, `waitpid`, `kill` and `pollFds` return -1. Files, dirs, stat,
  rename, env, dlopen, exeDir and the crash filter are real. The pub names match `sys.j3` exactly (V: comm of the pub lists).
- `j3.proc` (`proc.j3`, 390 lines, written against fork/execve/pipe/poll): `proc.run(["cmd","/c","echo hi"])` returns
  `Err` (V).
- `netsys_windows.j3`: UDP is real Winsock (`WSAStartup` on first `udp()`) but has never been run. `tcp`, `waitFd`,
  `connectTimeout`, `setTimeouts`, `noDelay`, `listenOn`, `accept`, `recv`, `send` and `lookup` are stubs. `AF_INET6` is the
  Linux value 10 (Windows uses 23). `tcp.dial` returns `Err` (V). `net/tls.j3` opens only `libssl.so.3` / `libssl.so` and has no CA
  source for Windows.
- `j3.instance`: `instance_other.j3` is used on every OS except Linux (`src/compile/loader/paths.rs`). `claim` always
  returns true (V).
- `j3.threadsys` Windows: `CreateThread` + SRW locks + condition variables. `enter`/`leave` are empty, so there is no crash
  stack. `13_threads` passes under wine (V).
- X11 only, through `dlopen("libX11.so.6")`, so each returns false or empty on Windows without crashing: `game/hotkey.j3`
  (`register` gives `Err`), `game/clipdata.j3` (`clipboardTypes`/`clipboardData`), `game/dragout.j3` (`dragFiles`),
  `game/winframe.j3:128` (the transparent-window visual), and `game/events.j3:592` (primary selection, via `libSDL2-2.0.so.0`).
- `j3.term`: `term/tty.j3` needs `ioctl` (raw mode, size), so `isTty`/`size`/`Raw.enter` fail. `term/pty.j3:192` opens
  `/dev/ptmx`. `examples/terminal` is Linux only.
- High DPI: `SDL_WINDOW_ALLOW_HIGHDPI` (`game/window.j3:92`) does nothing on Windows unless the process is DPI aware, and
  nothing sets that up (no manifest, no `SDL_HINT_WINDOWS_DPI_AWARENESS`). Unverified on hardware. Expect bitmap-stretched
  blur at 150%.
- No resources in the exe: no icon, no manifest, no version info (V: no `.rsrc` section). The window icon is set
  at runtime from `[dist] icon` PNG (`j3c doc ship`).
- GL entry points come from `SDL_GL_GetProcAddress` (`game/window.j3:318`, `gfx/glcalls.j3:5`). SDL falls back to
  `opengl32.dll` exports for GL 1.1 names that `wglGetProcAddress` refuses. Unverified on a real driver.

### Cross-cutting compiler bug (owned by plan 05)

`src/emit/llvm.rs:63` and `:265` hard-code `target triple = "x86_64-pc-linux-gnu"` when `opts.target` is empty. Both
sites also always write the ELF datalayout `e-m:e-...`, and Windows should get `e-m:w-...`. Cross builds still link
because clang takes the explicit `--target` (V: every build above). On a **Windows host** with no `[emit] target`,
though, `target_os("")` picks Windows (`target.rs:21`), so the loader takes `sys_windows.j3` while the IR claims Linux. That
build cannot work. [Plan 05](05-macos.md) fixes it (host triple plus a per-triple datalayout). Chunk 12 here depends on it.

### `j3c` on a Windows host

- `cargo build --release --target x86_64-pc-windows-gnu` (rustup target installed, `J3C_NO_INSTALL=1`) compiles with one
  warning (an unused `mut`) and gives `j3c.exe` at 13 MB (V). Every `std::os::unix` use is behind `#[cfg(unix)]`
  (`main.rs`, `editor/mod.rs`, `emit/host.rs:410`, `golden.rs`, `dist/deb.rs`, `dist/steam.rs`, `hot.rs`).
- Under wine, `j3c.exe version --verbose`, `check` of a console project and `doc search` work (V). **`check` of any
  SDL project fails** with "`j3.gfx.canvas` has no `loadPng`" (V). Root cause, `src/stdlib/mod.rs`: `embedded()` and
  `parts()` compare `strip_prefix(..).to_str()`, which is `gfx\canvas.j3` on Windows, with the `/`-keyed `FILES`
  names. Under wine the single files fall back to disk, because `Z:\home\...` happens to be the live tree. Part folders
  return an empty list from the embedded branch. On a real Windows box no nested stdlib module would load at all.
- Exe names: j3c passes `-o target/<name>` and mingw writes `<name>.exe`. The build then prints `./target/whello`, and
  **`j3c run` of a Windows-target project fails** with "run ./target/whello: No such file" (V). The same assumption is in
  `editor/frontend.rs` (`target/j3_editor` `.is_file()`), `scene/mod.rs` (`dir.join("target/j3_editor")`) and
  `run_exe_in` (`canonicalize` of the bare path). `cli/build.rs:33`, `dist.rs:41` and `hot.rs:357` patch around it locally.
- Paths keyed on `HOME`, which cmd/PowerShell do not set (Read): `editor/frontend.rs:124 cache_root`,
  `emit/host/target.rs:158 mingw_sdl_libs`, `dist.rs:151`, `project/steam.rs:29`, `project.rs:454`, `doctor.rs`
  ("~/.cache/j3 writable"). `config.rs` already uses `%APPDATA%`.
- `--headless` sets `SDL_VIDEODRIVER=offscreen` (`cli/mod.rs:322`, `scene/mod.rs:308`, `mcp/ops.rs:301`,
  `golden.rs:138`). The Windows SDL build's offscreen driver has no GL (`j3c doc ship`, unverified here), so headless runs,
  goldens and MCP `render` cannot work on a Windows host as written.
- LSP (`lsp/proto.rs:45`): `uri_to_path("file:///c%3A/x")` gives `/c:/x`, and `path_to_uri` formats a canonical
  `\\?\C:\...` as `file://\\?\C:\...`. Both are wrong on Windows (Read).
- Host tools that Windows lacks: `zip` (`dist.rs:101`, `dist/apk.rs:153`), `zenity` (`editor/play.rs:277`, which already
  degrades to a message), `sh`/`kill` in `golden.rs` (cfg-guarded, so goldens lose the group kill), `nm`/`addr2line` in
  `symbolize.rs`. `tools/fetch_sdl2_mingw.sh` is `sh`.
- `--hot` (`hot.rs`): it already runs a private copy (`run-0.exe` and `run-1.exe`) so the rebuild can replace the original.
  The signal trap is a no-op off Unix. Never run on Windows.

### CI, packaging, Steam

- `.github/workflows/ci.yml` and `release.yml` run only on `ubuntu-24.04`, `ubuntu-22.04` and `ubuntu-22.04-arm` (Read). No job
  builds a Windows target.
- [Plan 02](02-public-release.md) has open boxes for "Windows: zip + winget/Scoop" (§6) and "Windows: run the built exe,
  test in CI" (§7). [Plan 01](01-steam.md) item 7 covers the Windows Steam checks (`steam_api64.dll`, cloud,
  `J3_STEAM_FAKE`). Its status line says its Windows primitives were tested under wine.

## Gaps and blockers

In priority order. Bold ones block "a game runs and ships":

1. **Bugs 1–5 above** (monotonic, O_RDWR, UTF-8, `%g`, panic hang).
2. **No manifest**, so no UTF-8 code page and no DPI awareness. Also no icon or version resource, and a console
   window opens behind every release game.
3. **`j3c run`/editor/hot assume no `.exe`.** There is no run-under-wine loop and `--target` is silently ignored.
4. **`j3.net` tcp/dns/tls and `j3.proc` are stubs.** These block `j3.net.http` and `examples/openrouter-chat`.
5. `j3.instance` stub, hotkey, non-text clipboard, `j3.term` tty/pty.
6. A crash gives only raw offsets in `crash.log`, and the TLS relocation workaround turns ASLR off.
7. `j3c` on a Windows host: embedded stdlib path keys, the triple bug (plan 05), `HOME` caches, LSP URIs, headless GL.
8. No CI for Windows at all, and no Windows release of `j3c`.
9. Nothing has ever run on real Windows hardware: GPU drivers, DPI, audio (WASAPI), XInput pads, Defender/SmartScreen,
   Steam overlay.

## Design decisions

1. **Toolchain: mingw-w64 only (`x86_64-pc-windows-gnu`).** Cross builds use clang plus the distro mingw sysroot. A
   Windows host uses [llvm-mingw](https://github.com/mstorsjo/llvm-mingw) (one zip: clang, lld, windres, headers, UCRT).
   There is no `-windows-msvc` target, which would need link.exe or lld-link, MSVC SDL libs, another CRT and different
   unwind/SEH rules. Windows on arm64 can come later: llvm-mingw has it, but the SDL mingw kit ships no arm64 libs.
2. **CRT: whatever the sysroot has** (msvcrt on Ubuntu, UCRT on llvm-mingw). J3 uses only a few libc functions. Where
   their output is visible (`snprintf %g`, `strtod` in `data/json`, `j2`), bind mingw's own `__mingw_snprintf` /
   `__mingw_strtod` on Windows. They come from `libmingwex.a`, which is always linked statically and exists in both flavours,
   so output matches Linux. The rule for future code: no `printf`-family formatting through the CRT on Windows.
3. **UTF-8 through the manifest, not W APIs.** Keep every `*A` call and make the process code page UTF-8 with
   `<activeCodePage>UTF-8</activeCodePage>` (verified above). Also call `SetConsoleOutputCP(65001)` /
   `SetConsoleCP(65001)` in `start()` so `printLn` of UTF-8 shows in a console. Limits: Windows 10 1903+ (older systems get
   the ANSI page, documented) and MAX_PATH 260 for `*A` paths, unless `longPathAware` is in the manifest and the registry
   switch is on. Add `longPathAware`. W APIs plus UTF-16 conversion everywhere would be a much bigger diff for no gain on
   supported systems.
4. **Resources: j3c writes the `.rsrc` COFF object itself**, with no windres dependency, so it works from any host. One
   object carries the manifest (RT_MANIFEST 1: activeCodePage UTF-8, dpiAwareness PerMonitorV2 with dpiAware
   true/pm fallback, longPathAware, supportedOS Win10), the icon (RT_GROUP_ICON + RT_ICON, PNG-in-ICO from `[dist] icon` or
   `LOGO_PNG` at 256/48/32/16, resized with `src/png.rs`) and VERSIONINFO from `[package]` (name, version,
   description). It goes into every Windows link, dev and release alike, because UTF-8 has to behave the same in the dev loop.
   `x86_64-w64-mingw32-windres` (installed here) is only the test oracle: compare `objdump -s -j .rsrc` on both.
5. **Subsystem**: a release build of a program that links SDL2 gets `-Wl,--subsystem,windows`. Dev builds and console
   programs stay console. Projects override this with `[link] flags = ["-Wl,--subsystem,console"]` (a later flag wins), so
   no new key.
6. **Panic box rule**: show `MessageBoxA` only when nobody can see stderr. That means the `GetStdHandle(STD_ERROR)` handle is
   null or invalid (a GUI exe that was double-clicked), or it is a console that only this process owns
   (`GetConsoleProcessList` ≤ 1, a double-clicked console exe). A disk file or pipe (`GetFileType` 1 or 3) never gets a
   box. `J3_NO_PANIC_BOX=1` forces the box off for CI under wine, where the console heuristics differ.
7. **OS split stays per file, picked by the loader** (`src/compile/loader/paths.rs`), as `sys`, `netsys` and
   `threadsys` already are. New: `j3.proc` → `proc_windows.j3` and `j3.instance` → `instance_windows.j3`, each with the same pub
   API as the Linux file and the same doc comments, so `j3c doc` reads the same. Small Windows-only branches inside a
   shared file use `if sys.PLATFORM == sys.PLATFORM_WINDOWS`, which is compile time.
8. **Processes**: `CreateProcessA` (UTF-8 thanks to decision 3), `CreatePipe` with inheritable child ends,
   `PeekNamedPipe` for `tryRead`, `WaitForSingleObject` / `GetExitCodeProcess`, and `TerminateProcess` for `kill`. Quote
   argv the way `CommandLineToArgvW` parses it: backslashes before a quote double, and an argument with a space or tab gets
   quotes. Look programs up on `PATH` with `PATHEXT` (`.exe`, `.cmd`, `.bat`). `.cmd`/`.bat` go through `cmd /c` and are
   documented as unsafe with untrusted args. `Output.code` is the exit code (no signals).
9. **Network**: Winsock for tcp/dns, with `WSAPoll` for `waitFd`. `getaddrinfo` reads the Windows `ADDRINFOA`
   layout, which differs from Linux: `ai_addrlen` is `size_t`, and **`ai_canonname` comes before `ai_addr`**. Use
   `AF_INET6 = 23`, and `SO_RCVTIMEO`/`SO_SNDTIMEO` take a DWORD of milliseconds. TLS keeps OpenSSL 3, loaded from
   `libssl-3-x64.dll` / `libcrypto-3-x64.dll` beside the exe. No Schannel, which would be a second TLS stack. Roots come
   from the Windows `ROOT` store (`crypt32`: `CertOpenSystemStoreA`, `CertEnumCertificatesInStore`, then `d2i_X509` +
   `X509_STORE_add_cert`). C `long` is 32-bit on Win64, so declare OpenSSL `long` returns (`SSL_ctrl`,
   `SSL_CTX_ctrl`, `SSL_get_verify_result`) as `i32` in the Windows branch. `dist` ships the two OpenSSL DLLs when the exe
   reaches `j3.net.tls`. The ponytail version: when `[link] libs` or the program text names `j3.net.tls`/`http`.
10. **Headless on Windows means a hidden real window**, not `SDL_VIDEODRIVER=offscreen`. On a Windows host, `--headless`
    sets `J3_HIDDEN=1` and `SDL_AUDIODRIVER=dummy`, and `initGl` adds `SDL_WINDOW_HIDDEN` when `J3_HIDDEN` is on
    (`initGlHidden` already does this for the editor, verified under wine). Risk: a real driver may fail the pixel
    ownership test on a hidden default framebuffer. If hardware shows blank reads, the fallback is to render into an FBO
    when hidden. That is a separate chunk, only if needed.
11. **Test harness: wine + Xvfb on Linux.** It is verified for console programs, SDL/GL apps, `J3_CAPTURE` and the editor renderer.
    `j3c run` of a project whose only target is Windows runs the exe under `wine` (`WINEDEBUG=-all`,
    `WINEPATH` = the SDL DLL dir, `J3_NO_PANIC_BOX=1`). With `--headless` the command gets wrapped in `xvfb-run -a`, and
    if wine or xvfb-run is missing there is a one-line error. `--host` keeps meaning "build for the host instead".
12. **One exe path helper**: `Manifest::out_path_for` appends `.exe` for a Windows target (and for an empty target on a
    Windows host), and j3c passes that same path to clang. Every caller reads it from there, and the local
    `with_extension("exe")` patches go away.
13. **One cache root**: `editor::cache_root()` falls back to `%LOCALAPPDATA%` and then `%USERPROFILE%\AppData\Local` when
    `XDG_CACHE_HOME` and `HOME` are unset. Every `HOME/.cache/j3` site calls it, so `sdl2-mingw` and `pkgs`
    live under `%LOCALAPPDATA%\j3\` on Windows. The `~/` expansion in `project.rs:454` / `project/steam.rs:29` also
    accepts `USERPROFILE`.
14. **ASLR**: keep `--disable-dynamicbase` until real hardware has run. Then try lld (`-fuse-ld=lld`, llvm-mingw's
    default), which writes TLS base relocations correctly. Turn ASLR on wherever that is shown to work. Wine
    usually loads at the preferred base, so it cannot test this.
15. **What stays Linux only**, documented in each module doc and `j3c doc`: `dragFiles` (OLE `DoDragDrop` needs a
    COM `IDataObject`/`IDropSource` in J3, which is a lot of code for one feature) and transparent GL windows (SDL2 on Windows has
    no per-pixel-alpha GL window). Everything else in Goal 2 gets a Windows backend.

## Chunks

Each chunk ships alone. Unless a chunk says otherwise, test it under wine on Linux. The helper for that is chunk 3, and
until it lands, by hand: add `target = "x86_64-pc-windows-gnu"` under `[emit]` in a scratch project, `j3c build .`,
copy `~/.cache/j3/sdl2-mingw/dll/*.dll` beside the exe or set `WINEPATH`, then
`WINEDEBUG=-all timeout 60 xvfb-run -a wine target/X.exe > out.txt 2>&1`. Redirect to a file, not a pipe:
wineserver inherits pipes, see `targets.rs:171`. Run `wineserver -k` after a batch. Rebuild `j3c` with
`J3C_NO_INSTALL=1`, then `./build.sh` at the end (repo `AGENTS.md`).

### 1. Quick `sys_windows` fixes (bugs 1, 2, 5)

- Files: `j3c/stdlib/j3/sys_windows.j3`, `j3c/stdlib/tests/win_io_test.j3`.
- `monotonicMs`: `GetTickCount64()` from kernel32.
- `openMode`: `O_RDWR` → `GENERIC_READ | GENERIC_WRITE`. `O_WRONLY` stays write only.
- Panic box rule (decision 6): `GetFileType`, the `J3_NO_PANIC_BOX` check, and the same rule in `crashFilter`.
- Done when `win_io_test` also checks an `O_RDWR` write plus read-back and that `monotonicMs()` moves forward across a
  10 ms busy loop, and when a panicking test under `xvfb-run wine` with output redirected exits 101 at once instead of hanging.
- Test: `cargo test --lib windows_io_runs_under_wine`, and the `json_test` run from "Current state" exits 101 quickly.

### 2. Number formatting matches Linux (bug 4)

- Files: `j3c/stdlib/j3/data/json.j3` (the `use lib` block at :22), `data/json/write.j3:48`, `data/json/parse.j3`,
  `j2.j3:45`, `j2/lib.j3:122`, `j2/lex.j3:14`, plus any other `snprintf`/`strtod` user (`grep -rn 'snprintf\|strtod'
  j3c/stdlib/j3`).
- On Windows call `__mingw_snprintf` / `__mingw_strtod`. Either use a second `use lib` block, or put one wrapper pair in
  `sys_*.j3` (`sys.fmtG(buf, n, digits, x)`, `sys.parseF(p)`) so callers stop declaring libc themselves. Pick the wrapper
  if more than two modules need it.
- Done when `json_test` passes under wine, and the `j2_*` tests still pass under wine and on Linux.

### 3. Exe paths, `--target`, run under wine

- Files: `j3c/src/project.rs` (`out_path_for`), `src/emit/host.rs` (the `-o` path, `run_exe_in`), `src/cli/build.rs`
  (`--target` for `build`/`check`/`run` overrides `[emit] target`; remove the `.exe` patch at :33), `src/cli/mod.rs`
  (`dispatch`: reject unknown `--flags` with "unknown flag"), `src/dist.rs:41`, `src/hot.rs:357`, `src/devrun.rs`
  (launch through wine per decision 11), `src/editor/frontend.rs` and `src/scene/mod.rs` (`j3_editor` exe name via
  one helper), `INSTALL.md:75`, `roadmap.md:147`, `j3c/src/doc/pages/ship.md` (the wine paragraph).
- Done when `j3c build --target x86_64-pc-windows-gnu .` writes `target/<name>.exe` and prints that path. `j3c run .`
  on a Windows-target project prints the program's output through wine. `j3c run --headless --frames 30 --screenshot
  s.png .` writes a PNG through `xvfb-run wine`. `j3c build --bogus .` errors out.
- Test: a `src/cli/tests.rs` case for `--target` reaching `EmitOpts.target` (no wine). A `targets.rs` test, gated like
  `windows_io_runs_under_wine`, that `j3c run`s a hello project with a Windows target and finds "Hello".

### 4. Resources: manifest, icon, version (decisions 3, 4, 5)

- Files: new `j3c/src/emit/host/winres.rs` (the COFF `.rsrc` writer: resource directory tree, data entries with
  `IMAGE_REL_AMD64_ADDR32NB` relocations, manifest XML, ICO-from-PNG, VERSIONINFO), `src/emit/host/link.rs` (add the
  object to every Windows link, add `--subsystem,windows` for release builds that link SDL2), `src/png.rs` (resize, if it
  has none), `sys_windows.j3` `start()` (`SetConsoleOutputCP`/`SetConsoleCP` 65001), `ship.md`, `helptext/manifest.md`
  (`[dist] icon` is now also the exe icon).
- Done when a Windows build has a `.rsrc` that `x86_64-w64-mingw32-objdump -p` lists. Its manifest bytes equal the
  XML j3c wrote, and its icon shows in `wrestool`-style listings (or compare against a windres-built object). The `wprobe`
  case (`io.writeFile("café.txt")`) creates `café.txt` under wine. A release app is `Windows GUI` and a console hello stays
  `Windows CUI`.
- Test: a Rust unit test that parses the written object (section name, relocation count, manifest bytes at the data
  entry), plus a `targets.rs` wine test that writes and lists a non-ASCII file name and prints a non-ASCII `env` value.

### 5. `j3.proc` on Windows (decision 8)

- Files: new `j3c/stdlib/j3/proc_windows.j3` (same pub API and doc comments as `proc.j3`), a `j3.proc` arm in
  `src/compile/loader/paths.rs`, the kernel32 decls (`CreateProcessA`, `CreatePipe`, `SetHandleInformation`,
  `PeekNamedPipe`, `GetExitCodeProcess`, `TerminateProcess`, `WaitForSingleObject`, `SearchPathA` or a manual
  `PATH`/`PATHEXT` walk), and `proc.j3`'s module doc (drop "on other targets every call returns Fail" for Windows).
- `Child.stdin/stdout/stderr` are handles. Make sure `sys.read`/`sys.write`/`sys.close` accept them, which they already
  do since `handle()` passes non-std values through.
- Done when `stdlib/tests/proc_test.j3` passes under wine. Linux-only commands in it (`sh -c`, `cat`) need a
  `PLATFORM_WINDOWS` branch using `cmd /c`, `findstr`/`more`, or the test exe itself as the child. `dir_test` (which shells out)
  passes or gets a Windows branch.
- Test: `proc_test` and `dir_test` under wine, plus a quoting case: an argument with spaces, quotes and trailing
  backslashes comes back exactly when the child echoes its `sys.arg(1)`.

### 6. `j3.net` tcp, dns, tls, http on Windows (decision 9)

- Files: `j3c/stdlib/j3/netsys_windows.j3` (fill every stub, fix `AF_INET6`, `caFiles` → Windows ROOT store or a new
  `caStore` hook), `j3c/stdlib/j3/net/tls.j3` (DLL names on Windows, `long` as i32, the ROOT store load),
  `src/dist.rs:dlls` (ship `libssl-3-x64.dll`/`libcrypto-3-x64.dll` when the program uses `j3.net.tls`; they are found
  like the SDL DLLs, so add an OpenSSL kit download to the SDL fetch in chunk 11), and `roadmap.md:23`/`:148`.
- Done when `stdlib/tests/net_test.j3` (loopback server: chunked, redirects, timeouts, SSE) passes under wine, and
  `examples/openrouter-chat` against its local mock (`OPENROUTER_URL`) works under wine in all three modes. `--curl` needs
  chunk 5, plus `curl.exe` from Windows 10, or wine's.
- Test: `net_test` under wine. `tls_live_test` is network-gated, so the developer runs it on real Windows (see below).

### 7. `j3.instance` on Windows

- Files: new `j3c/stdlib/j3/instance_windows.j3` (same API as `instance.j3`), the `paths.rs` arm, and `j3c doc
  j3.instance` text (drop "Linux"). Mechanism: `CreateMutexA("Local\\j3-<name>")` plus `GetLastError() ==
  ERROR_ALREADY_EXISTS` for `claim`. A named pipe `\\.\pipe\j3-<user>-<name>` carries forwarded argv and request/reply, with
  the same framing as the Linux socket so `instance.j3`'s message code can be copied over. The server pipe is
  overlapped/non-blocking (`PeekNamedPipe`) so `next()` never blocks the frame.
- Done when `stdlib/tests/instance_test.j3` passes under wine. Two wine processes share a wineserver, so this is a real test.
- Test: `instance_test` under wine. A second copy's `claim` returns false and its args arrive at the first copy's `next()`.

### 8. Global hotkeys on Windows

- Files: `j3c/stdlib/j3/game/hotkey.j3` (a Windows branch), `game/sdlcore.j3` (`SDL_SetWindowsMessageHook`,
  `SDL_GetWindowWMInfo` already used by dragout for X11). `RegisterHotKey(hwnd, id, MOD_* | MOD_NOREPEAT, vk)` gets a
  J3 `Key` → VK table. The message hook sees `WM_HOTKEY` (0x0312) before SDL dispatches it and marks the hotkey fired
  for `game.poll`. `UnregisterHotKey` on drop.
- Done when `stdlib/tests/hotkey_test.j3` has a Windows path (register, a taken combo gives `Err`) passing under wine.
  Delivery of the real keypress is a hardware check (below).

### 9. Non-text clipboard on Windows

- Files: `j3c/stdlib/j3/game/clipdata.j3` (a Windows branch; user32 `OpenClipboard`, `EnumClipboardFormats`,
  `GetClipboardData`, `GetClipboardFormatNameA`; kernel32 `GlobalLock`/`GlobalSize`; shell32 `DragQueryFileA`). Map
  `CF_HDROP` → `text/uri-list` (`file:///C:/...`, percent-encoded), the registered `"PNG"` format → `image/png`, and
  `CF_DIBV5`/`CF_DIB` → `image/png` encoded with `src`'s J3 PNG writer (if it has none, return `image/bmp` with a
  `BITMAPFILEHEADER` added in front). Text keeps going through SDL.
- Done when `stdlib/tests/clipdata_test.j3` has a Windows path that puts a file list and a PNG on the clipboard
  (through the test's own `SetClipboardData`) and reads them back under wine. Also update `j3c doc recipes paste`.

### 10. `j3.term` tty and pty on Windows

- Files: `j3c/stdlib/j3/term/tty.j3` (Windows branch: `GetConsoleMode`/`SetConsoleMode` with
  `ENABLE_VIRTUAL_TERMINAL_INPUT`, `ENABLE_VIRTUAL_TERMINAL_PROCESSING`, no line/echo/processed input;
  `GetConsoleScreenBufferInfo` for `size`; `WaitForSingleObject(stdin, ms)` + `ReadFile` for `readFd`), and
  `term/pty.j3` (ConPTY: `CreatePseudoConsole`, `ResizePseudoConsole`, `ClosePseudoConsole`, plus
  `STARTUPINFOEXA` with `PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE`, reusing chunk 5's process code). `j3c doc term`
  says Windows 10 1809+.
- Done when `term_screen_test`, `term_input_test` and `term_pty_test` pass under wine, or, where wine's ConPTY is too
  thin, a wine failure is shown to be wine's own (note it in the test). `examples/terminal` runs `cmd.exe` on the developer's
  machine.
- Lowest priority of the stub chunks. A game rarely needs it.

### 11. Crash reports and the SDL/OpenSSL kit

- Files: `j3c/src/emit/host/symbolize.rs` (after a dev run of a Windows exe, through wine or on a Windows host, read
  `crash.log` and print `function at file:line` with `llvm-addr2line`/`addr2line -e exe base+offset`; the exe has DWARF
  in dev builds), `j3c/src/dist.rs` (release: keep an unstripped `target/<name>.debug.exe` next to the stripped one, so
  a player's `crash.log` can be symbolized later. `--no-insert-timestamp` keeps both identical apart from symbols), and
  `tools/fetch_sdl2_mingw.sh` → a `j3c -e fetch-mingw-libs` Rust command that drives `curl` + `tar` (both ship with
  Windows 10+). It writes to `cache_root()/sdl2-mingw` and adds an OpenSSL 3 mingw build for chunk 6. Keep the
  shell script as a thin wrapper, or delete it once the docs point at the command.
- Done when a dev crash under `j3c run` (wine) prints a symbolized stack like Linux does, and `j3c -e fetch-mingw-libs`
  fills a fresh `J3_SDL2_MINGW` dir that a Windows game build links against.
- Test: a `targets.rs` wine test that builds a program which writes through a bad pointer and asserts that the function name
  shows up in the output.

### 12. `j3c` on a Windows host (needs plan 05's triple fix)

- Files: `j3c/src/stdlib/mod.rs` (`embedded`, `parts`: turn `\` into `/` before comparing; add a unit test with
  a backslashed path that runs on every OS, built by hand with `Path::new` components), `src/editor/frontend.rs`
  (`cache_root` per decision 13), `src/emit/host/target.rs:158`, `src/dist.rs:151`, `src/project.rs:454`,
  `src/project/steam.rs:29`, `src/doctor.rs` (Windows rows: llvm-mingw clang finds `x86_64-w64-mingw32` headers,
  SDL kit present, `%LOCALAPPDATA%\j3` writable), `src/dist.rs:101` (no `zip` → `tar -a -c -f x.zip`, which is bsdtar and
  ships with Windows 10+), `src/lsp/proto.rs` (`file:///C:/` both ways, strip `\\?\`), `src/cli/mod.rs:322`,
  `src/scene/mod.rs:308`, `src/mcp/ops.rs:301`, `src/golden.rs:138` (decision 10, `cfg!(windows)`),
  `stdlib/j3/game/window.j3:41` (`initGl` honours `J3_HIDDEN`), `editors/vscode` (spawn `j3c.exe`).
- Done when `wine j3c.exe check` passes on `examples/3d-editor-scene` from a copy outside the repo
  (`/tmp/...`, so the disk fallback cannot hide the bug). The LSP unit tests cover Windows URIs. `cargo build --target
  x86_64-pc-windows-gnu` stays warning-free.
- Test: under wine you can only check `check`, `doc`, `fmt`, `lsp` on stdin and `-e scene` read ops, because there is no Windows clang in
  the prefix. Building and running on a Windows host goes in chunk 13's CI job and the hardware list.

### 13. CI

- Files: `.github/workflows/ci.yml`, `tools/ci/samples.sh`, `tools/ci/stdlib-tests.sh`, `tools/release/package.sh`,
  `.github/workflows/release.yml`.
- Linux job `windows-cross` (ubuntu-24.04): apt `clang mingw-w64 wine64 xvfb`, then `j3c -e fetch-mingw-libs` (cached), then
  build `speculative/01`–`08`, `13` and every example for `x86_64-pc-windows-gnu`, run the console ones under wine,
  and capture the examples with `golden.toml` under `xvfb-run wine` against their Linux goldens (`j3c golden` with a
  wine runner, or the script). `stdlib-tests.sh --windows` runs the `stdlib/tests` that pass under wine, from an
  allowlist that grows chunk by chunk (start with the 13 passing today, plus `win_io_test`).
- Windows job (`windows-latest`): `msys2/setup-msys2` with `mingw-w64-clang-x86_64-toolchain`, or an llvm-mingw zip
  plus the SDL kit, then `cargo build --release` and `cargo test --release`. Tests that need `sh`/`/tmp` are already
  `cfg(unix)` or must become so. Then `j3c build` + run of `speculative/01`–`07`. Runners have no GPU and their
  `opengl32.dll` is GDI GL 1.1, so GL runs need Mesa llvmpipe for Windows (`opengl32.dll` from mesa-dist-win beside the
  exe). Add that once the console path is green.
- Release: build `j3c.exe` (cross `--target x86_64-pc-windows-gnu` from the Linux job is fine, verified above) and
  attach `j3c-<ver>-x86_64-windows.zip` with checksums.
- Done when both jobs are green on GitHub, and plan 02 §7/§8 Windows boxes can be ticked.

### 14. Ship: Scoop/winget and docs

- Files: `packaging/scoop/j3c.json` (url, hash, `bin: j3c.exe`, a `depends`/notes line for llvm-mingw), a winget manifest
  once a release exists (`packaging/winget/`), `INSTALL.md` "Windows" (install llvm-mingw, unzip `j3c`, run `j3c doctor`,
  then `j3c -e fetch-mingw-libs`), `README.md` support line, `j3c doc ship` (Windows zip, GUI subsystem, icon,
  `crash.log`, SmartScreen note: unsigned exes warn; Authenticode signing is the developer's call), `roadmap.md`
  status lines. Tick plan 02 §6 Windows.
- Done when a fresh Windows VM gets from zero to `j3c new hello --app && j3c run --headless --frames 30 --screenshot s.png` by
  following `INSTALL.md` only (developer, see below).

### 15. ASLR back on (after hardware)

- Files: `j3c/src/emit/host/link.rs:50`, `target.rs` (Windows release flags).
- Try `-fuse-ld=lld` (when `ld.lld` or llvm-mingw is present) without `--disable-dynamicbase`. On real Windows, run a
  program that touches `#[thread]` globals from the main and worker threads (`13_threads`) under forced ASLR
  (`Set-ProcessMitigation -Name X.exe -Enable ForceRelocateImages`). Keep the flag for whichever linker still crashes.
- Done when DllCharacteristics shows `DYNAMIC_BASE | HIGH_ENTROPY_VA | NX_COMPAT` for lld links and `13_threads` passes
  on hardware.

## Testing without the hardware

- **wine + Xvfb is the loop** (verified 2026-11-11 for console exes, SDL/GL apps, `J3_CAPTURE`, the editor renderer and
  threads). Use it for every chunk. GL goes through wine's opengl32 to Mesa llvmpipe, so frames match Linux llvmpipe
  goldens. They matched exactly for the app template.
- Always: `WINEDEBUG=-all`, `timeout`, stdout to a file (not a pipe), `wineserver -k` afterwards,
  `J3_NO_PANIC_BOX=1` once chunk 1 lands, and `SDL_AUDIODRIVER=dummy`. Never run wine without `xvfb-run` for a windowed
  program, or a real window opens on the desktop.
- `x86_64-w64-mingw32-objdump -p X.exe`: imports, subsystem, DllCharacteristics, `.rsrc`.
  `x86_64-w64-mingw32-windres` is the oracle for chunk 4. `x86_64-w64-mingw32-nm` checks what libmingwex provides.
- `cargo build --target x86_64-pc-windows-gnu` (target installed) checks that the Rust side compiles, and `wine j3c.exe`
  runs the parts that need no clang.
- Wine cannot show: ASLR/TLS relocation crashes, real GPU drivers (pixel ownership, 1.1 entry points), DPI scaling,
  WASAPI/XInput specifics, Defender/SmartScreen, ConPTY edge cases, MSYS2 vs cmd vs PowerShell differences, or the Steam
  client. Those go below.

## Verify on real hardware (for the developer)

On a Windows 10 22H2 or 11 x64 machine, ideally one NVIDIA or AMD and one Intel iGPU, with a 150% display:

1. Unzip a release game zip (chunk 4+) and double-click it: no console window, the icon shows in Explorer and the
   taskbar, the window is sharp at 150% (DPI aware), and `game.drawableSize` is bigger than the window size when HiDPI is on.
2. A game in a folder with a non-ASCII name (`C:\Users\José\Spiel ä\`), saving to `%APPDATA%`: saves and loads.
3. GPU: `examples/3d-editor-scene` and `examples/desk-app` look like the Linux goldens. F4 stats show the real GL
   vendor, and a hidden-window `--headless` screenshot is not blank (decision 10).
4. Input/audio: keyboard layouts (AZERTY), an XInput pad and a PS pad through SDL, audio from `examples/...` with
   sound, and output-device switching.
5. A panic (`J3_FORCE_PANIC`-style test build, or a bad index in dev): a GUI exe shows the message box. Run from cmd it
   prints to stderr with no box. A crash writes `crash.log` beside the cwd and `j3c` symbolizes it (chunk 11).
6. `13_threads` with ForceRelocateImages (chunk 15).
7. `proc_test`, `net_test`, `tls_live_test` (real hosts, bad certs refused), `instance_test` (two copies), the hotkey
   (F8 while another app has focus), clipboard paste of a screenshot and of files copied in Explorer, and `examples/terminal`
   running `cmd.exe` and `pwsh`.
8. Host: install llvm-mingw, unzip `j3c`, follow `INSTALL.md`. Then `j3c new x --app`, `run`, `run --hot` (edit and save),
   `build --release`, `j3c editor`, `j3c -e mcp` + render from Claude Code, the VS Code extension (diagnostics, go to
   definition on `C:\` paths).
9. Steam: [plan 01](01-steam.md) item 7.
10. Defender/SmartScreen: does an unsigned zip game get flagged? Write down what players see.

## Out of scope

- An MSVC target (`x86_64-pc-windows-msvc`), Windows on arm64, 32-bit Windows, Windows 7/8 (the UTF-8 code page needs
  1903+).
- `dragFiles` (OLE drag source) and transparent GL windows on Windows: documented as Linux only (decision 15).
- An installer (MSI, Inno Setup, MSIX), Authenticode signing and the Microsoft Store. The zip and Steam are the channels.
- Switching every `*A` call to `*W` (decision 3), and Schannel (decision 9).
- Bundling clang/llvm-mingw inside the `j3c` zip.
