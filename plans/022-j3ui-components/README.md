# Plan 022: `.j3ui` components (Svelte-like UI on a fast 2D painter)

Goal: `examples/idle-breakout` builds and runs, and looks the same as its markup and CSS would in a browser. Along the way J3 gets a batched 2D painter (`j3.gfx.paint`) that any game can use, not only UI.

The example is the reference. Its `ui/*.j3ui`, `ui/theme.css`, and `README.md` are the spec for the format and the CSS subset. If a stage finds the spec impossible or wrong, change the spec in `examples/idle-breakout/README.md`, write down why in the stage summary, and keep going. Don't silently do something different.

## Stages

Each stage is its own file and is written for a fresh agent with no context. Do one stage per session.

| Status | Stage | What ships | Depends on |
|---|---|---|---|
| DONE | [01](01-harness.md) | Frame capture, image diff, `bench2d` perf baseline | — |
| DONE | [02](02-paint.md) | `j3.gfx.paint`: batched GL 2D (rounded rects, borders, circles, lines, text, images, clip, transforms) | 01 |
| DONE | [03](03-css.md) | `j3.ui.css`: CSS subset parser, selectors, cascade, `var()` | — |
| DONE | [04](04-layout.md) | `j3.ui.dom`: retained node tree, style resolution, block/flex/grid/absolute layout, text, scroll | 03 |
| DONE | [05](05-runtime.md) | Paint, input, events, form controls, `<canvas>`, transitions; a hand-written desugared component as the codegen target | 02, 04 |
| DONE | [06](06-compiler.md) | `j3c` compiles `ui/*.j3ui` to J3, with errors pointing into the `.j3ui` | 05 |
| DONE | [07](07-idle-breakout.md) | idle-breakout builds; goldens, perf budget, style hot reload, HTML snapshot, docs | 06 |
| DONE | [08](08-editor-extension.md) | Cursor/VS Code: `.j3ui` highlighting, diagnostics, completion, hover | 06 |

03 can run in parallel with 01–02. 08 can start its grammar after 06.

Status values: TODO | IN PROGRESS | DONE | BLOCKED (one-line reason).

## Rules for every stage

- Read the summaries of every DONE stage this one depends on before starting. They replace their plans, so they're the current truth.
- Project rules (`AGENTS.md`, `j3c/AGENTS.md`): no crates.io deps, no libLLVM, **no second J3 parser**. The markup and CSS parsers are for other languages. Every J3 expression and statement inside a `.j3ui` goes through the real lexer and parser.
- Stdlib is embedded in `j3c`: rebuild after stdlib edits (`cd j3c && cargo build --release`, or `./build.sh` to install). `J3_STDLIB=$PWD/j3c/stdlib` uses the files on disk instead.
- Wrap anything windowed or long in `timeout` (`timeout 60 j3c run ...`). Never leave a window open.
- The user checks look and feel themselves. Verify with numbers, goldens, and `j3c check`, not by eyeballing a running window over and over.
- `cargo test` already fails in `compile.rs` tests that name missing examples. Those aren't regressions, but don't add new ones.
- Mark deliberate shortcuts with a `// ponytail:` comment that names the limit and the upgrade path.
- Each new feature that hasn't been tested in a real game goes in the README's untested list (the root `README.md`, if it has one; otherwise the relevant module doc).
- Perf numbers: measure release builds (`j3c build --release`, then run the binary), 3 runs, take the median. Record dev-build (`opt = 0`) numbers next to them. Put the machine (CPU, GPU, driver) in the summary once.

## When a stage is done: compact it

1. Make sure the stage's **Verify** list passes. Put the output that proves it in the summary.
2. Replace everything in the stage file below the title with a `## Done` summary of no more than about 60 lines:
   - **Shipped**: modules, files, and public API (signatures, not bodies).
   - **Numbers**: perf and size measurements against the stage's targets.
   - **Deviations**: what differs from the plan, and why.
   - **Gaps**: known limits and `ponytail:` shortcuts with their ceilings.
   - **Next stage needs to know**: anything surprising.
   Delete the instructions (git has them). Don't keep implementation notes the code already says.
3. Set the stage's row above to DONE. Set the 022 row in `plans/README.md` to DONE only when stage 08 is done.
