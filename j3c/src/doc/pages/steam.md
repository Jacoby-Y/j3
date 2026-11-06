Steam in J3 (j3.steam) — achievements, cloud, overlay, Deck, depots

## What it is

`j3.steam` calls the Steamworks flat C API. It loads `libsteam_api.so` (Linux) or `steam_api64.dll` (Windows) with `sys.dlopen` when the game starts. Nothing links against Valve code. On a machine without Steam the same binary runs and every call does nothing: it returns false, 0 or "". `steam.on()` says whether Steam is live.

- Main thread only. Call everything from the thread that runs the frame loop.
- Android never loads the library. macOS has no depot (see Out of scope).
- Fake mode (dev builds only): `J3_STEAM_FAKE=1` or `steam.initFake(appId)`. Each part keeps its state in memory and prints `steam(fake): ...` to stderr. Callbacks and call results still go through the real dispatch path. Release builds ignore both. On Windows `J3_STEAM_FAKE` works too; use `initFake` if you want it in code.
- Status after `init`: `Ready`, `Fake`, `Absent` (no Steam), `Restart` (see Game code).
- Names and signatures: `j3c doc j3.steam --short`, `j3c doc j3.steam <name>`. A runnable program: `j3c doc recipes steam`.
- Not tested here: a real Steam client, the real SDK, a real Windows machine, a Steam Deck. The stub and fake tests pass; see the plan for the checklist to run with real Steam.

## Setup

1. Make a Steamworks partner account and download the SDK from partner.steamgames.com/downloads/list (a zip; unzip it anywhere, keep it out of the repo).
2. Get an app id from the partner site. For development use 480 (Valve's Spacewar test app); it needs no store page.
3. Fill `[steam]` in `j3.toml`. Every key is optional; `app_id` and `sdk` are needed for the `steam` dist output and for the dev environment.

        [steam]
        app_id = "480"                  # numbers are quoted strings
        sdk = "~/steamworks_sdk"        # unzipped SDK root (has redistributable_bin/); ~/ and project-relative work
        depot_linux = "481"             # needed when a linux target is built
        depot_windows = "482"           # needed when a windows target is built
        branch = "beta"                 # optional; the build goes live on this branch (not "default")
        desc = "my build"               # optional; default "<name> <version>"
        glibc_max = "2.31"              # optional; highest GLIBC_x.y the Linux build may need (sniper)
        runtime_libs = "sniper-libs"    # optional; folder of .so files copied beside the Linux exe

        [emit]
        target = ["x86_64-pc-linux-gnu", "x86_64-pc-windows-gnu"]

        [dist]
        build_outputs = ["steam"]

4. Check the bindings against your SDK once: `j3c -e steam-check --sdk DIR` (see SDK check).
5. Run it. `j3c run`, `j3c golden`, the editor's play and the MCP `play` tool set `SteamAppId` and `SteamGameId` from `[steam] app_id`, and `J3_STEAM_LIB` to `<sdk>/redistributable_bin/linux64/libsteam_api.so` (`win64/steam_api64.dll` on Windows) when that file exists. Anything you already set is left alone. So no `steam_appid.txt` is needed. The Steam client must be running.
6. Library lookup order: `$J3_STEAM_LIB` (a full path), the folder of the executable, then the plain name.

## Game code

    use j3.steam;
    use j3.steam.Status;
    use j3.steam.game.attach;

    let st = steam.init(480);
    if st == Status.Restart { sys.exit(0); }   // release build not started by Steam: Steam relaunches it
    attach(&app);                              // after init

- `init(appId)`: in a release build with a non-zero id and no Steam launch, it returns `Restart`. Exit at once. Otherwise `Ready`, `Absent` (reason on stderr) or `Fake`. Calling it again returns the current status.
- `attach(&app)` is in `j3.steam.game` because it links SDL (`j3.steam` alone does not). It pumps Steam every frame (`App.always`) and shuts it down at exit (`App.shutdown`). Register your own `app.shutdown` before `attach` if it needs Steam live.
- `attach` also pauses the app while the overlay is open (fixed steps stop; `always` and `draw` run) and restores the old paused state when it closes. `pauseOnOverlay(false)` turns that off; read `steam.overlayOpen()` and pause yourself.
- `attach` shows the Deck keyboard when a `.j3ui` text box gains focus on a Deck or in Big Picture, and hides it when focus leaves. It also hooks screenshots to the window. It replaces `j3.ui.inputs.setTextFocusHook` and `setTextBlurHook`. Call it once.
- A console program without `attach`: call `steam.frame()` each frame and `steam.shutdown()` at exit. `shutdown` runs `onShutdown` hooks first, stores changed stats, and is safe to call twice.
- `steam.status()`, `steam.on()`, `steam.appId()`, `steam.warned()` (count of distinct warnings, for tests).
- Menu navigation with a controller in `.j3ui`: `use j3.steam.ui.feedPad;`, `feedPad(&dom, handle, map)` each frame before the root frame fn. `map` is a `j3.steam.game.PadMap` of Steam Input actions (see Steam Input).
- Missing symbols and bad setups print one line to stderr, once per message.

## Achievements and stats

    steam.unlock("ACH_WIN_ONE_GAME");   steam.clear(name);   steam.unlocked(name): bool
    steam.indicate(name, cur, max)       // progress toast
    steam.setStatInt(name, v)   steam.statInt(name)   steam.setStatFloat   steam.statFloat
    steam.updateAvgRate(name, countThisSession, sessionLength)
    steam.store()                        // send changes
    steam.achievementCount()   steam.achievementName(i)   steam.achievementAttr(name, key)
    steam.resetAll(achievementsToo)      // dev only, wipes stats
    steam.requestGlobalPercents(f, ctx)  then steam.globalPercent(name)

- Stats load by themselves at `init` (SDK 1.61 and later). The bindings need SDK 1.59 or newer (`SteamAPI_InitFlat`) and are checked against 1.65; `onDeck()` is `IsRunningOnSteamHardware() == SteamDeck` there.
- `store()` at checkpoints: a level ends, the menu opens, quit. Not every frame; Steam rate-limits it. `shutdown` stores when something changed. A refused store prints a line and the stats stay marked as unsaved.
- `requestGlobalPercents(f, ctx)` calls `f(ok, ctx)` on a later frame. After `ok`, `globalPercent` has values.

## Leaderboards

    steam.findOrCreateBoard("High Score", steam.SORT_DESC, steam.DISPLAY_NUMERIC, onBoard, ctx);
    fn onBoard(board: u64, ok: bool, ctx: u64) { steam.uploadScore(board, steam.UPLOAD_KEEP_BEST, 100, details, onUp, ctx); }
    fn onUp(up: &Uploaded, ctx: u64) { ... up.ok, up.rank ... }
    steam.downloadScores(board, steam.DOWNLOAD_GLOBAL, 1, 10, onScores, ctx);
    fn onScores(s: &Scores, ok: bool, ctx: u64) { for i in 0..s.count { let e = s.entry(i); ... } }

- Every call is a call result: the handler runs on a later frame (on the next `frame()` with `ok` false when Steam is absent).
- `Uploaded` and `Scores` are valid only inside the handler. `Scores` is empty after it returns. Copy what you need.
- Also: `findBoard`, `downloadScoresForUsers(board, ids, f, ctx)`, `boardName`, `boardEntryCount`, `Scores.entryDetails(i, out)`. Up to 64 detail values per score (`DETAILS_MAX`).
- Fake: `fakeScore(board, user, score)` adds a rival.

## Friends, avatars, presence, join

    steam.userId()  steam.userName()  steam.friendCount(steam.FRIENDS_IMMEDIATE)  steam.friendAt(i, flags)  steam.friendName(id)
    steam.avatar(id, size) -> image handle (0 until it loads; steam.onAvatar(f) says when)
    steam.imageSize(img)  steam.imageRgba(img, out)
    steam.presence("status", "In a lobby")   steam.presence("connect", "+connect_lobby 123")   steam.clearPresence()
    steam.friendPresence(id, key)
    steam.onJoinRequest(fn(friend: u64, connect: string))
    steam.launchCommandLine()   steam.connectLobbyArg()

- The presence keys `status` and `connect` are special: friends see `status`, and "Join Game" uses `connect`.
- A join accepted while the game runs arrives in `onJoinRequest`. On a cold start the string is in `launchCommandLine()`; `connectLobbyArg()` reads `+connect_lobby <id>` from it or from `sys.arg` (0 when none). After `init`, call `joinLobby(steam.connectLobbyArg(), ...)`.

## Overlay, URLs, store

    steam.overlay("friends")             // also "community", "players", "settings", "officialgamegroup", "stats", "achievements"
    steam.overlayToUser(dialog, id)      steam.openUrl(url, modal)   steam.openStore(appId, flag)
    steam.overlayEnabled()  steam.overlayOpen()  steam.setNotifyPosition(pos)  steam.setNotifyInset(x, y)

- Steam draws the overlay into the GL swap. Nothing to call to get it. Shift+Tab opens it.

## Utils, Deck, Big Picture, keyboards

    steam.onDeck()  steam.bigPicture()  steam.country()  steam.battery()  steam.secondsSinceActive()
    steam.textInput(mode, x, y, w, h)    steam.dismissTextInput()    steam.onTextInputClosed(f)
    steam.gamepadTextInput(mode, lineMode, desc, maxChars, existing)    steam.onGamepadText(fn(ok: bool, text: string))
    steam.fakeDeck(true)    steam.fakeKeyboardCalls()    steam.fakeKeyboardHides()   // fake mode

- `textInput` is the floating keyboard over a box; typed text arrives as normal key input. `attach` calls it for `.j3ui` text boxes. `gamepadTextInput` is the full-screen box and returns its text in `onGamepadText`.
- When the player closes the floating keyboard the box keeps focus. The keyboard comes back when a text box gains focus again, or call `textInput` from `onTextInputClosed`.
- Typing on the Deck keyboard was not tried on hardware here.

## Language

    i18n.setLocale(steam.locale())      // "en" when Steam is absent or the language is unknown
    steam.lang()                        // Steam's name: "english", "schinese", ...
    steam.localeOf("schinese")

## Screenshots

    steam.screenshot()                  // trigger one
    steam.hookScreenshots(true)         // the game takes them: Steam sends a request instead of capturing
    steam.onScreenshotRequest(f)        // your own handler; otherwise the window is written for you
    steam.captureWindow(): u32          steam.addScreenshot(path, thumbPath, w, h)
    steam.onScreenshotReady(fn(handle: u32, ok: bool))   steam.screenshotLocation(handle, place)
    steam.setCapture(size, read)        // console programs: tell the default capture how to read pixels

`attach` sets the capture to the window (`j3.game.readWindow`). `writeScreenshot` is in the full list.

## Timeline

Marks for the Steam recording timeline. `timelineMode(mode)` (1 playing, 2 staging, 3 menus, 4 loading); `timelineTooltip(desc, delta)`, `timelineClearTooltip(delta)`; `timelineEvent(title, desc, icon, priority, offset, clip)` returns a handle; `timelineRange(...)`, or `timelineStart` / `timelineUpdate` / `timelineEnd` for a range you close later; `timelineRemove(handle)`. The method set follows SDK 1.6x and changed between versions; `steam-check` verifies it.

## Apps, DLC, ownership, betas

    steam.owns()  steam.ownsApp(id)  steam.dlcCount()  steam.dlcAt(i)  steam.dlcInstalled(id)
    steam.installDlc(id)  steam.uninstallDlc(id)  steam.dlcProgress(id)  steam.onDlcInstalled(f)
    steam.beta()  steam.buildId()  steam.appOwner()  steam.appInstalled(id)  steam.installDir(id)
    steam.purchaseTime(id)  steam.freeWeekend()  steam.familyShared()  steam.lowViolence()  steam.vacBanned()
    steam.languages()  steam.launchParam(key)  steam.onLaunchParams(f)  steam.markCorrupt(missingFilesOnly)

`dlcAt(i).name` and other returned strings are copies valid until the next frame. Fake mode owns the game and has two DLCs (1001 installed, 1002 not until `installDlc`).

## Steam Cloud

Two ways. Use the first unless you need control.

1. Auto-Cloud, no code. Save into `save.userDir("mygame")` (`use j3.data.save;`; call it once, it lives for the process). On the partner site (Application > Steam Cloud) add a root path for each OS that matches it:
   - Linux: root `LinuxHome`, subdirectory `.local/share/<game>` (`userDir` uses `$XDG_DATA_HOME/<game>` when that variable is set; then use that path or set up the root to match).
   - Windows: root `WinAppDataRoaming`, subdirectory `<game>` (`%APPDATA%\<game>`).
   Set the byte quota and file count, and a file pattern such as `*.save`.
2. The file API, for games that decide what syncs: `cloudOn()`, `cloudSetApp(on)`, `cloudWrite(name, data)`, `cloudRead(name, mem)`, `cloudExists`, `cloudPersisted`, `cloudDelete`, `cloudForget`, `cloudSize`, `cloudTime`, `cloudCount`, `cloudAt(i)`, `cloudQuota()`, `cloudBatch(on)` (wraps many writes in one batch), `cloudWriteAsync` and `cloudReadAsync`.
   `cloudSync(name, localPath, mem)` keeps one file in step: the newer side wins by whole-second time; a missing side is copied; equal times with different bytes push local and keep the cloud bytes as `<localPath>.cloud.bak`. A replaced local file is kept as `<localPath>.bak`. Call it at start before loading the save, and after saving. It is for games that do not use Auto-Cloud (using both would sync the same file twice).

## Steam Input

    steam.inputInit(true)                         // explicitFrame: frame() polls for you
    steam.inputManifest("/abs/path/input.vdf")    // dev: where the Input Action Manifest is
    steam.controllers(out): int     steam.inputType(h)     steam.controllerForGamepad(i)     steam.gamepadForController(h)
    steam.actionSet(name)  steam.activateSet(h, set)  steam.activateLayer / deactivateLayer / deactivateAllLayers(h)
    steam.digitalAction(name)  steam.analogAction(name)
    steam.digital(h, action): Digital      steam.analog(h, action): Analog      steam.motion(h): Motion
    steam.vibrate(h, left, right)  vibrateEx  hapticEvent  led  stopMomentum
    steam.digitalOrigins(h, set, action, out)  analogOrigins  digitalName  analogName
    steam.glyphPng(origin, steam.GLYPH_MEDIUM, 0): string   glyphSvg  originName  xboxOriginName  originFromXbox  translateOrigin
    steam.onPadConnected / onPadDisconnected / onPadConfig / onPadSlot

- Without this, Steam already shows configured controllers to SDL as ordinary gamepads, so `j3.game` pads and `j3.game.input` work. Use Steam Input for action sets, per-game bindings, glyphs and motion.
- Write a game actions manifest (`game_actions_<appid>.vdf`) and upload it on the partner site (Steam Input); `inputManifest` points a dev run at a local copy.
- Digital and Analog reads return small structs by value. The calling convention is handled in the module (SysV x86-64 and Win64 are built; arm64 is reasoned, not run). `motion` returns a large struct and works on x86-64 only (`sys.ARCH == sys.ARCH_X86_64`); elsewhere it logs once and returns zeros.
- Glyph paths are valid until the next frame. Fake mode has no glyph files; its first origin is 1.
- Fake pads: handle 1 is an Xbox One pad, handle 2 a Steam Deck. `fakePress(h, action, on)`, `fakeAnalog`.
- For `.j3ui` menus: `use j3.steam.game.PadMap; use j3.steam.ui.feedPad;`, build a `PadMap { menuSet, up, down, left, right, a, b, lb, rb }` from `actionSet` and `digitalAction`, then each frame `feedPad(&dom, h, map)` before the root frame fn. `h` 0 means unplugged. `j3.steam.game.padRaw(h, map)` gives the `PadRaw` if you feed `Dom.setPad` yourself.

## Lobbies

    steam.createLobby(steam.LOBBY_FRIENDS_ONLY, 4, onCreated, ctx)       // fn(lobby: u64, ok: bool, ctx: u64)
    steam.filterString(key, value, steam.CMP_EQUAL); steam.filterSlots(1); steam.findLobbies(onList, ctx)   // fn(count, ok, ctx); steam.lobbyAt(i)
    steam.joinLobby(lobby, onJoined, ctx)       // fn(lobby, response: int, ok: bool, ctx); steam.ENTER_SUCCESS
    steam.leaveLobby(lobby)   steam.memberCount  memberAt  memberLimit  setMemberLimit  lobbyOwner  setLobbyOwner  setLobbyType  setJoinable
    steam.lobbyData / setLobbyData / deleteLobbyData / lobbyDataCount / lobbyDataAt     steam.memberData / setMemberData
    steam.sendLobbyMsg(lobby, data)   steam.lobbyMsg(lobby, chatId, out)
    steam.inviteToLobby(lobby, user)   steam.overlayInvite(lobby)
    steam.onLobbyData / onLobbyMembers / onLobbyMsg / onLobbyServer / onLobbyInvite / onLobbyKicked / onLobbyJoinRequest

- Filters apply to the next `findLobbies` only. `onLobbyJoinRequest(fn(lobby, friend))` fires when a friend's invite is accepted; answer with `joinLobby`.
- Cold start from an invite: `connectLobbyArg()` (see Friends).
- Limits: key 255 bytes, value 8191, chat message 4096.
- Fake helpers: `fakeLobbyJoin(lobby, user)`, `fakeLobbyLeave`, `fakeLobbyMsg`, `fakeLobbyResult`.

## Networking messages and sockets

Both need the relay network; call `steam.relayInit()` early at startup. Messages (`ISteamNetworkingMessages`) are connectionless:

    steam.onNetRequest(fn(from: u64) { steam.netAccept(from); })
    steam.netSend(user, data, steam.SEND_RELIABLE, channel): i32      // steam.NET_OK on success
    steam.netReceive(channel, onMsg, ctx, max)   // fn(from, data, flags, ts, ctx); data valid only in the handler
    steam.netClose(user)  netCloseChannel  netSessionState  netSessionInfo  onNetFailed

- The first message from a user raises `onNetRequest`; call `netAccept` inside it, or their messages are not delivered. Do not call `netReceive` from inside a receive handler.
- At most 512 KiB (`NET_MAX_MESSAGE`) per message; the fake inbox holds 128.

Sockets (`ISteamNetworkingSockets`) are connections:

    server: steam.listenP2P(port); in onConnStatus(conn, old, info), when info.state == NET_STATE_CONNECTING and info.listenSocket != 0: acceptConn(conn)
    client: steam.connectP2P(serverId, port)
    both:   NET_STATE_CONNECTED -> sendConn(conn, data, flags) and receiveConn(conn, f, ctx, max)
            NET_STATE_CLOSED_BY_PEER / NET_STATE_PROBLEM_LOCALLY -> closeConn(conn, reason, debug, linger)
    poll groups: pollGroup(), setPollGroup(conn, group), receiveGroup(group, f, ctx, max), destroyPollGroup

- Handles are `u32`, 0 is never valid. Views of data are valid only in the handler.
- Also: `connInfo`, `connStatus` (ping, quality, rates), `setConnData`/`connData`, `setConnName`/`connName`, `flushConn`, `myIdentity`, `initAuth`, `authStatus`, `onRelayStatus`, `relayStatus`, `netTimestamp`.
- Fake mode loops connections back in process: `fakeNetDeliver(from, channel, data)`, `fakeNetEcho(on)`, `fakeNetFail`. Two machines on two accounts were not tried.

## Any other callback

Anything not wrapped is still reachable:

    steam.onCallback(id, fn(param: ptr, size: i32) { ... })   // id is the SDK's k_iCallback; param valid only in the call
    steam.await(call, fn(param: ptr, size: i32, ioFailure: bool, ctx: u64) { ... }, ctx)   // a SteamAPICall_t from an SDK call

From outside the module you get the callback and its struct bytes. Read the struct with `steam.lay("Struct_t", "field")` and `rdU32` / `rdU64` / `rdPtr` / `rdBool` / `rdF32` (they respect pack 4 on Linux and pack 8 on Windows). A `call` of 0 runs the handler on the next `frame()` with `ioFailure` true. Add new layout rows to `pack.j3` so `steam-check` can verify them.

## Shipping

    j3c build --release          // with build_outputs = ["steam"] and [steam] set

Output in `target/dist/steam/`:

    content/linux/      the game folder + libsteam_api.so
    content/windows/    the game folder + steam_api64.dll + the DLLs the program needs
    scripts/app_build_<appid>.vdf     steamcmd build script (depots, FileMapping, branch)
    scripts/README.txt                the upload line, runtime notes
    scripts/build-sniper.sh           builds the game inside the sniper SDK container

`steam_appid.txt` is dropped from the depots if it is in `[dist] include`. Upload yourself. j3c never uploads:

    steamcmd +login <your-steam-user> +run_app_build "<abs path>/target/dist/steam/scripts/app_build_<appid>.vdf" +quit

The build account needs "Edit App Metadata" and "Publish App Changes". A build goes live only when `[steam] branch` is set. Only linux-x86_64 and windows-x86_64 targets produce content.

Linux runtime. On the partner site (Installation > General Installation > Launch Options) add a Linux entry and set the compatibility tool to Steam Linux Runtime 3.0 (sniper) if you want it. Sniper is glibc 2.31. Today's host builds need GLIBC_2.34 (`__libc_start_main`), so a normal build on a recent distro already exceeds `glibc_max = "2.31"`; j3c warns and names the symbol. Two ways out:
- Build for sniper: `J3_SRC=/path/to/j3 target/dist/steam/scripts/build-sniper.sh` (podman; builds j3c and then the game inside the sniper SDK image). Libraries sniper lacks (libssl.so.3, SDL add-ons) go in one folder, set as `[steam] runtime_libs`; they are copied beside the exe and J3 opens them from there first. The script was not run here (no image pulled); check that the image has cargo and clang.
- Or ship without sniper (leave the compatibility tool unset). That works on the Steam Deck and on distros with glibc 2.34 or newer. It does not work on older distros.

Windows: the zip logic copies the DLLs the program needs; `steam_api64.dll` is added from the SDK. `sys.env` works on Windows, so `J3_STEAM_FAKE` and `J3_STEAM_LIB` do too.

The depot is the only place the Steam library is bundled; `j3c` does not ship anything else from the build host.

## SDK check

The bindings were written from the public flat API, without the SDK. Run the checker first, whenever you download or update the SDK:

    j3c -e steam-check --sdk ~/steamworks_sdk_162        # zip root, or its sdk/ folder
    j3c -e steam-check --sdk DIR --no-compile            # symbols and calls only (no C++ compiler)
    j3c -e steam-check --sdk DIR --no-windows            # skip the mingw pack-8 pass

It needs `clang++` (or `g++`) and `x86_64-w64-mingw32-g++` for struct layouts. It prints every mismatch; treat each as real until you checked the header. Fixes:
- `X is not in steam_api_flat.h`: the symbol was renamed or removed. Edit the string literal at the file:line shown (the method/accessor literal in the part file).
- `J3 cast has N params, SDK has M` or `param i`: edit the `as fn(...)` cast at that line. Self is the first param.
- `Struct.field: ours N, SDK M (linux|windows, pack 4|8)`: edit the `addLayout` row in `j3c/stdlib/j3/steam/pack.j3` or `pack_net.j3` (pack4 is the Linux value, pack8 Windows).
- `callback id ours N, SDK M`: edit the `addCallback` row in `steam/callbacks.j3` and the `CB_*` const.
- const `ours N, SDK M`: edit the const named, or its row in the CONSTS table in `j3c/src/steamcheck/consts.rs`. A public const in neither CONSTS nor UNCHECKED there is an error.
- warning `header has ..._v0NN`: a newer interface version exists. Add an `addAccessor` row above the old one in `steam/fns.j3` after reading the SDK changelog.
- `probe error ... no member`: a layout row's field name does not map to a C++ member. Add an exception in `consts.rs::member`.

Rebuild `j3c` after editing the stdlib (it is embedded), or use `J3_STDLIB=$PWD/j3c/stdlib` while iterating. The first real run will likely show some values that were written from memory.

## Deck pass

Check each on a Deck:
- Every menu works with the controller alone, no mouse. Steam Input glyphs show the right buttons.
- The screen is 1280x800 (16:10). Use `game.setVirtualSize` so other shapes letterbox. Text is readable at that size; offer a text scale.
- Text boxes open the on-screen keyboard (`attach` does it for `.j3ui` boxes).
- Suspend and resume while playing, and while a save is writing, does not lose state. Start with no network (offline).
- The game starts from the Steam library without a launcher or a login window.

Steam Deck behaviour in this repo was reasoned from the docs; it was not run on a Deck.

## Store readiness

- Controller glyphs match the device (`steam.inputType`, `glyphPng`); text says "press (A)" with the right glyph.
- Focus: the game pauses or mutes when the overlay opens (`attach` pauses) and on window focus loss.
- Crash log: decide where it goes. Under `save.userDir(game)` is a good place and Auto-Cloud can ignore it with a file pattern.
- No absolute paths in saves or assets. Use paths relative to the executable's folder or `save.userDir`.
- License notices for what you ship: SDL2, SDL2_image, SDL2_mixer, SDL2_ttf (zlib), the Steamworks redistributable (follow the SDK license), and every font and sound asset.
- Offline start works. A missing Steam must not stop the game.
- Achievements, stats, cloud and the Input manifest are configured on the partner site and published before release.

## Out of scope

Workshop (UGC), microtransactions, and a macOS depot. The dispatch loop, `onCallback` and `await` are the base such a feature builds on.
