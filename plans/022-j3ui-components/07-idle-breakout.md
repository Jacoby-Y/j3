# Stage 07: idle-breakout ships: goldens, perf budget, hot reload, docs

## Done

**Shipped**
- `examples/idle-breakout` builds and runs (`[link] libs` has `GL`, `app.useGl()`, balls are `paint.circle`, SPECULATIVE notes gone). `j3c check examples/idle-breakout` passes.
- Goldens: `IDLE_GOLDEN=<balls|upgrades|prestige|stats|settings|paused>` in `main.j3` `boot` (seed 7, 3 kinds bought, 4 s simulated, gold set). State is set by writing the instance fields from outside (`root.c1.tab = N`; a `let` is a plain field), no prop or `pub fn`. A tab is rebuilt fresh when it opens, so the Prestige confirm flag is set from `draw` after frame 1. The UI clock is fixed (`dom.setFrameMs(16)`) in golden mode. `golden/*.png` (3.9 MB each, uncompressed PNG) and `tools/golden.sh [--update]` (rebuilds, captures `J3_CAPTURE=..:30 J3_FIXED_DT=0.016`, `j3c -e imgdiff`, PASS/FAIL per state). Command is in the example README.
- `j3.ui.domhtml.dumpHtml(d: &Dom, path: string): bool`; F12 under `j3c run` (and `IDLE_SNAPSHOT=N`) writes `target/ui-snapshot.html`. `Css.srcs` keeps each parsed sheet text; scoped sheets get `.s<scope>` on the last compound (the cascade's rule). `<canvas>` is a grey box.
- Style hot reload: `j3.ui.domhot.hotDue(css): bool`, `hotFile(css, path, style, restHash, ui): bool`, `hotWarns(css)`; `Css.hot`, `HotFile`, `parse` uses the on-disk text of a changed file. The generated root has `hot()` (called from `render`): it polls theme.css and every `.j3ui`, then `css.reset()`, re-parse, `dom.restyle()`. Markup/script edits print `ui/X.j3ui: markup or script changed, rebuild to see it` (hash of the non-style part computed by `gen.rs`, same function at runtime). Active only when `J3_ROOT` is set (`j3c run`).
- CSS warnings: runtime, not `j3c check` (check cannot run J3). Printed at startup and after a reload as `warning: ui/theme.css:53:10: ...` (verified with a `!important` line).
- `IDLE_BENCH=1`: 50 balls, Upgrades tab, prints `ui_ms total_ms draws` medians over 540 frames (first 60 skipped).
- Docs: `j3c doc j3ui` (new page), `j3c doc j3ui-immediate` (old page, linked both ways), `j3c doc recipes components` (checked by `recipes_check`), `j3c new <name> --template ui` (`scaffold::create_ui`, 12 lines, reuses the recipe files; checked and run headless), AGENTS.md rows (Write UI, Examples), `roadmap.md` line, `j3c/AGENTS.md` mention, root README "Untested features" section (new).
- Two fixes in `j3.game`: `ATLAS_FONTS` 4 -> 16 (idle-breakout uses about 8 font/bold variants; with 4 the glyph atlases were rebuilt every frame), and one shared 1024x4096 glyph texture for all fonts (`sheetPlace`), so text in different sizes stays in one batch. `Key.F12`, `game.env` public (+ headless stub).

**Numbers** (Intel Core Ultra 7 270K Plus, RTX 5060 Ti, driver 580.173.02, offscreen GL, median of 3 runs, 540 frames)

| | release | dev (opt 0) | budget |
|---|---|---|---|
| `ui_ms` (update+layout+paint+field `on:draw`+GL submit) | 0.032 | 0.076 | <= 1.0 |
| `total_ms` (that + sim tick) | 0.032 | 0.077 | <= 2.0 |
| draws per frame (whole frame, field included) | 18 | 18 | UI <= 10 (MISSED) |
- Before the atlas fix: 1.50 ms release, 4.02 ms dev (atlas thrash). Binary: dev 1.18 MB, release 202 KB.
- Draw-call miss, measured by cause per frame: 14 texture switches, 2 clips, 1 view in, 1 out. The texture switches are non-ASCII strings (`Buy · 34`, `Level 1 · ...`): `j3.game` bakes those as one texture per string, outside the shared sheet. Fixing it means non-ASCII glyphs in the atlas.
- Goldens: 3 clean-rebuild runs (`rm -rf target`, `tools/golden.sh`): 6 PASS each, 0 pixels differ. Before fixing the clock, `prestige` differed by about 6400 pixels (real-time fade): the nondeterminism was the wall clock.
- Hot reload: dev binary built, then `--accent` edited, run with `J3_ROOT` set: `236 of 972800 pixels differ` against the golden (the tab underline); same edit without `J3_ROOT`: 0 differ; file restored: 0 differ.
- Browser (headless Chrome 151, `balls`): sidebar panel and card colors cover the same area to within 0.1%; 1.7% of the sidebar pixels differ by more than 40/255, all text (synthetic vs real bold, subpixel placement). Listed in the example README.
- Regressions: css_test 78 ok, dom_test 172 ok, `speculative/12_ui_runtime test` 13 PASS 0 FAIL, bench2d `shapes` golden 0 differ, goldens above still exact after the atlas change, `cargo test`: 252 pass, the same 11 old failures.

**Deviations**
- Hot reload polls file contents (10 small files every 250 ms), not mtimes: `j3.sys` has no stat. Dev detection is `J3_ROOT` (set by `j3c run`), because there is no compile-time dev/release flag; `j3c run --release` also polls.
- Upgrades golden cuts off after the Heavy rows (760 px tall); Splash is below the scroll fold.
- The perf draw-call target is missed (above); times are far under budget. `ui_ms` includes the field.
- Scoped CSS in the snapshot is one extra class (our rule), not Svelte's class on every compound.

**Gaps**
- Goldens are uncompressed PNGs (`png::encode_rgba`): 23 MB checked in. A `deflate` encoder would shrink them.
- `SHEET_H` 4096 rows hold about 60 font variants; after that sheets get their own textures (correct, more draws). A cleared atlas leaves its band unused (`ponytail:`).
- Hot reload does not reload `@font-face` fonts or re-read markup; a missing file is ignored.

**Next stage needs to know**
- Stage 08 (editor) can use `dump-ui`/`Generated.map` as before; nothing in the format changed. The generated root now has `hot()` and `render` calls `hotDue` first.
- Always set `SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy` inline on each command: the Bash tool does not keep `export`s between calls (this stage opened a visible window by mistake for a few bench runs before it was noticed; those numbers were discarded).
