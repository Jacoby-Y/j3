# Rich textarea

Fixture for plan 023 stage 5b: `<textarea rich bind:value bind:spans>` as a chat box. A headless run types text,
swaps the trigger `@ja` for an atomic `@Jane` chip with `Dom.insertToken`, bolds a word with Ctrl+B, and checks the
bound text and spans and the caret rect (5 PASS lines). `golden/chat.png` is the frame (chip, bold run, caret).

```sh
J3_STDLIB="$PWD/j3c/stdlib" SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy timeout 180 j3c/target/release/j3c run speculative/25_ui_rich --headless --frames 16
J3C=$PWD/../../j3c/target/release/j3c tools/golden.sh [--update]   # from this directory
```

5c: typing `@ja` fires `on:trigger` and opens a `popover="caret"` list under the caret; Down/Up move, Enter inserts the `@Jane` chip
(`golden/popup.png` is the open list with the second entry active; 12 PASS lines).
