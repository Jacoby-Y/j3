# Plan 013: Editor and agent tools on Windows and macOS

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, do NOT update `plans/README.md`
> (the reviewer does that).
>
> **Drift check (run first)**: the tree has uncommitted work on top of
> `b086a5b`, so check by comparing the "Current state" excerpts below to the
> live files, not by SHA. On a mismatch, STOP.

## Status

- **Priority**: P3
- **Effort**: L
- **Risk**: HIGH for "done" (needs real Windows and macOS machines); LOW for the Linux-side refactors
- **Depends on**: none
- **Category**: portability
- **Planned at**: 2026-09-25
- **Progress (2026-10-25, plan 027 chunk 3)**: `j3c` itself passes `cargo check --target x86_64-pc-windows-gnu` and its release build links (`editor.rs` stderr clone split by `cfg(unix)`/`cfg(windows)`); the exe has not been run on Windows.

## Why this matters

The editor and the agent tools (`j3c -e scene|render|gen|mcp`) only work on Linux.
EDITOR.md "Still left > Other platforms" lists the blockers. The compiler already
cross-targets (`sys_windows.j3`/`sys_macos.j3`, target-aware link flags), so the work is in
the editor frontend, the host, and the runtime library loading.

## Current state

**libc straight from the editor.** `j3c/editor/src/util.j3:1-15`:
```j3
// Small text, file, and number helpers. The only module that talks to libc.
use lib "c" as libc {
    fn getenv(name: cstr): cstr;
    fn strlen(s: cstr): int;
    fn access(path: cstr, mode: i32): i32;
    fn unlink(path: cstr): i32;
    fn mkdir(path: cstr, mode: i32): i32;
    fn open(path: cstr, flags: i32): i32;
    fn lseek(fd: i32, offset: int, whence: i32): int;
    fn read(fd: i32, buf: ptr, count: int): int;
    fn close(fd: i32): i32;
}
```
The generated game file `behaviors.j3` also declares `getenv`/`strlen` from libc
(`j3c/src/editor.rs:786`).

**The stdlib already has per-OS sys modules.** `j3c/stdlib/j3/sys_windows.j3` exports
`write`, `open`, `openMode`, `read`, `close`, `lseek`, `mmapAnon`, `munmap`, `exit`, and
`panic` (lines 89-185). `sys.j3` is chosen by `[emit] target` (root AGENTS.md).

**Shared libraries by Linux soname.** `j3c/stdlib/j3/game.j3:211-221` (`soTry`/`soOpen`
via `dyn.dlopen`, declared at `:104`), with names like `"libSDL2_image-2.0.so.0"` (`:233`,
`:240`, `:247`, `:1198`, `:1222`, `:1241`, and panics at `:1981`, `:2239`, `:2374`,
`:2421`). `gfx.j3:13` has `use lib "GL" as gl` and gets entry points from `game.glProc`
(`gfx.j3:3`).

**Host paths.** `j3c/src/editor.rs`:
- the frontend exe is `target/j3_editor` with no `.exe` (`:937`, `:965`, `:1042`, `:1055`,
  `:1245`)
- the cache dir comes from `XDG_CACHE_HOME`/`HOME` (`:1009`)
- the file picker is `zenity` (`:1210-1222`, "ponytail: zenity, so Linux desktops only")

**The font is already bundled:** `editor.rs:56` `include_bytes!("../editor/font.ttf")`,
written next to the build (`:954`) and passed as `J3_EDITOR_FONT` (`:1248`). The EDITOR.md
"default font path" blocker may only apply to games' default font. Step 1 checks this.

**Link flags are already target-aware:** `j3c/src/emit/host.rs:255-280` (`link_gc_flag`,
`windows_target`, `foreign_target`).

## Design

1. **Move util.j3's libc calls behind the stdlib.** Add `j3.fs` (or extend `j3.io`) with
   `exists(path)`, `remove(path)`, `makeDir(path)`, `readAll(path, arena)`, and
   `env(name, fallback)`, implemented in `sys.j3`, `sys_windows.j3`, and `sys_macos.j3`
   (Windows: `GetFileAttributesA`, `DeleteFileA`, `CreateDirectoryA`, `GetEnvironmentVariableA`,
   or the CRT `_access`/`_unlink`/`_mkdir`/`getenv`, whichever the Windows sys module
   already links). util.j3 then uses those and drops `use lib "c"`. The generated
   `behaviors.j3` uses `j3.fs.env` too.
2. **Library names per OS.** `game.j3` gets `fn soName(base: string): string` giving
   `libSDL2-2.0.so.0` on Linux, `SDL2.dll` on Windows, and `libSDL2-2.0.0.dylib` on macOS
   (plus `SDL2_image.dll` etc.), picked by a const from the sys module
   (`sys.OS: int`). `dyn.dlopen`/`dlsym` become `LoadLibraryA`/`GetProcAddress` on Windows
   through the sys module. GL: `opengl32.dll` plus `wglGetProcAddress` on Windows, and
   `/System/Library/Frameworks/OpenGL.framework/OpenGL` on macOS (which caps at GL 4.1
   core; 3.3 core needs the forward-compatible flag, see `game.initGl`).
3. **Host paths.** An `exe_name("j3_editor")` helper adds `.exe` on Windows (use
   `std::env::consts::EXE_SUFFIX`). Cache dir: `LOCALAPPDATA` on Windows,
   `~/Library/Caches` on macOS, via a `cache_dir()` that already exists (extend it).
4. **File picker.**
   - Windows: `powershell -NoProfile -Command "Add-Type -AssemblyName System.Windows.Forms; $d=New-Object System.Windows.Forms.OpenFileDialog; $d.Filter='Models|*.gltf;*.glb;*.j3mesh'; if($d.ShowDialog() -eq 'OK'){$d.FileName}"`.
   - macOS: `osascript -e 'POSIX path of (choose file)'`.
   - Keep zenity on Linux, and fall back to "drop the file on the window" everywhere.
5. **MCP/scene/render/gen** only depend on the host exe path, the frontend build, and
   `render`'s hidden GL context, so they follow from 2-3. Render uses
   `SDL_VIDEODRIVER=offscreen` (EGL); on Windows/macOS use a hidden window instead (the
   fallback already exists, per the Agent use notes).

## Commands you will need

| Command | Purpose |
|---|---|
| `cd j3c && cargo build --release && cargo test --release` | build and tests |
| `timeout 120 ./target/release/j3c check <proj>` with `[emit] target = "x86_64-pc-windows-gnu"` in a temp copy's j3.toml | typecheck a project for Windows on Linux |
| same with `"arm64-apple-darwin"` | typecheck for macOS |
| `J3_EDITOR_SRC=editor/src timeout 120 ./target/release/j3c -e scene list <proj>` | the editor frontend still builds and works on Linux |

## Scope

In: design items 1-5 and Linux regression checks. What can be verified on Linux:
cross-target typechecks, unit tests for path/name helpers, and the unchanged Linux editor.
What needs real machines (done by the user or CI later): launching the editor, render,
picking files, and loading the runtime libraries.
Out: packaging and installers, code signing and notarization on macOS, and Metal/D3D
backends.

## Git workflow

Branch `plan-013-other-platforms` in the main checkout. One commit per design item.

## Steps

1. **Inventory.**
   - Grep the editor, host, and stdlib for Linux-only things: `use lib "c"`, `.so`, `/usr`,
     `/proc`, `HOME`, `zenity`, `xdg`, and `std::os::unix`. Put the list in this plan's
     Maintenance notes.
   - Check what `sys_windows.j3`/`sys_macos.j3` already provide and how they link
     (kernel32 vs CRT).
   - Confirm whether games have a hardcoded default font path.
   Verify: the list exists, and each item maps to a design item or a new one.
2. **Design 1 (fs and env in the stdlib), then port util.j3 and the generated behaviors.**
   Verify: the Linux editor builds (`J3_EDITOR_SRC=... -e scene list` works). Cross-target
   `j3c check` of a temp copy of `examples/3d-editor-scene` for Windows and macOS gets past
   util-related errors (other errors are expected until later steps).
   A J3 self-check `j3c/examples/fs_check.j3` (make a dir, write, exists, read, remove)
   passes on Linux.
3. **Design 2 (library names and loading).**
   Verify: the Linux runtime is unchanged (headless `j3c run examples/3d-editor-scene --frames 5 --screenshot /tmp/x.png --headless`
   works). A unit-style J3 check prints `soName("SDL2")` for each OS const (make the helper
   pure: pass the OS in).
4. **Design 3 (host paths).**
   Verify: Rust unit tests for `exe_name` and `cache_dir` per OS (make them take the OS and
   env as parameters for testability).
5. **Design 4 (file pickers).**
   Verify: a unit test that the command built for each OS is right (pure function returning
   program and args); Linux behavior is unchanged.
6. **Cross-target typecheck of the editor frontend itself** (Windows and macOS) through a
   temp copy of the frontend sources with an `[emit] target`.
   Verify: 0 errors.
7. **Docs and handoff.** EDITOR.md "Other platforms" becomes "Needs testing on a real
   machine" with a checklist for the user: editor opens, import picker, render, MCP
   `render`, and `play`.

## Test plan

`fs_check.j3` and the pure-helper tests (steps 2-5), cross-target typechecks (steps 2 and
6), and Linux regression runs after each step. The real-machine checklist goes in EDITOR.md.

## Done criteria

- The editor frontend and a sample game typecheck for Windows and macOS targets.
- Linux behavior is unchanged.
- The real-machine checklist is written.

## STOP conditions

- `sys_windows.j3` can't call Win32/CRT functions needed for fs/env without compiler
  changes to `use lib` on Windows.
- Cross-target typecheck needs SDK headers or libraries that aren't on this machine.
  Report which, and stop that part.

## Maintenance notes

- The inventory from step 1 goes here.
- Keep every OS-specific string in the sys modules or one helper per concern. No `if OS`
  checks spread through the editor.

Added 2026-10-21: a shipped Steam game also needs the game runtime (not just the editor) proven on Windows and macOS: SDL2/ttf/mixer and GL linking, the `.j3ui` + `j3.gfx.paint` path, HiDPI and fractional scale, file dialogs, and the crash handler (Linux x86-64 only today). Plan 025 depends on this.
