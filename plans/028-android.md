# Plan 028: Run J3 apps on Android

## Status

- **Priority**: P3
- **Effort**: XL
- **Risk**: HIGH for "done" (needs a real device or emulator); MEDIUM for each chunk, since every chunk can be checked on Linux
- **Depends on**: none (013 is a sibling port, but none of its steps are needed here)
- **Category**: portability
- **Planned at**: 2026-10-27

## Goal

A `j3.app`/`j3.game` project (2D, `.j3ui`, and `gfx`/`scene3d` 3D) builds into a signed debug `.apk` with
`[dist] build_outputs = ["apk"]`, installs with `adb`, and runs on an arm64 Android phone with touch input.
It should also run on an x86_64 emulator.

## Defaults chosen (change before running if you disagree)

- **ABIs**: `aarch64-linux-android` (phones) and `x86_64-linux-android` (emulator). `[emit] target` already takes
  a list (`project.rs:308`, built per triple in `cli.rs:1091`). **minSdk 26** (Android 8), target/compile SDK 35, which is installed.
- **SDL2 stays.** Android's Java side is SDL's own `SDLActivity` (from the SDL2 source tree). `j3c` emits a
  `libmain.so` that exports `SDL_main`, and the Android app loads it.
- **No Gradle.** The APK is built with SDK build-tools 35 (`aapt2`, `javac` against `platforms/android-35/android.jar`,
  `d8`, `zipalign`, `apksigner`) plus a debug keystore. This matches the repo's "no extra deps" rule.
- **Toolchain**: the NDK's clang (found through `ANDROID_NDK_HOME`, then `ANDROID_NDK_ROOT`, then the highest
  `$ANDROID_HOME/ndk/*`). The system clang 18 is not used for Android.
- **libc instead of raw syscalls on Android.** `sys.j3` uses x86-64 syscall numbers (`sys.j3:15-52`, e.g. `open`=2,
  `fork`=57), and arm64 has neither of those calls. New `sys_android.j3` copies the libc style of `sys_macos.j3`
  (0 `@syscall`, `use lib "c"`). That makes it independent of the CPU architecture.
- **Networking is a stub on Android at first** (pub names that fail, like `netsys_macos.j3`). Threads work: bionic
  pthreads, `pthread_mutex_t`/`cond_t` sizes match LP64 glibc (`thread.j3:54-83`).
- **GLES 3.0** context on Android. Shaders keep their source; one function rewrites `#version 330 core` at compile
  time. `J3_GLES=1` forces an ES context on desktop too, so the port is tested **headless on Linux** (Mesa
  supports ES 3.x offscreen).
- **Assets**: on first launch (and when the APK's version changes), files under the APK's `assets/` are copied to
  internal storage, and the program `chdir`s there. Then `io.j3`'s raw `open`, SDL_ttf, SDL_image and `.j3scene` loading
  keep working on plain relative paths, and no code changes per loader.
- **Touch**: SDL already turns touch into mouse events by default. Finger events and app lifecycle get an API
  on top of that. Losing the GL context on resume is out of scope (noted below).

## Current state (from survey, 2026-10-27)

- **Target plumbing**: the triple reaches clang as `--target=` (`emit/host.rs:137,238,292,367`). `TargetOs` is chosen by
  substring (`host.rs:545-564`); android is `Other`, which gets **no link flags and the Linux `sys`**.
  The stdlib variant is picked in `compile.rs:478-507` (`sys`, `threadsys`, `netsys`). Embedded files are listed in `stdlib/mod.rs:43-45,163-165`.
- **Output** is always an executable (`-o`, no `-shared` anywhere). Entry: `llvm.rs:511-531` makes `j3.sys.start` the host `@main`.
  `sys.j3:169` reads envp from `ARGV+(ARGC+1)*8`.
- **Crash handler** `llvm_crash.rs:6` is gated on `x86_64 && linux`. `x86_64-linux-android` matches that, so it **must be excluded**.
- **SDL**: core is linked (`game.j3:55 use lib "SDL2"`). image/mixer/ttf are `SDL_LoadObject`'d by Linux soname
  (`game.j3:277-329`, `libSDL2_ttf-2.0.so.0` etc.); on Android they are `libSDL2_ttf.so`.
- **GL**: `gfx.j3:17 use lib "GL"` only for `glGetError`. Everything else is loaded through `game.glProc` (`gfx/glcalls.j3`).
  The context is 3.3 core (`game/window.j3:59-63`).
- **Shaders**: about 15 inline `#version 330 core` sources in `game/p2.j3`, `game/p2fx.j3`, `gfx/shader.j3`, `gfx/fxprog.j3`,
  `gfx/post.j3`, `gfx/urp/forest.j3`, `gfx/urp/shaders.j3`, plus `.j3fx` effect files (`scene3d/fx.j3`).
  Not in GLES 3.0: `glDrawBuffer` (`gfx/glcalls.j3:236`, used `gfx/shader.j3:174`), and `GL_CLAMP_TO_BORDER` + `GL_TEXTURE_BORDER_COLOR` (`gfx/shader.j3`, shadow map).
- **Events** (`game/events.j3`): no `SDL_FINGER*`, no `SDL_APP_*` lifecycle events.
- **Packaging**: `[dist] build_outputs` → `Output { Zip, Deb }` (`project.rs:53-59`, `ALL` table), `dist.rs` match, and `dist/deb.rs` as the model for `dist/apk.rs`. Asset dirs: `dist.rs:20`.
- **Machine**: `~/Android/Sdk` has build-tools 34/35, platforms 34/35, platform-tools (`adb`), and an emulator dir.
  JDK 17 is installed. There is **no NDK, no cmdline-tools (`sdkmanager`), no system image, and no device attached**.

## Prerequisites (the user does these; nothing below runs without them)

1. Install an NDK (r27 or newer) and cmdline-tools, from Android Studio's SDK Manager or the cmdline-tools zip.
   Then `sdkmanager "ndk;27.2.12479018"`.
2. Optional, for the emulator: `sdkmanager "system-images;android-35;google_apis;x86_64"` and `avdmanager create avd ...`.
3. Chunk 2 downloads SDL sources (network).

## Chunks

Every chunk runs and is checked on Linux, headless. Running the APK on hardware is the last step, done by the user.

- [ ] 1. [sonnet, full] **Android target in the compiler and stdlib.** Add `TargetOs::Android` (triple contains `android`;
  it comes before the Linux arm) with a `release_flags` row. Exclude Android from `llvm_crash::supported`.
  Add the `module_path` picks for `sys_android.j3` (libc, following `sys_macos.j3`; bionic constants; `environ` from libc, not the argv
  layout), `threadsys_android.j3` (`sysconf`/`sigaltstack` through libc), and `netsys_android.j3` (failing stubs like
  `netsys_macos.j3`). Register them in `stdlib/mod.rs`. Every `sys_*` gets a `pub const` platform id that later
  chunks branch on. Reason for Sonnet: compiler plus a new OS layer where flag values differ per arch (`O_DIRECTORY`/`O_NOFOLLOW` differ on arm64).
  **Done when**: `j3c check` passes for `speculative/01`–`08` and `examples/desk-app` with `[emit] target = "aarch64-linux-android"`.
  A host build and `cargo test` match the baseline.
- [ ] 2. [sonnet, full] **SDL2 for Android, built once.** `tools/android/build-sdl.sh`: pinned SDL2 2.30.x, SDL2_image,
  SDL2_mixer and SDL2_ttf tarballs, checked by sha256. It builds them with the NDK's CMake toolchain file for `arm64-v8a` and `x86_64`
  (API 26, vendored deps for freetype/png/ogg, no other system libs) into `~/.cache/j3/android/sdl/<abi>/{lib,include}`,
  and copies `android-project/app/src/main/java/org/libsdl/app/*.java` to `~/.cache/j3/android/sdl/java/`. Reason for Sonnet: build-system debugging.
  **Done when**: 4 `.so` per ABI exist. `llvm-readelf -d` shows `SONAME libSDL2.so` (etc.) and NEEDED only Android system libs.
- [ ] 3. [sonnet, full] **Link `libmain.so`.** For an Android triple, `j3c build` uses the NDK clang
  (`--target=<triple>26`, NDK sysroot) and links `-shared -fPIC` against the chunk 2 libs (`-L ~/.cache/j3/android/sdl/<abi>/lib`).
  It writes `target/<slug>/lib<name>.so` and names the entry `SDL_main(argc, argv)` instead of `main` (`llvm.rs:511`).
  A missing NDK or missing SDL cache stops with one line that names the fix (the prerequisite or `tools/android/build-sdl.sh`).
  Reason for Sonnet: emit/host edits across several call sites.
  **Done when**: a scratch `j3c new --app` project with both triples builds two `.so` files. `llvm-readelf --dyn-syms` shows
  `SDL_main` exported, and NEEDED lists `libSDL2.so`, `libc.so`, `libm.so` and `libdl.so`, with no `GLIBC_` versions. The host build is unchanged.
- [ ] 4. [sonnet, full] **Runtime platform bits** (`game.j3`, `game/window.j3`, `gfx.j3`). Use the add-on sonames from the platform id
  (`libSDL2_ttf.so` ...). Request a GLES 3.0 context (`SDL_GL_CONTEXT_PROFILE_ES`) on Android or when `J3_GLES=1`, and expose
  `game.isGles()`. Get `glGetError` through `glProc` and drop `use lib "GL"`. Make it fullscreen on Android, with
  `SDL_HINT_ORIENTATIONS` from `j3.toml` (`[android] orientation`, default landscape). Reason for Sonnet: main-thread
  startup order, plus a public API.
  **Done when**: the desktop build is unchanged (the desk-app golden passes). `J3_GLES=1 j3c run ... --headless --frames 3` creates an ES context
  and logs the `GL_VERSION` (shaders may still fail here; chunk 5 fixes them).
- [ ] 5. [sonnet, full] **GLES shaders.** Add one prep function on the compile path that every shader goes through (stdlib and `.j3fx`):
  on ES, `#version 330 core` becomes `#version 300 es` plus `precision highp float; precision highp int;` and precision for
  `sampler2DShadow`/`sampler3D`/`sampler2DArray`. Replace `glDrawBuffer` with `glDrawBuffers` and `CLAMP_TO_BORDER` with edge clamp plus
  an in-shader range test for the shadow map. Fix any other ES compile errors the runs find (`int`/`float` mixing,
  `texture2D`, missing `out` precision, `GL_RGBA32F` render targets that need `EXT_color_buffer_float`). Reason for Sonnet: GLSL dialect
  edge cases across renderers.
  **Done when**: with `J3_GLES=1` headless, `examples/desk-app`, `examples/idle-breakout`, `examples/3d-editor-scene`
  and `examples/fps-shooter` each run 60 frames with no shader or GL errors. Their screenshots match the desktop GL ones within
  `j3c -e imgdiff` tolerance (or the differences are explained). The desktop goldens are unchanged.
- [ ] 6. [sonnet, full] **Assets on Android** (`sys_android.j3` / `game` startup). Before `main`, open `assets/j3assets.txt` (a file list
  plus build stamp, written by chunk 7) through `SDL_RWFromFile`, which reads APK assets.
  If `SDL_AndroidGetInternalStoragePath()/j3assets.stamp` differs, copy every listed file (directories made, write to a temp file then
  rename) and write the stamp. Then `chdir` there. Save files (`j3.data.save`) go there too. Reason for Sonnet: partial-write safety
  and init order.
  **Done when**: the code is unit-testable on Linux behind a seam (the copy routine takes a reader), and a test covers a fresh copy, a skipped
  copy on the same stamp, and a recopy on a new stamp. The Android build links.
- [ ] 7. [sonnet, full] **`apk` output** (`project.rs` `Output::Apk` + `ALL`, `dist.rs` arm, new `dist/apk.rs` following `dist/deb.rs`).
  It generates `AndroidManifest.xml` (package from `[android] package`, default `com.j3.<name>`; label and icon from the existing `[dist]`
  keys; `glEsVersion 0x00030000`; minSdk 26, target 35; `extractNativeLibs=true`) and a 3-line `J3Activity extends SDLActivity`
  whose `getLibraries()` lists SDL2, SDL2_image, SDL2_mixer, SDL2_ttf and main. Build steps: `javac` (SDL java + activity) against
  `android.jar`, `d8`, `aapt2 compile/link`, then add `classes.dex`, `lib/<abi>/*.so` for every Android triple built and
  `assets/` (the `dist.rs:20` dirs + `j3assets.txt`) with the repo's own zip writer. Then `zipalign` and `apksigner` with
  `~/.android/debug.keystore` (created with `keytool` if missing). Reason for Sonnet: multi-tool pipeline, file formats.
  **Done when**: `j3c build --release` of the scratch app plus `examples/desk-app` produces `dist/*.apk`, and `apksigner verify`, `aapt2 dump badging`
  (package, sdk 26/35, native-code arm64-v8a x86_64) and `unzip -l` (assets, libs, dex) all pass.
- [ ] 8. [haiku, light] **`j3c android` helper.** `j3c -e android run [dir]`: build the apk, then `adb install -r`,
  `adb shell am start -n <pkg>/.J3Activity`, and `adb logcat -s SDL j3` until the process exits (with `timeout`). It copies the command pattern of an
  existing `-e` subcommand in `cli.rs`. If no device is found, it stops with one line. Reason for Haiku: small CLI wiring over chunk 7.
  **Done when**: `j3c -e android run` with no device prints the one-line error and returns 1. `--help` lists it.
- [ ] 9. [sonnet, full] **Touch and lifecycle** (`game/events.j3`, `game.j3`, `j3.app`). Add `game.fingers()` (id, x, y in window
  pixels through the letterbox map, down/held/up) from `SDL_FINGERDOWN/MOTION/UP`, with touch-to-mouse left on by default.
  `SDL_APP_WILLENTERBACKGROUND` → `game.backgrounded()`, and `App.run` skips update, draw and swap while it is set.
  `SDL_APP_DIDENTERFOREGROUND` clears it. Android Back (`SDL_SCANCODE_AC_BACK`) → `closeRequested`. Text input: `textInput` shows the
  on-screen keyboard (SDL does this; check it is called on focus in `j3.ui.inputs`). Reason for Sonnet: event state machine plus public API.
  **Done when**: a stdlib test feeds synthetic `SDL_Event`s (pattern: `j3.ui.testkit`/`keys.parse`) and checks fingers, the background
  skip and Back. Desktop goldens are unchanged.
- [ ] 10. [haiku, light] **Docs.** A `j3c doc android` page in `src/doc.rs` covering prerequisites, `build-sdl.sh`, the j3.toml keys, `build_outputs = ["apk"]`,
  `j3c -e android run` and the limits below. Also an `AGENTS.md` table row, a line in `j3c doc ship`, a `plans/README.md` row, and an `[android]` key
  completion in `editors/vscode/toml.js`. Add each feature to the README's untested list.
  **Done when**: `j3c doc android` prints, and `j3c doc search android` finds it.
- [ ] 11. **User: on-device test.** Install the scratch app, `desk-app` and `fps-shooter`. Check that it starts, shows text, takes touch, plays audio,
  survives home and resume, and quits on Back. Bugs go back in as fix chunks.

## Out of scope (later)

- **GL context loss on resume.** SDL2 normally keeps the EGL context; if a device drops it, every GL object needs re-upload. Look at
  this after chunk 11 shows whether it happens.
- `j3.net`/`j3.proc` on Android. `proc` has no meaning there. `net` needs bionic `addrinfo` order (`ai_canonname` before `ai_addr`, unlike glibc).
- The editor and the MCP tools on Android. Play Store release signing and AAB.
- Multi-touch gestures and gamepad mapping beyond what SDL does.
