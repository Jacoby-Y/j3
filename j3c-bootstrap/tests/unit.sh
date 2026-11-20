#!/bin/bash
# Unit tests of j3c-bootstrap modules: every tests/unit/*_test.j3 (or the names given) is a program with
# `fn main` that may `use j3c.*`; it passes when it exits 0 (fail with `panic`).
# Each test builds as its own tiny package with the seed (tools/seed.sh list's first, reading this tree's stdlib):
# the object cache key holds the building compiler's id, so a seed that does not change between edits keeps
# every unchanged module cached, where target/j3c (rebuilt on each src/ edit) would rebuild them all.
# J3_UNIT_CC=target uses target/j3c instead (a codegen change), J3_UNIT_CC=PATH another compiler. Built at
# opt 0 (clang is most of a test's build). target/j3c is still built (tests/boot.sh): some tests run it.
# Package layout:
# target/unit/NAME/j3.toml (entry = the test, dependencies j3c = this package and unitkit = tests/unit/kit,
# the expected-file helper), because `j3c run FILE` inside j3c-bootstrap builds src/main.j3 instead.
# Expected output lives in tests/unit/expect/; J3_UNIT_BLESS=1 rewrites what the tests run compare.
# The binary runs headless with cwd = j3c-bootstrap/. Prints `ok NAME` / `FAIL NAME` (with the tail
# of its output), then a summary; exit 1 on any failure.
# Tests run in parallel (J3_TEST_JOBS, default 2: each build holds ~400 MB): each one owns target/unit/NAME (its
# build, objects, binary and log) and its own scratch paths; results print in name order.
# Half the cores at most: J3_TEST_JOBS tests side by side, each build with J3_JOBS = (nproc/2) / J3_TEST_JOBS
# clang processes (at least 1); a J3_JOBS you set wins.
#   tests/unit.sh [NAME...]     NAME = file name without _test.j3 (e.g. smoke)
set -eu
cd "$(dirname "$0")/.."
budget=$(( $(nproc) / 2 )); [ "$budget" -ge 1 ] || budget=1
jobs=${J3_TEST_JOBS:-2}; [ "$jobs" -ge 1 ] 2>/dev/null || jobs=1
tests/boot.sh || { echo "unit.sh: cannot build target/j3c" >&2; exit 1; }   # runs alone: boot.sh takes the whole budget
per=$(( budget / jobs )); [ "$per" -ge 1 ] || per=1
export J3_JOBS=${J3_JOBS:-$per}
case ${J3_UNIT_CC:-} in
    target) J3C=$PWD/target/j3c ;;
    "") J3C=$(tools/seed.sh list | head -1); [ -n "$J3C" ] || J3C=$PWD/target/j3c ;;
    *) J3C=$J3_UNIT_CC ;;
esac
export J3_UNIT_STDLIB=$(cd .. && pwd)/j3c/stdlib
if [ $# -gt 0 ]; then
    names=$(printf '%s\n' "$@" | awk '!seen[$0]++')   # a name twice would share its target/unit dir
else
    names=$(for f in tests/unit/*_test.j3; do [ -e "$f" ] && basename "$f" _test.j3; done)
fi

# one NAME: build and run one test; writes target/unit/NAME/{log.txt,rc}.
one() {
    local n=$1 f=tests/unit/${1}_test.j3 d=target/unit/$1 r=0
    mkdir -p "$d"; rm -f "$d/rc"
    printf '[package]\nname = "%s_test"\nentry = "../../../%s"\n\n[dependencies]\nj3c = "../../.."\nunitkit = "../../../tests/unit/kit"\n\n[cc]\nopt = "0"\n' "$n" "$f" >"$d/j3.toml"
    local log=$d/log.txt t=300
    if [ ! -f "$f" ]; then
        echo "no such test: $f" >"$log"; r=1
    elif ! J3_STDLIB=$J3_UNIT_STDLIB timeout 300 "$J3C" build "$d" >"$log" 2>&1; then
        r=1
    else
        SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy timeout "$t" "$d/target/${n}_test" >"$log" 2>&1 </dev/null || r=1
    fi
    echo $r >"$d/rc"
}
export -f one; export J3C
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
