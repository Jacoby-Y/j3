#!/bin/sh
# Unit tests of j3c-bootstrap modules: every tests/unit/*_test.j3 (or the names given) is a program with
# `fn main` that may `use j3c.*`; it passes when it exits 0 (fail with `panic`).
# Each test builds as its own tiny package, target/unit/NAME/j3.toml (entry = the test, dependency
# j3c = this package), because `stage0/j3c run FILE` inside j3c-bootstrap builds src/main.j3 instead.
# The binary runs headless with cwd = j3c-bootstrap/. Prints `ok NAME` / `FAIL NAME` (with the tail
# of its output), then a summary; exit 1 on any failure.
#   tests/unit.sh [NAME...]     NAME = file name without _test.j3 (e.g. smoke)
set -eu
cd "$(dirname "$0")/.."
S0=stage0/j3c
[ -x "$S0" ] || { echo "unit.sh: no $S0 (tools/stage0.sh)" >&2; exit 1; }
if [ $# -gt 0 ]; then
    names=$*
else
    names=$(for f in tests/unit/*_test.j3; do [ -e "$f" ] && basename "$f" _test.j3; done)
fi
ok=0; fail=0
for n in $names; do
    f=tests/unit/${n}_test.j3
    d=target/unit/$n
    mkdir -p "$d"
    printf '[package]\nname = "%s_test"\nentry = "../../../%s"\n\n[dependencies]\nj3c = "../../.."\n' "$n" "$f" >"$d/j3.toml"
    log=$d/log.txt
    if [ ! -f "$f" ]; then
        echo "no such test: $f" >"$log"; r=1
    elif ! timeout 300 "$S0" build "$d" >"$log" 2>&1; then
        r=1
    else
        r=0
        SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy timeout 120 "$d/target/${n}_test" >"$log" 2>&1 || r=1
    fi
    if [ $r -eq 0 ]; then
        ok=$((ok + 1)); echo "ok $n"
    else
        fail=$((fail + 1)); echo "FAIL $n"; tail -15 "$log" | sed 's/^/    /'
    fi
done
echo "unit.sh: $ok ok, $fail FAIL"
[ $fail -eq 0 ]
