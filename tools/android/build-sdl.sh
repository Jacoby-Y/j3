#!/usr/bin/env bash
# Build SDL2, SDL2_image, SDL2_mixer and SDL2_ttf for Android (one-time, cached).
#
#   tools/android/build-sdl.sh
#
# Needs the Android NDK (ANDROID_NDK_HOME, ANDROID_NDK_ROOT, or the highest
# $ANDROID_HOME/ndk/*, ANDROID_HOME defaulting to ~/Android/Sdk), cmake, ninja, curl.
# Downloads pinned release tarballs (sha256 checked), builds shared libs for each ABI
# with the NDK toolchain file (android-26, Release, stripped, vendored deps, no libc++_shared),
# and installs:
#   $J3_SDL_CACHE/<abi>/{lib,include}   (default ~/.cache/j3/android/sdl)
#   $J3_SDL_CACHE/java/org/libsdl/app/  (SDLActivity & co, same SDL2 version as libSDL2.so)
# Sources and build trees live in ~/.cache/j3/android/build. A version stamp makes a
# second run a no-op; delete $J3_SDL_CACHE/<abi>/.stamp to force one ABI to rebuild.
# Overrides: J3_ANDROID_ABIS (default "arm64-v8a x86_64"), J3_SDL_CACHE.
set -euo pipefail

SDL2_VER=2.32.10;  SDL2_SHA=5f5993c530f084535c65a6879e9b26ad441169b3e25d789d83287040a9ca5165
IMG_VER=2.8.8;     IMG_SHA=2213b56fdaff2220d0e38c8e420cbe1a83c87374190cba8c70af2156097ce30a
MIX_VER=2.8.1;     MIX_SHA=cb760211b056bfe44f4a1e180cc7cb201137e4d1572f2002cc1be728efd22660
TTF_VER=2.24.0;    TTF_SHA=0b2bf1e7b6568adbdbc9bb924643f79d9dedafe061fa1ed687d1d9ac4e453bfd
API=26
STAMP="sdl2=$SDL2_VER image=$IMG_VER mixer=$MIX_VER ttf=$TTF_VER api=$API cfg=2"

ABIS=${J3_ANDROID_ABIS:-"arm64-v8a x86_64"}
CACHE=${J3_SDL_CACHE:-$HOME/.cache/j3/android/sdl}
BUILD=$HOME/.cache/j3/android/build
export GIT_TERMINAL_PROMPT=0

# --- NDK
NDK=${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}
if [ -z "$NDK" ]; then
  NDK=$(ls -d "${ANDROID_HOME:-$HOME/Android/Sdk}"/ndk/*/ 2>/dev/null | sort -V | tail -n1 || true)
  NDK=${NDK%/}
fi
if [ -z "$NDK" ] || [ ! -f "$NDK/build/cmake/android.toolchain.cmake" ]; then
  echo "error: no Android NDK found; set ANDROID_NDK_HOME or install one under ~/Android/Sdk/ndk/<version>" >&2
  exit 1
fi
for t in cmake ninja curl tar; do
  command -v "$t" >/dev/null || { echo "error: $t not found in PATH" >&2; exit 1; }
done

# --- skip when everything is current
todo=()
for abi in $ABIS; do
  [ "$(cat "$CACHE/$abi/.stamp" 2>/dev/null)" = "$STAMP" ] || todo+=("$abi")
done
if [ ${#todo[@]} -eq 0 ] && [ -f "$CACHE/java/org/libsdl/app/SDLActivity.java" ]; then
  echo "SDL for Android is up to date ($CACHE)"
  exit 0
fi

# --- sources
mkdir -p "$BUILD/dl" "$BUILD/src"
fetch() { # name ver sha
  local name=$1 ver=$2 sha=$3 tgz="$BUILD/dl/$1-$2.tar.gz" tag=release-$2
  local repo=SDL; [ "$name" = SDL2 ] || repo=SDL_${name#SDL2_}
  if [ ! -f "$tgz" ] || [ "$(sha256sum "$tgz" | cut -d' ' -f1)" != "$sha" ]; then
    timeout 900 curl -fL --retry 2 --connect-timeout 20 --max-time 900 -o "$tgz" \
      "https://github.com/libsdl-org/$repo/releases/download/$tag/$name-$ver.tar.gz" < /dev/null
  fi
  [ "$(sha256sum "$tgz" | cut -d' ' -f1)" = "$sha" ] || { echo "error: sha256 mismatch for $tgz" >&2; exit 1; }
  [ -d "$BUILD/src/$name-$ver" ] || timeout 300 tar xzf "$tgz" -C "$BUILD/src" < /dev/null
}
fetch SDL2 $SDL2_VER $SDL2_SHA
fetch SDL2_image $IMG_VER $IMG_SHA
fetch SDL2_mixer $MIX_VER $MIX_SHA
fetch SDL2_ttf $TTF_VER $TTF_SHA

# vendored freetype/harfbuzz are git submodules, absent from some tarballs
ttf=$BUILD/src/SDL2_ttf-$TTF_VER
if [ ! -f "$ttf/external/freetype/CMakeLists.txt" ] || [ ! -f "$ttf/external/harfbuzz/CMakeLists.txt" ]; then
  (cd "$ttf/external" && timeout 600 ./download.sh < /dev/null)
fi

# cmake <srcdir> <abi> <name> [opts...]: configure, build, install into $CACHE/<abi>
cm() {
  local src=$1 abi=$2 name=$3; shift 3
  local bd="$BUILD/$abi/$name"
  rm -rf "$bd"
  timeout 600 cmake -S "$src" -B "$bd" -G Ninja \
    -DCMAKE_TOOLCHAIN_FILE="$NDK/build/cmake/android.toolchain.cmake" \
    -DANDROID_ABI="$abi" -DANDROID_PLATFORM=android-$API -DANDROID_STL=c++_static \
    -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=ON \
    -DCMAKE_INSTALL_PREFIX="$CACHE/$abi" -DCMAKE_PREFIX_PATH="$CACHE/$abi" \
    -DCMAKE_FIND_ROOT_PATH_MODE_PACKAGE=BOTH \
    "$@" < /dev/null
  timeout 1800 cmake --build "$bd" -j"$(nproc)" < /dev/null
  timeout 300 cmake --install "$bd" < /dev/null
}

for abi in $ABIS; do
  [ "$(cat "$CACHE/$abi/.stamp" 2>/dev/null)" = "$STAMP" ] && continue
  echo "== $abi"
  rm -rf "${CACHE:?}/$abi"; mkdir -p "$CACHE/$abi"
  sdl2dir="$CACHE/$abi/lib/cmake/SDL2"
  cm "$BUILD/src/SDL2-$SDL2_VER" "$abi" SDL2 -DSDL_TEST=OFF -DSDL_STATIC=OFF -DSDL_SHARED=ON
  # only what the repo needs: stb for png/jpg, no libs that need a download (avif/tiff/webp/xmp/opus/flac/...)
  cm "$BUILD/src/SDL2_image-$IMG_VER" "$abi" image -DSDL2_DIR="$sdl2dir" \
    -DSDL2IMAGE_SAMPLES=OFF -DSDL2IMAGE_DEPS_SHARED=OFF -DSDL2IMAGE_VENDORED=ON \
    -DSDL2IMAGE_AVIF=OFF -DSDL2IMAGE_TIF=OFF -DSDL2IMAGE_WEBP=OFF -DSDL2IMAGE_JXL=OFF
  cm "$BUILD/src/SDL2_mixer-$MIX_VER" "$abi" mixer -DSDL2_DIR="$sdl2dir" \
    -DSDL2MIXER_SAMPLES=OFF -DSDL2MIXER_DEPS_SHARED=OFF -DSDL2MIXER_VENDORED=ON \
    -DSDL2MIXER_FLAC_LIBFLAC=OFF -DSDL2MIXER_MOD=OFF -DSDL2MIXER_MIDI_FLUIDSYNTH=OFF \
    -DSDL2MIXER_OPUS=OFF -DSDL2MIXER_WAVPACK=OFF -DSDL2MIXER_MP3_MPG123=OFF \
    -DSDL2MIXER_VORBIS=STB -DSDL2MIXER_MP3_MINIMP3=ON -DSDL2MIXER_WAVE=ON
  cm "$ttf" "$abi" ttf -DSDL2_DIR="$sdl2dir" \
    -DSDL2TTF_SAMPLES=OFF -DSDL2TTF_VENDORED=ON -DSDL2TTF_HARFBUZZ=ON
  # drop debug info (the libs go into every APK); dynamic symbols and SONAME stay
  "$NDK"/toolchains/llvm/prebuilt/*/bin/llvm-strip --strip-unneeded "$CACHE/$abi"/lib/*.so
  echo "$STAMP" > "$CACHE/$abi/.stamp"
done

# Java side must match libSDL2.so
rm -rf "$CACHE/java/org/libsdl/app"; mkdir -p "$CACHE/java/org/libsdl/app"
cp "$BUILD/src/SDL2-$SDL2_VER/android-project/app/src/main/java/org/libsdl/app/"*.java "$CACHE/java/org/libsdl/app/"
echo "done: $CACHE"
