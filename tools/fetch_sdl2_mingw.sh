#!/bin/sh
# The SDL2 mingw dev kits for `[emit] target = "x86_64-pc-windows-gnu"` builds: now `j3c setup --sdl-only`
# (pinned versions, sha256-checked, into <cache>/sdl2-mingw). Kept for old instructions and scripts.
exec "${J3C:-j3c}" setup --sdl-only "$@"
