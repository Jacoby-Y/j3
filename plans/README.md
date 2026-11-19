# Plans

Handoff plans for open work. Finished plans are deleted; git history keeps them. Code comments that
say "plan 023" and the like point at those old, deleted plans.

| Plan | What | Status |
|------|------|--------|
| 01 | Steam: what to verify with the real SDK, client, Windows, Deck and sniper | MOVED to the `steam` package's `VERIFY.md` (`../j3-packages/j3-steam`) |
| [02](02-public-release.md) | Public release checklist: clone-ability, license, hygiene, README, installers, CI | IN PROGRESS |
| [03](03-ui-cpu.md) | `.j3ui` CPU per frame: layout boundaries, retained paint, cheaper restyles (stress scene) | DONE (pending review) |
| [04](04-windows.md) | Windows: run the exe, fill `sys`/`net`/`proc` stubs, Linux-only features, SDL DLLs, ship | TODO |
| [05](05-macos.md) | macOS: host triple bug, first run, GL, stubs, `.app` bundle, codesign, ship | TODO |
| [06](06-web.md) | Web as a platform package `j3web` (no Emscripten): `[platform]` hook in j3c, C/JS SDL subset over WebGL2, JSPI yield, wasm64 | TODO (needs `lld-18` from chunk 3) |
| [07](07-android.md) | Android: GL context loss, real device runs, touch/back, ship to Play | TODO |
| [version1](version1.md) | Public release readiness audit (2026-10-29): what blocks a 1.0 | TODO |
| [08](boostrap_features.md) | Language and stdlib features needed to self-host j3c (match exprs, patterns, char literals, builder, embed, collections, dyn, tuples) | DONE (open follow-ups listed in its Status line and `roadmap.md`) |
| [09](09-package-tools.md) | Package tools: `[commands]` (`j3c pkg NAME CMD`), `[dist_outputs]` hooks, manifest JSON; then all Steam out of `j3c` | DONE (follow-ups in its "Found while doing plan 10") |
| [10](10-steam-package.md) | The `steam` package at `../j3-packages/j3-steam`: bindings, `check` tool, depot dist output, dev env, tests | DONE (tag `v0.1.0`, local repo, no remote yet) |
| [11](11-pre-bootstrap.md) | Before porting j3c to J3: anti-footgun setup (pinned stage 0, diff harness, oracles, coverage, freeze), the new compiler's shape (driver, diags, ids, per-module work, pass/backend tables), port gates | C6 DONE 2026-11-18 (j3c in J3 is the installed `j3c`; tag `rust-final` keeps the Rust compiler); Part D open |
| [12](12-deferred-tools.md) | Tools left unported in j3c-boot (LSP server and scanners, MCP server, `j3c editor`, `-e android`, `-e web`, asset import): stubs, Rust to port, gate. Broken after plan 11 C6 until done | TODO |
| [13](13-speed.md) | Make j3c in J3 faster, smaller and lighter than Rust j3c on every bench metric (`j3c-bootstrap/tools/bench.sh`) | IN PROGRESS |
| [14](14-windows-j3c.md) | Ship `j3c.exe`: host triple, `.exe`/cache paths, `j3c setup` (llvm-mingw + SDL kit), cross-built release zip, `install.ps1`, Scoop, Windows CI | TODO |
| [15](15-retire-stage0.md) | Retire stage 0: seed from the installed `j3c` / a pinned release, freeze the Rust oracle into expected files, unit tests built by `target/j3c` (fast, no `J3_STDLIB`) | TODO (seed choice is the user's) |

Status values: TODO | IN PROGRESS | DONE | BLOCKED (with one-line reason). A new plan takes the next
number. Open direction lives in `roadmap.md` and `EDITOR.md`.

Since plan 11 C6 the compiler is J3 (`j3c-bootstrap/`) and nothing is built with cargo except the seed
(`j3c-bootstrap/tools/stage0.sh rust-final`). `./build.sh` installs `j3c`.
