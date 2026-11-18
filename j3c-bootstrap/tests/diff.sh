#!/bin/bash
# Differential harness (plan 11 A4): run stage0/j3c (the Rust oracle) and target/j3c-boot on the same
# inputs and compare. See tests/README.md. Exit 0 all ok or known, 1 new failure, 2 harness/setup error.
set -euo pipefail
usage() {
    cat <<'E'
usage: tests/diff.sh MODE [--norm] [--user] [--es] [--no-coverage] [--update-known] [--list] [-j N] [files...]
  MODE            fmt | ast | err | ir | llvm | glsl | ui | run | bc | self
  --norm          ir only: renumber temps by first appearance on both sides (tests/norm_ir.awk)
  --user          ir only: compare only the entry module (the dump's first `module` section);
                  its known list is tests/known/ir-user.txt
  --es            glsl only: `dump-glsl --es` (the OpenGL ES text); its known list is tests/known/glsl-es.txt
  --no-coverage   skip tools/coverage.sh (also DIFF_NO_COVERAGE=1); tools/layers.sh always runs
  --update-known  rewrite the known list (tests/known/MODE.txt) to this run's DIFF/CRASH files
  --list          print the expanded inputs, one per line, and exit (nothing runs)
  -j N            parallel jobs (default: core count, 4 for run)
  files           inputs instead of tests/corpus.txt (paths or project folders)
     glsl runs `dump-glsl PATH`; ui runs `dump-ui PROJECT/ui` (the project's `.j3ui` folder).
     tests/bad-ui/NAME is a UI project with one error: ui compares the `dump-ui` error text, and err takes
     such a folder (any folder with a `j3.toml`) as one case, `check .` in a scratch copy.
     ir/llvm/glsl/ui/run export J3_STDLIB=<repo>/j3c/stdlib to both sides, so stage0's build-dir stdlib path
     stays out of the output.
     run: a project's `target/.j3c` (stamps, objects) is removed before each compiler and after the last, so
     neither reuses the other's build and nothing is left in the corpus (a file input inside a project counts
     as that project; a file outside any project builds to a temp file, writes no stamps and runs in a scratch cwd). An input whose line in
     tests/known/run.txt has a `# skip: CAUSE` comment is never run (network). A line with `# nondet: CAUSE`
     runs, and a NONDET (or any other outcome) of it does not fail the run.
     bc: every compiler works on its own scratch copy (never the tree: `run` and `dump-bc` write `target/j3s/`
     and `scripts/target/`). A project input runs `run --headless --frames 3 .` (dev mode: scripts on the VM) and
     compares the exit code and the set of `*.j3sb` files with their sha256 (not stdout: timings); a `.j3s` file
     input runs `dump-bc` on it in a copy of its project (or of its folder when that project is j3c-bootstrap)
     and compares stdout, stderr, exit code and the written `.j3sb`.
     self: stage 1 (j3c-boot) builds a scratch copy of j3c-bootstrap/{j3.toml,src,text} under <repo>/target/
     (never in place: that would overwrite the running binary); stage 2 is then compared with stage 1 on
     fmt, ast, err, ui, glsl, ir --norm and llvm (one input per call). SELF_BUILD_TIMEOUT seconds (default 1800).
     SELF_LLVM=A:B: only llvm inputs A..B of `llvm --list` (the other modes only when A is 1), to run it in pieces.
Env: DIFF_TIMEOUT seconds per command (default 30; ir/llvm 120; run, bc 300).
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
norm=0; user=0; es=0; cov=${DIFF_NO_COVERAGE:-0}; update=0; list=0; jobs=; files=()
while [ $# -gt 0 ]; do
    case $1 in
        --norm) norm=1 ;;
        --user) user=1 ;;
        --es) es=1 ;;
        --no-coverage) cov=1 ;;
        --update-known) update=1 ;;
        --list) list=1 ;;
        -j) [ $# -ge 2 ] || die "-j needs a number"; jobs=$2; shift ;;
        -*) die "unknown flag $1" ;;
        *) files+=("$(realpath -m --relative-to="$root" "$1")") ;;
    esac
    shift
done
case $mode in
    fmt|ast|err) to=30 ;;
    ir|llvm|glsl|ui) to=120 ;;
    run|bc) to=300; : "${jobs:=4}" ;;
    self) to=0 ;;
    *) usage >&2; exit 2 ;;
esac
[ $norm = 0 ] || [ $mode = ir ] || die "--norm is for ir only"
[ $user = 0 ] || [ $mode = ir ] || die "--user is for ir only"
[ $es = 0 ] || [ $mode = glsl ] || die "--es is for glsl only"
to=${DIFF_TIMEOUT:-$to}
: "${jobs:=$(nproc)}"
[ -x "$S0" ] || die "no $S0: run tools/stage0.sh --worktree"

# Fail fast on the structural checks (not for --list: it only prints).
if [ $list = 1 ]; then :; elif [ "$cov" = 0 ]; then "$boot/tools/coverage.sh" >&2 || die "tools/coverage.sh failed (--no-coverage skips it)"; fi
[ $list = 1 ] || "$boot/tools/layers.sh" >&2 || die "tools/layers.sh failed"
if [ $mode = ast ] && [[ $("$S0" dump-ast /dev/null 2>&1) == *'unknown command'* ]]; then
    die "stage0 has no dump-ast yet: rebuild it with tools/stage0.sh --worktree"
fi

# One build of j3c-boot when it is missing or older than its sources.
if [ $list = 0 ] && [ -z "${DIFF_BOOT:-}" ] && { [ ! -x "$BOOT" ] || [ -n "$(find "$boot/src" "$boot/j3.toml" -newer "$BOOT" -print -quit)" ] || [ "$S0" -nt "$BOOT" ]; }; then
    echo "diff.sh: building j3c-boot" >&2
    log=$(mktemp)
    if ! timeout 900 "$S0" build "$boot" >"$log" 2>&1; then tail -20 "$log" >&2; rm -f "$log"; die "j3c-boot build failed"; fi
    rm -f "$log"; touch "$BOOT"  # a cached build may leave the old mtime
fi

# self (plan 11 A4): stage 1 = $BOOT builds stage 2 from a scratch copy, then stage 2 must print what stage 1
# prints. The copy sits at <repo>/target/self.PID/j3c-bootstrap with a `j3c` link beside it, so the sources'
# `@embed("../../../j3c/...")` paths still resolve. Never an in-place build.
if [ $mode = self ]; then
    scratch=$root/target/self.$$
    trap 'rm -rf "$scratch"' EXIT
    mkdir -p "$scratch/j3c-bootstrap"
    cp -r "$boot/j3.toml" "$boot/src" "$boot/text" "$scratch/j3c-bootstrap/"
    ln -s "$root/j3c" "$scratch/j3c"
    echo "diff.sh self: stage 1 builds stage 2 in $scratch" >&2
    log=$scratch/build.log; touch "$scratch/start"
    if ! (cd "$scratch" && timeout -k 5 "${SELF_BUILD_TIMEOUT:-1800}" "$BOOT" build j3c-bootstrap) >"$log" 2>&1; then
        tail -20 "$log" >&2; die "self: stage 1 could not build stage 2"
    fi
    stage2=$scratch/j3c-bootstrap/target/j3c-boot
    [ -x "$stage2" ] || die "self: no $stage2 after the build"
    [ ! "$boot/target/j3c-boot" -nt "$scratch/start" ] || die "self: the build wrote j3c-bootstrap/target/j3c-boot"
    export DIFF_STAGE0=$BOOT DIFF_BOOT=$stage2 DIFF_NO_COVERAGE=1
    bad=0; n=0
    # A clean comparison prints its summary line; a failed one prints everything (the DIFF/CRASH detail).
    sub() {
        n=$((n + 1))
        if "$0" "$@" >"$scratch/sub.out" 2>&1; then tail -1 "$scratch/sub.out"
        else bad=$((bad + 1)); echo "FAILED: diff.sh $*"; cat "$scratch/sub.out"; fi
    }
    # SELF_LLVM=A:B runs only llvm inputs A..B of the list (1-based), and the other modes only when A is 1:
    # a full `self` in pieces, for callers with a time cap per command.
    la=1; lb=999999
    if [ -n "${SELF_LLVM:-}" ]; then la=${SELF_LLVM%%:*}; lb=${SELF_LLVM##*:}; fi
    if [ "$la" -le 1 ]; then
        for m in fmt ast err ui glsl; do sub $m; done
        sub glsl --es
        sub ir --norm
    fi
    # llvm: one input per call (a whole-corpus llvm run fills /tmp).
    li=0
    while IFS= read -r f; do
        li=$((li + 1))
        if [ $li -ge "$la" ] && [ $li -le "$lb" ]; then sub llvm "$f"; fi
    done < <("$0" llvm --list)
    echo "diff.sh self: $((n - bad)) ok, $bad failed (of $n comparisons)"
    [ $bad = 0 ] || exit 1
    exit 0
fi

# After the build: stage0 builds j3c-boot with its own embedded stdlib.
case $mode in ir|llvm|glsl|ui|run|bc) export J3_STDLIB=$root/j3c/stdlib ;; esac

# Inputs: corpus lines from sections naming this mode, expanded per mode.
cd "$root"
expand() {
    local p=$1
    [ -e "$p" ] || die "input $p does not exist"
    if [ -f "$p" ]; then echo "$p"; return; fi
    # err: a folder with a `j3.toml` is one project case (tests/bad-ui/NAME), checked as a whole.
    if [ $mode = err ] && [ -f "$p/j3.toml" ]; then echo "$p"; return; fi
    case $mode in
        fmt|ast|err) find "$p" \( -name target -o -name node_modules -o -name '*.d' \) -prune -o -name '*.j3' -type f -print | sort ;;
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

kname=$mode; [ $user = 0 ] || kname=$mode-user   # --user compares less, so it has its own list
[ $es = 0 ] || kname=$mode-es
known=$here/known/$kname.txt
knownset=$( [ -f "$known" ] && sed 's/#.*//; s/[[:space:]]*$//; /^$/d' "$known" || true )
isknown() { grep -qxF -- "$1" <<<"$knownset"; }
# `PATH  # skip: CAUSE` in the known list: the input is never run (network). It stays a known entry.
skipset=$( [ -f "$known" ] && sed -n 's/[[:space:]]*#[[:space:]]*skip:.*//p' "$known" || true )
if [ -n "$skipset" ]; then
    skipped=$(grep -xF -- "$skipset" <<<"$inputs" || true)
    inputs=$(grep -vxF -- "$skipset" <<<"$inputs" || true)
    [ -z "$skipped" ] || sed "s|^|skip |" <<<"$skipped" >&$((list + 1))
    [ -n "$inputs" ] || die "every input for $mode is skipped"
fi
# `PATH  # nondet: CAUSE`: the oracle's own output changes run to run (timings, pids). The input still runs (a
# j3c-boot crash on it would show as CRASH when stage0's two runs agree), but no outcome of it fails the run.
nondetset=$( [ -f "$known" ] && sed -n 's/[[:space:]]*#[[:space:]]*nondet:.*//p' "$known" || true )
isnondet() { [ -n "$nondetset" ] && grep -qxF -- "$1" <<<"$nondetset"; }
if [ $list = 1 ]; then echo "$inputs"; exit 0; fi
# run builds in the project's own target/: never j3c-bootstrap itself (the running binary lives there).
[ $mode != run ] || ! grep -qxF j3c-bootstrap <<<"$inputs" || die "run: j3c-bootstrap is not a run input (use self)"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# one IDX FILE: run both compilers, write $tmp/IDX/{status,detail}.
one() {
    set +e
    local d=$tmp/$1 f=$2 cwd=$root arg=$2 who
    mkdir -p "$d"
    if [ $mode = err ] && [ -d "$root/$f" ]; then
        # A project case: `check .` in a copy (no target/ left in the tree).
        mkdir -p "$d/w"; cp -r "$root/$f/." "$d/w/"; rm -rf "$d/w/target"; cwd=$d/w; arg=.
    elif [ $mode = err ]; then
        mkdir -p "$d/w"; cp "$root/$f" "$d/w/"; cwd=$d/w; arg=$(basename "$f")
        # A case's companion files (a `.j3s` beside it): tests/bad/NAME.d/ is copied next to NAME.j3.
        [ ! -d "$root/${f%.j3}.d" ] || cp -r "$root/${f%.j3}.d/." "$d/w/"
        # A companion with a `j3.toml` (entry NAME.j3, modules in `src/`) makes the case a project.
        [ ! -f "$d/w/j3.toml" ] || arg=.
    fi
    local -a cmd
    case $mode in
        fmt) cmd=(fmt "$arg") ;;
        ast) cmd=(dump-ast "$arg") ;;
        err) cmd=(check "$arg") ;;
        ir) cmd=(dump-ir "$arg") ;;
        llvm) cmd=(dump-llvm "$arg") ;;
        glsl) cmd=(dump-glsl "$arg"); [ $es = 0 ] || cmd+=(--es) ;;
        # ponytail: every UI project keeps its components in `ui/` (`[ui] dir`); read j3.toml if one moves.
        ui) cmd=(dump-ui "$arg/ui") ;;
        run) cmd=(run --headless --frames 3 "$arg") ;;
        bc) ;;
    esac
    # bc: the folder copied per compiler run ($bsrc) and what runs in the copy. A file belongs to the nearest
    # `j3.toml` above it; j3c-bootstrap's own (tests/cases/scripts/tool.j3s) is too big to copy: its folder then.
    local bsrc=
    if [ $mode = bc ]; then
        if [ -d "$root/$f" ]; then bsrc=$root/$f; cmd=(run --headless --frames 3 .)
        else
            bsrc=$(dirname "$root/$f")
            while [ ! -f "$bsrc/j3.toml" ] && [ "$bsrc" != "$root" ] && [ "$bsrc" != / ]; do bsrc=$(dirname "$bsrc"); done
            # ponytail: one named exception; list others here if a corpus file sits in another huge project.
            if [ ! -f "$bsrc/j3.toml" ] || [ "$bsrc" = "$boot" ]; then bsrc=$(dirname "$root/$f"); fi
            local rel=$root/$f; cmd=(dump-bc "${rel#"$bsrc"/}")
        fi
    fi
    # run: the project the input builds in. A folder is its own; a file belongs to the nearest `j3.toml`
    # above it (`j3c run FILE` builds that project); a file outside any project builds to a temp file and
    # leaves nothing behind.
    local proj=
    if [ $mode = run ]; then
        proj=$root/$f; [ -d "$proj" ] || proj=$(dirname "$proj")
        while [ ! -f "$proj/j3.toml" ] && [ "$proj" != "$root" ] && [ "$proj" != / ]; do proj=$(dirname "$proj"); done
        [ -f "$proj/j3.toml" ] || proj=
        # A lone file runs in a scratch cwd: what it writes by relative path dies with $d/w
        # (j3c/stdlib/tests/win_io_test.j3 makes `C:\j3_win_io_test\...` folders in the cwd on Linux).
        if [ -z "$proj" ]; then mkdir -p "$d/w"; cwd=$d/w; cmd=(run --headless --frames 3 "$root/$f"); fi
    fi
    for who in a a2 b; do
        local bin=$S0; [ $who = b ] && bin=$BOOT
        # Each compiler builds from nothing: no stamp or object of the other one (or of an earlier run).
        [ -z "$proj" ] || rm -rf "$proj/target/.j3c"
        if [ -n "$bsrc" ]; then
            rm -rf "$d/w"; mkdir -p "$d/w"; cwd=$d/w
            (cd "$bsrc" && tar cf - --exclude=./target .) | (cd "$d/w" && tar xf -)
        fi
        (cd "$cwd" && SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy timeout -k 5 "$to" "$bin" "${cmd[@]}" \
            >"$d/$who.out" 2>"$d/$who.err" </dev/null)
        echo $? >"$d/$who.rc"
        # ponytail: the entry module is the dump's first `module` section; later sections are imports.
        [ $user = 1 ] && awk '/^module /{n++} n<=1' "$d/$who.out" >"$d/$who.n" && mv "$d/$who.n" "$d/$who.out"
        # ponytail: a windowed program's `J3STATS` / `J3UISTATS` lines are frame timings; dropped whole (their
        # counters go too). Keep the counter fields if a paint-count difference ever needs catching here.
        # bc: the bytecode files the run wrote, by content (a project's stdout is not compared: timings).
        if [ -n "$bsrc" ]; then
            [ -d "$root/$f" ] && : >"$d/$who.out"
            { echo "--- j3sb"; (cd "$d/w" && find . -name '*.j3sb' -type f | LC_ALL=C sort | xargs -r sha256sum); } >>"$d/$who.out"
            [ -d "$root/$f" ] && : >"$d/$who.err"
            rm -rf "$d/w"
        fi
        [ $mode = run ] && sed -i '/^J3STATS \|^J3UISTATS /d' "$d/$who.out"
        # ponytail: tests print their own timings (`29 us per frame`, `5.233 ms`): the number before a time unit
        # is masked. A test whose result is a duration would be hidden by this; none in the corpus.
        [ $mode = run ] && sed -i -E 's/[0-9]+(\.[0-9]+)? (us|ms|ns|s)\b/N \2/g' "$d/$who.out"
        # ponytail: thread-race counters (a11y_inbox_test: `full N times, woke N`); add patterns here, not entries.
        [ $mode = run ] && sed -i -E 's/full [0-9]+ times, woke [0-9]+/full N times, woke N/' "$d/$who.out"
        [ $norm = 1 ] && awk -f "$here/norm_ir.awk" "$d/$who.out" >"$d/$who.n" && mv "$d/$who.n" "$d/$who.out"
        { cat "$d/$who.out"
          if [ $mode != run ]; then echo "--- stderr"; cat "$d/$who.err"; fi
          echo "--- exit $(cat "$d/$who.rc")"; } >"$d/$who.txt"
    done
    local ra rb; ra=$(cat "$d/a.rc"); rb=$(cat "$d/b.rc")
    if ! cmp -s "$d/a.txt" "$d/a2.txt"; then
        echo NONDET >"$d/status"; diff -u --label stage0-run1 --label stage0-run2 "$d/a.txt" "$d/a2.txt" | head -40 >"$d/detail"
    elif { [ "$rb" -ge 124 ] && [ "$rb" != "$ra" ]; } || { [ "$rb" != 0 ] && [ "$ra" = 0 ]; }; then
        echo CRASH >"$d/status"; { echo "j3c-boot exit $rb (stage0 exit $ra)"; grep -v '^j3c: \[\| modules compiled$' "$d/b.err" | tail -8; } >"$d/detail"
    elif cmp -s "$d/a.txt" "$d/b.txt"; then
        echo ok >"$d/status"
    else
        echo DIFF >"$d/status"; diff -u --label stage0 --label j3c-boot "$d/a.txt" "$d/b.txt" | head -40 >"$d/detail"
    fi
    [ -z "$proj" ] || rm -rf "$proj/target/.j3c"
    # Only status and detail are read after this; dumps of a whole corpus would fill the disk.
    rm -rf "$d/w" "$d"/*.out "$d"/*.err "$d"/*.txt "$d"/*.rc
    true
}
export -f one
export tmp root boot mode norm user es here S0 BOOT to

start=$SECONDS
echo "$inputs" | awk '{print NR; print}' | xargs -d '\n' -n2 -P "$jobs" bash -c 'one "$@"' _

nok=0; ndiff=0; ncrash=0; nnon=0; new=0; fixed=(); failing=(); i=0
while IFS= read -r f; do
    i=$((i + 1)); st=$(cat "$tmp/$i/status" 2>/dev/null || echo CRASH)
    case $st in
        ok) nok=$((nok + 1)); echo "ok $f"; if isknown "$f" && ! isnondet "$f"; then fixed+=("$f"); fi; continue ;;
        DIFF) ndiff=$((ndiff + 1)) ;;
        CRASH) ncrash=$((ncrash + 1)) ;;
        NONDET) nnon=$((nnon + 1))
                if isnondet "$f"; then echo "NONDET $f (known)"; continue; fi
                new=$((new + 1)); echo "NONDET $f (stage0 printed different text on two runs)"
                sed 's/^/    /' "$tmp/$i/detail"; continue ;;
    esac
    echo "$st $f"; failing+=("$f")
    if ! isknown "$f"; then new=$((new + 1)); [ $update = 1 ] || sed 's/^/    /' "$tmp/$i/detail" 2>/dev/null; fi
done <<<"$inputs"

if [ $update = 1 ]; then
    mkdir -p "$here/known"
    oldknown=$(cat "$known" 2>/dev/null || true)
    { echo "# diff.sh $kname: known DIFF/CRASH inputs. Only ever shrinks: remove a line when its input passes."
      # ponytail: an existing list only shrinks; a new failure is never blessed (delete the file to reseed).
      { grep -vxF -- "$inputs" <<<"$knownset" || true
        # a `# nondet:` entry passes or is NONDET, so it is in neither list above: keep it
        [ -z "$nondetset" ] || echo "$nondetset"
        if [ -n "$knownset" ]; then printf '%s\n' "${failing[@]+"${failing[@]}"}" | grep -xF -- "$knownset" || true
        else printf '%s\n' "${failing[@]+"${failing[@]}"}"; fi; } | sed '/^$/d' | sort -u |
        # a kept entry keeps its `# cause` / `# skip:` comment
        awk 'NR == FNR { k = $0; sub(/[[:space:]]*#.*/, "", k); if (k != "") old[k] = $0; next } { print (($0 in old) ? old[$0] : $0) }' <(echo "$oldknown") -
    } >"$known"
    echo "diff.sh: wrote tests/known/$kname.txt" >&2
    [ -n "$knownset" ] || new=$nnon; fixed=()
fi
for f in "${fixed[@]+"${fixed[@]}"}"; do echo "fixed $f: remove it from tests/known/$kname.txt"; done
sum="diff.sh $mode: $nok ok, $ndiff DIFF, $ncrash CRASH"
[ $nnon = 0 ] || sum="$sum, $nnon NONDET"
echo "$sum ($((SECONDS - start)) s)"
[ $new = 0 ] && [ ${#fixed[@]} = 0 ] || exit 1
