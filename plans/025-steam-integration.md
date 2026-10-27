# Plan 025: Steam integration

Stub, written 2026-10-21. **Do later**, after a game is near shipping. Nothing here exists yet: a search of `j3c/` and `plans/`
found no Steam code. Drift-check that before starting.

**Goal.** A J3 game ships on Steam with achievements, cloud saves, overlay, controller support through Steam Input, and a
depot that installs and runs on Linux, Windows, and Steam Deck, with no change to game code beyond calling `j3.steam`.

**Approach.** The Steamworks SDK is C++ with a flat C API (`steam_api_flat.h`, `SteamAPI_*`, accessor functions like
`SteamAPI_SteamUserStats_v013()`), so it fits `use lib "steam_api"` FFI like the other native libraries. No crates, no
bindgen: hand-write the small set of externs in a new `j3.steam` module. Build must work with the SDK absent: the module
compiles to stubs unless the project names it (`[steam]` in `j3.toml`), so `j3c run` on a dev machine without Steam still works.
The SDK itself is not redistributable in the repo; the project points at a local SDK path and the license is the dev's.

## Steps (rough order)

1. **Init and lifecycle.** `SteamAPI_RestartAppIfNecessary(appid)`, `SteamAPI_Init`, `SteamAPI_RunCallbacks` once per frame
   in an `App` stage, `SteamAPI_Shutdown` at exit. `steam_appid.txt` for dev runs. Degrade to a no-op when Steam is not running.
   Callbacks use C structs polled via `SteamAPI_ManualDispatch_*` (flat API way), not C++ callback classes.
2. **Achievements and stats.** `steam.unlock(id)`, `steam.setStat`, `storeStats`; a dev-mode fake that logs to the console.
3. **Cloud saves.** Map `j3.data.save` to Steam Remote Storage when enabled (`ISteamRemoteStorage` file write/read), with a
   local fallback and a conflict rule (newer timestamp wins, keep the loser as a backup).
4. **Overlay and rich presence.** Overlay needs the game to render through the normal swap (GL is fine); verify with the
   overlay on in `j3c run`. Rich presence strings. `steam.openUrl` for the store page.
5. **Steam Input and glyphs.** `ISteamInput`: action sets matching plan 017 item 5, digital/analog actions, and
   `GetGlyphForActionOrigin` PNGs feeding `<glyph>` in plan 023 stage 16. Steam Deck: handle the floating on-screen keyboard
   (`ShowFloatingGamepadTextInput`) for `.j3ui` text inputs.
6. **Packaging.** `j3c build --release` per triple into `[dist]` zips already exist; add a depot layout (`content/` per OS),
   `app_build.vdf`/`depot_build.vdf` generation, and the runtime library question: ship `libSDL2` etc. beside the binary
   (rpath `$ORIGIN`, already a roadmap "pack/install" item) or depend on the Steam Linux Runtime (Sniper) container.
   Test on Steam Deck in Proton-less native mode.
7. **Store-readiness checklist** (a doc, not code): controller-glyph and focus audit, text scale option (needs plan 023 stage
   15 `rem`), accessibility basics, crash-log location, offline start, resolution/aspect support (`game/letterbox.j3`),
   no hard-coded absolute paths, license notices for SDL/ttf/mixer and bundled fonts.

## Open questions

- Own `lib "steam_api"` bindings vs. a documented "bring your own SDK" shim? Recommended: own bindings, flat API only.
- Linux: native build only, or also test under the Steam Linux Runtime container? Recommended: both.
- Multiplayer (Steam Networking, lobbies, matchmaking) is out of scope; a separate plan if a game needs it.
- Workshop/mod support: out of scope, but `j3.ui` list virtualization (plan 023 stage 1) is a prerequisite for a mod browser.

**STOP conditions.** Do not commit SDK headers/binaries. Do not add crates. If a callback needs C++ vtable calls, stop and
write down which; the flat API should cover everything above.
