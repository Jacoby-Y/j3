# Stage 08: Cursor / VS Code support for `.j3ui`

## Done

**Shipped**
- `editors/vscode` 0.3.0: language `j3ui` (`.j3ui`, `language-configuration-j3ui.json`: `<!-- -->`, `{ }`/`( )`/`[ ]` pairs, fold markers for `{#if}`..`{/if}`, `{#each}`..`{/each}`, `<script>`, `<style>`), grammar `syntaxes/j3ui.tmLanguage.json` (scope `source.j3ui`), `extension.js` follows `j3` and `j3ui` buffers (and `**/ui/*.css`, completion only) and registers hover, definition, completion (extra triggers `<` space `:` `-`), signature help for `["j3","j3ui"]`. `check-grammars.js` is the offline grammar check. `j3-0.3.0.vsix` is built (gitignored like the others; `vsce` is not installed, so it is a zip of the same layout as 0.2.2).
- Grammar is generated: `j3c dump-syntax --ui <file>` (`parse::syntax::ui_tm_grammar`, element names from `ui::gen::ELEMENTS`; test `shipped_ui_grammar_is_dump_syntax`). `<script>` embeds `source.j3`, `<style>` `source.css`, `{expr}` and `{#if ...}` embed `source.j3`, `{#if}`/`{:else}`/`{#each ... as}` are `keyword.control`, `on:`/`bind:`/`class:`/`transition:` directives, capitalised tags are `entity.name.type.class.component`, unknown lowercase tags are `invalid.illegal`.
- `j3c/src/lsp/ui.rs`: `.j3ui` requests regenerate the module and use the span map. `compile::analyze_ui(path, overlays) -> (Result<Program, CheckError>, Option<Generated>)` (`analyze_checked` is its `.0`); `ui::try_build` / `BuildErr` and `Loader.ui_err` keep the position of markup errors, so they are diagnostics with line:col, not rendered text. Diagnostics for a `.j3ui` are published on the `.j3ui` URI; errors in `ui/X.j3ui` found while editing a `.j3` are published there too (`err_path`).
- Hover and definition in `{...}` and `<script>` (definition into generated glue lands on the element or script text); member completion after `sim.` (`complete::member_items`; the half-typed `sim.pre` is swapped for a stand-in value that typechecks there); signature help; tag completion (elements, components from `ui/*.j3ui`, buffers included); attribute, `on:`, `class:`, `bind:`, `transition:` completion per element (`ui::gen::element_attrs`); a component's props (with types) and `on:<event>`, skipping the ones already written; CSS property names, keyword values and `var(--` tokens (`:root` of the theme and the file) in `<style>` and `ui/*.css`, with the tables read from the embedded `j3/ui/css.j3` at run time (no copy).
- `SpanMap::to_gen` prefers a copy that contains the position over one that only ends there (the cursor at `canBuy` in `!this.canBuy()` landed before the inserted `this.`).

**Verify output**
- `cargo test --lib lsp::ui`: 8 passed (diagnostic at 31:48 of BallCard, markup error with position, hover on `sim.gold` gives `Sim.gold: int`, definition of `shop()` lands on `fn shop` in the `.j3ui`, `sim.` lists `gold` `ballCount`, `<Ba` lists `BallCard` `BallShop`, `<UpgradeRow ` lists `label value cost gold on:buy`, CSS names/keywords/`var(--`, map round trip over every copied segment). `cargo test --lib ui::` 14 passed.
- The same over stdio against the installed binary (`j3c lsp`, JSON-RPC piped in): `DIAG BallCard.j3ui {'line': 30, 'character': 47} no method buyBal`, `HOVER Sim.gold: int`, `COMPLETE sim. ['acc','alive','ballCount',...]`, `COMPLETE props ['label','value','cost','gold','on:buy']`, `COMPLETE <Ba ['BallCard','BallShop']`.
- `node editors/vscode/check-grammars.js`: j3, j3ui, j3toml grammars parse, all includes resolve, all regexes compile, `.j3ui` rules hit their sample lines.
- `cargo test --lib`: 261 pass, 11 fail, all old (tests naming missing examples, `phase`, `incomplete_app_dot_lists_methods` which reads `examples/minecraft`).

**Deviations**
- No CSS diagnostics (stage 07 made CSS warnings runtime-only, not `j3c check`).
- No `.j3ui` formatting: the formatter reads J3 only and `props {}` / `events {}` are not J3; a script-only formatter would be a second one.
- No document symbols for `.j3ui`; no completion of `{#if` / `{#each` block keywords; no `theme.css` diagnostics.
- The `.vsix` was not made with `vsce` (not installed, no network assumed).

**Gaps**
- A `.j3ui` is only analyzed when `src/` imports `<package>.ui` (generation is lazy), so an unused component gets no J3 features.
- Member completion works for `ident.` where `ident` is a local of the generated function (props, `let`, `{#each}` vars); `a.b.` and `f().` give nothing. Bare-name completion and signature help need the buffer to typecheck as typed (`ponytail:` in `lsp/ui.rs`); in `<script>` a half-typed `sim.` line is healed only if `0` fits there.
- One analysis per request that is not cached (healing tries up to four stand-ins).
- A source range emitted twice (a condition used in two places) maps back to the first copy; same text, same answer.

**Next stage needs to know**
- This was the last stage of 022. The user installs `editors/vscode/j3-0.3.0.vsix` in Cursor and checks highlighting, hover and completion by eye; nothing here opened an editor.
- `cargo test` debug stack: `compile::tests::script_code_edit_swaps_live` overflows the default 2 MB test-thread stack in this tree and passes with `RUST_MIN_STACK=3000000`; the code path it uses (script loader/check) was not touched by this stage, but it was not checked against the earlier tree.
