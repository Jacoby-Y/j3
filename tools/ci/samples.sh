#!/bin/sh
# Headless: run speculative/01-07 and speculative/lang/*, then `j3c check` every examples/*.
# Exit 1 if any failed. J3C=path/to/j3c (default j3c-bootstrap/target/j3c).
# speculative/08 needs a game loop with input (not run here).
set -u
root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root" || exit 2
j3c=${J3C:-$root/j3c-bootstrap/target/j3c}
[ -x "$j3c" ] || { echo "samples.sh: no j3c at $j3c (./build.sh, or J3_STDLIB=$PWD/j3c/stdlib j3c-bootstrap/stage0/j3c build --opt 2 j3c-bootstrap)" >&2; exit 2; }
unset DISPLAY WAYLAND_DISPLAY
export SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy
log=$(mktemp)
trap 'rm -f "$log"' EXIT

pass=0 fail=0 failed=
step() { # label, command...
  label=$1; shift
  if timeout 240 "$@" > "$log" 2>&1 < /dev/null; then
    printf '%-40s pass\n' "$label"; pass=$((pass + 1))
  else
    printf '%-40s FAIL (exit %s)\n' "$label" "$?"
    tail -n 5 "$log" | sed 's/^/    | /'
    fail=$((fail + 1)); failed="$failed $label"
  fi
}

for f in speculative/0[1-7]_*.j3 speculative/lang/*.j3; do
  step "run $f" "$j3c" run --headless --frames 5 "$f"
done
for d in examples/*/ examples/ui_app_compare/j3/; do
  [ -f "$d/j3.toml" ] || continue
  step "check ${d%/}" "$j3c" check "$d"
done
echo "samples: $pass passed, $fail failed"
[ $fail -eq 0 ] || { echo "failed:$failed"; exit 1; }
