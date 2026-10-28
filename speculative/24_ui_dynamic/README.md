# Dynamic components

Generated mixed-message fixture for plan 023 stage 4. Runs 14 assertions covering all three candidates,
integer and enum selectors, state destruction/re-entry, keyed moves, invalid indices, default/named slots,
class/style passthrough and one selector evaluation per update. It reuses the lifecycle fixture's font.

From the repository root:

```sh
env J3_STDLIB="$PWD/j3c/stdlib" SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy __GL_SHADER_DISK_CACHE=0 timeout 180 j3c/target/release/j3c run speculative/24_ui_dynamic --headless --frames 20
```

`Of_TextMsg_ImageMsg_Embed` is generated once for the ordered candidate set. Other enums and non-`int`
selectors are errors; indices outside 0–2 render nothing. Existing match lifecycle and keyed-row storage
own every arm; no runtime component registry is involved.
