A key that works while the app is in the background: F8 pauses and resumes a recording from any
window. `j3.game.hotkey` grabs the combo on the X11 root window; `j3c doc j3.game.hotkey` has the rules.

- `hotkey.register(Key.F8, hotkey.NONE)` returns `Result<Hotkey, HotkeyError>`. Modifiers add up:
  `hotkey.CTRL + hotkey.SHIFT` (`SHIFT`, `CTRL`, `ALT`, `SUPER`).
- Poll `hk.pressed()` from the frame loop (main thread). It is true once per press; held keys do
  not repeat. A grabbed key reaches the hotkey even when this window has focus (SDL never sees
  it), so `hk.pressed() || game.keyPressed(Key.F8)` covers both the grab and the fallback.
  `Hotkey {}` is an empty handle that never fires.
- While registered, the focused app never gets that combo. Pick one people do not type.
- Errors: `Taken` (another app holds it), `Unsupported` (Wayland, no libX11, not Linux),
  `NoDisplay` (no X server, `--headless`), `UnknownKey` (not on this keymap). Fall back to the
  in-window key; never panic on them.
- `hk.unregister()` frees the combo; the X server also drops grabs when the process exits.
- Test the fallback path headless; test the grab by hand (another window focused, press F8).
