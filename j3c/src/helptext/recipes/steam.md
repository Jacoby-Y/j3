A small game that talks to Steam: it unlocks an achievement and posts a score when the run is won, sets
rich presence, and stores stats on quit. It runs the same with no Steam (every call does nothing), and
in a dev build `J3_STEAM_FAKE=1` fakes Steam and prints `steam(fake): ...` lines. The full guide is
`j3c doc steam`.

- `steam.init(appId)` returns a `Status`. `Absent` means no Steam: carry on. `Restart` means a release
  build was not started by Steam; exit and Steam relaunches it.
- `use j3.steam.game.attach; attach(&app);` pumps Steam every frame and shuts it down when the loop
  ends. It is a separate module because it pulls in SDL. Register your own `app.shutdown` first if it
  needs Steam.
- `steam.unlock(name)` and `steam.store()`: unlock at the moment, store at a checkpoint (not per frame).
- Leaderboards are call results: `findOrCreateBoard(name, sort, display, handler, ctx)` calls
  `handler(board, ok, ctx)` on a later frame, and `uploadScore` takes its own handler. Read the
  `Uploaded` in the handler; it is not valid after.
- Test headless: `J3_STEAM_FAKE=1 j3c run --frames 30 --headless` prints each call.
- Real Steam: add `[steam] app_id = "480"` to `j3.toml` and run with Steam open (`j3c doc steam`).
