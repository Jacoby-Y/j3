#!/bin/sh
# Download the SDL2 mingw dev kits (x86_64 import libs + DLLs for SDL2, SDL2_image, SDL2_ttf,
# SDL2_mixer) into ~/.cache/j3/sdl2-mingw for `[emit] target = "x86_64-pc-windows-gnu"` builds.
# Prints the SDL2 lib dir (the one for [link] search) on line 1 and the DLL dir on line 2.
set -e
OUT=${J3_SDL2_MINGW:-$HOME/.cache/j3/sdl2-mingw}
V=${SDL2_VERSION:-2.30.9}
mkdir -p "$OUT"
get() { # repo dir tarball
    [ -d "$OUT/$2" ] || curl -fsSL "https://github.com/libsdl-org/$1/releases/download/release-${2##*-}/$3" | tar -xz -C "$OUT"
}
get SDL SDL2-$V SDL2-devel-$V-mingw.tar.gz
get SDL_image SDL2_image-2.8.2 SDL2_image-devel-2.8.2-mingw.tar.gz
get SDL_ttf SDL2_ttf-2.22.0 SDL2_ttf-devel-2.22.0-mingw.tar.gz
get SDL_mixer SDL2_mixer-2.8.0 SDL2_mixer-devel-2.8.0-mingw.tar.gz
mkdir -p "$OUT/dll"
cp "$OUT"/SDL2*/x86_64-w64-mingw32/bin/*.dll "$OUT/dll/"
echo "$OUT/SDL2-$V/x86_64-w64-mingw32/lib"
echo "$OUT/dll"
