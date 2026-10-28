Plan 023/3c generated-markup interaction fixture. Run from the repository root:

```
J3_STDLIB=$PWD/j3c/stdlib SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy timeout 90 j3c/target/release/j3c run speculative/23_ui_context --headless --frames 105
```

The scripted pointer opens a context menu, types a prefix, closes with Escape, then opens a custom hover card. It prints one PASS and exits. Detailed windowless behavior/cost checks are in `j3c/stdlib/tests/context_test.j3`.

The owner is a wrapper, with `on:contextmenu={menu = true}` and `bind:tooltip={card}`. Conditional direct child popovers hold arbitrary content; `popover="context"` captures the request anchor, while a plain `popover` is the hover card. The boolean binding owns hover delay and close state; an `on:close` handler still owns context-menu state.
