# Plans

Handoff plans for open work. Finished plans are deleted; git history keeps them. Numbers were compacted on
2026-11-19, so code comments citing a plan by number (`plan 023`, `plan 11 B8`, `plan 08 (bootstrap features)`)
may point at an old, deleted plan; check `git log -- plans/` for the name.

| Plan | What | Status |
|------|------|--------|
| [01](01-public-release.md) | Public release checklist: clone-ability, license, hygiene, README, installers, CI | IN PROGRESS |
| [02](02-windows.md) | Windows: run the exe, fill `sys`/`net`/`proc` stubs, Linux-only features, SDL DLLs, ship | TODO |
| [03](03-macos.md) | macOS: host triple bug, first run, GL, stubs, `.app` bundle, codesign, ship | TODO |
| [04](04-web.md) | Web as a platform package `j3web` (no Emscripten): `[platform]` hook in j3c, C/JS SDL subset over WebGL2, JSPI yield, wasm64 | TODO (needs `lld-18` from chunk 3) |
| [05](05-android.md) | Android: GL context loss, real device runs, touch/back, ship to Play | TODO |
| [06](06-deferred-tools.md) | Tools left unported in j3c (LSP server and scanners, MCP server, `j3c editor`, `-e android`, `-e web`, asset import): stubs, Rust to port, gate | TODO |
| [07](07-speed.md) | j3c in J3 faster, smaller and lighter than Rust j3c on every bench metric (`j3c-bootstrap/tools/bench.sh`) | IN PROGRESS (phase 4: widen thin margins, final review) |
| [08](08-windows-j3c.md) | Ship `j3c.exe`: host triple, `.exe`/cache paths, `j3c setup` (llvm-mingw + SDL kit), cross-built release zip, `install.ps1`, Scoop, Windows CI | TODO |

Status values: TODO | IN PROGRESS | DONE | BLOCKED (with one-line reason). A new plan takes the next
number. Open direction lives in `roadmap.md` and `EDITOR.md`; follow-ups from deleted plans are in `roadmap.md` **Left**.
