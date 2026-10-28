Android: build an APK, run it on a phone or emulator

Once:
- JDK 17 (javac, keytool) on JAVA_HOME or PATH. `zip` on PATH.
- The SDK at $ANDROID_HOME (default ~/Android/Sdk): build-tools 35.0.0, platforms;android-35, platform-tools (adb).
      sdkmanager "build-tools;35.0.0" "platforms;android-35" "platform-tools"
- The NDK, r27 or newer, at $ANDROID_HOME/ndk/<version>, or at ANDROID_NDK_HOME (then ANDROID_NDK_ROOT).
      sdkmanager "ndk;27.2.12479018"        (or the SDK Manager in Android Studio, or the zip from developer.android.com)
- SDL2 for Android, from the repo (needs cmake, ninja, curl):
      tools/android/build-sdl.sh
  It builds SDL2, SDL2_image, SDL2_mixer and SDL2_ttf for arm64-v8a and x86_64 (API 26) into
  ~/.cache/j3/android/sdl/<abi>/{lib,include}. A second run does nothing. J3_ANDROID_ABIS="arm64-v8a"
  builds fewer ABIs; J3_SDL_CACHE moves the cache.

j3.toml:

    [package]
    name = "mygame"
    version = "0.1.0"

    [emit]
    target = ["aarch64-linux-android", "x86_64-linux-android"]   # phones, and the emulator

    [dist]
    build_outputs = ["apk"]
    title = "My Game"                # launcher label. Default: the package name
    icon = "assets/icon.png"         # .png only

    [android]
    package = "com.example.mygame"   # Java package, 2+ segments, each starting with a letter. Default com.j3.<name>
    orientation = "landscape"        # landscape (default), portrait, sensorLandscape, sensorPortrait, fullSensor, unspecified
    native_libs = "android/libs"     # extra programs and libraries: android/libs/<abi>/lib<name>.so. Default none

- `apk` needs an Android triple in [emit] target, or the build stops.
- versionCode is major * 1000000 + minor * 1000 + patch from the first three numbers of `version`
  (0.1.0 is 1000, 1.2.3 is 1002003). Minor and patch must be below 1000, major below 2000.
- `[dist] include` (default: whichever of assets, scripts, scenes, locales exist) goes into the APK.

Build and run:

    j3c build --release                          target/dist/<name>-<version>.apk, debug-signed
    j3c -e android run [dir] [--serial S] [--secs N] [--no-build]

- The APK is signed with ~/.android/debug.keystore, which keytool makes on first use. There is no release signing yet.
- `-e android run` builds (unless --no-build), installs with `adb install -r`, starts <package>/.J3Activity, and prints
  the app's log until it exits or --secs passes (default 60). --serial picks one device when several are attached.
  With no device it stops with one line.
- By hand: `adb install -r target/dist/<name>-<version>.apk`, then `adb shell am start -n <package>/.J3Activity`.
- `j3c run` refuses an Android target. Run the APK on a device or emulator.

Extra programs and libraries ([android] native_libs):
- Every file in `<native_libs>/<abi>/` (arm64-v8a, x86_64) goes into the APK's `lib/<abi>/` beside libmain.so. A
  missing <abi> folder is skipped. Each file must be named `lib<name>.so` (Android only extracts those), and not
  libmain.so or an SDL library name; otherwise the build stops with one line.
- At runtime `sys.env("J3_NATIVE_LIB_DIR")` is the app's nativeLibraryDir, where those files are.
- To run a bundled program (a static executable such as Stockfish, renamed libstockfish.so), spawn
  `$J3_NATIVE_LIB_DIR/libstockfish.so` with `j3.proc` (fork/exec). Android 10+ refuses to exec from app storage,
  so a program copied there will not start.

What differs on Android:
- Assets: the include folders are inside the APK. On first launch, and again whenever a listed file changes, they are
  copied to the app's internal storage and the program starts there. Relative paths, scenes, fonts and saves work as on
  desktop. The copy is listed in logcat.
- `j3.sys.PLATFORM` is 3 (0 Linux, 1 Windows, 2 macOS).
- OpenGL ES 3.0; `game.isGles()` says so. Write shaders as GLSL 330 core: `game.prepGl` rewrites them for ES.
- Touch: `game.fingerCount()` and `game.finger(i)` (`Finger { id, x, y, down, held, up }`, 10 slots, oldest first).
  Touch also arrives as mouse events; a tap whose down and up land in one frame still gives `mousePressed` that frame
  and `mouseReleased` the next.
- Screen density: with `App.hiDpi()` (`game.setHiDpi(true)`), `game.dpiScale()` is densityDpi / 160 (3 on a 480 dpi
  phone), so 2D and `.j3ui` lay out in Android dp. `J3_UI_SCALE` still overrides it. Without hiDpi it stays 1.
- `game.backgrounded()` is true from SDL's background event to its foreground event. `App.run` then skips update, draw
  and swap. A hand-written loop should call `game.waitBackground()` instead.
- Back key: with `game.holdClose(true)`, `game.closeRequested()` is true on that frame. Otherwise the app quits.
- `game.textInput(true)` opens the on-screen keyboard. `j3.ui` text fields call it on focus.
- Network: `j3.net` (tcp, dns, tls, http) works; the APK always asks for INTERNET. An app that uses https needs
  OpenSSL built once with `tools/android/build-openssl.sh` (~/.cache/j3/android/openssl, J3_OPENSSL_CACHE moves it);
  the APK then carries libssl.so and libcrypto.so (about 6.5 MB per ABI), since apps may not load the system's.
  Certificates are checked against the phone's CA store.

Testing on Linux, with no device:
- `J3_GLES=1` gives an ES 3.0 context on desktop. Headless ES needs `app.useGl()`. For the strict Mesa check, add
  `__EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json LIBGL_ALWAYS_SOFTWARE=1`.
      SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy timeout 120 j3c run <dir> --headless --frames 60
- `J3_GL_INFO=1` prints GL_VERSION, GL_SHADING_LANGUAGE_VERSION and GL_RENDERER (stderr; SDL_Log on Android).
- `J3_GL_CHECK=1` prints the glGetError code seen after each frame.
- On a device: `adb logcat -d -s SDL` for the SDL_Log lines (asset copy, GL info), `adb logcat -d` for a crash.

Not on Android yet:
- GL context loss on resume is not handled.
- Release signing and AAB (Play Store). Only debug-signed APKs.
- The editor and the MCP tools.
