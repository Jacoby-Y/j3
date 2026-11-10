#!/bin/sh
# Run every j3c/stdlib/tests/*.j3 headless (offscreen SDL, no audio, no window) and print a
# pass/fail table. Exit 1 if any test failed. Args: test names to run only those (no .j3).
# J3C=path/to/j3c (default j3c/target/release/j3c), J3_TEST_TIMEOUT=seconds per test (240),
# J3_TEST_LOGS=dir for per-test logs (default a temp dir, kept when something fails).
set -u
root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root" || exit 2
j3c=${J3C:-$root/j3c/target/release/j3c}
[ -x "$j3c" ] || { echo "stdlib-tests.sh: no j3c at $j3c (cd j3c && cargo build --release)" >&2; exit 2; }
limit=${J3_TEST_TIMEOUT:-240}
logs=${J3_TEST_LOGS:-$(mktemp -d)}
mkdir -p "$logs"
# Never a real window or sound, even on a desktop.
unset DISPLAY WAYLAND_DISPLAY
export SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy

# Not run: wine-only (writes C:\ files), needs the network, a benchmark.
skip() {
  case $1 in
    win_io_test) echo "wine only" ;;
    tls_live_test) echo "network" ;;
    j2_bench) echo "benchmark, not a test" ;;
    *) return 1 ;;
  esac
}

if [ $# -gt 0 ]; then names=$*; else
  names=$(for f in j3c/stdlib/tests/*.j3; do basename "$f" .j3; done)
fi

pass=0 fail=0 skipped=0 failed=
printf '%-28s %-6s %5s  %s\n' TEST RESULT SECS NOTE
for n in $names; do
  if why=$(skip "$n"); then
    printf '%-28s %-6s %5s  %s\n' "$n" skip - "$why"; skipped=$((skipped + 1)); continue
  fi
  extra=
  [ "$n" = idle_key_test ] && extra="J3_IDLE_SKIP=1"
  t0=$(date +%s)
  # shellcheck disable=SC2086 # $extra is one VAR=value word or empty
  env $extra timeout "$limit" "$j3c" run "j3c/stdlib/tests/$n.j3" > "$logs/$n.log" 2>&1 < /dev/null
  rc=$?
  secs=$(( $(date +%s) - t0 ))
  if [ $rc -eq 0 ]; then
    printf '%-28s %-6s %5s\n' "$n" pass "$secs"; pass=$((pass + 1))
  else
    note="exit $rc"
    [ $rc -eq 124 ] && note="timed out after ${limit}s"
    grep -qiE 'opengl|egl|glx|gl context|SDL_CreateWindow|video' "$logs/$n.log" && note="$note (GL/video error: needs mesa/EGL?)"
    printf '%-28s %-6s %5s  %s\n' "$n" FAIL "$secs" "$note"
    tail -n 5 "$logs/$n.log" | sed 's/^/    | /'
    fail=$((fail + 1)); failed="$failed $n"
  fi
done
echo "stdlib tests: $pass passed, $fail failed, $skipped skipped (logs: $logs)"
[ $fail -eq 0 ] || { echo "failed:$failed"; exit 1; }
[ -n "${J3_TEST_LOGS:-}" ] || rm -rf "$logs"
