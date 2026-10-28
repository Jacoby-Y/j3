# Plan 023 restart handoff

## Resume here (third run paused, 2026-10-22)

Everything below this section describes the first pause and is history; this section wins.

- Done in the third run, all Sonnet, each with self-review plus an independent Sonnet review: 6d (review), 7a–c, 8a–c,
  9a–b, 10, 11a–b, 12a–b, 15b. One-line notes per chunk in [`023-chunks.md`](023-chunks.md); Done notes per stage here
  in [`023-j3ui-app-readiness.md`](023-j3ui-app-readiness.md).
- **Next: 15a**, then 15c1, 15c2, 15d, 15e, 15f, 12c, 14, then one final review-only pass (full build + tests,
  cross-chunk consistency, file sizes). **13 (a11y) and 16 (gamepad) are deferred: do not implement unless the user asks.**
- **Rebuild the release binary first**: the embedded stdlib predates 15b's last edits (its checks ran via `J3_STDLIB`).
- Baseline at pause: full cargo test 340 passed / 10 known failures (list below). Memory: `ulimit -v 16000000`
  (4 GB aborts cargo test and the example goldens), cargo `-j 4`, tests `-- --test-threads=4`.
- Per-chunk snapshots lived in the third run's session scratchpad and are gone; review new work against fresh copies.
- No commits were made. The working tree holds lots of unrelated uncommitted work; leave it alone.

### Brief given to every executor in the third run (reuse it)

Repo /home/jacoby/Documents/code/ai-stuff/j3: J3, an AOT game language (`j3c`, Rust crate, no deps) with `.j3ui`
components (Svelte-like markup + CSS) compiled by `j3c/src/ui/` (markup.rs parse, gen.rs codegen to J3, pass.rs, span
map) onto the runtime in `j3c/stdlib/j3/ui/` (dom.j3 layout/runtime, css*.j3, inputs, overlays, ...). Plan:
`plans/023-j3ui-app-readiness.md` — read its intro and your stage; Done notes of earlier stages describe the seams.
`plans/023-chunks.md` has one-line notes per finished chunk. Read `AGENTS.md` and `j3c/AGENTS.md` (rules: no crates,
no libLLVM, no second parser). User docs: `j3c doc j3ui` / `j3c doc css` text lives in `j3c/src/doc.rs`; update it
for anything users see. Fill in a short **Done (<chunk>)** note under your stage in the plan, like earlier ones.

### Commands (every one under `ulimit -v 16000000`; capture real rc: `cmd > log 2>&1; echo rc=$?`)
- Build: `env RUSTC_WRAPPER= timeout 600 cargo build --release -j 4 --manifest-path j3c/Cargo.toml`
- UI compiler tests: `env RUSTC_WRAPPER= RUST_MIN_STACK=67108864 timeout 600 cargo test --release -j 4 --manifest-path j3c/Cargo.toml --lib ui:: -- --test-threads=4`
- Full tests: same without `--lib ui::`, with `J3_STDLIB` UNSET (4 tests fail when it is set). Expect only the known failures.
- Runtime: `export J3C=$PWD/j3c/target/release/j3c J3_STDLIB=$PWD/j3c/stdlib SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy __GL_SHADER_DISK_CACHE=0`, then `timeout 180 "$J3C" run j3c/stdlib/tests/<x>_test.j3`. Stdlib is embedded in the binary: rebuild after stdlib edits, or rely on J3_STDLIB.
- Generated apps: `"$J3C" run <dir> --headless --frames N`.
- Goldens: `timeout 600 "$J3C" golden examples/desk-app` (and `examples/idle-breakout`, `speculative/26_ui_grid`, `25_ui_rich`, `21_ui_select`); states in each `golden.toml`. Never touch `~/.bin/j3c`. `speculative/22_ui_popups` fails its six states (known AA drift).

### Known failures before this run (not yours)
Full cargo test: 340 passed, 10 failed, all `compile::tests::`: bounce/defense/dungeon/island/minecraft/raytrace
project typechecks, project_emit_cwd_is_package_root, target_list_picks_a_sys_file_per_triple,
windows_dungeon_checks_and_missing_sdl_is_a_link_error, ir_goldens_01_through_08_and_samples.
`speculative/22_ui_popups` goldens differ at AA edges (environmental); 4 `speculative/21_ui_select` goldens drift ≤2
(use `--tol 2`). Runtime suites in `j3c/stdlib/tests/` all pass (dom 245, grid 146, input 359, overlay 272, ...).

### J3 pitfalls
A `string` interpolated inside a function and returned is garbage after return (return a view of a `Text`).
No bitwise ops. No capturing closures; callbacks
are module fns with data in a module `let`. A let/local named like generated code (`c2`) is rejected in .j3ui.

### Testing — headless only
Never open a window, steal focus, or play audio. Wrap everything in `timeout`. If something needs a real window,
don't launch it; say so.

### Snapshot (before any edit)
`D=<your chunk dir>; mkdir -p $D && touch $D/.start && (git stash create || git rev-parse HEAD) > $D/.sha`.
Before your first edit to any existing file: `cp --parents <file> $D` (path relative to repo root). Every file.
Never commit, stash for real, reset or clean. The tree has lots of unrelated uncommitted work: leave it alone.

### Report format
Brief, no code or diffs. First run `find . -newer $D/.start -type f -not -path '*/.git/*' -not -path '*/target/*'`
and check every source file listed is in $D or new.
- **Snapshot:** $D, and any changed file you failed to copy.
- **What I did:** 2-5 lines: approach and decisions.
- **Where:** each changed file, functions (file:line ranges), rough line count. Mark new files **new**.
- **Review here:** tricky spots, assumptions, edge cases handled or not, anything unsure.
- **Tests:** commands, pass/fail, counts. What's untested and why.
- **For later chunks:** what later chunks must know.
Expect a self-review round after this.


Updated 2026-10-21 after the user paused execution, then requested documentation for a harness restart.
This documentation update does not resume implementation. No agents are running and no commits were made.

## Resume here (first pause, 2026-10-21 — history)

Stages **1–3 are complete**. Chunks **3b, 3c and 3d passed self-review and independent greater-tier review**;
each final review was CLEAN. **Chunk 4 is implemented but NOT review-complete.** The user paused for usage
limits during its self-review. The executor was interrupted; no independent chunk-4 review has run.

Read the repo's `AGENTS.md`, `j3c/AGENTS.md`, then:

1. [`023-chunks.md`](023-chunks.md): authoritative completion checklist and execution order.
2. [`023-j3ui-app-readiness.md`](023-j3ui-app-readiness.md): current numbered stages and detailed Done notes.
3. [`023-done.md`](023-done.md): historical archive using **old** stage numbers; old stage 10 popup limits are superseded.

Resume chunk 4 with self-review, then an independent greater-tier full review; keep its checkbox unchecked
until both finish. Read Stage 4's implementation notes and inspect the live workspace for any partial self-review
edits. The implementation normalizes dynamic tags before shared scans, generates ordered candidate enums,
and uses typed `@uiChoice(SetEnum, selector, armIndex)` equality lowering because J3 forbids ordinary overloads.
Only int or the exact enum is accepted; invalid indices render no arm. Review AST traversal, selector validation,
source mapping, naming collisions, slots/passthrough, keyed lifecycle and steady/reentry allocation carefully.

Implementer-reported checks before self-review: release build PASS; UI 55, parser 28, checker 12 tests PASS;
grammar checks PASS; `speculative/24_ui_dynamic` 14 assertions PASS; fixtures 14–20 PASS counts
14/12/42/6/24/32/13. These claims have not had independent chunk-4 review. No full-suite rerun after implementation.
Snapshot/report evidence: `/tmp/orchestrate/023/chunk-4/` (14 existing files copied, nine new fixture files;
`commands.txt`, `audit.txt`, `review.diff`, logs). Preserve `speculative/24_ui_dynamic` too.

Fresh resume baseline: release build PASS; full tests **304 passed, 10 failed, 1 ignored**, with the same
failure names listed below. Logs: `/tmp/orchestrate/023/baseline-resume/{build,test}.log`.
The current mismatch direction is generated `07_memory.ir` output omitting expected remove/swapRemove functions.
Use greater tier/full review and work sequentially. Implementation remains paused until the user resumes it.

## Preserve the working tree

The checkout contains extensive pre-existing changes, deletions and untracked compiler/UI files. The work is
**in this working tree**, including new files; a clean checkout or worktree will not contain it. Do not reset,
stash, clean or overwrite unrelated edits. Examples may be ignored by Git. Preserve the whole workspace when
switching harnesses, including `speculative/21_ui_select`, `22_ui_popups` and `23_ui_context` and their assets.

Per-chunk dirty-file snapshots and logs are under `/tmp/orchestrate/023/` (`baseline`, `chunk-3b`, `chunk-3c`,
`chunk-3d`). These are temporary evidence, not prerequisites for resuming. Compare new work to fresh copies of
the live files; `git diff HEAD` also includes unrelated work and is not a chunk-specific review boundary.

## What landed in this run

| Chunk | Tier / review | Result and implementation seams |
|---|---|---|
| 3b | Greater / full, CLEAN after one review fix | Real title-tooltip and color-panel DOM subtrees; wrapped titles; CSS sizing/shadows; interactive `ptRoot` and independent passive `ttRoot`. Color drag bounds and markers use actual layout. `flatColor` must check dynamic-slot validity so restyle/reload restores unchanged colors. |
| 3c | Greater / full, CLEAN after one review fix | `popover="context"` anchors current requests; Shift+F10; `bind:tooltip={boolPlace}` controls delayed custom hover content; shared menu/select type-ahead. `ctxPending` defers programmatic fallback anchoring until owner layout, including first mount. Inputs/selects and IME keep their own text handling. |
| 3d | Greater / full, CLEAN with no review findings | `Dom.optionVisuals` / `refreshOptionVisuals` mirror only visible option content after generated updates, preserving source nodes. `optLabel` caches full descendant text. `dynattr.build` now invalidates only when effective inherited values/custom-property contents change, eliminating measured reopen growth. |

Important constraints for later chunks:

- `ptRoot` is one interactive runtime popup slot; `ttRoot` is passive. `popnode.place`/`ptNat` belong to the interactive slot; titles use `settleTip`.
- Custom hover content uses an owner wrapper and one non-context **direct-child** conditional popover. It is not a `tooltip={component}` API. Crossing grace is 300ms; focus inside retains the card. Context siblings are excluded from association.
- Menu labels use `aria-label` or a visible text descendant, not concatenated fragments. Prefix matching is ASCII-insensitive and exact for non-ASCII UTF-8, with timeout/level/close resets.
- Option mirrors retain **source selector ancestry**, but inherit from the runtime row. Media is borrowed from source nodes; mirror-owned `Dyn` storage is separate. Cleanup/reuse must preserve that ownership boundary.
- Custom options support visual markup, not controls, component instances, handlers, bindings or focus/overlay attributes. Handle selection on the select; include spaces between text fragments. Large selects still require uniform themed row heights.
- Inherited-style cache comparisons include custom-property **contents**, not transient set IDs. `dynattr.build` uses `Css.out` only after cascading; do not introduce a nested cascade into that scratch interval.
- General `max-width` and complete min/max sizing still belong to 15a; component-wrapper removal still waits for 15c's `align-self`.

## Verification already completed

Release build passed after 3d. Latest recorded targeted suites:

| Check | Passing count / result |
|---|---|
| Rust UI compiler tests | 49 |
| `option_test.j3` | 27 |
| `dom_test.j3` / `css_test.j3` | 202 / 117 |
| `overlay_test.j3` / `input_test.j3` / `context_test.j3` | 272 / 183 / 51 |
| `speculative/23_ui_context` generated fixture | PASS |
| `speculative/22_ui_popups` goldens | Six exact comparisons |
| `speculative/21_ui_select` | Generated behavior/cost checks pass; custom SVG/image/styled-text golden exact and visually inspected |
| Independent 3d review probe | 33 checks; two consecutive 1000-reopen replays each grow zero bytes |

The custom-option fixture keeps 5000 source options bounded to visible mirrors, and 300 steady frames at zero
layouts/restyles/arena growth. The original inherited-style invalidation path grew 518,614,920 bytes over 1000
custom-option reopen cycles; the fixed path is flat after warmup. Reopen checks intentionally warm the shared
free-node pool's buffer capacities before measuring repeated identical sequences.

Four older select PNGs (`default`, `themed`, `multi`, `long`) have **pre-existing exact rounding drift**, at most
two channel values. Current captures match pre-change runtime captures exactly; stored baselines pass tolerance 2.
Do not overwrite these baselines to hide the drift. The new `custom` state and six popup states pass exactly.
Default tooltip/color migration has documented small border/compositing differences from the old painted path;
see Done (3b). All testing was headless; no real-window, real-HiDPI or cross-platform verification was performed.

No aggregate full-suite final audit ran after the chunks because the user paused execution. Targeted suite
counts above are completed-run evidence, not checks rerun during this documentation-only update.

## Reproduce checks

Run from the repo root. Use the local release binary: an installed `~/.bin/j3c` or running MCP server may be stale.
Stdlib is embedded, so rebuild after edits or use the source override below. `RUSTC_WRAPPER=` disables the repo's
automatic install wrapper for local checks. `RUST_MIN_STACK` avoids the known UI/LSP test-thread stack overflow.

```sh
env RUSTC_WRAPPER= timeout 180 cargo build --release --manifest-path j3c/Cargo.toml
env RUSTC_WRAPPER= RUST_MIN_STACK=67108864 timeout 180 cargo test --manifest-path j3c/Cargo.toml --lib ui::

export J3C="$PWD/j3c/target/release/j3c"
export J3_STDLIB="$PWD/j3c/stdlib"
export SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy __GL_SHADER_DISK_CACHE=0
for suite in option dom css overlay input context; do
    timeout 120 "$J3C" run "j3c/stdlib/tests/${suite}_test.j3" || break
done
timeout 240 sh speculative/22_ui_popups/tools/golden.sh
timeout 240 sh speculative/21_ui_select/tools/golden.sh
```

The select script runs all five states and behavior checks, but its four historical **exact** comparisons may
fail for the known drift. After capture, assess those four with tolerance 2; keep `custom` exact:

```sh
for state in default themed multi long; do
    "$J3C" -e imgdiff "speculative/21_ui_select/golden/$state.png" "speculative/21_ui_select/target/golden/$state.png" --tol 2 || break
done
"$J3C" -e imgdiff speculative/21_ui_select/golden/custom.png speculative/21_ui_select/target/golden/custom.png
```

Never launch a real window. Use `--headless --frames N` for generated app runs, offscreen/dummy drivers for
built binaries, and `timeout` for long or windowed commands, as required by `AGENTS.md`.

## Known full-suite baseline failures

Before 3b, the release build passed and full `cargo test` reported **302 passed, 10 failed, 1 ignored**.
Nine failures reference deleted example projects; all names below are under `compile::tests::`:

- `bounce_project_typechecks`
- `defense_project_typechecks`
- `dungeon_project_typechecks`
- `island_project_typechecks`
- `minecraft_project_typechecks`
- `project_emit_cwd_is_package_root`
- `raytrace_project_typechecks_with_nested_stdlib`
- `target_list_picks_a_sys_file_per_triple`
- `windows_dungeon_checks_and_missing_sdl_is_a_link_error`

`ir_goldens_01_through_08_and_samples` is the tenth failure: expected `07_memory.ir` lacks existing `remove`
and `swapRemove` output. None was fixed in chunks 3b–3d. Logs were saved as
`/tmp/orchestrate/023/baseline/build.log` and `test.log`.

Rebaseline before the next implementation; do not assume these are the only failures after a harness/environment
change. Use offscreen/dummy environment and `RUST_MIN_STACK=67108864` for full tests too. If a later chunk touches
`compile.rs`, follow `j3c/AGENTS.md` and the root instructions about obsolete example tests.
