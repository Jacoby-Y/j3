#!/usr/bin/env bash
# Disk a developer needs to build each app: the toolchain, downloaded dependencies, the build cache
# left after a release build (run ./bench.sh first), and the system libraries the build links against.
# Not counted: the system linker and libc headers (gcc/binutils/libc6-dev) that every AOT stack here uses.
set -uo pipefail
cd "$(dirname "$0")"

b() { du -scb "$@" 2>/dev/null | tail -1 | cut -f1; }              # bytes on disk
pkg() { dpkg-query -W -f='${Installed-Size}\n' "$@" 2>/dev/null | awk '{ s += $1 } END { print s * 1024 }'; }
mb() { awk -v n="$1" 'BEGIN { printf "%.0f MB", n / 1048576 }'; }

# Rust: what a minimal rustup install puts down (rustc, cargo, std), not docs, rust-src or clippy.
rust_tc() { local s h; s=$(rustc --print sysroot); h=$(rustc -vV | sed -n "s/^host: //p"); b "$s/bin/rustc" "$s/bin/cargo" "$s"/lib/librustc_driver-*.so "$s"/lib/libLLVM* "$s/lib/rustlib/$h/lib" "$s/lib/rustlib/$h/bin"; }
# Crates in Cargo.lock: their sources and .crate files in ~/.cargo/registry.
crates() {
    awk '/^name = /{ n = $3 } /^version = /{ v = $3; gsub(/"/, "", n); gsub(/"/, "", v); print n "-" v }' "$1/Cargo.lock" | sort -u |
        while read -r c; do ls -d ~/.cargo/registry/src/*/"$c" ~/.cargo/registry/cache/*/"$c.crate" 2>/dev/null; done | xargs -r du -scb | tail -1 | cut -f1
}
CLANG=$(dpkg -S "$(readlink -f "$(command -v clang)")" | cut -d: -f1)   # e.g. clang-18
V=${CLANG#clang-}
WEBKIT=$(pkg libwebkit2gtk-4.1-dev libwebkit2gtk-4.1-0 libjavascriptcoregtk-4.1-dev libjavascriptcoregtk-4.1-0 libgtk-3-dev libsoup-3.0-dev)

row() {   # name toolchain deps cache system
    local t=$(($2 + $3 + $4 + $5))
    printf '| %s | %s | %s | %s | %s | **%s** |\n' "$1" "$(mb "$2")" "$(mb "$3")" "$(mb "$4")" "$(mb "$5")" "$(mb "$t")"
}

echo "| Stack | Toolchain | Downloaded deps | Build cache | System libs | Total |"
echo "|---|---:|---:|---:|---:|---:|"
row "J3 + .j3ui" "$(( $(b "$(command -v j3c)") + $(pkg "$CLANG" "libclang-cpp$V" "libllvm$V" "libclang-common-$V-dev" "libclang1-$V") ))" 0 \
    "$(b j3/target)" "$(pkg libsdl2-2.0-0 libsdl2-ttf-2.0-0)"
NODE=$(dirname "$(dirname "$(readlink -f "$(command -v node)")")")
row "Electron" "$(b "$NODE")" "$(( $(b electron/node_modules) + $(b ~/.cache/electron/*/electron-v"$(node -p "require('./electron/node_modules/electron/package.json').version")"-linux-x64.zip) ))" 0 0
row "Tauri (Rust)" "$(rust_tc)" "$(crates tauri)" "$(b tauri/target)" "$WEBKIT"
row "Wails (Go)" "$(b "$(go env GOROOT)")" "$(cd wails && go list -m -f '{{.Dir}}' all | grep -vx "$PWD" | xargs -r du -scb 2>/dev/null | tail -1 | cut -f1)" \
    "$(b wails/target/gocache)" "$WEBKIT"
row "Dioxus native (Rust)" "$(rust_tc)" "$(crates dioxus)" "$(b dioxus/target)" 0
