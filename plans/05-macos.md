# Plan 05: macOS: host triple, run, bundle, ship

**Status (2026-11-11): TODO.** Host builds on a Mac cannot link today (Linux `sys.j3`, `.so` probes, GNU ld flags), and threads would silently not lock; chunks 1-6 get `j3c run` working, 7-9 ship `.app` bundles and `j3c` itself.

## Goal

On an Apple Silicon Mac (first class) and an Intel Mac (best effort), with Xcode command line tools and Homebrew SDL2:
`j3c new --app`, `j3c run`, `j3c run --headless --frames N --screenshot`, `j3c editor`, the MCP server, and `j3c build --release`
all work, and `build_outputs = ["app"]` writes a signed `.app` (zip) that runs on a Mac with nothing installed.
`j3c` itself ships as a universal tarball and a Homebrew formula. Linux CI catches macOS regressions without a Mac.

## Current state (verified)

Verified here = run on this x86-64 Linux box (clang 18.1.3, llvm-18 tools in `/usr/lib/llvm-18/bin`: `llvm-nm`, `llvm-lipo`,
`llvm-otool`, `llvm-install-name-tool`; no `ld64.lld`, no macOS SDK, no Rust std for `*-apple-darwin`, no osxcross, no darling).
Scratch projects in `/tmp/claude-1000/.../scratchpad/mac/`.

**Host triple (the cross-cutting bug). Confirmed, and worse than the triple string.**

- `j3c/src/emit/llvm.rs:62-66` and `:264-268`: `target triple` is `x86_64-pc-linux-gnu` when `[emit] target` is empty, and the
  `target datalayout` line (`:70`, `:272`) is the x86-64 ELF string **for every target**, including explicit Android/macOS/Windows.
  Verified: `j3c dump-llvm` of hello prints both; with `target = "arm64-apple-macos11"` the triple changes, the datalayout does not.
- That string by itself is harmless with clang 18: verified `clang --target=X -c host.ll` for `aarch64-unknown-linux-gnu`,
  `arm64-apple-macos11`, `x86_64-apple-macos10.15` prints `-Woverride-module` and rewrites **both** triple and datalayout
  (`-S -emit-llvm` shows `e-m:o-...` for Mach-O; `llvm-nm` shows `_`-prefixed Mach-O names). With no `--target` (a native
  build on a non-x86 host) clang uses its own default triple the same way. A file with no datalayout line compiles cleanly for all three.
  So explicit `--target` "masks" the datalayout, and a native aarch64 host gets correct-arch objects too.
- The real damage is the other places that read `""` as "Linux x86-64" (verified by reading):
  - `j3c/src/compile.rs:49-53` `os_kind("")` returns `OsKind::Linux` on every host, so a Mac host build loads `sys.j3`
    (raw `@syscall` with Linux amd64 numbers, `__errno_location`), `netsys.j3`, `instance.j3` (all x86-64 Linux syscalls).
    Verified: hello's host `.ll` declares `@syscall` and `@__errno_location`, the macOS-target `.ll` declares `@__error`,
    `@_NSGetExecutablePath`. libSystem has no `___errno_location`: **a Mac host `j3c run hello` fails to link** (unverified on a Mac, follows from the symbol).
  - `j3c/src/emit/host/target.rs:20-28` `target_os("")` *does* use `cfg!(target_os)`, so release flags and the loader disagree on a Mac.
  - `llvm_crash::supported(triple)` (`emit/llvm_crash.rs:6`) is fed the hardcoded triple, so every non-x86-Linux host gets the
    x86-64 Linux crash handler (ucontext rip at +168, Linux `sigaction`/`sigaltstack` layouts, Linux signal numbers) in debug builds.
  - `emit/host/target.rs:129` `link_gc_flag("")` = `-Wl,--gc-sections` (ld64 rejects it); `foreign_target("")` = false, so
    `emit/host/link.rs` probes `lib*.so` in Linux dirs (`cannot find library SDL2` on a Mac) and adds `-Wl,-rpath-link` (ld64 rejects it).
  - `compile.rs:36` `arch_kind("")` uses `std::env::consts::ARCH`: correct (the `j3c` binary's arch).
  - The C printer has no triple; clang/cc pick the host. It is hit by `os_kind` the same way.
- Same bug on a Windows host (`os_kind("")` = Linux): plan 04 depends on chunk 1.
- **aarch64 Linux** (plan 02 ships an `ubuntu-22.04-arm` prebuilt, `INSTALL.md` says "Linux (x86-64, arm64)"): chunk 1 fixes the
  triple and drops the x86 crash handler there, but `sys.j3`, `netsys.j3`, `instance.j3`, `threadsys.j3` hardcode **amd64 syscall
  numbers** (`sys.j3:14-40`; aarch64 has no `open`/`fork`/`access`/`dup2`/`pipe`/`poll` syscalls at all) and the amd64 `struct stat`
  layout. Programs built on aarch64 Linux are broken. Not owned here: see Out of scope.

**Loader and stdlib layers** (`j3c/src/compile/loader/paths.rs:5-55`, verified by reading)

| Module | macOS file | State |
|---|---|---|
| `j3.sys` | `sys_macos.j3` (395 lines) | libc. Real: `write`/`open`/`read`/`close`/`lseek`/`mmapAnon`/`munmap`/`exit`/`errno` (`__error`)/`dlopen`/`dlsym`/args/env/`exeDir` (`_NSGetExecutablePath`). Stubs: `monotonicMs` (0), `setThreadName`, `fork`, `execve`, `pipe`, `dup2`, `chdir`, `ioctl`, `setsid`, `canExec`, `statAt`, `lstatAt`, `dirents`, `rename`, `unlink`, `mkdir`, `waitpid`, `kill`, `getpid`, `pollFds`, `ignoreSigpipe`. Pub names match `sys.j3` (diffed). |
| `j3.threadsys` | `threadsys.j3` (the Linux file) | Uses `@syscall(131)` (Linux `sigaltstack`; Darwin 131 is `flock`), `sysconf(84)` (Darwin `_SC_NPROCESSORS_ONLN` is 58; `cores()` clamps to 1). Verified: desk-app's macOS objects import `_syscall`. |
| `j3.thread` `Mutex`/`Cond` | `thread.j3:54-86` | 40 / 48 bytes, "zeroed is ready". Darwin `pthread_mutex_t` is 64 bytes, `pthread_cond_t` 48, and a zeroed one has no signature (`_PTHREAD_MUTEX_SIG_init` 0x32AAABA7, cond 0x3CB0B1BB), so lock/wait return EINVAL and **do not lock**; the mutex also overruns by 24 bytes. From memory of Apple libpthread, unverified. |
| `j3.netsys` | `netsys_macos.j3` (124) | UDP real, "untested". TCP, `waitFd`, `connectTimeout`, timeouts, `noDelay`, listen/accept/recv/send, DNS `lookup`, `caFiles`: stubs. `netsys_android.j3` (369) is the libc port to copy. |
| `j3.net.tls` | `net/tls.j3:101,125` | dlopens `libssl.so.3`; exe-dir copy is Linux only. |
| `j3.proc` | `proc.j3` | No platform check of its own: works once `sys` fork/execve/pipe/dup2/chdir/waitpid/pollFds are real. |
| `j3.instance` | `instance_other.j3` | Stub (every copy alone). Linux uses an abstract Unix socket. |
| `j3.term` | `term/tty.j3`, `pty.j3`, `screen.j3` | Linux ioctl numbers (`TCGETS` 0x5401...), `/dev/ptmx`; `pty.j3:185`, `screen.j3:174` refuse non-Linux. |
| hotkey, dragout, clipboard types, `winframe` X11 bits | `game/hotkey.j3`, `dragout.j3`, `clipdata.j3`, `winframe.j3` | dlopen `libX11.so.6`, gated on `DISPLAY`: return `Err`/false/empty on a Mac (no XQuartz). Text clipboard is SDL and works. |
| SDL add-ons (ttf, image, mixer) | `game/addons.j3:135-185` | dlopened by Linux soname, then the Windows DLL name. No macOS row: fonts/images/sound **panic** on a Mac. |
| Crash handler | `emit/llvm_crash.rs` | x86-64 desktop Linux only (correct once fed the real triple). |
| Save dir | `data/save.j3:315` | macOS branch exists (`~/Library/Application Support`). |
| Steam | `steam/fns.j3:66` | `libsteam_api.dylib` name exists; `[dist] steam` is Linux/Windows only (plan 01). |

**ABI (verified)**

- Apple arm64 passes variadic arguments **on the stack** (verified: `clang --target=arm64-apple-macos11` of
  `open(p, 0x201, 0644)` stores the mode to `[sp]`). `sys_macos.j3` binds `open(path, flags, mode)` with fixed params and
  `netsys_macos.j3` binds `fcntl` the same, so on arm64 `mode`/`arg` are garbage. `ioctl` (pty, tty) is variadic too.
  `use lib` has no `...`; only the emitter prelude knows variadics (`emit/llvm.rs:470-500`, `PRELUDE_DECLS`, `variadic_call_ty`).
- C `int` returns bound as J3 `int` (i64): `open`, `close`, `munmap` in `sys_macos.j3` (and `sys_android.j3`). On arm64 and x86-64
  a 32-bit `-1` arrives as 0x00000000FFFFFFFF, so a failed `open` reads as fd 4294967295. `sys_android.j3` already binds its
  newer calls as `i32` ("so -1 reads as -1"); its `open`/`close` have the same bug (plan 07).
- x86-64 macOS: `stat`/`fstatat`/`readdir`/`fdopendir` with 64-bit inodes are the `$INODE64` symbols; the plain names are the
  old layout. arm64 has only the plain names. A J3 name cannot contain `$`; there is no `link_name`. From memory, unverified.

**GL, windows, DPI (verified by reading)**

- `game/window.j3:79-81`: 3.3 core, no `SDL_GL_CONTEXT_FORWARD_COMPATIBLE_FLAG`. macOS gives a 4.1 core context for any core
  request >= 3.2, and SDL2's Cocoa backend does not need the flag (from memory of SDL2 `SDL_cocoaopengl.m`, unverified).
- Every GL entry point is loaded through `SDL_GL_GetProcAddress` (verified: the macOS objects of the fps template import no
  `_gl*` symbol), all are GL <= 3.3 (no compute, SSBO, DSA, debug output, `glLineWidth`). Shaders: 32 `#version 330 core`,
  11 `#version 300 es`. Nothing above 4.1 is used.
- High DPI: `setHiDpi` adds `SDL_WINDOW_ALLOW_HIGHDPI` and scales by drawable/window ratio (`window.j3:230-275`); off by default,
  so 3D games render at point size on Retina. A bundle needs `NSHighResolutionCapable` for a Retina drawable; whether a bare
  binary gets one is unverified.
- Headless: `--headless`, goldens, `-e render`, MCP `play`/`render` set `SDL_VIDEODRIVER=offscreen` (`cli/mod.rs:322`,
  `golden.rs:138`, `scene/mod.rs:308`, `mcp/ops.rs:301`). SDL2's offscreen GL is EGL-only; a Mac has no EGL, so headless GL
  (and so the MCP render tools) fails there. Unverified on a Mac.
- `Key.Ctrl` drives shortcuts in 17 places (`ui/keys.j3:80`, `ui/imm_edit.j3`, `editor/src/input.j3`, ...). Mac users expect Command.

**Toolchain and packaging (verified by reading unless said)**

- Release flags `-Wl,-x -Wl,-S` for macOS (`target.rs:68`) are pinned by a unit test only; never run against ld64.
- Linking: `link.rs:62-71` passes `-lNAME` through for Apple triples; no Homebrew search dirs (`/opt/homebrew/lib` is not in
  Apple clang's default path), no `-F`/`-framework`, no rpath. Manifests and the editor's own `j3.toml`
  (`editor/frontend.rs:70`) list `GL`, which has no `libGL.dylib` on a Mac (dropping it is safe: see GL above).
- Cross build reaches the link: verified `j3c build` with `target = "arm64-apple-macos11"` compiles all 34 modules of the fps
  template and all of `examples/desk-app` to Mach-O arm64 objects, then fails only at link (`/usr/bin/ld: unrecognised emulation`).
  `--emit c` fails earlier (needs an SDK's headers).
- No `.app`, `Info.plist`, `.icns`, codesign, lipo or notarization code anywhere (grep). `dist.rs:122` names zips `macos-arm64`.
- `j3c` (Rust) itself: no `std::os::linux`; Unix bits are `#[cfg(unix)]`; Linux-only bits are gated (`phase.rs:214` ioctl,
  `editor/frontend.rs:12` `/proc/self/exe`, `incr.rs:48` ThinLTO, `target.rs:99` mold); `config.rs:21` has a macOS path.
  Not compiled for Darwin (no target std here): unverified.
- `j3c doctor` skips library/font/package checks off Linux (`doctor.rs:49`). `install.sh:15` refuses non-Linux. `tools/release/package.sh`
  and `.github/workflows/release.yml` are Linux-only; `ci.yml` has no macOS job. No Homebrew formula (`packaging/` has `aur/` only).
- Homebrew's `sdl2` formula may now be sdl2-compat over SDL3 (unverified); it should behave as SDL2 2.30+.

## Gaps and blockers

1. Host triple: `""` means "x86-64 Linux" in 6+ places (above). Blocks every native non-x86-Linux host, including Windows and aarch64 Linux.
2. Link line on a Mac: `.so` probing, `-rpath-link`, `--gc-sections`, `-lGL`, no Homebrew dirs, no rpath.
3. `sys_macos.j3`: 21 stubs, variadic `open`, i64 returns for C `int`, `$INODE64` on Intel, no pipe2 on Darwin.
4. Threads: wrong `Mutex` size, zeroed pthread objects invalid on Darwin, Linux syscall in `threadsys`.
5. SDL add-ons, TLS: no macOS library names or paths; `dlopen("/usr/lib/libssl.dylib")` aborts the process on modern macOS
   (Apple's "loading libcrypto in an unsafe way"), so TLS must never try that path.
6. Headless GL on a Mac (offscreen needs EGL). Blocks goldens, `-e render`, MCP on a Mac.
7. Shipping: `.app` layout, Info.plist, `.icns`, cwd (Finder launches with cwd `/`), bundled SDL frameworks, rpath, universal
   binary, ad-hoc and Developer ID signing, hardened runtime vs dlopen (library validation), notarization, Gatekeeper quarantine on zips.
8. `j3c` distribution: macOS release artifacts, install.sh, Homebrew tap, CI.
9. Blocker for full verification: no Mac. Linking needs an SDK (Xcode license limits it to Apple hardware) or CI on GitHub macOS runners.
   Notarization needs a paid Apple Developer ID (the developer).

## Design decisions

- **One resolved host triple.** `host_triple()` in `emit/host/target.rs` from `cfg!(target_os)` + `std::env::consts::ARCH`
  (same source as `arch_kind`, no process spawn): `arm64-apple-macosx11.0.0`, `x86_64-apple-macosx11.0.0`, `aarch64-unknown-linux-gnu`,
  `x86_64-pc-linux-gnu`, `x86_64-pc-windows-gnu` (match plan 04's host toolchain). Every OS/arch question about `""` goes through
  `resolve(target)`. `""` keeps meaning "native build" where that is the question (output dir `host`, `cc`, lib probing, ThinLTO, mold, `run`).
  Not `clang -print-target-triple`: under Rosetta it would disagree with `arch_kind`, and it costs a spawn per build.
- **Drop the `target datalayout` line.** clang fills the target's own (verified); one wrong constant gone, zero behaviour change.
- **macOS 11.0 floor** for both arches (arm64 starts at 11; one number; SDL2 2.30 supports it). Spelled in the default triple.
- **arm64 first class, x86-64 best effort.** Intel Macs are end-of-life; x86-64 gets built and lipo'd, tested in CI only.
- **Darwin libc layer, ported from `sys_android.j3` / `netsys_android.j3`** (libc calls, Darwin constants and struct offsets),
  not Darwin syscall numbers (unstable, `syscall()` is deprecated).
- **Variadics through the prelude**: add `open`, `fcntl`, `ioctl` to `PRELUDE_DECLS` as variadic (`i32 (ptr, i32, ...)` etc.), the
  mechanism `snprintf` already uses. No new `use lib` syntax. `$INODE64` symbols: `dlsym(RTLD_DEFAULT, "fstatat$INODE64")` once on x86-64
  (`if ARCH == ARCH_X86_64`), plain `use lib` names on arm64. Add a `link_name` attribute only if a third case appears.
- **Mutex/Cond**: grow `Mutex` to 8 words (64 bytes) on every OS (cost: 24 bytes per mutex). Keep "zeroed is ready": macOS
  `threadsys` CASes word 0 from 0 to the static-initializer signature (`@atomicCas`) before each pthread call; libpthread then lazy-inits.
- **GL stays.** 3.3 core maps to 4.1 core; nothing used needs more. No Metal path: it needs an MSL shader backend beside GLSL,
  a second renderer in `gfx`/`urp`, and a second UI painter, for a platform with no tester. If Apple removes GL, the cheap route is
  ANGLE (GLES on Metal) plus J3's existing ES 3.0 path (`J3_GLES`, `#version 300 es`), not Metal; ANGLE would also give headless EGL.
- **Headless on a Mac = hidden Cocoa window** (`SDL_WINDOW_HIDDEN`, real GL), chosen by the Rust side instead of `SDL_VIDEODRIVER=offscreen`.
  Needs a logged-in GUI session; GitHub's macOS runners have one (unverified). If a hidden window's back buffer reads back black,
  capture through an FBO.
- **Ship with the official SDL2 `.framework`s** (libsdl-org release dmgs: universal, self-contained, install name
  `@rpath/SDL2.framework/...`), cached in `~/.cache/j3/sdl2-macos/` like `sdl2-mingw`. Dev uses Homebrew dylibs (absolute install
  names, so dev runs need no rpath). Homebrew dylibs are single-arch and pull freetype/harfbuzz/png from `/opt/homebrew`; bundling them is a dead end.
- **`.app` is a dist output** (`build_outputs = ["app"]`), zipped as `<name>-macos-<arch|universal>.zip`; `zip` stays for a bare folder.
  Two macOS triples in `target` are lipo'd into one universal executable.
- **Signing**: always ad-hoc (`codesign -s -`) after any `install_name_tool`/lipo on a Mac host (arm64 refuses to run unsigned code;
  ld64 signs only its own output). Developer ID via `[dist] macos_identity`; hardened runtime; entitlement
  `com.apple.security.cs.disable-library-validation` only when the bundle dlopens a library signed by another team (Steam).
  Notarization is documented (`xcrun notarytool submit --wait`, `xcrun stapler staple`), not automated: one command, needs the developer's credentials.
- **Command = primary modifier**: `Key.Primary` (Ctrl, or Command on macOS); UI and editor shortcut sites move to it. Games keep raw `Key.Ctrl`.
- **`j3c` on macOS ships universal** (`cargo build` for both darwin targets on one arm64 runner, `lipo`), as a tarball and a Homebrew tap.
  `curl | sh` downloads are not quarantined, so the CLI needs no notarization at first.

## Chunks

Each chunk ends with `cargo test` green in `j3c/` (with `J3C_NO_INSTALL=1`) and no change to Linux x86-64 output except where stated.

1. **Host triple** (cross-cutting; plan 04 and plan 02's aarch64 build depend on it).
   Files: `j3c/src/emit/host/target.rs` (`host_triple()`, `resolve()`, `foreign_target`/`link_gc_flag`/`keep_flag`/`windows_target` and
   `target_os("")` go through `resolve`), `j3c/src/compile.rs:36-60` (`os_kind`/`arch_kind` use `resolve`), `j3c/src/emit/llvm.rs:62,264`
   (triple = `resolve`, drop the datalayout lines `:70`, `:272`), `j3c/src/emit/host/link.rs` (the `.so` probe and `-rpath-link` only when
   `resolve(target)` is Linux), `j3c/src/emit/llvm_crash.rs` (fed the resolved triple; no code change if callers pass it).
   Keep `host_triple` a pure function of `(os, arch)` so tests can call `triple_for("macos", "aarch64")` on Linux.
   Done when: unit tests show that a macOS host resolves to `sys_macos.j3`/`netsys_macos.j3`/`instance_other.j3`, `-Wl,-dead_strip`, no
   `-rpath-link`, no crash handler; an aarch64 Linux host gets the `aarch64-unknown-linux-gnu` triple and no crash handler; x86-64 Linux
   `dump-llvm` of hello differs from today only by the missing datalayout line. Note in `j3c/AGENTS.md` (emit paragraph) and the
   `os_kind` comment that `""` resolves to the host.

2. **Linux-side macOS regression checks** (do this before 3-6 so they have a net).
   Files: `j3c/src/compile/tests/targets.rs` (extend `macos_hello_uses_libc`). For `arm64-apple-macos11` and `x86_64-apple-macos11`, emit
   LLVM for hello, `speculative/0*`, a `j3.thread` program, a `j3.proc` program, the fps template and the editor frontend; assert the IR
   has no `@syscall`, `@__errno_location`, `@pipe2`, `@getdents64`, `"libX11`, and (after chunk 3) that `open`/`fcntl`/`ioctl` calls use
   the variadic call type. Optional second test (skips when `llvm-nm` is missing): compile the `.ll` with `clang --target=... -c` and check
   `llvm-nm -u` against an allowlist (libSystem names seen today plus `SDL_*`), which catches a new libc dependency.
   Done when: the tests fail on today's `threadsys.j3` (`@syscall`) and pass after chunk 4; they run in Linux CI with no SDK.

3. **`sys_macos.j3` filled in.**
   Files: `j3c/stdlib/j3/sys_macos.j3`, `j3c/src/emit/llvm.rs` `PRELUDE_DECLS` (+ the matching C prelude in `emit/c.rs` if the C backend
   declares them), `j3c/stdlib/j3/netsys_macos.j3` (`fcntl` binding only). Model: `sys_android.j3`.
   - Bind C `int` returns as `i32` (`open`, `close`, `munmap`, ...) and widen with `int(...)`.
   - `monotonicMs`: `clock_gettime_nsec_np(CLOCK_UPTIME_RAW = 8)` / 1e6. `setThreadName`: `pthread_setname_np(name)` (one arg on Darwin).
   - `fork`, `execve`, `dup2`, `chdir`, `setsid`, `kill`, `getpid`, `waitpid`, `access` (`canExec`), `rename`, `unlink`, `mkdir`, `poll`
     (`pollFds`), `signal(SIGPIPE=13, SIG_IGN)` (`ignoreSigpipe`), `ioctl` (variadic): libc, as in `sys_android.j3`.
   - `pipe(fds, flags)`: no `pipe2` on Darwin: `pipe` then `fcntl(F_SETFD=2, FD_CLOEXEC=1)` / `fcntl(F_SETFL=4, O_NONBLOCK=4)` per flag.
   - `statAt`/`lstatAt`: `fstatat(dir, path, buf, flags)` with Darwin `AT_FDCWD = -2`, `AT_SYMLINK_NOFOLLOW = 0x20`; 64-bit-inode
     `struct stat` (144 bytes): `st_mode` u16 @4, `st_mtimespec.tv_sec` @48, `st_size` @96. x86-64: `fstatat$INODE64` via `dlsym`.
   - `dirents(fd, buf)`: `fdopendir(dup(fd))` + `readdir` (Darwin `struct dirent`: `d_reclen` u16 @16, `d_namlen` u16 @18, `d_type` @20,
     `d_name` @21), repacked into the `linux_dirent64` records `io.listDir` parses (`d_reclen` @16, name @19). Keep the `DIR*` per fd in a
     small table; `closedir` when `readdir` returns null. x86-64: `$INODE64` names via `dlsym`.
   - Darwin flag values for every `O_*` the pub API exposes (`O_CLOEXEC = 0x1000000`, `O_NONBLOCK = 4`, `O_NOCTTY = 0x20000`, `O_DIRECTORY = 0x100000`).
   - `start()`: if `exeDir()` ends in `.app/Contents/MacOS`, `chdir` to `../Resources` before `main` (Finder starts apps in `/`).
   Done when: chunk 2's checks pass; `j3c check` with a macOS target passes for `stdlib/tests/proc_test.j3` and `io` users; the IR shows
   the variadic `open`. Real run: hardware steps 2-3.

4. **Threads on Darwin.**
   Files: `j3c/stdlib/j3/threadsys_macos.j3` (new), `j3c/src/compile/loader/paths.rs:28-34` (`OsKind::Macos => "threadsys_macos.j3"`),
   `j3c/src/stdlib/mod.rs` (embed list), `j3c/src/doc/mod.rs:26` (`HIDDEN`), `j3c/stdlib/j3/thread.j3` (`Mutex` to 8 words, doc comment).
   `lock`/`tryLock`/`wait` CAS word 0 of the mutex from 0 to 0x32AAABA7 and of the cond from 0 to 0x3CB0B1BB first; `enter`/`leave`
   use libc `sigaltstack` with Darwin `stack_t` (`ss_sp` @0, `ss_size` @8, `ss_flags` i32 @16; `SS_DISABLE = 4`); `sysconf(58)`.
   Done when: `@syscall` is gone from macOS IR of a thread program (chunk 2 test); `stdlib/tests/thread*_test.j3` still pass on Linux.

5. **Runtime libraries and input on macOS.**
   Files: `j3c/stdlib/j3/game/addons.j3` (macOS row: `libSDL2_ttf-2.0.0.dylib` etc.; try `exeDir/../Frameworks/SDL2_ttf.framework/SDL2_ttf`,
   `exeDir/<dylib>`, `/opt/homebrew/lib/<dylib>`, `/usr/local/lib/<dylib>`, then the leaf name; `soBeside` not Linux-only),
   `game/events.j3:592` (skip the PRIMARY lookup off Linux/Android), `net/tls.j3` (macOS: `exeDir` copy, `/opt/homebrew/opt/openssl@3/lib/libssl.3.dylib`,
   `/usr/local/opt/openssl@3/lib/libssl.3.dylib`; never `/usr/lib/libssl*`), `netsys_macos.j3` (port TCP, timeouts, listen/accept/recv/send,
   `getaddrinfo` from `netsys_android.j3`: `sin_len` byte, `AF_INET6 = 30`, `SOL_SOCKET = 0xffff`, `SO_RCVTIMEO = 0x1006`,
   `SO_NOSIGPIPE = 0x1022` instead of `MSG_NOSIGNAL`, `EINPROGRESS = 36`, `EAGAIN = 35`, `struct addrinfo` has `ai_canonname` @24
   **before** `ai_addr` @32; `caFiles`: `/etc/ssl/cert.pem`), `game/keys.j3` + `ui/keys.j3` + the 17 `Key.Ctrl` shortcut sites (`Key.Primary`).
   Done when: chunk 2 IR checks pass for a ttf/image/mixer program and `net_test.j3` under a macOS target; Linux behaviour unchanged.

6. **Link line, GL, headless on a Mac host.**
   Files: `j3c/src/emit/host/link.rs` + `target.rs` (`macos_lib`: `GL` dropped, `dl`/`pthread`/`c`/`m` kept; `-L/opt/homebrew/lib` and
   `-L/usr/local/lib` when they exist; `-F` for `~/.cache/j3/sdl2-macos` in release; `-Wl,-rpath,@executable_path/../Frameworks` for Apple
   triples), `j3c/src/emit/host/hints.rs` (brew install hint for a missing `-lSDL2`), `j3c/src/doctor.rs` (macOS rows: `xcrun --find clang`,
   SDL2 dylibs in the brew dirs, `brew install sdl2 sdl2_ttf sdl2_image sdl2_mixer`), the four `SDL_VIDEODRIVER=offscreen` sites
   (`cli/mod.rs:322`, `golden.rs:138`, `scene/mod.rs:308`, `mcp/ops.rs:301`: on a macOS host set `J3_HIDDEN_WINDOW=1` instead) and
   `game/window.j3` (`J3_HIDDEN_WINDOW` adds `SDL_WINDOW_HIDDEN`).
   Done when: unit tests pin the macOS link args for an Apple triple (no `-lGL`, no `-rpath-link`, `-dead_strip`, the rpath);
   `doctor` prints macOS rows. Real check: hardware steps 1-5.

7. **`.app` bundle output.**
   Files: `j3c/src/dist/app.rs` (new), `j3c/src/dist.rs` (dispatch, skip zip-of-folder for app-only), `j3c/src/project.rs` (`Output::App`,
   `[dist] bundle_id` default via the existing `default_java_package`-style helper, `macos_identity`), `tools/fetch_sdl2_macos.sh` (official dmgs ->
   `~/.cache/j3/sdl2-macos/*.framework`; `hdiutil` on a Mac, `7z x` on Linux), `j3c/src/doc/pages/ship.md`, `j3c help manifest` text.
   Layout: `<Name>.app/Contents/{Info.plist, MacOS/<name>, Resources/<[dist] include>, Resources/<name>.icns, Frameworks/SDL2*.framework}`.
   Info.plist: `CFBundleExecutable`, `CFBundleIdentifier`, `CFBundleName`, `CFBundleShortVersionString`/`CFBundleVersion` = `[package] version`,
   `CFBundleIconFile`, `CFBundlePackageType APPL`, `LSMinimumSystemVersion 11.0`, `NSHighResolutionCapable true`. `.icns` written by hand
   (header + one `ic08`/`ic09`/`ic10` entry holding the PNG as is, by its size; `[dist] icon` or `LOGO_PNG`). Two Apple triples built: `lipo -create`
   (`lipo` or `llvm-lipo`) into one exe. Frameworks copied only for libs the program links or dlopens. On a Mac host: `codesign --force -s -`
   (or the identity with `--options runtime --timestamp`, entitlements file when `[steam]` is set) on each framework, then the app. Zip with
   symlinks kept (`zip -y`; `ditto -c -k --keepParent` on a Mac).
   Done when: unit tests check the tree, Info.plist text, icns bytes (magic, length, entry type), and that a Linux host builds the bundle unsigned
   with a note that it must be signed on a Mac.

8. **`j3c` on macOS: CI, release, install.**
   Files: `.github/workflows/ci.yml` (job `macos-14`: `brew install sdl2 sdl2_ttf sdl2_image sdl2_mixer`, `cargo build --release`,
   `cargo test --release` (expect a first round of Linux-assuming tests to mark `#[cfg(target_os = "linux")]`), `j3c run` hello, a headless
   screenshot of the `--app` template), `.github/workflows/release.yml` (macOS job: both darwin targets, `lipo`), `tools/release/package.sh`
   (Darwin: `j3c-<ver>-universal-macos.tar.gz`, no `.deb`), `install.sh:15` (Darwin branch), `INSTALL.md` macOS section, `packaging/homebrew/j3c.rb`
   (formula template for a tap: the release tarball + sha256, `depends_on "sdl2"` etc. as recommended; Homebrew item in plan 02 points here).
   Done when: the workflows parse (`actionlint` if installed), `package.sh` logic for Darwin is a no-op on Linux, and the CI job is green on
   the first GitHub run (developer).

9. **Later, each its own small chunk once 1-8 run on a Mac** (not required for a first release):
   `j3.instance` on a socket file in `$TMPDIR` (`instance_macos.j3`); `j3.term` tty/pty (`tcgetattr`/`tcsetattr` with the 72-byte Darwin
   `termios` whose flags are `unsigned long`, `TIOCGWINSZ = 0x40087468`, `posix_openpt`/`grantpt`/`unlockpt`/`ptsname`) and lift the
   `PLATFORM_LINUX` gates in `pty.j3`/`screen.j3`; a Darwin crash handler (`__ss.__pc` in the arm64 `mcontext`, Darwin signal numbers) with
   `atos`/`llvm-symbolizer` in `host/symbolize.rs`; ThinLTO through ld64 (`incr.rs:48`); global hotkey, image/file clipboard and drag-out
   through `objc_msgSend` (Carbon `RegisterEventHotKey`, `NSPasteboard`, `NSDraggingSession`).

## Testing without the hardware

- Chunk 2's IR tests run in plain `cargo test` on Linux. `j3c build` with `target = "arm64-apple-macos11"` compiles every module to Mach-O
  today and fails only at link: use it as a smoke test (`j3c check` + the object stage), and `llvm-nm -u target/.j3c/<triple>/<name>/obj/*.o`
  to see what libSystem/SDL must provide.
- `clang --target=arm64-apple-macos11 -S` of a C snippet shows the real Darwin ABI (that is how the variadic `open` was confirmed).
  Struct offsets in chunks 3-5 should each be cross-checked the same way once an SDK header is at hand (CI: `clang -E` on the runner), never trusted from memory.
- Linking from Linux would need `ld64.lld` (Ubuntu `lld-18`, not installed) and libSystem `.tbd` stubs; Apple's SDK may not be used on
  non-Apple hardware, and hand-written `.tbd` stubs are more upkeep than a CI runner. Use GitHub's `macos-14` (arm64) runner for real links,
  runs and headless screenshots; x86-64 is the same runner via `--target x86_64-apple-macos11` under Rosetta.
- Bundle tests (chunk 7) are pure file-tree checks on Linux; `llvm-lipo -info` and `llvm-otool -L` (installed) inspect a CI-built binary.

## Verify on real hardware (for the developer)

1. Apple Silicon Mac, Xcode CLT (`xcode-select --install`), `brew install sdl2 sdl2_ttf sdl2_image sdl2_mixer`, Rust. `cargo build --release`
   in `j3c/`, `j3c doctor`, then `cargo test --release`; send the failures.
2. `j3c new hello && cd hello && j3c run` (link + libc layer). `j3c new w --app && j3c run w` (window, input, SDL), then
   `j3c run w --headless --frames 30 --screenshot s.png` (hidden-window headless).
3. `speculative/01`-`08`, `stdlib/tests` thread, proc, io (listDir), net and tls tests (`tools/ci/stdlib-tests.sh`).
4. `j3c new g --template fps`: GL 4.1 core context (`J3_GL_INFO=1` prints it), shaders compile, Retina with and without `setHiDpi`.
5. `examples/desk-app` with `setHiDpi`: sharp text, Command+C/V/A/Z in text boxes. `j3c editor examples/editor_village`, then the MCP server's `render` and `play`.
6. `j3c build --release` with `build_outputs = ["app"]` and both Apple triples: `lipo -info`, `codesign -dv --verbose=4`, `spctl -a -vv`,
   double-click from Finder (cwd, fonts, assets, save dir), copy to a second Mac without Homebrew.
7. With a Developer ID: `macos_identity`, `xcrun notarytool submit dist/<zip> --wait`, `xcrun stapler staple`, download through a browser
   (quarantine) and open. With `[steam]`: the bundle still loads `libsteam_api.dylib` under the hardened runtime.
8. Intel Mac or Rosetta (`arch -x86_64`): steps 2-4 with the x86-64 slice (the `$INODE64` paths).

## Out of scope

- **aarch64 Linux `sys` layer**: found here (amd64 syscall numbers and `struct stat` in `sys.j3`, `netsys.j3`, `instance.j3`, `threadsys.j3`),
  but it is a Linux fix, and plan 02's arm64 prebuilt is already shipped by the release workflow. It needs its own plan (or a plan 02 item):
  `if ARCH == ARCH_AARCH64` syscall tables (`openat`, `clone`, `faccessat`, `dup3`, `ppoll`, `renameat`, `unlinkat`, `mkdirat`, the generic
  `struct stat`) or a libc layer shared with this plan's Darwin port. Until then INSTALL.md overstates arm64 Linux.
- The `open`/`close` i64-return bug in `sys_android.j3` (same as chunk 3's): plan 07.
- Metal, MoltenVK, SDL3 (see Design decisions: ANGLE first if GL goes away).
- Steam on macOS (depots, overlay in a bundle): plan 01 lists macOS depots out of scope; chunk 7 only keeps the entitlement hook.
- iOS, the Mac App Store (sandbox entitlements), automated notarization, a `.dmg` output, Sparkle-style updates.
- Intel-only test hardware: x86-64 is CI-tested best effort.
