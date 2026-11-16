#!/bin/sh

# A special build script for me, the developer

# Release-build j3c and install it onto PATH, then the composer tool.
# Install dir: $J3_BIN_DIR, else ~/.bin if it exists, else ~/.local/bin.
set -eu
cd "$(dirname "$0")"
missing=
command -v cargo >/dev/null 2>&1 || { echo "build.sh: cargo not found (install Rust: https://rustup.rs)"; missing=1; }
command -v clang >/dev/null 2>&1 || { echo "build.sh: clang not found (j3c compiles through it; e.g. apt install clang)"; missing=1; }
[ -z "$missing" ] || exit 1
if [ -n "${J3_BIN_DIR:-}" ]; then bin=$J3_BIN_DIR
elif [ -d "$HOME/.bin" ]; then bin=$HOME/.bin
else bin=$HOME/.local/bin
fi
mkdir -p "$bin"
# Plan 11 A3: nothing from j3c-bootstrap/ (stage0, j3c-boot) is ever installed.
inst() { case "$(realpath "$1")" in */j3c-bootstrap/*) echo "build.sh: refusing to install $1 (from j3c-bootstrap/)" >&2; exit 1 ;; esac; install -m 755 "$1" "$2"; }
cargo build --release --manifest-path j3c/Cargo.toml
inst j3c/target/release/j3c "$bin/j3c"
echo "installed $bin/j3c"
# Composer (tools/composer): its own program, built with the j3c just installed.
"$bin/j3c" build tools/composer --release
inst tools/composer/target/composer "$bin/j3c-composer"
echo "installed $bin/j3c-composer"
