# J3 public release readiness

Audit date: 2026-10-29. Build audited: `j3c 0.0.1`, docs `bcaa230c`, working tree with uncommitted changes on top of `f4229e4`.

**Score: 38/100 for a full public release.** About 60–65 as a Linux-only "early alpha, expect breakage" preview. The compiler is good; what holds it back is everything around it: license, platforms, docs, packaging, CI.

## What was checked

- `cargo test --release` on a scratch copy of `j3c/`: 398 passed, 2 failed.
- `j3c new`, `run` and `check` on a fresh project, plus deliberately broken code.
- README, `roadmap.md`, `plans/`, the platform `sys_*.j3` files and the repo layout.

## Scores

| Area | Score | Notes |
|---|---|---|
| Compiler core | 80 | No dependencies, ~59k lines of Rust plus ~101k of J3 stdlib. A new project builds and runs in 0.09s. Error spans are clean; runtime checks report `file:line` (exit 102). |
| Tooling | 75 | LSP, `fmt`, built-in `doc`/`recipes`, `new` templates, golden tests, `.deb`/zip shipping, VS Code extension. |
| Proof it works | 75 | Large real programs: fps-shooter, desk-app, idle-breakout. |
| Platforms | 15 | Linux only in practice. The Windows exe builds but has never been run; macOS is untested; `j3.net` and `j3.proc` are stubs off Linux. |
| Legal | 0 | No LICENSE file, so nobody can legally use it. 141 binary assets in examples with unchecked provenance. |
| Docs for humans | 30 | Docs are written for agents (`AGENTS.md`, MCP-first). `j3c doc lang` is 70 lines. No tutorial, language reference or site. |
| Install | 25 | Version 0.0.1, no release binaries. Needs Rust plus `clang` on PATH; `build.sh` installs into `~/.bin`. |
| Quality gates | 30 | No CI. Tests are red (below). 519 `.unwrap()` in `j3c/src` and no fuzzing, so bad input can probably crash the compiler. The leak in `memory-leak.md` is unresolved. |
| First-impression accuracy | 20 | 11 of the 12 examples the README lists don't exist (`bounce`, `dungeon`, `defense`, `island`, `minecraft`, `leaves`, `canvas`, `canvas_preview`, `raytrace`, `ocean`, `forest`). |
| Stability promise | 20 | No versioning or breaking-change policy. Language and formats still move; no packages/deps yet. |

## Found directly

- **Failing tests** (both look like fallout from uncommitted work, not real regressions):
  - `compile::tests::ir_goldens_01_through_08_and_samples`: golden `07_memory.ir` drifted; `remove` and `swapRemove` are missing from the output.
  - `doc::tests::recipes_check`: the `display` recipe loads `assets/font.ttf`, which doesn't exist.
- **One error at a time:** a file with two type errors reports only the first.
- **Number literal on the left:** `2 * far` fails where `far * 2` works (listed in `roadmap.md` under Language friction).
- **Repo hygiene:** six `.vsix` builds committed in `editors/vscode/`, a 59 MB `.git`, a large uncommitted diff, scratch docs at the root (`memory-leak.md`, `EPIC_COMPILER.md`, `j3ui-support.html`).

## Shortest path to a credible public alpha (~65)

1. Add a LICENSE and audit the example assets.
2. Get tests green; add GitHub Actions running `cargo test` and the `speculative/` programs.
3. Rewrite the README for humans: real examples, a 5-minute tutorial, a clear "Linux only for now".
4. Prebuilt Linux release binaries and a tagged `v0.1.0`.
5. Report several errors per check; fix the literal-on-left typing.

## Before a real 1.0 (~85+)

- Windows and macOS actually run and are tested.
- A human-facing language reference.
- A stability and breaking-change policy.
- Packages/deps (`roadmap.md`, Left).
- Fuzz the parser and checker.
- Close out the memory leak investigation.
