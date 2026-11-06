# Shipping on Steam

`j3.steam` lets a game use Steam: achievements, stats, leaderboards, Steam Cloud, the overlay, rich presence, the Steam Deck keyboard, Steam Input, lobbies and peer-to-peer networking. It loads Valve's library when the game starts. If Steam is not there, every call does nothing, so the same binary runs everywhere.

This is the short version. The full guide, kept in step with your `j3c`, is `j3c doc steam`. A small program is `j3c doc recipes steam`.

## Try it

```j3
use j3.steam;
use j3.steam.Status;
use j3.steam.game.attach;

let st = steam.init(480);                 // 480 is Valve's Spacewar test app
if st == Status.Restart { sys.exit(0); }  // Steam relaunches the game
attach(&app);                             // pumps Steam each frame, shuts it down at exit

steam.unlock("ACH_WIN_ONE_GAME");
steam.store();
```

Everything runs on the main thread. In a dev build, `J3_STEAM_FAKE=1` fakes Steam and prints each call, so you can test without the client.

## Set up

Make a Steamworks partner account and download the SDK. Then in `j3.toml`:

```toml
[steam]
app_id = "480"
sdk = "~/steamworks_sdk"
depot_linux = "481"
depot_windows = "482"
branch = "beta"
```

`j3c run` gives the game the app id and the library path, so you do not need a `steam_appid.txt`. The bindings were written from the public API without the SDK. Run `j3c -e steam-check --sdk ~/steamworks_sdk` once to compare them with your copy.

## Build a depot

```toml
[emit]
target = ["x86_64-pc-linux-gnu", "x86_64-pc-windows-gnu"]

[dist]
build_outputs = ["steam"]
```

`j3c build --release` writes `target/dist/steam/` with a folder per OS and a `steamcmd` build script. j3c never uploads: you run `steamcmd +login <user> +run_app_build <script> +quit` yourself. On Linux, today's builds need glibc 2.34, newer than Steam's sniper runtime, so either build inside the sniper container (`scripts/build-sniper.sh`) or ship without sniper. The page `j3c doc steam` explains both, and lists the Steam Deck and store checks.

## What is not covered

Workshop, in-game purchases and macOS depots are not part of `j3.steam`. The Steam client, the real SDK, a Windows machine and a Deck have not yet been used to test it; `plans/01-steam.md` lists what to check.
