# Plan 01: Steam, verify with the real client

**Status (2026-11-02): done, chunks 1-11 built; chunk 12 (final review) open.** Verified here: stubs and fake mode (`stdlib/tests/steam_*_test.j3`), pack-4/pack-8 decoding against hand-made bytes, Windows primitives under wine, `j3c check` for Windows/macOS/Android targets, the dist tree and `.vdf` (unit tests), the doc page and recipe (`recipes_check`, headless fake run). **Not verified:** any real Steam client, the real SDK (every symbol name, signature, struct offset and callback id was written from memory; `j3c -e steam-check --sdk DIR` has never run against a real SDK), a Windows machine, a Steam Deck, `scripts/build-sniper.sh` (no podman image pulled), two accounts online. Docs: `j3c doc steam`, `j3c doc recipes steam`, `docs/src/compiler/steam.md`.

### Verify with real Steam (for the developer)

1. Download the SDK, run `j3c -e steam-check --sdk DIR`. Fix every finding (the page `j3c doc steam`, "SDK check", says how), rebuild `j3c`, run again until clean.
2. Spacewar (480): `[steam] app_id = "480"`, Steam client running, `j3c run` a program that unlocks `ACH_WIN_ONE_GAME`. See the toast, open the overlay with Shift+Tab, check the game pauses (`attach`) and resumes.
3. Stats and leaderboards: set a stat, `store()`, restart, read it back; upload a score and download it.
4. Cloud: Auto-Cloud with `save.userDir` paths on the partner site, then `cloudSync` and the file API; save on one machine, load on another.
5. Steam Input and Deck: upload an action manifest, check glyphs, `feedPad` menu navigation, the on-screen keyboard in a `.j3ui` text box, suspend and resume, offline start.
6. Lobbies and networking with two accounts: create, find, join, chat, invite, join from the friends list; messages and sockets across the relay (`relayInit`).
7. Windows: build the `windows` target, run on Windows, check `steam_api64.dll` loads, saves, cloud, env fake (`J3_STEAM_FAKE`).
8. Sniper: run `scripts/build-sniper.sh`, upload to a test branch with the sniper compatibility tool, install on a clean machine; then the same without sniper on a glibc 2.34+ distro and the Deck.

