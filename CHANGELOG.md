# Changelog

All notable changes to J3 (`j3c`, its stdlib, editor, and tools). Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

**Versioning.** J3 follows semver-ish numbering. While the version is 0.x, any release may
break anything: the language, the stdlib, file formats (`.j3scene`, `.j3ui`, `j3.toml`), and
command-line flags. Each release lists its breaking changes under **Breaking**.

## [Unreleased]

### Breaking
- Steam left `j3c` and is now the `steam` package (`steam = { git = "URL", tag = "v0.1.0" }` under
  `[dependencies]`; `j3c doc steam`). `use j3.steam;` becomes `use steam;` (and `j3.steam.game` /
  `j3.steam.ui` become `steam.game` / `steam.ui`); the old path fails with a line saying what to add.
  `j3c -e steam-check --sdk DIR` becomes `j3c pkg steam check --sdk DIR` (the old command prints that
  and exits 1). The built-in `steam` dist output is now the package's (`[dist] build_outputs =
  ["steam"]` needs the dependency), and the `[steam]` table is read by the package, not `j3c`.
  `j3c run` no longer sets `SteamAppId` / `J3_STEAM_LIB`; the package's dev-build `steam.init` does.
  `j3c doc recipes steam` is the package's `examples/spacewar`.

### Fixed
- `.j3ui`: a UI with more than about 2000 distinct computed styles (easy with `class="a b{x}"`
  holes on many elements) no longer grows memory every frame. The style cache grows instead of
  being cleared when full.

## [0.1.0] - 2026-11-10

First public release.

### Added
- Windows (`x86_64-pc-windows-gnu`) backends with the same pub API: `j3.term.pty` on ConPTY, `j3.proc` on
  `CreateProcessW` (pipes, exit codes, cwd, `PATH` + `.exe`/`.bat`/`.cmd`), `j3.io.watch` on
  `ReadDirectoryChangesW`, `j3.instance` on a named pipe, `j3.game.hotkey` on `RegisterHotKey`.
  `Pty.waitReadable(ms)` on every platform is the reader thread's wait (no fd polling). `j3.io.watch`
  no longer links raw `syscall` on macOS or web (a stub there). Tested under wine (`win_os_test.j3`).
- `j3c`: an ahead-of-time compiler for the J3 game language, one Rust crate with no
  dependencies, compiling through clang to native Linux binaries (Windows, macOS, Android targets in progress).
- Language: structs, enums, generics, traits, closures, UFCS, `extend`, SIMD, `f16`, and GPU
  shaders written in J3 (`#[shader]`, compiled to GLSL).
- Standard library embedded in the binary: windows, input, audio, 2D painting, 3D scenes,
  physics, ECS, pathfinding, threads, files, saves, JSON, HTTP/TLS, processes, terminals.
- `.j3ui` components (Svelte-like markup plus a large CSS subset) with layout, events,
  bindings, accessibility, and style hot reload.
- 3D scene editor (`j3c editor`) with `.j3scene` text files, and an MCP server (`j3c -e mcp`)
  plus CLI twins so agents can build and render scenes.
- Scripting: `.j3s` scripts (AOT or VM) and j2 runtime scripts for player mods.
- Tooling: `j3c new` templates, `j3c doc` (stdlib reference, guides, recipes), `j3c lsp` and a
  VS Code / Cursor extension, `j3c fmt`, `j3c doctor` (checks clang, SDL2, GL, fonts).
- Testing: headless runs and screenshots (`--headless --frames N`), golden images
  (`j3c golden`), idle-cost checks (`J3_STEADY`).
- Shipping games: release builds, zip and `.deb` outputs, Steam depots and Steamworks
  bindings, Android apk.
- Distribution of `j3c` itself: release tarballs and `.deb` for x86_64 and aarch64 Linux,
  `install.sh`, and an AUR `j3c-bin` package.
