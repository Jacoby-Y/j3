#!/usr/bin/env bash
# Build OpenSSL 3 (libssl.so + libcrypto.so) for Android (one-time, cached). An APK whose code uses
# j3.net.tls / https gets these two libraries packed beside libmain.so; Android does not let apps
# load the system's own libssl.
#
#   tools/android/build-openssl.sh
#
# Needs the Android NDK (found like build-sdl.sh), perl, make, curl. Downloads a pinned release
# (sha256 checked), builds shared libs for each ABI (android-26, no tests/docs, unversioned SONAME
# so the APK can carry them as lib*.so), and installs:
#   $J3_OPENSSL_CACHE/<abi>/lib/{libssl.so,libcrypto.so}   (default ~/.cache/j3/android/openssl)
# A version stamp makes a second run a no-op. Overrides: J3_ANDROID_ABIS (default "arm64-v8a x86_64").
set -euo pipefail

VER=3.5.9; SHA=603f5602e2eef00d77fbd429d34dcd5822bb301757a1bc9cdb24c670f1eb859a
API=26
STAMP="openssl=$VER api=$API cfg=1"

ABIS=${J3_ANDROID_ABIS:-"arm64-v8a x86_64"}
CACHE=${J3_OPENSSL_CACHE:-$HOME/.cache/j3/android/openssl}
BUILD=$HOME/.cache/j3/android/build

NDK=${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}
if [ -z "$NDK" ]; then
  NDK=$(ls -d "${ANDROID_HOME:-$HOME/Android/Sdk}"/ndk/*/ 2>/dev/null | sort -V | tail -n1 || true)
  NDK=${NDK%/}
fi
if [ -z "$NDK" ] || [ ! -d "$NDK/toolchains/llvm/prebuilt" ]; then
  echo "error: no Android NDK found; set ANDROID_NDK_HOME or install one under ~/Android/Sdk/ndk/<version>" >&2
  exit 1
fi
for t in perl make curl tar; do
  command -v "$t" >/dev/null || { echo "error: $t not found in PATH" >&2; exit 1; }
done
BIN=$(ls -d "$NDK"/toolchains/llvm/prebuilt/*/bin | head -n1)

todo=()
for abi in $ABIS; do
  [ "$(cat "$CACHE/$abi/.stamp" 2>/dev/null)" = "$STAMP" ] || todo+=("$abi")
done
if [ ${#todo[@]} -eq 0 ]; then
  echo "OpenSSL for Android is up to date ($CACHE)"
  exit 0
fi

mkdir -p "$BUILD/dl"
tgz="$BUILD/dl/openssl-$VER.tar.gz"
if [ ! -f "$tgz" ] || [ "$(sha256sum "$tgz" | cut -d' ' -f1)" != "$SHA" ]; then
  timeout 900 curl -fL --retry 2 --connect-timeout 20 --max-time 900 -o "$tgz" \
    "https://github.com/openssl/openssl/releases/download/openssl-$VER/openssl-$VER.tar.gz" < /dev/null
fi
[ "$(sha256sum "$tgz" | cut -d' ' -f1)" = "$SHA" ] || { echo "error: sha256 mismatch for $tgz" >&2; exit 1; }

for abi in "${todo[@]}"; do
  case $abi in
    arm64-v8a) target=android-arm64 ;;
    x86_64) target=android-x86_64 ;;
    *) echo "error: unsupported ABI $abi" >&2; exit 1 ;;
  esac
  echo "== $abi"
  src="$BUILD/$abi/openssl-$VER"
  rm -rf "$src"; mkdir -p "$BUILD/$abi"
  timeout 300 tar xzf "$tgz" -C "$BUILD/$abi" < /dev/null
  (
    cd "$src"
    export ANDROID_NDK_ROOT=$NDK PATH="$BIN:$PATH"
    timeout 600 ./Configure "$target" -D__ANDROID_API__=$API shared no-tests no-docs no-apps no-engine < /dev/null
    # SHLIB_VERSION_NUMBER empty: SONAME libssl.so / libcrypto.so (an APK only carries lib*.so)
    timeout 1800 make -j"$(nproc)" build_libs SHLIB_VERSION_NUMBER= SHLIB_EXT=.so < /dev/null
  )
  rm -rf "${CACHE:?}/$abi"; mkdir -p "$CACHE/$abi/lib"
  cp "$src/libssl.so" "$src/libcrypto.so" "$CACHE/$abi/lib/"
  "$BIN/llvm-strip" --strip-unneeded "$CACHE/$abi"/lib/*.so
  echo "$STAMP" > "$CACHE/$abi/.stamp"
done
echo "done: $CACHE"
