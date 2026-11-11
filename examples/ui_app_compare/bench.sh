#!/usr/bin/env bash
# Build each todo app from clean, rebuild after a one-line UI edit, and measure what ships.
# Dependencies are fetched first and not timed (npm install, cargo fetch, go mod download).
# Usage: ./bench.sh [j3 electron tauri wails dioxus]   (default: all)
set -euo pipefail
cd "$(dirname "$0")"
ROOT=$PWD
OUT=$ROOT/results
mkdir -p "$OUT"
APPS=${*:-j3 electron tauri wails dioxus}

# Cold means cold: no shared compiler caches.
export RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WRAPPER= J3C_NO_INSTALL=1

now() { date +%s.%N; }
secs() { awk -v a="$1" -v b="$2" 'BEGIN { printf "%.1f", b - a }'; }
human() { awk -v n="$1" 'BEGIN { split("B KB MB GB", u); i = 1; while (n >= 1024 && i < 4) { n /= 1024; i++ } printf "%.1f %s", n, u[i] }'; }

# edit FILE: append "!" to the "Todos" heading (the one-line change for the rebuild); unedit FILE puts it back
edit() { sed -i 's/>Todos</>Todos!</; s/h1 { "Todos" }/h1 { "Todos!" }/' "$1"; }
unedit() { sed -i 's/>Todos!</>Todos</; s/h1 { "Todos!" }/h1 { "Todos" }/' "$1"; }

# per app: fetch, clean, build, ui file, ship (copies what a user gets into $OUT/<app>/ship)
fetch_j3() { :; }
clean_j3() { rm -rf j3/target; }
build_j3() { (cd j3 && j3c build --release . >/dev/null 2>&1); }
ui_j3() { echo j3/ui/Screen.j3ui; }
ship_j3() { cp j3/target/todo "$1/"; cp -r j3/assets "$1/"; }   # the font is loaded at runtime

fetch_electron() { (cd electron && npm install --no-audit --no-fund >/dev/null 2>&1); }
clean_electron() { rm -rf electron/out electron/web; }
build_electron() { rm -rf electron/web && cp -r web electron/web && (cd electron && npm run build >/dev/null 2>&1); }
ui_electron() { echo web/index.html; }
ship_electron() { cp -r electron/out/todo-linux-x64/. "$1/"; }

fetch_tauri() { (cd tauri && cargo fetch >/dev/null 2>&1); }
clean_tauri() { rm -rf tauri/target; }
build_tauri() { (cd tauri && cargo build --release >/dev/null 2>&1); }
ui_tauri() { echo web/index.html; }
ship_tauri() { cp tauri/target/release/todo-tauri "$1/"; }

# Go keeps one build cache per user; point it into the project so a clean build is cold.
fetch_wails() { (cd wails && go mod download >/dev/null 2>&1); }
clean_wails() { rm -rf wails/build wails/target; }
build_wails() {
    rm -rf wails/frontend && mkdir -p wails/frontend/dist && cp web/* wails/frontend/dist/
    # what `wails build` runs; its CLI (v2.11) fails on Go 1.27 while generating bindings, which this app has none of
    (cd wails && mkdir -p build/bin && GOCACHE=$PWD/target/gocache \
        go build -tags desktop,production,webkit2_41 -trimpath -ldflags "-s -w" -o build/bin/todo . >/dev/null 2>&1)
}
ui_wails() { echo web/index.html; }
ship_wails() { cp wails/build/bin/todo "$1/"; }

fetch_dioxus() { (cd dioxus && cargo fetch >/dev/null 2>&1); }
clean_dioxus() { rm -rf dioxus/target; }
build_dioxus() { (cd dioxus && cargo build --release >/dev/null 2>&1); }
ui_dioxus() { echo dioxus/src/main.rs; }
ship_dioxus() { cp dioxus/target/release/todo-dioxus "$1/"; }

for app in $APPS; do
    echo "== $app" >&2
    "fetch_$app"
    "clean_$app"
    t0=$(now); "build_$app" || { echo "$app: build failed" >&2; continue; }; t1=$(now)
    ui=$("ui_$app")
    edit "$ui"
    t2=$(now); "build_$app" || { unedit "$ui"; echo "$app: rebuild failed" >&2; continue; }; t3=$(now)
    unedit "$ui"
    "build_$app"   # leave the shipped files built from the unedited source
    rm -rf "$OUT/$app" && mkdir -p "$OUT/$app/ship"
    "ship_$app" "$OUT/$app/ship"
    size=$(du -sb "$OUT/$app/ship" | cut -f1)
    tar -czf "$OUT/$app/ship.tar.gz" -C "$OUT/$app" ship
    gz=$(stat -c %s "$OUT/$app/ship.tar.gz")
    printf '%s\t%s\t%s\t%s\t%s\n' "$app" "$(secs "$t0" "$t1")" "$(secs "$t2" "$t3")" "$(human "$size")" "$(human "$gz")" | tee "$OUT/$app/row.tsv" >&2
done

{
    echo "| App | Clean build (s) | Rebuild after 1-line UI edit (s) | Shipped size | .tar.gz |"
    echo "|---|---:|---:|---:|---:|"
    for app in j3 electron tauri wails dioxus; do
        [ -f "$OUT/$app/row.tsv" ] && awk -F'\t' '{ printf "| %s | %s | %s | %s | %s |\n", $1, $2, $3, $4, $5 }' "$OUT/$app/row.tsv"
    done
    echo
    echo "Machine: $(nproc) threads, $(awk -F: '/model name/ { print $2; exit }' /proc/cpuinfo | sed 's/^ *//')"
} | tee "$OUT/results.md"
