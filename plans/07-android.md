# Plan 07: Android: context loss, device runs, ship

**Status (2026-11-11): TODO.** APK builds here (3.9 MB hello, both ABIs, 4 s); before Play it needs: 16 KB page alignment (every `.so` is 4 KB today), stale statics on relaunch, GL reset handling, logcat output, targetSdk 36, release signing and AAB.

## Goal

A J3 game goes from `j3c build --release` to a signed `.aab` that Google Play accepts. It survives the Android lifecycle (home button, app switch, rotation, process death, relaunch, GL context loss) without a black screen or a crash, and its logs and crashes can be read. "Done" means the developer has played a real game (an `--template fps` project and `examples/desk-app`) on an arm64 phone and an x86_64 emulator, uploaded it to an internal testing track, and Play's pre-launch report showed no errors.

## Current state (verified)

Checked on 2026-11-11 on this machine. "Built" means I made a scratch project in `/tmp/claude-1000/andr/hello` (`j3c new --app`, `[emit] target` = both Android triples, `build_outputs = ["apk"]`) and ran `j3c build --release` with the installed `j3c 0.1.0`.

Toolchain on this machine:
- `$ANDROID_HOME=~/Android/Sdk`: build-tools 34.0.0 and 35.0.0, platforms android-34 and android-35 (**no android-36**), platform-tools (`adb`), NDK 27.2.12479018 (clang 18.0.3), emulator 34.2.16. **No `cmdline-tools` (no `sdkmanager`/`avdmanager`), no `system-images`, no AVD** (`~/.android/avd` is empty). `/dev/kvm` exists. JDK `javac`/`keytool`/`java` are on PATH; `zip` is there. No `bundletool`. `ANDROID_NDK_HOME`/`ROOT` are unset (j3c finds the NDK under `$ANDROID_HOME/ndk`). Verified: `ls`, `which`.
- SDL cache `~/.cache/j3/android/sdl/{arm64-v8a,x86_64}/lib` holds SDL2 2.32.10, image 2.8.8, mixer 2.8.1, ttf 2.24.0, plus `java/org/libsdl/app/*.java`. OpenSSL 3.5.9 cache has **arm64-v8a only**, so an https app built for both ABIs fails at the x86_64 copy step until `tools/android/build-openssl.sh` runs again. Verified: `ls`.
- `adb devices`: nothing attached. `~/.android/adbkey` is from 2023, so a phone was paired at some point.

Build pipeline:
- `j3c build --release` makes `target/<triple>/lib<name>.so` per triple, then `target/dist/<name>-<version>.apk` (`j3c/src/dist/apk.rs`): javac + d8 for SDL's Java, `J3Activity` (`j3c/src/dist/apk/J3Activity.java`) and `J3Service`; aapt2 for the manifest and icon; `zip` adds dex, libs and assets; `zipalign -p 4`; `apksigner` with `~/.android/debug.keystore`. Verified: built. `apksigner verify` passes (v2+v3), `aapt2 dump badging` shows `minSdk 26`, `targetSdk 35`, `uses-gl-es 0x30000`, permissions VIBRATE, INTERNET, POST_NOTIFICATIONS (always, even for a hello that uses none).
- APK layout: `lib/<abi>/libmain.so` plus the four SDL libs (about 4 MB per ABI before compression), `assets/j3assets.txt`, `res/mipmap/ic_launcher.png` (no density qualifier, so Android treats it as mdpi and scales it up). `android:extractNativeLibs="true"`, so the libs are stored compressed. Verified: `unzip -l`.
- **Every `.so` has 4 KB `LOAD` alignment**: `libmain.so` (both ABIs), all four SDL libs, `libssl.so`, `libcrypto.so`. Verified: `readelf -lW`. `emit/android.rs` `link_flags` passes no `-z max-page-size`, `build-sdl.sh` and `build-openssl.sh` pass no page-size option, and NDK r27 still defaults to 4 KB. `zipalign -c -P 16` passes, but only because the libs are compressed. The ELF alignment is what Play checks.
- NDK flags (`j3c/src/emit/android.rs`): `--target=<triple>26 --sysroot -fPIC`, link `-shared -Wl,--no-undefined -L<sdl> -ldl -lm -llog`; `GL` maps to `GLESv3`; entry `SDL_main` (`emit/llvm/entry.rs`). Only `aarch64` and `x86_64` map to an ABI (`abi()`); `armv7` is rejected. Verified: read the code and ran the tests in the file.
- Triple and datalayout: `emit/llvm.rs:63`/`:265` fall back to `x86_64-pc-linux-gnu` only when `opts.target` is empty, so an Android build always gets its explicit triple (plan 05 owns the host bug, and Android is not affected by it). Both sites always print the **x86-64 datalayout**, even for aarch64. I compiled a `.ll` with that layout through NDK clang 18 for `aarch64-linux-android26`: clang **silently replaces it** with the aarch64 layout, and only warns `-Woverride-module` about the triple (the IR says `aarch64-linux-android`, the flag says `...26`). Harmless today. A future clang that rejects a mismatched layout would break it. Verified: scratch `.ll`.
- `j3c -e android run` (`j3c/src/android_run.rs`): builds, `adb install -r`, `am start`, then `logcat --pid` until the app exits or `--secs` pass. Not run (no device).
- Crash handler: `emit/llvm_crash.rs` `supported()` excludes Android, and release builds are stripped (`libmain.so` is 238 KB), with no unstripped copy kept. Verified: read the code and the file sizes.

Runtime:
- `sys_android.j3` `start()`: copies the APK assets (`apkassets.boot`), `chdir`s to internal storage, calls `main()` and **returns without `exit(0)`** (desktop `sys.j3:576` calls `exit(0)`). SDL's `SDLMain.run` then calls `finish()`, and the process stays cached. On the next launch SDL calls `SDL_main` again **in the same process**, so every J3 `#[singleton]` and global (`HUD`, `G`, glyph sheet, caches holding dead GL ids, `BACKGROUNDED`, arenas) keeps its old value. Verified by reading `SDLActivity.java` (`SDLMain.run`, `onDestroy`) and `sys_android.j3`. Not seen on a device.
- stdout/stderr: `printLn`, `panic`, `@j3_abort` and `game.logLine` write to fd 1/2, which go to `/dev/null` on Android. Only `SDL_Log` calls reach logcat (`logLine` in `game/touch.j3`, the GL info in `game/window.j3`, the asset copy). **A panic or failed bounds check on a phone shows nothing in logcat** except the process dying. Verified: grep (`__android_log` appears nowhere; `-llog` is linked but unused).
- Lifecycle events (`game/touch.j3:129`): `SDL_APP_WILL/DIDENTERBACKGROUND` set `BACKGROUNDED`, the foreground events clear it, `TERMINATING` quits, `LOWMEMORY` logs. `App.run` (`app.j3:535`) skips update/draw/swap and calls `waitBackground` while backgrounded. **There is no hook for "going to background, save now".** SDL2 blocks its thread inside the next event pump after delivering the background events (`Android_PumpEvents_Blocking`, `SDL_ANDROID_BLOCK_ON_PAUSE` default on), so the app gets exactly one pass over those events. SDL itself pauses and resumes audio (`ANDROIDAUDIO`/`openslES`/`aaudio_PauseDevices`). Verified: SDL 2.32.10 source in `~/.cache/j3/android/build/src`.
- GL context loss: SDL keeps the EGL context across pause on its own. `android_egl_context_backup` saves it, and `android_egl_context_restore` makes it current again on resume. Only if that fails does SDL **create a new context and push `SDL_RENDER_DEVICE_RESET` (0x2001)**. J3 handles no `0x2001`/`0x2000` event anywhere (grep). Every GL name the stdlib holds would then be invalid. The result would be draws with dead ids, a black screen or a driver crash. Verified by reading SDL source and grep. How often a modern phone takes that path is **unverified**: GLSurfaceView's `setPreserveEGLContextOnPause` problem does not apply, because SDL never calls `eglTerminate` on pause.
- GLES: `game.prepGl` rewrites `#version 330 core` to `300 es` (`gfx/glsl.j3`, `stdlib/tests/glsl_test.j3`). Verified: `J3_GLES=1 J3_GL_CHECK=1` headless run of `j3c new --template fps` on this machine gives `OpenGL ES 3.2 NVIDIA`, 27 frames, no GL error lines. Not run on Mali, Adreno or the emulator's SwiftShader/ANGLE.
- Touch, Back, density, text input: `game.finger`, `closeRequested` under `holdClose` (`SDL_SCANCODE_AC_BACK`), `dpiScale` = densityDpi/160 under `hiDpi`, `game.textInput` → `SDL_StartTextInput`, IME composition through `SDL_TEXTEDITING` (`game/events.j3:208`), `textInputRect`. Checked on desktop only (`stdlib/tests/touch_test.j3`, per roadmap). Unverified on a device.
- Gamepads: `ensurePads` opens SDL game controllers 0 and 1 (`game/keymap.j3:176`); SDL's `SDLControllerManager` feeds Android InputDevices. HIDAPI and Bluetooth permissions are left out of the manifest. Unverified on a device.
- Saves: `j3.data.save.userDir` returns `"."` on Android (`data/save.j3:305`), which is the internal storage folder where the assets are copied too. `android:allowBackup="true"` therefore backs up the copied assets along with the saves. Verified: read the code.
- Networking: `netsys_android.j3` (bionic `addrinfo` offsets, BSD `EAI_*`, CA folder `/apex/com.android.conscrypt/cacerts` then `/system/etc/security/cacerts`), `threadsys_android.j3`, OpenSSL bundled when `libmain.so` contains `OPENSSL_init_ssl`. Verified: read the code.
- Not for Android: `j3.instance` (`instance.j3` makes raw x86-64 `@syscall`s with no platform guard, so the numbers are wrong on arm64 if a game links it), `j3.term.pty`/`tty`/`screen` (amd64 numbers), `j3.game.hotkey` (X11), `dragFiles` (XDND), Steam (a no-op without the client). Verified: grep for `instance.j3`; the others come from `AGENTS.md`.
- CI: no Android job in `.github/workflows`. Verified: grep.

**Roadmap contradiction, resolved.** `roadmap.md:148` says an arm64 Android 16 phone ran a probe APK (DNS v4+v6, http, https, bad certificates refused). `roadmap.md:149` says "no APK has been installed on a phone or emulator". Both lines first appear in the squash commit `f4229e4` (2026-10-28) and were only moved by `3ac040d`, so git cannot order them (`git log -S` on both phrases). The code settles it. `netsys_android.j3` holds fixes that only a device run finds: bionic's `ai_canonname`@24 before `ai_addr`@32, BSD `EAI_*` numbers, and OpenSSL 3 not reading Android's old-hash CA names. The OpenSSL cache exists only for arm64, which matches a single phone probe. **Line 149 is stale**: a networking probe APK was installed on a phone. What is still true is that no windowed/GL/touch game APK has been seen running on a device or emulator. Chunk 13 rewrites line 149 to say exactly that.

## Gaps and blockers

In order of how hard they block shipping:

1. **16 KB pages (Play blocker).** Play rejects new apps and updates targeting Android 15+ whose native libs are not 16 KB aligned (enforced since 2025-11-01; check the Play Console for the current date and any extension). Every `.so` here is 4 KB. On a 16 KB-page device (Pixel 8+ with the developer option, and newer devices) a 4 KB lib fails to load, so the app does not start.
2. **targetSdk.** The APK targets 35. Play's yearly rule (targetSdk within one year of the latest release, deadline each Aug 31) means **36 for new apps and updates after 2026-08-31**; confirm in the Console. `platforms;android-36` is not installed here. Targeting 36 changes three things that hit a game:
   - **Predictive back is on by default.** `onBackPressed` is not called, and `KEYCODE_BACK` stops reaching `dispatchKeyEvent`, which is how SDL2 makes `SDL_SCANCODE_AC_BACK`. `closeRequested` would never fire, and Back would exit the activity.
   - **Large screens (sw ≥ 600dp) ignore `screenOrientation` and resizability** unless the app declares `android:appCategory="game"`.
   - Edge-to-edge is already forced at 35.
3. **AAB and release signing.** Play takes only `.aab`. j3c signs only with the debug key, and Play rejects debug-signed uploads.
4. **Stale statics on relaunch** (`start()` has no `exit`). The second launch runs `main` over old globals and dead GL ids. This is likely the black screen people would actually hit after "Back, then reopen".
5. **No logs.** stdout/stderr go to `/dev/null`. Panics and aborts are invisible, and release `.so`s are stripped with no symbol copy kept, so tombstones and the Play crash reports have no names.
6. **GL context loss** (`SDL_RENDER_DEVICE_RESET` is not handled). It is rare on the SDL path, but when it happens the GL state is invalid with no recovery. Holders of GL state that would go bad (from grep for `glGen*`/`glCreate*`/`SDL_CreateTexture`):
   - `j3.game`: `HUD` (`game/p2.j3`, `p2init.j3`: p2 program, VAO/VBO, `genTextures`), glyph sheet (`game/atlas.j3` `sheetPlace`), file and canvas textures (`game/draw.j3` `glFileTexture`, `p2tex.j3` `hudUpload`; `Texture.raw` *is* the GL id), p2 effects, layers and backdrop FBOs (`game/p2fx.j3`), text bakes (`textbake.j3`), splash (`game/splash.j3`).
   - `j3.gfx`: programs, LitLocs and shadow map (`gfx.j3` `G`, `gfx/shader.j3`, `glprocs.j3` `LIT`/`LITS`), effect and post programs plus the pool (`gfx/fxprog.j3` `FXP`, `POST`, `POOL`), meshes (`gfx/upload.j3`: user-held `Mesh` VAO/VBO), textures (`upload.j3`), view targets (`gfx/view.j3`).
   - `j3.gfx.post` (`post/programs.j3`, `post/targets.j3`), `j3.gfx.urp` (`forest.j3`, `raytrace.j3`).
   - `j3.scene3d` (`scene3d/shared.j3` mesh/texture cache by path, `effects.j3` `EFFECTS`), `j3.ui.media` (texture cache by path), `j3.term.view`.
   - User code: any `Mesh`, `Texture`, `@shader` program it holds.
   - GL function pointers (`game.glProc`, `gfx.j3` `P`) stay valid, because EGL proc addresses do not depend on the context.
7. **Manifest and permissions.** INTERNET and POST_NOTIFICATIONS are always declared, which drives the Data safety form and looks odd for an offline game. `allowBackup` backs up the asset copy. The icon is a single mdpi PNG with no adaptive icon.
8. **Insets.** At target 35+ the app draws edge-to-edge. SDL sets `LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES` in fullscreen, so UI can sit under a notch, the camera hole or the gesture bar. SDL2 has no safe-area API (SDL3's `SDL_GetWindowSafeArea` is out of reach), and J3 exposes no insets.
9. **Save point.** There is no `App` hook on background. Android kills cached processes at will, so a game that does not save on `WILLENTERBACKGROUND` loses progress.
10. **Unrun on real hardware:** GL on Mali/Adreno, touch, Back, IME with `.j3ui` text fields (does the soft keyboard cover the field?), gamepads, rotation and multi-window resize, the x86_64 emulator, audio after resume.
11. **Small ones:**
    - x86_64 OpenSSL is not built here.
    - `j3.instance` makes wrong syscalls on arm64 if a game links it.
    - The IR triple has no API level (`-Woverride-module` warning), and the IR always carries the x86-64 datalayout.
    - No audio focus (music keeps playing over a phone call's ringtone app; SDL pauses only on background).
    - The asset copy doubles on-device storage (APK plus copy).

## Design decisions

- **No Gradle, no new crates.** Keep the hand-driven SDK tool chain in `dist/apk.rs`. AAB uses Google's `bundletool` jar, run with `java -jar`, which is a tool on the developer's machine like `aapt2` (not a code dependency). j3c finds it at `$J3_BUNDLETOOL`, else `$ANDROID_HOME/bundletool*.jar`, else `~/.cache/j3/android/bundletool-all-*.jar`. If it is missing, j3c fails with one line giving the download URL. Writing `BundleConfig.pb` by hand is not worth it.
- **16 KB everywhere, checked at build.** Link `libmain.so` with `-Wl,-z,max-page-size=16384`. Rebuild SDL with `-DANDROID_SUPPORT_FLEXIBLE_PAGE_SIZES=ON` plus the same linker flag (bump its `STAMP` `cfg=3` so caches rebuild), and OpenSSL with `LDFLAGS=-Wl,-z,max-page-size=16384`. `apk.rs` reads each packed `.so`'s ELF program headers (about 30 lines of Rust, no `readelf`) and stops the build with the file name when a `PT_LOAD` alignment is below 16384. That check also catches `[android] native_libs` from third parties. Keep `extractNativeLibs="true"` for the APK. Libraries stored compressed need no zip alignment, and bundletool aligns its own.
- **GL context loss: prevent, then restart; rebuild in place only if devices show it is needed.** The loss path is rare under SDL, and Android already kills background game processes all the time, so every game must already resume from its save. The default policy on `SDL_RENDER_DEVICE_RESET` is therefore:
  1. Run the app's background/save hook if it has not run yet.
  2. Log one line.
  3. Ask `J3Activity.j3Restart()` to relaunch the activity (a `FLAG_ACTIVITY_NEW_TASK|CLEAR_TASK` intent, then exit the process).

  That needs no GL bookkeeping and cannot draw with dead ids. A full in-place rebuild means each stdlib cache checks a `game.glGeneration()` counter and rebuilds lazily, and user handles get `App.onGlReset`. That is chunk 5, and it waits on real-device evidence from chunk 12 that the reset path happens on target phones. Rejected: replaying GL state (ES lets you `glBindTexture` an ungenerated name, but programs and VAOs cannot be re-created with the same names, and keeping CPU copies of every mesh and texture doubles memory). Rejected: `setPreserveEGLContextOnPause`, which only exists on `GLSurfaceView`, and SDL already behaves that way.
- **Exit after `main` on Android**, same as desktop. Every launch is then a fresh process, which removes the stale-statics class of bugs and the dead-GL-id-after-relaunch case at once. `exit(0)` runs on the SDL thread while `onDestroy` may be waiting in `mSDLThread.join()`. That is fine, since the process just ends. Call `exit` rather than `_exit` so stdio and the logcat pipe flush.
- **Logs: dup2 fd 1/2 into a pipe, with a reader thread calling `__android_log_write`** (tag = package name, stderr lines at `ERROR`, stdout lines at `INFO`). One place catches J3, SDL, OpenSSL and C library prints. `@j3_abort` and `panic` additionally call `__android_log_write` directly before dying, so the last message is not lost in the pipe.
- **Symbols.** Keep the unstripped `lib<name>.so` per ABI in `target/dist/symbols/<abi>/` and zip it as `<name>-<version>-symbols.zip` (the layout Play's "native debug symbols" upload takes). bundletool also embeds it under `BUNDLE-METADATA/com.android.tools.build.debugsymbols/`. Do not port the desktop crash handler. Android's `debuggerd` already writes tombstones (`adb logcat -b crash`), and `ndk-stack -sym target/dist/symbols/<abi>` turns them into names.
- **Permissions from what is linked, not from a byte scan.** The build knows the linked modules. INTERNET is declared when `j3.netsys` is linked, POST_NOTIFICATIONS when `j3.android.notify` is, VIBRATE when `j3.android` is. `[android] permissions = [...]` adds more. The existing `uses_tls` byte scan stays for OpenSSL packing (it works) but moves to the same module list if that turns out easy.
- **Release signing config.** Paths go in `j3.toml` and secrets never do: `[android] keystore = "~/keys/game.jks"`, `key_alias = "upload"`; passwords come from `J3_ANDROID_KS_PASS` / `J3_ANDROID_KEY_PASS` (else `keytool`'s prompt on a TTY, else error). With no `keystore`, a release build is still debug-signed and j3c says so in one line. AABs are signed with the upload key through `jarsigner` (JDK); Play App Signing holds the real app key.
- **targetSdk 36, minSdk stays 26.** Add `android:appCategory="game"`. Handle predictive back by registering an `OnBackInvokedCallback` in `J3Activity` (API 33+) that sends SDL the Back key (`SDLActivity.onNativeKeyDown/Up(KeyEvent.KEYCODE_BACK)`). Keep `enableOnBackInvokedCallback` unset. If device tests show SDL's own path still works at 36, drop the callback.
- **Insets as one `game.safeArea()` rect** (UI units, like `windowSize`) from `WindowInsets` (system bars + display cutout, API 30+; cutout only on 28/29), read through JNI the same way as `j3Vibrate`. Also add `game.keyboardInset()` (IME inset height) so a `.j3ui` app can lift a focused field. The `.j3ui` side is a CSS `env(safe-area-inset-*)` value only if that is cheap in `cssvalue`; otherwise apps pad from `safeArea()`.
- **Saves.** `userDir` stays `"."` for compatibility, but the docs say to use a subfolder. The manifest gets `android:dataExtractionRules` / `fullBackupContent` that exclude the copied asset folders (the `[dist] include` list) and `j3assets.stamp`. Without that rule, a restored backup could hold stale assets under a new stamp.
- **No armv7.** Play needs only 64-bit, 32-bit-only phones are a small, shrinking share, and the stdlib's bionic offsets were written for LP64. See Out of scope.

## Chunks

Each chunk is one sitting. Do them in order. 1 to 3 are small, independent and unblock device testing, so do them first. Every chunk ends with `cd j3c && J3C_NO_INSTALL=1 cargo test` green, the touched doc pages updated (`j3c/src/doc/pages/android.md`, `j3c help manifest` text in `src/helptext/manifest.md`, `j3c/AGENTS.md` rows), and a scratch APK built as in "Testing without the hardware".

### 1. Fresh process per launch, and a save hook

- Files: `j3c/stdlib/j3/sys_android.j3` (`start`: `main(); exit(0);`), `j3c/stdlib/j3/app.j3`, `j3c/stdlib/j3/game/touch.j3`.
- `App.onBackground(fn(&W))`: called once per transition into background (edge of `BACKGROUNDED`), before `waitBackground`. Add `game.enteredBackground()` (true one frame) for hand-written loops. Doc: "save here; the process may never come back".
- Done when a desktop test fakes `SDL_APP_WILLENTERBACKGROUND` (extend `stdlib/tests/touch_test.j3`, which already fakes app events) and sees the hook run exactly once, and `sys_android.j3` calls `exit`.
- Test: `touch_test.j3` via `cargo test` (find its runner with `grep -rn touch_test j3c/src`); `j3c check --host` on the fps template; build the scratch APK.

### 2. stdout and stderr to logcat

- Files: `j3c/stdlib/j3/sys_android.j3` (pipe + `dup2` + a pthread via `threadsys_android.j3` before `apkassets.boot`, so the copy log goes there too), `use lib "log"` for `__android_log_write`. The abort and panic paths: find where `@j3_abort`/`panic` write, in `emit/llvm/entry.rs` `abort_routine` and the C backend's `J3_FAIL`. On Android, add a direct `__android_log_write(ANDROID_LOG_FATAL, ...)` of the same text.
- The tag is the package name (`J3_PACKAGE`, set by `J3Activity.onCreate` with `Os.setenv` like `J3_NATIVE_LIB_DIR`). Lines are split on `\n` and long lines are cut at 4000 bytes (logcat's limit).
- `android_run.rs`: also print `logcat -b crash -d` lines for the pid when the app dies.
- Done when a scratch APK's `libmain.so` imports `__android_log_write` and `pipe`/`dup2` (`llvm-nm -D`), and IR/C goldens for Android abort text are updated if any exist.
- Test: `cargo test`; `j3c build --release` of a scratch app that panics on frame 3; inspect the symbols. The developer confirms the panic line in `adb logcat` (Verify step 2).

### 3. 16 KB page alignment and toolchain hygiene

- Files: `j3c/src/emit/android.rs` (`link_flags`: `-Wl,-z,max-page-size=16384`), `tools/android/build-sdl.sh` (`-DANDROID_SUPPORT_FLEXIBLE_PAGE_SIZES=ON -DCMAKE_SHARED_LINKER_FLAGS=-Wl,-z,max-page-size=16384`, `STAMP ... cfg=3`), `tools/android/build-openssl.sh` (same LDFLAGS, bump its stamp), `j3c/src/dist/apk.rs` (new `fn load_align(so) -> Result<u64>` parsing the ELF64 header and `PT_LOAD` `p_align`; error under 16384 naming the file and the fix: rerun the build script, or for `native_libs` relink with that flag), `j3c/src/emit/llvm.rs` `:63`/`:265` (print the target's datalayout: a small table for `aarch64*` and `x86_64*`; others keep today's string; coordinate with plan 05, which edits the same lines), and the Android IR triple with the API level (`aarch64-linux-android26`) so `-Woverride-module` goes away.
- Done when every `.so` in a fresh scratch APK reads `0x4000` in `readelf -lW`, and a unit test feeds `load_align` a tiny hand-made ELF with 4 KB and 16 KB `LOAD` headers.
- Test: rerun both build scripts (they download pinned tarballs, so allow up to 30 min, wrapped in `timeout`, in the foreground). `j3c build --release` the scratch app. `for f in $(unzip -Z1 app.apk 'lib/*'); do ...; done` with `readelf -lW`. Building with `J3_SDL_CACHE` pointed at an old 4 KB cache must fail with the new message.

### 4. GL reset: detect, test harness, restart policy

- Files: `j3c/stdlib/j3/game/state.j3` (`SDL_RENDER_TARGETS_RESET 0x2000`, `SDL_RENDER_DEVICE_RESET 0x2001`), `game/events.j3` / `game/touch.j3` (set `GL_LOST`, bump `GL_GEN`), `game/window.j3` (a desktop fake: `J3_GL_RESET_AT=N` deletes and re-creates the context with `SDL_GL_DeleteContext`/`SDL_GL_CreateContext` and pushes `0x2001` at frame N, which is what SDL's Android path does), `app.j3` (on loss: call the background hook if not yet called, then `game.glLostPolicy()`), `j3c/src/dist/apk/J3Activity.java` (`j3Restart()`: start `J3Activity` with `NEW_TASK|CLEAR_TASK`, then `Runtime.exit(0)`), `j3c/stdlib/j3/android.j3` (JNI call, as `vibrate` does).
- Policy: on Android, restart. On desktop under `J3_GL_RESET_AT`, the default is to quit with exit code 3 and the line `j3: GL context lost` (desktop drivers basically never do this). `game.glGeneration()` and `game.glLost()` are public for chunk 5.
- Done when `J3_GL_RESET_AT=10` on the fps template headless exits 3 after the hook ran, with no GL errors (`J3_GL_CHECK=1`) before that.
- Test: `SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy J3_GL_RESET_AT=10 timeout 120 j3c run --host --headless --frames 30`; the same with `J3_GLES=1`.

### 5. GL reset: rebuild in place (only after chunk 12 shows resets happen)

- Every stdlib holder in gap 6 keeps the `glGeneration()` it was built under. On first use after a change it drops its ids (no `glDelete*`, the names are dead) and rebuilds:
  - p2 programs, VAO/VBO and the glyph sheet (the sheet is a cache, so glyphs re-bake on demand).
  - The text cache, p2fx FBOs, gfx programs (`G`, `LIT*`, `FXP`, `POST`, `POOL`), the shadow map, view targets, `post` programs and targets.
  - The scene3d shared cache (reload from path), effects, and the `ui.media` cache (by path).
- `Texture` from `loadTexture(path)`: keep a path table indexed by GL id, re-upload under a new id, and map the old id to the new one in `p2`'s texture bind. Canvas textures and `gfx.upload` meshes cannot be rebuilt; they go through `App.onGlReset(fn(&W))`, and a game that sets that hook opts out of the restart policy.
- Files: the ones listed in gap 6, `app.j3`, `j3c doc` pages `android.md` and `testing.md`.
- Done when `J3_GL_RESET_AT=10 --frames 30 --screenshot` of the fps template and of `examples/desk-app` matches a no-reset capture within `j3c -e imgdiff --tol 2`.
- Test: that golden pair under both `J3_GLES=1` and desktop GL; `stdlib/tests/post_*render_test.j3` with a reset in the middle.

### 6. Manifest for targetSdk 36 and Play

- Files: `j3c/src/dist/apk.rs` (`TARGET_SDK` 36; `Tools::find` error names `platforms;android-36`), `manifest_xml`, `J3Activity.java`, `j3c/src/project.rs` (`[android] permissions`, `backup = false|true`), and the build call site that passes the linked module list into `apk::build` (`j3c/src/dist.rs`).
- Manifest changes:
  - `android:appCategory="game"`.
  - Permissions from the linked modules (see Design decisions).
  - `android:allowBackup` plus a generated `res/xml/backup_rules.xml` (`dataExtractionRules`, API 31+) and `fullBackupContent` (26 to 30) excluding the include folders and `j3assets.*`.
  - Drop `extractNativeLibs`? No: keep it true (the decision above).
  - `android:resizeableActivity="true"`.
- `J3Activity`: `OnBackInvokedCallback` on API 33+ forwarding Back to SDL; audio focus (`AudioManager.requestAudioFocus(AUDIOFOCUS_GAIN)` on resume, abandon on pause, and on loss call `nativePause`-like mixer pause through a J3-visible flag `game.audioFocusLost()`). Keep this last part small, or split it out if it grows.
- Done when the `manifest_contents` test checks the new attributes, a hello APK's badging shows `targetSdkVersion:'36'` and only VIBRATE, and an app using `j3.net.http` also shows INTERNET.
- Test: `cargo test`; `aapt2 dump badging`; `aapt2 dump xmltree --file AndroidManifest.xml`. Needs `platforms;android-36` installed by the developer; until then the test uses a fake jar path (the `Tools` struct is easy to fake).

### 7. Icons

- Files: `j3c/src/dist/apk.rs`, `j3c/src/project.rs`.
- `[dist] icon` PNG goes to `res/mipmap-xxxhdpi/ic_launcher.png` (Android downscales; 192 px or larger recommended; warn when smaller). Optional `[android] icon_foreground = "assets/fg.png"` (432 px, 108 dp with a 72 dp safe zone) and `icon_background = "#1e1e2e"` generate `res/mipmap-anydpi-v26/ic_launcher.xml` (adaptive, `<monochrome>` is the same foreground) and `res/values/colors.xml`. The 512 px Play Store icon is uploaded in the Console; the docs say so.
- Done when the badging of a scratch APK shows `application-icon-640` and an adaptive icon entry.
- Test: `cargo test` (extend `manifest_contents`), `aapt2 dump resources`.

### 8. Release signing

- Files: `j3c/src/project.rs` (`keystore`, `key_alias`), `j3c/src/dist/apk.rs` (`keystore()` picks release or debug; passwords from env via `--ks-pass env:J3_ANDROID_KS_PASS`, which apksigner supports, so no secret ever sits in argv), `j3c doc android` ("Signing": make an upload key with `keytool -genkeypair -keystore upload.jks -alias upload -keyalg RSA -keysize 4096 -validity 10000`, keep it out of git, enroll in Play App Signing).
- Done when, with a scratch keystore made in `/tmp/claude-1000/`, `apksigner verify --print-certs` shows that cert, and a release build with no `keystore` key prints one "debug-signed" line.
- Test: `cargo test` (a unit test for the key parse; the signing path is run by hand on the scratch project).

### 9. AAB output and debug symbols

- Files: new `j3c/src/dist/aab.rs` (share the staging with `apk.rs`; split `build_in` into "stage" and "pack" so both use it), `j3c/src/dist.rs` (`build_outputs = ["aab"]`; `["apk", "aab"]` allowed), `j3c/src/emit/android.rs` (keep the unstripped `.so`: link unstripped, copy to `target/dist/symbols/<abi>/libmain.so`, then `llvm-strip --strip-unneeded` the packed copy).
- Steps:
  1. `aapt2 link --proto-format` → `base.zip`.
  2. Lay out a module zip (`manifest/AndroidManifest.xml`, `dex/classes.dex`, `lib/<abi>/`, `assets/`, `res/`, `resources.pb`).
  3. `java -jar bundletool build-bundle --modules=base.zip --output=<name>-<version>.aab` with `--metadata-file=com.android.tools.build.debugsymbols/<abi>/libmain.so.sym:...`.
  4. `jarsigner -keystore ... -storepass:env J3_ANDROID_KS_PASS` with the upload key (debug key if none).
  5. Also write `<name>-<version>-symbols.zip`.
- Size: warn when the base module is over 200 MB (Play's download limit for the base module; check the current number). Play Asset Delivery is out of scope.
- `j3c -e android run --aab`: `bundletool build-apks --local-testing --connected-device` + `install-apks`, so the AAB itself gets device-tested.
- Done when `bundletool validate --bundle` passes on a scratch AAB, `bundletool build-apks --mode=universal` makes an APK whose libs are 16 KB aligned, and `jarsigner -verify` is OK.
- Test: needs the bundletool jar (developer downloads it once; the plan never installs it). Unit tests cover the zip layout with a fake bundletool path (`J3_BUNDLETOOL=/bin/false` gives the right error).

### 10. Safe area and soft keyboard inset

- Files: `J3Activity.java` (`j3Insets()`: `int[]{l,t,r,b,ime}` in pixels from `getWindow().getDecorView().getRootWindowInsets()`; `WindowInsets.Type.systemBars()|displayCutout()` and `ime()` on 30+, `getDisplayCutout()` on 28/29, zeros below), `j3c/stdlib/j3/android.j3` (JNI), `j3c/stdlib/j3/game/window.j3` (`pub fn safeArea(): Rect` in UI units, through the same mapping as `windowSize`/`physToUi`; the whole window off Android; `J3_SAFE_AREA=l,t,r,b` fakes it on desktop for tests), `game/events.j3` (`keyboardInset()`), and `j3.ui.inputs` (scroll the focused text field above `keyboardInset()` when it would be covered).
- Optional: CSS `env(safe-area-inset-top)` etc. in `ui/cssvalue.j3`, only if `env()` fits the existing value parser cleanly. Otherwise document padding from `game.safeArea()`.
- Done when `J3_SAFE_AREA=0,80,0,40` on a headless `.j3ui` scratch app moves a `safeArea()`-padded header (golden).
- Test: headless golden; `cargo test`.

### 11. Emulator path and x86_64

- Files: `j3c/src/android_run.rs` (`--emulator NAME`: start `$ANDROID_HOME/emulator/emulator -avd NAME -no-window -no-audio -no-snapshot -gpu swiftshader_indirect`, wait for `adb wait-for-device` + `getprop sys.boot_completed` = 1 with a timeout, then run as today and kill the emulator at the end), `j3c/src/doc/pages/android.md` (the one-time AVD setup commands for the developer), `tools/android/build-openssl.sh` (no change; doc: run it for both ABIs).
- Add `--screenshot F.png` (`adb exec-out screencap -p`) and `--frames N` passed through as an intent extra, which `J3Activity` turns into `J3_CAPTURE`/`J3_FRAMES` env vars via `Os.setenv`. A run can then end itself and be compared like a desktop golden.
- Done when `j3c -e android run --emulator j3x86 --secs 30 --screenshot s.png` on the fps template gives a non-black PNG (checked by `j3c -e imgdiff` against a black image, or a pixel stat).
- Test: needs a system image and an AVD (developer, Verify step 1). Without them the agent checks the argument parsing and the clean error for a missing AVD.

### 12. Real-device verification pass (developer with agent support)

- The agent writes `examples/android_probe/`: a small `App` with `hiDpi`. It shows:
  - fingers;
  - `safeArea()` as a frame;
  - a `.j3ui` text field at the bottom (IME test);
  - `closeRequested` count;
  - background/foreground counts and `glGeneration()`;
  - gamepad buttons;
  - a looping sound;
  - a `gfx` cube plus p2 text.

  It logs each event. A self-check mode `PROBE_TEST=1` runs headless on desktop.
- The developer runs "Verify on real hardware" steps 2 to 9 with it and reports logcat. Findings become fixes in the owning chunk. Whether chunk 5 is needed is decided here: did `glGeneration` ever change on a real resume?
- Done when the probe passes headless on desktop and the developer's report is folded into this plan.

### 13. Docs, roadmap and CI

- `j3c/src/doc/pages/android.md`: signing, AAB, symbols, 16 KB, insets, background hook, GL reset policy, logs; rewrite "Not on Android yet".
- `docs/src/` book page if one exists for Android (`grep -ril android docs/src`).
- `roadmap.md:149`: rewrite to "a networking probe APK ran on an arm64 Android 16 phone; no windowed/GL game APK yet" (then update it after chunk 12).
- `j3c/AGENTS.md` rows for `apk.rs`, `aab.rs`, `android.rs`.
- CI: a job in `.github/workflows/ci.yml` that installs the SDK cmdline-tools, NDK r27 and platform 36, runs `build-sdl.sh` (cached), builds the hello APK and AAB and runs the 16 KB check. Optionally an `reactivecircus/android-emulator-runner`-style x86_64 boot (KVM on GitHub Linux runners) running the probe for 30 s and failing on a `FATAL` logcat line. Mark it `continue-on-error` until it has been green for a week.
- Done when `j3c doc android` reads end to end with no stale line, and the workflow YAML passes `actionlint` if available (not run on GitHub by the agent).

## Testing without the hardware

What an agent can do here, with no device and no emulator image:

- Unit tests: `cd j3c && J3C_NO_INSTALL=1 timeout 600 cargo test` (`dist::apk`, `emit::android`, the new ELF and AAB tests).
- Build a scratch APK (both ABIs):
  ```
  mkdir -p /tmp/claude-1000/andr && cd /tmp/claude-1000/andr && j3c new hello --app
  # j3.toml: [emit] target = ["aarch64-linux-android", "x86_64-linux-android"]; [dist] build_outputs = ["apk"]
  J3C_NO_INSTALL=1 timeout 300 j3c build --release
  ```
  Use the `j3c` built in this worktree (`j3c/target/release/j3c`) when testing changes, not `~/.bin/j3c`.
- Inspect it: `BT=~/Android/Sdk/build-tools/35.0.0`; `$BT/aapt2 dump badging`, `$BT/aapt2 dump xmltree --file AndroidManifest.xml`, `$BT/apksigner verify --print-certs -v`, `$BT/zipalign -c -P 16 -v 4`, `unzip -l`, `readelf -lW` on each extracted `lib/*/*.so` (16 KB), `llvm-nm -D` (from the NDK) for imported symbols.
- Run the same program on the desktop as Android would see GL: `J3_GLES=1 J3_GL_CHECK=1 SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy timeout 120 j3c run --host --headless --frames 60 --screenshot s.png`. For a strict Mesa ES check add `__EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json LIBGL_ALWAYS_SOFTWARE=1`.
- Fake the lifecycle on desktop: the `touch_test.j3` event fakes (background, foreground, Back), `J3_GL_RESET_AT=N` (chunk 4), `J3_SAFE_AREA` (chunk 10), `game.backKeyAnywhere(true)`.
- Never run `adb install` here, even if a device shows up in `adb devices`. Leave device runs to the developer.
- Emulator in CI, once a system image exists: `emulator -avd X -no-window -no-audio -gpu swiftshader_indirect` plus `adb wait-for-device`. SwiftShader gives GLES 3.0 in software, which is good enough for "starts, draws something, no GL errors". It does not stand in for Mali or Adreno.

## Verify on real hardware (for the developer)

1. **Setup once.** Install the cmdline-tools from developer.android.com (puts `sdkmanager` in `$ANDROID_HOME/cmdline-tools/latest/bin`). Then:
   - `sdkmanager "platforms;android-36" "build-tools;36.0.0" "system-images;android-36;google_apis;x86_64"`
   - `avdmanager create avd -n j3x86 -k "system-images;android-36;google_apis;x86_64"`
   - `J3_ANDROID_ABIS="arm64-v8a x86_64" tools/android/build-openssl.sh`
   - Download `bundletool-all-<ver>.jar` from github.com/google/bundletool releases into `~/.cache/j3/android/`.
2. **Logs and statics (chunks 1-2).** `j3c -e android run` the probe on the phone. Press Back to quit, reopen from the launcher, and check that it starts fresh (counts at 0). Trigger the probe's panic button and see the panic line in `adb logcat -s <package>`.
3. **Lifecycle.**
   - Home, wait, reopen: picture back, sound back, `glGeneration` unchanged?
   - App switch 20 times quickly.
   - Turn on Developer options → "Don't keep activities", background, reopen.
   - Lock and unlock the screen.
   - Rotate (with `orientation = "fullSensor"`).
   - Split-screen / multi-window resize.
   - `adb shell am kill <package>` while backgrounded, then reopen: did the save hook's file survive?
4. **GL reset (chunk 4).** Watch for any `j3: GL context lost` line over a day of normal use, and with Developer options → "Force GPU... / ANGLE" variations if the phone offers them. This decides whether chunk 5 is needed.
5. **16 KB.** On a Pixel 8 or newer: Developer options → "Boot with 16 KB page size", reboot, `adb shell getconf PAGE_SIZE` shows 16384, run the probe.
6. **Input.**
   - Multi-touch with 5 fingers.
   - Back gesture and the 3-button nav Back: `closeRequested` should fire and the app stay up.
   - The `.j3ui` text field at the bottom: does the keyboard cover it? Do IME composition (Gboard swipe or a CJK input) and autocorrect commit properly?
   - A Bluetooth or USB gamepad: buttons and sticks, hot-plug.
7. **Screen.** Notch or camera-hole phone in both landscape orientations: `safeArea()` frame correct? Gesture bar area? A tablet or foldable (or emulator `pixel_tablet`) with targetSdk 36: orientation is respected thanks to `appCategory="game"`.
8. **Audio.** Background with music playing (silent?); a phone call or alarm while playing (audio focus); Bluetooth headphones latency.
9. **Network.** `examples/openrouter-chat` style https from a worker thread on the phone and on the x86_64 emulator.
10. **Play.**
    - Create the app in the Play Console and enroll in Play App Signing with the upload key from chunk 8.
    - Upload the `.aab` and `-symbols.zip` to Internal testing.
    - Fill Data safety (matches the permissions chunk 6 emits), content rating (IARC), target audience and a privacy policy URL (needed if INTERNET is declared and anything is collected).
    - Read the **pre-launch report** (Play's own device farm) for crashes, 16 KB warnings, accessibility and screenshot issues.
    - Install from the testing link on the phone.

## Out of scope

- **armv7 / x86 (32-bit).** Play does not require them. The bionic `addrinfo` offsets, `sys_android.j3` and the IR ABI (`ir/abi.rs` SysV LP64) assume 64-bit. Revisit only if a publisher needs Android Go devices.
- **SDL3** (native safe area, better lifecycle API). A separate migration for all platforms.
- **Play Asset Delivery / asset packs, Play Games Services, in-app billing, ads, Play Integrity.** Each one is a JNI-and-manifest job of its own, worth doing when a game needs it.
- **The editor and MCP tools on Android.**
- Linux-only modules: `j3.instance`, `j3.term.pty`/`tty`/`screen`, `j3.game.hotkey`, `dragFiles`, `j3.steam`. Not applicable on Android. Small follow-up (not a chunk): make `j3.instance` return `Err` when `sys.PLATFORM != PLATFORM_LINUX` so linking it on arm64 cannot make wrong raw syscalls.
- Vulkan, and Android-specific GPU tuning (ASTC textures, tile-friendly passes). Performance work comes after it runs correctly.
- Gradle / Android Studio project export.
