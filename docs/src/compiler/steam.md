# Shipping on Steam

Steam support is the `steam` package, not part of `j3c` (it was the `j3.steam` stdlib module). It lets a game use Steam: achievements, stats, leaderboards, Steam Cloud, the overlay, rich presence, the Steam Deck keyboard, Steam Input, lobbies and peer-to-peer networking. It loads Valve's library when the game starts. If Steam is not there, every call does nothing, so the same binary runs everywhere.

This is the short version. `j3c doc steam` says how to add the package; the package's `README.md` is the full guide, and its `examples/spacewar` is a small program.

## Add it

```toml
[dependencies]
steam = { git = "https://github.com/Jacoby-Y/j3-steam", tag = "v0.1.0" }   # or a path to a checkout: steam = "../j3-steam"
```

## Try it

```j3
use steam;
use steam.Status;
use steam.game.attach;

let st = steam.init(480);                 // 480 is Valve's Spacewar test app
if st == Status.Restart { sys.exit(0); }  // Steam relaunches the game
attach(&app);                             // pumps Steam each frame, shuts it down at exit

steam.unlock("ACH_WIN_ONE_GAME");
steam.store();
```

Everything runs on the main thread. In a dev build, `J3_STEAM_FAKE=1` fakes Steam and prints each call, so you can test without the client.

## Set up

Make a Steamworks partner account and download the SDK. Then in `j3.toml` (the package reads this table; `j3c` ignores it):

```toml
[steam]
app_id = "480"
sdk = "~/steamworks_sdk"
depot_linux = "481"
depot_windows = "482"
branch = "beta"
```

In a dev build `steam.init` reads the app id and the library path from there, so you do not need a `steam_appid.txt`. The bindings were written from the public API without the SDK. Run `j3c pkg steam check --sdk ~/steamworks_sdk` once to compare them with your copy.

## Build a depot

```toml
[emit]
target = ["x86_64-pc-linux-gnu", "x86_64-pc-windows-gnu"]

[dist]
build_outputs = ["steam"]
```

`steam` is the package's dist output: `j3c build --release` runs it and it writes `target/dist/steam/` with a folder per OS and a `steamcmd` build script. Nothing uploads: you run `steamcmd +login <user> +run_app_build <script> +quit` yourself. On Linux, today's builds need glibc 2.34, newer than Steam's sniper runtime, so either build inside the sniper container (`scripts/build-sniper.sh`) or ship without sniper. The package's README explains both, and lists the Steam Deck and store checks.

## What is not covered

Workshop, in-game purchases and macOS depots are not part of the package. The Steam client, the real SDK, a Windows machine and a Deck have not yet been used to test it; the package's `VERIFY.md` lists what to check.
