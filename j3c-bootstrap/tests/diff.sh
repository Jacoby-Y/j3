#!/bin/bash
# Differential harness (plan 11 A4): run stage0/j3c (the Rust oracle) and target/j3c-boot on the same
# inputs and compare. See tests/README.md. Exit 0 all ok or known, 1 new failure, 2 harness/setup error.
set -euo pipefail
usage() {
    cat <<'E'
usage: tests/diff.sh MODE [--norm] [--no-coverage] [--update-known] [-j N] [files...]
  MODE            fmt | ast | err | ir | llvm | run | self
  --norm          ir only: renumber temps by first appearance on both sides (tests/norm_ir.awk)
  --no-coverage   skip tools/coverage.sh (also DIFF_NO_COVERAGE=1); tools/layers.sh always runs
  --update-known  rewrite tests/known/MODE.txt to this run's DIFF/CRASH files
  -j N            parallel jobs (default: core count, 4 for run)
  files           inputs instead of tests/corpus.txt (paths or project folders)
Env: DIFF_TIMEOUT seconds per command (default 30; ir/llvm 120; run 300).
     DIFF_STAGE0 / DIFF_BOOT replace the two compilers (default stage0/j3c, target/j3c-boot;
     DIFF_BOOT skips the j3c-boot rebuild).
E
}
die() { echo "diff.sh: $*" >&2; exit 2; }

here=$(cd "$(dirname "$0")" && pwd)
boot=$(dirname "$here")
root=$(dirname "$boot")
S0=${DIFF_STAGE0:-$boot/stage0/j3c}
BOOT=${DIFF_BOOT:-$boot/target/j3c-boot}

[ $# -ge 1 ] || { usage >&2; exit 2; }
case $1 in -h|--help) usage; exit 0 ;; esac
mode=$1; shift
norm=0; cov=${DIFF_NO_COVERAGE:-0}; update=0; jobs=; files=()
while [ $# -gt 0 ]; do
    case $1 in
        --norm) norm=1 ;;
        --no-coverage) cov=1 ;;
        --update-known) update=1 ;;
        -j) [ $# -ge 2 ] || die "-j needs a number"; jobs=$2; shift ;;
        -*) die "unknown flag $1" ;;
        *) files+=("$(realpath -m --relative-to="$root" "$1")") ;;
    esac
    shift
done
case $mode in
    fmt|ast|err) to=30 ;;
    ir|llvm) to=120 ;;
    run) to=300; : "${jobs:=4}" ;;
    self) die "self: not yet: needs stage 1 to build stage 2 (a j3c-boot that can build j3c-bootstrap)" ;;
    *) usage >&2; exit 2 ;;
esac
[ $norm = 0 ] || [ $mode = ir ] || die "--norm is for ir only"
to=${DIFF_TIMEOUT:-$to}
: "${jobs:=$(nproc)}"
[ -x "$S0" ] || die "no $S0: run tools/stage0.sh --worktree"

# Fail fast on the structural checks.
if [ "$cov" = 0 ]; then "$boot/tools/coverage.sh" >&2 || die "tools/coverage.sh failed (--no-coverage skips it)"; fi
"$boot/tools/layers.sh" >&2 || die "tools/layers.sh failed"
if [ $mode = ast ] && [[ $("$S0" dump-ast /dev/null 2>&1) == *'unknown command'* ]]; then
    die "stage0 has no dump-ast yet: rebuild it with tools/stage0.sh --worktree"
fi

# One build of j3c-boot when it is missing or older than its sources.
if [ -z "${DIFF_BOOT:-}" ] && { [ ! -x "$BOOT" ] || [ -n "$(find "$boot/src" "$boot/j3.toml" -newer "$BOOT" -print -quit)" ] || [ "$S0" -nt "$BOOT" ]; }; then
    echo "diff.sh: building j3c-boot" >&2
    log=$(mktemp)
    if ! timeout 900 "$S0" build "$boot" >"$log" 2>&1; then tail -20 "$log" >&2; rm -f "$log"; die "j3c-boot build failed"; fi
    rm -f "$log"; touch "$BOOT"  # a cached build may leave the old mtime
fi

# Inputs: corpus lines from sections naming this mode, expanded per mode.
cd "$root"
expand() {
    local p=$1
    [ -e "$p" ] || die "input $p does not exist"
    if [ -f "$p" ]; then echo "$p"; return; fi
    case $mode in
        fmt|ast|err) find "$p" \( -name target -o -name node_modules \) -prune -o -name '*.j3' -type f -print | sort ;;
        *) if [ -f "$p/j3.toml" ]; then echo "$p"
           else find "$p" \( -name target -o -name node_modules \) -prune -o -name j3.toml -print | sed 's|/j3.toml$||' | sort; fi ;;
    esac
}
inputs=$(
    if [ ${#files[@]} -gt 0 ]; then
        for f in "${files[@]}"; do expand "$f"; done
    else
        on=0
        while read -r line; do
            line=${line%%#*}
            [ -n "${line// /}" ] || continue
            case $line in \[*) case " ${line//[\[\]]/} " in *" $mode "*) on=1 ;; *) on=0 ;; esac; continue ;; esac
            [ $on = 1 ] || continue
            for p in $line; do expand "$p"; done   # unquoted: the line is a glob
        done < "$here/corpus.txt"
    fi | awk '!seen[$0]++'
)
[ -n "$inputs" ] || die "no inputs for $mode"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# one IDX FILE: run both compilers, write $tmp/IDX/{status,detail}.
one() {
    set +e
    local d=$tmp/$1 f=$2 cwd=$root arg=$2 who
    mkdir -p "$d"
    if [ $mode = err ]; then mkdir -p "$d/w"; cp "$root/$f" "$d/w/"; cwd=$d/w; arg=$(basename "$f"); fi
    local -a cmd
    case $mode in
        fmt) cmd=(fmt "$arg") ;;
        ast) cmd=(dump-ast "$arg") ;;
        err) cmd=(check "$arg") ;;
        ir) cmd=(dump-ir "$arg") ;;
        llvm) cmd=(dump-llvm "$arg") ;;
        run) cmd=(run --headless --frames 3 "$arg") ;;
    esac
    for who in a a2 b; do
        local bin=$S0; [ $who = b ] && bin=$BOOT
        (cd "$cwd" && SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy timeout -k 5 "$to" "$bin" "${cmd[@]}" \
            >"$d/$who.out" 2>"$d/$who.err" </dev/null)
        echo $? >"$d/$who.rc"
        [ $norm = 1 ] && awk -f "$here/norm_ir.awk" "$d/$who.out" >"$d/$who.n" && mv "$d/$who.n" "$d/$who.out"
        { cat "$d/$who.out"
          if [ $mode != run ]; then echo "--- stderr"; cat "$d/$who.err"; fi
          echo "--- exit $(cat "$d/$who.rc")"; } >"$d/$who.txt"
    done
    local ra rb; ra=$(cat "$d/a.rc"); rb=$(cat "$d/b.rc")
    if ! cmp -s "$d/a.txt" "$d/a2.txt"; then
        echo NONDET >"$d/status"; diff -u --label stage0-run1 --label stage0-run2 "$d/a.txt" "$d/a2.txt" | head -40 >"$d/detail"
    elif { [ "$rb" -ge 124 ] && [ "$rb" != "$ra" ]; } || { [ "$rb" != 0 ] && [ "$ra" = 0 ]; }; then
        echo CRASH >"$d/status"; { echo "j3c-boot exit $rb (stage0 exit $ra)"; head -5 "$d/b.err"; } >"$d/detail"
    elif cmp -s "$d/a.txt" "$d/b.txt"; then
        echo ok >"$d/status"
    else
        echo DIFF >"$d/status"; diff -u --label stage0 --label j3c-boot "$d/a.txt" "$d/b.txt" | head -40 >"$d/detail"
    fi
    true
}
export -f one
export tmp root mode norm here S0 BOOT to

start=$SECONDS
echo "$inputs" | awk '{print NR; print}' | xargs -d '\n' -n2 -P "$jobs" bash -c 'one "$@"' _

known=$here/known/$mode.txt
knownset=$( [ -f "$known" ] && sed 's/#.*//; s/[[:space:]]*$//; /^$/d' "$known" || true )
isknown() { grep -qxF -- "$1" <<<"$knownset"; }

nok=0; ndiff=0; ncrash=0; nnon=0; new=0; fixed=(); failing=(); i=0
while IFS= read -r f; do
    i=$((i + 1)); st=$(cat "$tmp/$i/status" 2>/dev/null || echo CRASH)
    case $st in
        ok) nok=$((nok + 1)); echo "ok $f"; if isknown "$f"; then fixed+=("$f"); fi; continue ;;
        DIFF) ndiff=$((ndiff + 1)) ;;
        CRASH) ncrash=$((ncrash + 1)) ;;
        NONDET) nnon=$((nnon + 1)); new=$((new + 1)); echo "NONDET $f (stage0 printed different text on two runs)"
                sed 's/^/    /' "$tmp/$i/detail"; continue ;;
    esac
    echo "$st $f"; failing+=("$f")
    if ! isknown "$f"; then new=$((new + 1)); [ $update = 1 ] || sed 's/^/    /' "$tmp/$i/detail" 2>/dev/null; fi
done <<<"$inputs"

if [ $update = 1 ]; then
    mkdir -p "$here/known"
    { echo "# diff.sh $mode: known DIFF/CRASH inputs. Only ever shrinks: remove a line when its input passes."
      # ponytail: an existing list only shrinks; a new failure is never blessed (delete the file to reseed).
      { grep -vxF -- "$inputs" <<<"$knownset" || true
        if [ -n "$knownset" ]; then printf '%s\n' "${failing[@]+"${failing[@]}"}" | grep -xF -- "$knownset" || true
        else printf '%s\n' "${failing[@]+"${failing[@]}"}"; fi; } | sed '/^$/d' | sort -u
    } >"$known"
    echo "diff.sh: wrote tests/known/$mode.txt" >&2
    [ -n "$knownset" ] || new=$nnon; fixed=()
fi
for f in "${fixed[@]+"${fixed[@]}"}"; do echo "fixed $f: remove it from tests/known/$mode.txt"; done
sum="diff.sh $mode: $nok ok, $ndiff DIFF, $ncrash CRASH"
[ $nnon = 0 ] || sum="$sum, $nnon NONDET"
echo "$sum ($((SECONDS - start)) s)"
[ $new = 0 ] && [ ${#fixed[@]} = 0 ] || exit 1
