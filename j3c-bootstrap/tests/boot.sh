#!/bin/bash
# Make target/j3c current: build it with the seed (J3_SEED, else the first of tools/seed.sh list, else the pinned release) when it is
# missing, older than src/, or carries a git build version (./build.sh embeds text/version-build.txt, then
# removes it; expected files hold the plain J3C_VERSION). `--opt 2` (plan 07): the j3.toml stays opt 0 for the
# self build; only the binary under test is optimized. The seed reads this tree's stdlib (it may predate
# modules the compiler uses). Used by diff.sh and unit.sh; prints only when it builds. Exit 1 on failure.
# J3_JOBS defaults to half the cores; the seed may predate it, so it is also pinned to that many CPUs (taskset).
set -eu
budget=$(( $(nproc) / 2 )); [ "$budget" -ge 1 ] || budget=1
export J3_JOBS=${J3_JOBS:-$budget}
pin=
if [ "$J3_JOBS" -lt "$(nproc)" ] 2>/dev/null && taskset -c "0-$((J3_JOBS - 1))" true 2>/dev/null; then pin="taskset -c 0-$((J3_JOBS - 1))"; fi
boot=$(cd "$(dirname "$0")/.." && pwd)
BOOT=$boot/target/j3c
die() { echo "boot.sh: $*" >&2; exit 1; }
want="j3c $(sed -n 's/^pub const J3C_VERSION: string = "\(.*\)";$/\1/p' "$boot/src/project/config.j3")"
[ "$want" != "j3c " ] || die "no J3C_VERSION in src/project/config.j3"
if [ ! -x "$BOOT" ] || [ -n "$(find "$boot/src" "$boot/j3.toml" -newer "$BOOT" -print -quit)" ] || [ "$("$BOOT" version 2>/dev/null)" != "$want" ]; then
    seed=$("$boot/tools/seed.sh" list | head -1)
    [ -n "$seed" ] || seed=$("$boot/tools/seed.sh") || die "no seed j3c to build j3c-boot (none installed or pinned; set J3_SEED)"
    echo "boot.sh: building j3c-boot with $seed" >&2
    log=$(mktemp)
    if ! J3_STDLIB=$boot/../j3c/stdlib timeout 900 $pin "$seed" build --opt 2 "$boot" >"$log" 2>&1; then tail -20 "$log" >&2; rm -f "$log"; die "j3c-boot build failed"; fi
    rm -f "$log"; touch "$BOOT"  # a cached build may leave the old mtime
    [ "$("$BOOT" version)" = "$want" ] || die "j3c-boot says '$("$BOOT" version)', want '$want' (a stale text/version-build.txt?)"
fi
