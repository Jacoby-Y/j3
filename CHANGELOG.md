# Changelog

All notable changes to J3 (`j3c`, its stdlib, editor, and tools). Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

**Versioning.** J3 follows semver-ish numbering. While the version is 0.x, any release may
break anything: the language, the stdlib, file formats (`.j3scene`, `.j3ui`, `j3.toml`), and
command-line flags. Each release lists its breaking changes under **Breaking**.

## [0.1.0] - 2026-11-10

First public release.

### Added
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
