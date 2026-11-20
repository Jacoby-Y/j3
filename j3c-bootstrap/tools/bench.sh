#!/usr/bin/env bash
# Plan 07 bench: stage 0 (the Rust j3c, built by tools/stage0.sh rust-final) against target/j3c on 16 metrics, wall time and peak RSS.
# Usage: tools/bench.sh [--only N,M,...] [--list]. Env: BENCH_N (runs per metric, median; default 3),
# BENCH_STAGE0 / BENCH_BOOT (the two compilers). Every run works on one scratch copy under <repo>/target/bench/w
# (tar without `target/`), deleted at the end; writes target/bench/result.md and result.tsv (rows of metrics
# left out by --only are kept from the last run).
# Cold = `target/` removed before each run; warm = built once, then timed again; leaf = a comment line
# appended to one module before each run. `%M` is the largest process in the tree (the compiler, or clang).
# Short metrics (11, 13, 14, 15: under ~0.2 s) run K times in one timed loop, K picked so a loop takes >= 1 s
# (BENCH_K overrides), and report wall per run; RSS is the largest single run of the loop.
set -eu
budget=$(( $(nproc) / 2 )); [ "$budget" -ge 1 ] || budget=1
export J3_JOBS=${J3_JOBS:-$budget}   # half the cores at most: clang processes per build (repo script rule)
here=$(cd "$(dirname "$0")" && pwd); boot=$(dirname "$here"); root=$(dirname "$boot")
S0=$(realpath "${BENCH_STAGE0:-${XDG_CACHE_HOME:-$HOME/.cache}/j3/stage0/rust-final/j3c}"); BT=$(realpath "${BENCH_BOOT:-$boot/target/j3c}")
N=${BENCH_N:-3}; out=$root/target/bench; W=$out/w; tf=$out/time.txt
export J3C_NO_INSTALL=1 SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy
# stage 0 needs the stdlib on disk to build j3c-bootstrap (it uses modules newer than its tag); both sides read it
export J3_STDLIB=$W/j3c/stdlib
names=('' 'self build cold' 'self build warm' 'self build leaf edit' 'editor program cold' 'desk-app cold'
    'editor_village cold' 'desk-app leaf edit' 'check corpus (sum, max RSS)' 'check grid_test' 'check j3c-bootstrap'
    'fmt --check j3c/stdlib/j3' 'dump-ir examples/desk-app' 'dump-bc scripted/fighter.j3s'
    'doc + doc j3.game --short + doc search mesh' 'new + first check' 'binary size (stripped, MB)')
only=$(seq 1 16)
while [ $# -gt 0 ]; do case $1 in
    --list) for i in $(seq 1 16); do echo "$i ${names[$i]}"; done; exit 0 ;;
    --only) only=$(tr , '\n' <<<"$2" | sort -n); shift ;;
    *) echo "bench.sh: unknown argument $1" >&2; exit 2 ;;
esac; shift; done

# tm CMD...: run it once under /usr/bin/time; prints "wall rss_kb rc".
tm() { local rc=0; /usr/bin/time -f '%e %M' -o "$tf" timeout 1800 "$@" >/dev/null 2>&1 || rc=$?; echo "$(tail -1 "$tf") $rc"; }
# rep BODY: run the bash snippet BODY ("$0" is the compiler) K times in one timed process; prints "wall/K rss rc".
rep() { local r; r=$(tm bash -c "for _ in \$(seq $K); do $1 || exit 1; done" "$C")
    awk -v k="$K" '{ printf "%.4f %s %s\n", $1 / k, $2, $3 }' <<<"$r"; }
need0() { [ "$3" = 0 ] || { echo "bench.sh: $C failed (rc $3) in $PWD" >&2; exit 1; }; echo "$1 $2"; }
build() { (cd "$W/$1" && rm -rf target && tm "$C" build . >/dev/null); }   # untimed pre-build
cold() { (cd "$W/$1" && rm -rf target && need0 $(tm "$C" build .)); }
warm() { (cd "$W/$1" && need0 $(tm "$C" build .)); }
leaf() { (cd "$W/$1" && echo '// bench' >>"$2" && need0 $(tm "$C" build .)); }

pre() { case $1 in 2|3) build j3c-bootstrap ;; 7) build examples/desk-app ;; esac; }
run1() { case $1 in
    1) cold j3c-bootstrap ;;
    2) warm j3c-bootstrap ;;
    3) leaf j3c-bootstrap src/cmd/guide.j3 ;;
    4) cold editor ;;
    5) cold examples/desk-app ;;
    6) cold examples/editor_village ;;
    7) leaf examples/desk-app src/prefs.j3 ;;
    8) while read -r p; do   # every corpus input, sequential; sum of walls, max RSS
           if [ -d "$W/$p" ]; then (cd "$W/$p" && tm "$C" check .); else (cd "$W" && tm "$C" check "$p"); fi
       done <"$out/corpus.lst" | awk '{ s += $1; if ($2 > m) m = $2 } END { printf "%.2f %d\n", s, m }' ;;
    9) cd "$W/j3c/stdlib/tests" && need0 $(tm "$C" check grid_test.j3) ;;
    10) cd "$W/j3c-bootstrap" && need0 $(tm "$C" check .) ;;
    11) cd "$W/j3c/stdlib" && need0 $(rep '{ "$0" fmt --check j3; [ $? -le 1 ]; }') ;;   # rc 1: the same files need formatting
    12) cd "$W/examples/desk-app" && need0 $(tm "$C" dump-ir .) ;;
    13) cd "$W/examples/scripted" && need0 $(rep '{ rm -rf target && "$0" dump-bc scripts/fighter.j3s; }') ;;
    14) cd "$W" && need0 $(rep '{ "$0" doc && "$0" doc j3.game --short && "$0" doc search mesh; }') ;;
    15) cd "$W" && need0 $(rep '( rm -rf new && mkdir new && cd new && "$0" new demo && cd demo && "$0" check . )') ;;
    16) strip -o "$out/stripped" "$C" && echo "$(stat -c %s "$out/stripped") 0" && rm -f "$out/stripped" ;;
esac; }
med() { sort -n | awk '{ a[NR] = $1 } END { print a[int((NR + 1) / 2)] }'; }

trap 'rm -rf "$W" "$tf"' EXIT
rm -rf "$W"; mkdir -p "$W"
echo "bench.sh: scratch copy in $W" >&2
tar -C "$root" --exclude=target -cf - j3c j3c-bootstrap speculative examples | tar -C "$W" -xf -
mkdir -p "$W/editor"; cp -r "$W/j3c/editor/src" "$W/editor/src"; cp "$W/j3c/editor/font.ttf" "$W/editor/"
printf '[package]\nname = "j3_editor"\nentry = "src/main.j3"\n\n[link]\nlibs = ["SDL2", "GL"]\n' >"$W/editor/j3.toml"
"$boot/tests/diff.sh" run --list >"$out/corpus.lst"

# Rows of metrics this run skips are kept from the last result.tsv, so --only runs add up.
touch "$out/result.tsv"; grep -vE "^($(echo $only | tr ' ' '|'))	" "$out/result.tsv" >"$out/keep" || true
mv "$out/keep" "$out/result.tsv"
for i in $only; do
    row=$i
    for C in "$S0" "$BT"; do
        echo "bench.sh: [$i] ${names[$i]}: $C" >&2
        pre "$i"
        K=1; case $i in 11|13|14|15)   # calibrate: one run, then enough runs per loop for >= 1 s
            K=${BENCH_K:-$( (run1 "$i") | awk '{ k = int(1 / ($1 > 0.005 ? $1 : 0.005)) + 1; print (k > 200 ? 200 : k) }')} ;; esac
        : >"$out/runs"; for _ in $(seq 1 "$N"); do (run1 "$i") >>"$out/runs"; done
        row="$row	$(cut -d' ' -f1 "$out/runs" | med)	$(cut -d' ' -f2 "$out/runs" | med)"
    done
    echo "$row" >>"$out/result.tsv"
done
rm -f "$out/runs" "$out/corpus.lst"; sort -n -o "$out/result.tsv" "$out/result.tsv"

{ echo "# Bench $(date -u '+%Y-%m-%d %H:%M UTC'), median of $N"
  echo; echo "stage0: \`$S0\`  "; echo "boot: \`$BT\`"; echo
  echo '| # | Metric | stage0 wall | stage0 RSS | boot wall | boot RSS | wall ratio | RSS ratio | wall | RSS |'
  echo '|---|---|---|---|---|---|---|---|---|---|'
  awk -F'\t' -v names="$(printf '%s\n' "${names[@]:1}")" '
    BEGIN { split(names, nm, "\n") }
    function r(a, b) { return a > 0 ? sprintf("%.2fx", b / a) : (b > 0 ? "inf" : "1.00x") }
    function v(a, b) { return b < a ? "WIN" : (b == a ? "TIE" : "LOSE") }
    $1 == 16 { printf "| 16 | %s | %.2f MB | - | %.2f MB | - | %s | - | %s | - |\n", nm[16], $2 / 1e6, $4 / 1e6, r($2, $4), v($2, $4); next }
    function t(x) { return sprintf(x < 1 ? "%.3f s" : "%.2f s", x) }
    { printf "| %s | %s | %s | %d MB | %s | %d MB | %s | %s | %s | %s |\n", $1, nm[$1], t($2), $3 / 1024, t($4), $5 / 1024,
        r($2, $4), r($3, $5), v($2, $4), v($3, $5) }' "$out/result.tsv"
} >"$out/result.md"
cat "$out/result.md"
