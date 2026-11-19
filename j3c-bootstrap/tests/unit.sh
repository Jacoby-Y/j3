#!/bin/bash
# Unit tests of j3c-bootstrap modules: every tests/unit/*_test.j3 (or the names given) is a program with
# `fn main` that may `use j3c.*`; it passes when it exits 0 (fail with `panic`).
# Each test builds as its own tiny package, target/unit/NAME/j3.toml (entry = the test, dependency
# j3c = this package), because `stage0/j3c run FILE` inside j3c-bootstrap builds src/main.j3 instead.
# The binary runs headless with cwd = j3c-bootstrap/. Prints `ok NAME` / `FAIL NAME` (with the tail
# of its output), then a summary; exit 1 on any failure.
# Tests run in parallel (J3_TEST_JOBS, default half the cores): each one owns target/unit/NAME (its
# build, objects, binary and log) and its own scratch paths; results print in name order.
#   tests/unit.sh [NAME...]     NAME = file name without _test.j3 (e.g. smoke)
set -eu
cd "$(dirname "$0")/.."
S0=stage0/j3c
[ -x "$S0" ] || { echo "unit.sh: no $S0 (tools/stage0.sh)" >&2; exit 1; }
if [ $# -gt 0 ]; then
    names=$(printf '%s\n' "$@" | awk '!seen[$0]++')   # a name twice would share its target/unit dir
else
    names=$(for f in tests/unit/*_test.j3; do [ -e "$f" ] && basename "$f" _test.j3; done)
fi
jobs=${J3_TEST_JOBS:-$(( $(nproc) / 2 ))}; [ "$jobs" -ge 1 ] || jobs=1

# one NAME: build and run one test; writes target/unit/NAME/{log.txt,rc}.
one() {
    local n=$1 f=tests/unit/${1}_test.j3 d=target/unit/$1 r=0
    mkdir -p "$d"; rm -f "$d/rc"
    printf '[package]\nname = "%s_test"\nentry = "../../../%s"\n\n[dependencies]\nj3c = "../../.."\n' "$n" "$f" >"$d/j3.toml"
    local log=$d/log.txt t=300
    case $n in load_core) t=900 ;; esac   # 163 stage0 runs: ~4 min alone, more under parallel load
    if [ ! -f "$f" ]; then
        echo "no such test: $f" >"$log"; r=1
    # stage 0 embeds its tag's stdlib; j3c-bootstrap uses newer modules (j3.data.toml), so it reads the tree on disk
    elif ! J3_STDLIB=$PWD/../j3c/stdlib timeout 300 "$S0" build "$d" >"$log" 2>&1; then
        r=1
    else
        SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy timeout "$t" "$d/target/${n}_test" >"$log" 2>&1 </dev/null || r=1
    fi
    echo $r >"$d/rc"
}
export -f one; export S0
printf '%s\n' $names | xargs -d '\n' -P "$jobs" -I{} bash -c 'one "$1"' _ {}

ok=0; fail=0
for n in $names; do
    d=target/unit/$n
    if [ "$(cat "$d/rc" 2>/dev/null)" = 0 ]; then
        ok=$((ok + 1)); echo "ok $n"
    else
        fail=$((fail + 1)); echo "FAIL $n"; tail -15 "$d/log.txt" 2>/dev/null | sed 's/^/    /'
    fi
done
echo "unit.sh: $ok ok, $fail FAIL"
[ $fail -eq 0 ]
