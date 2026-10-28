# Stage 06: `j3c` compiles `.j3ui`

## Done

**Shipped**
- `j3c/src/ui/`: `markup.rs` (container format: elements, attrs, `x={expr}`, `x="a {b}"`, `on:`/`bind:`/`class:`/`transition:`, text holes, `{#if}{:else if}{:else}`, `{#each X as x, i}`, comments; `{...}` extents by brace counting that skips strings/templates; `<script>` split into `use`/`props`/`events`/`let`/`fn` items with the **real lexer**), `gen.rs` (J3 text generator), `map.rs` (`Out`, `SpanMap`: `to_src(lo,hi)` and `to_gen(file,pos)`), `mod.rs` (`build`, `Generated::{remap, render_parse_error, sources}`, `dump`), `tests.rs`.
- No second J3 parser: J3 from the file is copied verbatim into generated text, which goes through `parse::parse`. Only token-level edits: `this.` before an instance `let`/script `fn`, `emit x` -> `ev.x = true`.
- Name access: the **token-rewrite fallback** (props bound to same-named locals `let sim = p.sim;`, as stage 05's target does; `.j3s` bare-name machinery was not used because props change per frame). Shadowing a prop/let/fn/generated name with a local or `{#each}` var is an error.
- One module `<package>.ui` per project; per component `SCOPE_Name`, `CSS_Name`, `NameProps`, `NameEvents`, `Name`. `use idle.ui.{ Game, GameProps };`. Roots (components nobody contains) own `css`/`dom` and `render`. `examples/idle-breakout/src/main.j3` updated.
- Pipeline: `compile.rs` `Loader::load_ui` generates lazily when something imports `<package>.ui` (overlays honored, so the LSP sees unsaved `.j3ui`); check errors (`load_and_check_with`, `analyze_checked`) and parse errors in the generated file are remapped onto `ui/X.j3ui:L:C` with the `.j3ui` line shown. Rebuilds: the generated text (CSS, markup code) is part of the program source, so `source_key`/IR hashes change on any real edit.
- `[ui] dir = "ui"`, `theme = "ui/theme.css"` in `project.rs` (`Manifest.ui: Option<Ui>`), `toml.js`, `j3c help manifest`. `j3c dump-ui <file.j3ui|project dir>`.
- Compile-time checks, each with line:col (tests in `ui/tests.rs`): unknown element, unknown attribute/event for the element, unknown component (lists known), unknown/missing prop, `on:x` the child doesn't declare, `emit x` not in `events`, `bind:` on a non-place, untyped `let`, shadowing, unclosed tags/blocks, bad `{#each}` head, cycles.
- Checker/parser fixes made on the way: `f().x` works when `f` returns a reference (`base_place` in `check/lower.rs`); spans of template holes (`` `a {x.y}` ``) now point into the file (they were relative to the hole, so errors showed line 1).
- `speculative/12_ui_runtime`: hand-written `src/ui/*.j3` and `embed.py` removed; `ui/*.j3ui` + `[ui]` are the source; README rewritten for the one-module scheme.

**Numbers** (Intel Core Ultra 7 270K Plus, RTX 5060 Ti, offscreen)
- `j3c check examples/idle-breakout`: 51 ms total, load + parse of 17 files including generation 13 ms (budget: +30 ms).
- Generated module for idle-breakout: 1611 lines. Per component (Game 196, Sidebar 228, Upgrades 173, BallCard 200, Settings 164, UpgradeRow 108) all smaller than the hand-written files. Dev build of idle-breakout: 1,095,528 bytes; code in module `idle` 182 KB (sim, field, main, ui together).
- `idle steady 5000` release: 4.66 to 4.78 s = 0.93 ms/frame (same as stage 05); layouts 0, style recomputes 0, arena growth 0.

**Verify output**
- `cargo test --lib ui::`: 6 passed. Full `cargo test`: 250 pass, 11 fail, all failing before this stage's code is involved (tests naming missing examples, and `phase::tests::report_nests_steps_under_their_phase`, which passes alone and is flaky in parallel).
- `j3c check examples/idle-breakout`, `j3c check speculative/12_ui_runtime`: pass.
- Generated code behaves like the hand-written: `idle test` 13 PASS, 0 failures; `golden` capture vs stage 05's `golden.png`: `0 of 972800 pixels differ`; `steady 1000`: layouts 0, recomputes 0, growth 0.
- `sim.buyBal(kind)` in BallCard: `error: no method buyBal --> ui/BallCard.j3ui:31:48`, with that line and `did you mean buyBall`.

**Deviations**
- Props live in `p: &NameProps` (arena slot, `*this.p = p`) because J3 has no zero value to initialise a by-value `p` of unknown type.
- Text holes are always templates (`` d.text(`{label}`) ``): the generator doesn't know types.
- `styles()` calls each distinct direct child's `styles`, so a component used under two parents has its CSS parsed twice (harmless, same rules).
- Single module means `SCOPE_Name`/`CSS_Name` instead of `Name.SCOPE`; `use` lines are merged and split to one name each.
- The module is loaded lazily (only when imported), so an unused broken `.j3ui` is not reported.

**Gaps** (limits)
- Components or stateful `{#if}` inside two nested `{#each}` blocks: error. A range loop has no index var; `..=` unsupported. `{#each}` over an array is untested (slices and ranges are).
- `class="a {b}"` is an error (use `class:name={cond}`); text next to `{}` can't hold a backtick; no `{`/`}` escape except `&lbrace;` `&rbrace;`.
- Compile-time CSS checking is still stage 07's.

**Next stage needs to know**
- `Generated.map` (`SpanMap::to_src`, `to_gen`) and `Generated::remap` are what stage 08's LSP uses; `Loader.ui_gen` holds it after a compile, `analyze_checked` already remaps.
- Root `new(mem)` reads the theme at compile time (`THEME` const), so theme edits need a rebuild; hot reload is stage 07.
- stage 07 owns the root README untested-features entry (`.j3ui` was not added there).
- The 11 failing `compile.rs` tests are old; fix or delete when touching that file.
