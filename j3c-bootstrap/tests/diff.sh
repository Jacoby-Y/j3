#!/bin/bash
# Differential harness (plan 11 A4, frozen in plan 15): run target/j3c on every corpus input and compare its
# normalized output with the checked-in tests/expect/MODE/INPUT.out (the Rust oracle's output until plan 15, then
# reviewed). --bless rewrites them. See tests/README.md. Exit 0 all ok, 1 a failure, 2 harness/setup error.
set -euo pipefail
usage() {
    cat <<'E'
usage: tests/diff.sh MODE [--bless] [--norm] [--user] [--es] [--list] [-x] [-j N] [files...]
       tests/diff.sh affected [--list] [REV] [-j N]
  MODE            fmt | ast | err | ir | llvm | glsl | ui | run | bc | tool | self
  affected        the modes the files changed since REV (default HEAD, plus untracked files) can change, cheapest
                  first, each with -x; stops at the first failing mode. --list prints the modes. Never self.
  -x              stop starting inputs after the first DIFF, CRASH or NONDET (the rest print `notrun`)
  --bless         write this run's output as the expected files (tests/expect/KIND/INPUT.out, KIND = MODE, ir-user
                  or glsl-es); each input runs twice and one that prints different text is NONDET, not written.
                  A whole-corpus bless also deletes expected files no input produces. Review the change in git.
  --norm          ir with DIFF_REF only: renumber temps by first appearance on both sides (tests/norm_ir.awk)
  --user          ir only: compare only the entry module (the dump's first `module` section); tests/expect/ir-user/
  --es            glsl only: `dump-glsl --es` (the OpenGL ES text); tests/expect/glsl-es/
  --list          print the expanded inputs, one per line, and exit (nothing runs)
  -j N            parallel jobs (default J3_TEST_JOBS, else half the cores; llvm a quarter: each job
                  holds up to ~1 GB of memory and three dumps of up to ~80 MB on disk). Half the cores at
                  most: each job's builds get J3_JOBS = (nproc/2) / N clang processes (at least 1), the
                  j3c-boot and self stage 2 builds (alone) all nproc/2; a J3_JOBS you set wins
  Inputs start slowest first (seconds of the last run, target/diff-times/KIND; new inputs first); the
  summary names the slowest of this run.
  files           inputs instead of tests/corpus.txt (paths or project folders; tool: case files)
     glsl runs `dump-glsl PATH`; ui runs `dump-ui PROJECT/ui` (the project's `.j3ui` folder).
     tests/bad-ui/NAME is a UI project with one error: ui compares the `dump-ui` error text, and err takes
     such a folder (any folder with a `j3.toml`) as one case, `check .` in a scratch copy.
     ir/llvm/glsl/ui/run export J3_STDLIB=<repo>/j3c/stdlib, so the output names one stdlib path.
     Compared text: stdout, `--- stderr`, stderr, `--- exit N` (run: no stderr); a stdout or stderr over 64 KiB
     is replaced by one `--- sha256 HASH, N lines, N bytes` line (whole-program ir and llvm dumps are megabytes).
     run: a project's `target/.j3c` (stamps, objects) is removed before each run and after the last, so no run
     reuses another's build and nothing is left in the corpus (a file input inside a project counts as that
     project; a file outside any project builds to a temp file, writes no stamps and runs in a scratch cwd).
     A corpus line `!PATH  # CAUSE` drops that input from its section's modes (printed as `skip PATH`: network).
     bc: every compiler works on its own scratch copy (never the tree: `run` and `dump-bc` write `target/j3s/`
     and `scripts/target/`). A project input runs `run --headless --frames 3 .` (dev mode: scripts on the VM) and
     compares the exit code, the set of `*.j3sb` files with their sha256 (not stdout: timings) and, when it
     fails, stderr without the `[n/6]` lines (the refusal text); a `.j3s` file
     input runs `dump-bc` on it in a copy of its project (or of its folder when that project is j3c-bootstrap)
     and compares stdout, stderr, exit code and the written `.j3sb`.
     tool: the inputs are cases, one per line of tests/tool/*.txt (id STEM/NAME, STEM = the file's name):
       NAME<TAB>PROJECT|-<TAB>ARGV[<TAB>OPTION]...   (`#` lines and blank lines skipped)
     ARGV is split like xargs (quotes group words). Each compiler run gets a fresh scratch copy of PROJECT
     (repo-relative, `target/` left out; `-` = an empty folder) as its cwd and compares stdout, stderr, the exit
     code and the OPTION files. OPTIONs: stdin=FILE (relative to tests/tool/; default /dev/null), out=GLOB (each
     matching file: text, or sha256 when binary, with its mode; `**` recurses), unpack=GLOB (a .zip/.deb/.tar.gz
     compared by its member list and unpacked files), mask=SED (`sed -E` on everything compared), env=K=V (`@ROOT@`
     is the repo), timeout=SECONDS. Built-in masks: the scratch path -> $DIR, `docs  XXXXXXXX` -> `docs  ID`, the MCP
     `serverInfo` version, timings (`5.2 ms` -> `N ms`), and dropped ticking progress lines (`      linking: 3s`:
     piped, one prints every 2 s, so whether it shows depends on machine load).
     self: stage 1 (j3c-boot) builds a scratch copy of j3c-bootstrap/{j3.toml,src,text} under <repo>/target/
     (never in place: that would overwrite the running binary); stage 2 is then compared with stage 1 (DIFF_REF)
     on fmt, ast, err, ui, glsl, ir --norm and llvm (one input per call). SELF_BUILD_TIMEOUT seconds (default 1800).
     SELF_LLVM=A:B: only llvm inputs A..B of `llvm --list` (the other modes only when A is 1), to run it in pieces.
Env: DIFF_TIMEOUT seconds per command (default 30; ir/llvm 120; tool 60; run, bc 300).
     DIFF_BOOT replaces the compiler under test (default target/j3c, rebuilt by the seed when it is missing, older
     than src/, or not the plain J3C_VERSION build; DIFF_BOOT skips that). J3_SEED names the seed (else
     tools/seed.sh list's first). DIFF_REF=J3C compares with that compiler's output instead of the expected files
     (self uses it; `DIFF_REF=~/.bin/j3c.prev diff.sh llvm FILE` shows the text behind a changed hash).
E
}
die() { echo "diff.sh: $*" >&2; exit 2; }

here=$(cd "$(dirname "$0")" && pwd)
boot=$(dirname "$here")
root=$(dirname "$boot")
REF=${DIFF_REF:-}
BOOT=${DIFF_BOOT:-$boot/target/j3c}

[ $# -ge 1 ] || { usage >&2; exit 2; }
case $1 in -h|--help) usage; exit 0 ;; esac
mode=$1; shift
# affected: map changed paths to modes; then run each mode with -x, cheapest first.
if [ "$mode" = affected ]; then
    alist=0; rev=HEAD; pass=()
    while [ $# -gt 0 ]; do
        case $1 in --list) alist=1 ;; -j) pass+=(-j "$2"); shift ;; -*) die "affected: unknown flag $1" ;; *) rev=$1 ;; esac
        shift
    done
    all="fmt ast err ui glsl glsl-es bc tool ir ir-user llvm run"
    changed=$(cd "$root" && { git diff --name-only "$rev" --; git ls-files --others --exclude-standard; } | sort -u)
    want=" "
    add() { want="$want$* "; }
    while IFS= read -r f; do
        [ -n "$f" ] || continue
        case $f in
            j3c-bootstrap/src/syntax/*|j3c-bootstrap/src/base/*|j3c-bootstrap/j3.toml) add $all ;;
            j3c-bootstrap/src/load*|j3c-bootstrap/src/sema*|j3c-bootstrap/src/project*|j3c-bootstrap/src/driver*)
                add err ui glsl glsl-es bc tool ir ir-user llvm run ;;
            j3c-bootstrap/src/lower*|j3c-bootstrap/src/ir*) add glsl glsl-es bc ir ir-user llvm run ;;
            j3c-bootstrap/src/backend*|j3c-bootstrap/src/cache*|j3c-bootstrap/src/toolchain*) add tool llvm run ;;
            j3c-bootstrap/src/shader*) add glsl glsl-es ;;
            j3c-bootstrap/src/ui*) add err ui tool run ;;
            j3c-bootstrap/src/devmode*) add bc ;;
            j3c-bootstrap/src/*|j3c-bootstrap/text/*|j3c/editor/*) add tool ;;
            j3c/stdlib/*) add ui glsl glsl-es bc tool ir ir-user llvm run ;;
            speculative/*|examples/*) add fmt ast ui glsl glsl-es bc ir ir-user llvm run ;;
            j3c-bootstrap/tests/expect/*) k=${f#j3c-bootstrap/tests/expect/}; add "${k%%/*}" ;;
            j3c-bootstrap/tests/tool/*) add tool ;;
            j3c-bootstrap/tests/bad*|j3c-bootstrap/tests/diag/*) add err ui ;;
            j3c-bootstrap/tests/proj/*|j3c-bootstrap/tests/corpus.txt|j3c-bootstrap/tests/diff.sh) add $all ;;
        esac
    done <<<"$changed"
    modes=; for m in $all; do case $want in *" $m "*) modes="$modes $m" ;; esac; done
    if [ $alist = 1 ]; then echo $modes; exit 0; fi
    [ -n "$modes" ] || { echo "diff.sh affected: no mode covers the changed files"; exit 0; }
    echo "diff.sh affected:$modes" >&2
    for m in $modes; do
        case $m in ir-user) a=(ir --user) ;; glsl-es) a=(glsl --es) ;; *) a=("$m") ;; esac
        "$0" "${a[@]}" -x ${pass[@]+"${pass[@]}"} || { echo "diff.sh affected: stopped at ${a[*]}"; exit 1; }
    done
    echo "diff.sh affected: all ok ($modes )"
    exit 0
fi
norm=0; user=0; es=0; bless=0; list=0; failfast=0; jobs=; files=()
while [ $# -gt 0 ]; do
    case $1 in
        --norm) norm=1 ;;
        --user) user=1 ;;
        --es) es=1 ;;
        --bless) bless=1 ;;
        --list) list=1 ;;
        -x) failfast=1 ;;
        -j) [ $# -ge 2 ] || die "-j needs a number"; jobs=$2; shift ;;
        -*) die "unknown flag $1" ;;
        *) files+=("$(realpath -m --relative-to="$root" "$1")") ;;
    esac
    shift
done
case $mode in
    fmt|ast|err) to=30 ;;
    ir|llvm|glsl|ui) to=120 ;;
    run|bc) to=300 ;;
    tool) to=60 ;;
    self) to=0 ;;
    *) usage >&2; exit 2 ;;
esac
[ $norm = 0 ] || [ $mode = ir ] || die "--norm is for ir only"
[ $user = 0 ] || [ $mode = ir ] || die "--user is for ir only"
[ $es = 0 ] || [ $mode = glsl ] || die "--es is for glsl only"
[ $norm = 0 ] || [ -n "$REF" ] || die "--norm compares two compilers: set DIFF_REF (the expected files hold the raw dump)"
[ $bless = 0 ] || [ -z "$REF" ] || die "--bless writes the expected files: unset DIFF_REF"
to=${DIFF_TIMEOUT:-$to}
if [ $mode = llvm ] || [ $mode = self ]; then : "${jobs:=${J3_TEST_JOBS:-$(( $(nproc) / 4 ))}}"; fi
: "${jobs:=${J3_TEST_JOBS:-$(( $(nproc) / 2 ))}}"
[ "$jobs" -ge 1 ] 2>/dev/null || jobs=1
budget=$(( $(nproc) / 2 )); [ "$budget" -ge 1 ] || budget=1
solo=${J3_JOBS:-$budget}   # a build that runs alone (boot.sh, the self stage 2)
per=$(( budget / jobs )); [ "$per" -ge 1 ] || per=1
export J3_JOBS=${J3_JOBS:-$per}
[ -z "$REF" ] || [ -x "$REF" ] || die "DIFF_REF: no $REF"

# Fail fast on the structural check (not for --list: it only prints).
# A `self` sub-call (DIFF_SUB) skips it: the parent ran it once.
[ $list = 1 ] || [ -n "${DIFF_SUB:-}" ] || "$boot/tools/layers.sh" >&2 || die "tools/layers.sh failed"

# One build of j3c-boot when it is missing, stale, or a git-versioned ./build.sh binary (tests/boot.sh).
if [ $list = 0 ] && [ -z "${DIFF_BOOT:-}" ]; then J3_JOBS=$solo "$here/boot.sh" || die "cannot build j3c-boot"; fi

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
    if ! (cd "$scratch" && J3_JOBS=$solo timeout -k 5 "${SELF_BUILD_TIMEOUT:-1800}" "$BOOT" build j3c-bootstrap) >"$log" 2>&1; then
        tail -20 "$log" >&2; die "self: stage 1 could not build stage 2"
    fi
    stage2=$scratch/j3c-bootstrap/target/j3c
    [ -x "$stage2" ] || die "self: no $stage2 after the build"
    [ ! "$boot/target/j3c" -nt "$scratch/start" ] || die "self: the build wrote j3c-bootstrap/target/j3c"
    export DIFF_REF=$BOOT DIFF_BOOT=$stage2
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
    # llvm: one input per call (a whole-corpus llvm run fills /tmp), $jobs calls at a time, each writing its
    # own $scratch/llvm.N; printed in list order as `sub` would.
    # the list is repo-relative: each call gets an absolute path (a relative one resolves against the cwd)
    export self=$here/diff.sh scratch root DIFF_SUB=1
    llist=$("$0" llvm --list)
    awk -v a="$la" -v b="$lb" 'NR >= a && NR <= b { print NR; print }' <<<"$llist" |
        xargs -d '\n' -n2 -P "$jobs" bash -c '"$self" llvm "$root/$2" >"$scratch/llvm.$1" 2>&1; echo $? >"$scratch/llvm.$1.rc"' _
    for o in $(ls "$scratch" | sed -n 's/^llvm\.\([0-9]*\)$/\1/p' | sort -n); do
        n=$((n + 1)); f=$scratch/llvm.$o
        if [ "$(cat "$f.rc")" = 0 ]; then tail -1 "$f"
        else bad=$((bad + 1)); echo "FAILED: diff.sh llvm $(sed -n "${o}p" <<<"$llist")"; cat "$f"; fi
    done
    echo "diff.sh self: $((n - bad)) ok, $bad failed (of $n comparisons)"
    [ $bad = 0 ] || exit 1
    exit 0
fi

# One stdlib path in every dump, whichever binary embedded which stdlib.
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
# tool: the case ids of the case files (tests/tool/*.txt, or the files given), checked for shape.
toolcases() {
    local f
    for f in "$@"; do
        [ -f "$f" ] || die "tool: no case file $f"
        [ "$(dirname "$(realpath "$f")")" = "$here/tool" ] || die "tool: case files live in tests/tool/ ($f)"
        awk -F '\t' -v s="$(basename "$f" .txt)" -v f="$f" '/^#/ || /^[[:space:]]*$/ { next }
            NF < 3 || $1 !~ /^[A-Za-z0-9_.-]+$/ { print "diff.sh: " f ":" NR ": want NAME<TAB>PROJECT<TAB>ARGV[<TAB>OPTION]..." > "/dev/stderr"; exit 2 }
            { print s "/" $1 }' "$f" || exit 2
    done
}
inputs=$(
    if [ $mode = tool ]; then
        if [ ${#files[@]} -gt 0 ]; then toolcases "${files[@]}"; else toolcases "$here"/tool/*.txt; fi
    elif [ ${#files[@]} -gt 0 ]; then
        for f in "${files[@]}"; do expand "$f"; done
    else
        on=0
        while read -r line; do
            line=${line%%#*}
            [ -n "${line// /}" ] || continue
            case $line in \[*) case " ${line//[\[\]]/} " in *" $mode "*) on=1 ;; *) on=0 ;; esac; continue ;; esac
            [ $on = 1 ] || continue
            case $line in '!'*|'?'*) continue ;; esac
            for p in $line; do expand "$p"; done   # unquoted: the line is a glob
        done < "$here/corpus.txt"
    fi | awk '!seen[$0]++'
)
# Marked corpus lines in a section of this mode (`MARK PATH  # CAUSE`): `!` never runs the input (network), `?`
# runs it but a DIFF or NONDET of it does not fail the run (a timing assertion that fails under load).
marked() {
    local on=0 line
    while read -r line; do
        line=${line%%#*}
        case $line in \[*) case " ${line//[\[\]]/} " in *" $mode "*) on=1 ;; *) on=0 ;; esac; continue ;; esac
        [ $on = 1 ] || continue
        case $line in "$1"*) line=${line#"$1"}; echo ${line// /} ;; esac
    done < "$here/corpus.txt"
}
skipset=$(marked '!')
flakyset=$(marked '?')
isflaky() { [ -n "$flakyset" ] && grep -qxF -- "$1" <<<"$flakyset"; }
[ -n "$inputs" ] || die "no inputs for $mode"
[ $mode != tool ] || [ -z "$(sort <<<"$inputs" | uniq -d)" ] || die "tool: case names repeat: $(sort <<<"$inputs" | uniq -d | tr '\n' ' ')"

kname=$mode; [ $user = 0 ] || kname=$mode-user   # --user compares less, so it has its own expected files
[ $es = 0 ] || kname=$mode-es
expect=$here/expect/$kname
if [ -n "$skipset" ]; then
    skipped=$(grep -xF -- "$skipset" <<<"$inputs" || true)
    inputs=$(grep -vxF -- "$skipset" <<<"$inputs" || true)
    [ -z "$skipped" ] || sed "s|^|skip |" <<<"$skipped" >&$((list + 1))
    [ -n "$inputs" ] || die "every input for $mode is skipped"
fi
if [ $list = 1 ]; then echo "$inputs"; exit 0; fi
# run builds in the project's own target/: never j3c-bootstrap itself (the running binary lives there).
[ $mode != run ] || ! grep -qxF j3c-bootstrap <<<"$inputs" || die "run: j3c-bootstrap is not a run input (use self)"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# projof FILE: the project folder a run input builds in (nearest `j3.toml` at or above it), or nothing.
projof() {
    local p=$root/$1; [ -d "$p" ] || p=$(dirname "$p")
    while [ ! -f "$p/j3.toml" ] && [ "$p" != "$root" ] && [ "$p" != / ]; do p=$(dirname "$p"); done
    [ ! -f "$p/j3.toml" ] || echo "$p"
}

# unpacked ARCHIVE: its members and unpacked files (mode, sha256, path), times left out (tool `unpack=`).
unpacked() {
    local f=$1 x t; x=$(mktemp -d)
    case $f in
        *.zip) unzip -Z1 "$f" | LC_ALL=C sort; unzip -qq "$f" -d "$x" ;;
        *.deb) ar t "$f"; t=$(realpath "$f"); (cd "$x" && ar x "$t")
               for t in "$x"/*.tar*; do
                   echo "--- $(basename "$t")"; tar -tvf "$t" | awk '{ print $1, $2, $3, $6 }'
                   mkdir -p "$t.d"; tar xf "$t" -C "$t.d"; rm -f "$t"
               done ;;
        *.tar.gz|*.tgz) tar -tvzf "$f" | awk '{ print $1, $2, $3, $6 }'; tar xzf "$f" -C "$x" ;;
        *) echo "unpack: not an archive this knows: $f" ;;
    esac
    # ponytail: paths with spaces or newlines print as they are; no archive in the corpus has one.
    (cd "$x" && find . -type f | LC_ALL=C sort | while IFS= read -r p; do
        echo "$(stat -c %a "$p") $(sha256sum <"$p" | cut -c1-64) $p"; done)
    rm -rf "$x"
}

# one_tool IDX ID: one tool case (tests/tool/STEM.txt, line NAME) on both compilers, each in a fresh scratch copy.
one_tool() {
    local d=$tmp/$1 stem=${2%%/*} name=${2#*/} line who o g f
    mkdir -p "$d"
    line=$(awk -F '\t' -v n="$name" '!/^#/ && $1 == n' "$here/tool/$stem.txt" | head -1)
    local -a col args envs=() outs=() masks=() unpacks=()
    # a tab is IFS white space (runs of it would merge empty columns): split on \x1f instead
    IFS=$'\x1f' read -ra col <<<"${line//$'\t'/$'\x1f'}"
    local proj=${col[1]-} stdin=/dev/null tmo=$to
    mapfile -d '' -t args < <(printf '%s' "${col[2]-}" | xargs -r printf '%s\0')
    for o in "${col[@]:3}"; do
        case $o in
            stdin=*) stdin=$here/tool/${o#stdin=} ;;
            out=*) outs+=("${o#out=}") ;;
            unpack=*) unpacks+=("${o#unpack=}") ;;
            mask=*) masks+=(-e "${o#mask=}") ;;
            env=*) envs+=("${o#env=}"); envs[-1]=${envs[-1]//@ROOT@/$root} ;;
            timeout=*) tmo=${o#timeout=} ;;
            *) echo CRASH >"$d/status"; echo "harness: unknown option $o in tests/tool/$stem.txt" >"$d/detail"; return ;;
        esac
    done
    for who in $whos; do
        local bin=$BOOT; [ $who = a ] && bin=$REF
        rm -rf "$d/w"; mkdir -p "$d/w"
        [ "$proj" = - ] || (cd "$root/$proj" && tar cf - --exclude=./target .) | (cd "$d/w" && tar xf -)
        (cd "$d/w" && env "${envs[@]+"${envs[@]}"}" SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy \
            timeout -k 5 "$tmo" "$bin" "${args[@]+"${args[@]}"}" <"$stdin" >"$d/$who.out" 2>"$d/$who.err")
        echo $? >"$d/$who.rc"
        { cat "$d/$who.out"; echo "--- stderr"; cat "$d/$who.err"; echo "--- exit $(cat "$d/$who.rc")"
          (cd "$d/w" && shopt -s globstar nullglob
           for g in "${outs[@]+"${outs[@]}"}"; do
               for f in $g; do
                   [ -f "$f" ] || continue
                   echo "--- out $f $(stat -c %a "$f")"
                   if [ ! -s "$f" ] || grep -Iq . "$f"; then cat "$f"; else sha256sum <"$f" | cut -c1-64; fi
               done
           done
           for g in "${unpacks[@]+"${unpacks[@]}"}"; do for f in $g; do echo "--- unpack $f"; unpacked "$f"; done; done)
        } | sed -E -e "s|$d/w|\$DIR|g" -e "s|$rootre|\$ROOT|g" -e "s|$homere|\$HOME|g" -e 's/(docs +)[0-9a-f]{8}/\1ID/g' \
              -e 's/("serverInfo":\{"name":"j3c","version":)"[^"]*"/\1"V"/g' \
              -e '/^      (\[[^]]*\] )?[a-z]+: [0-9]+s$/d' \
              -e 's/[0-9]+(\.[0-9]+)? (us|ms|ns|s)\b/N \2/g' ${masks[@]+"${masks[@]}"} >"$d/$who.txt"
        digest "$d/$who.txt"
        rm -rf "$d/w" "$d/$who.out" "$d/$who.err"
    done
}

# one IDX FILE: run both compilers, write $tmp/IDX/{status,detail}.
one() {
    set +e
    if [ $mode = tool ]; then
        one_tool "$1" "$2"
        if [ ! -f "$tmp/$1/status" ]; then verdict "$tmp/$1" "$2"; fi
        rm -f "$tmp/$1"/*.txt "$tmp/$1"/*.rc
        return
    fi
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
        proj=$(projof "$f")
        # A lone file runs in a scratch cwd: what it writes by relative path dies with $d/w
        # (j3c/stdlib/tests/win_io_test.j3 makes `C:\j3_win_io_test\...` folders in the cwd on Linux).
        if [ -z "$proj" ]; then mkdir -p "$d/w"; cwd=$d/w; cmd=(run --headless --frames 3 "$root/$f"); fi
    fi
    for who in $whos; do
        local bin=$BOOT; [ $who = a ] && bin=$REF
        # Each compiler builds from nothing: no stamp or object of the other one (or of an earlier run).
        # A dev run (a project with `.j3s` scripts) also writes `target/j3s/` (bytecode, reload counter): gone too.
        [ -z "$proj" ] || rm -rf "$proj/target/.j3c" "$proj/target/j3s"
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
            # a failed project run keeps its error text (the refusal), less the `[n/6]` progress lines
            if [ -d "$root/$f" ]; then
                if [ "$(cat "$d/$who.rc")" = 0 ]; then : >"$d/$who.err"; else sed -i '/^j3c: \[/d' "$d/$who.err"; fi
            fi
            rm -rf "$d/w"
        fi
        [ $mode = run ] && sed -i '/^J3STATS \|^J3UISTATS /d' "$d/$who.out"
        # ponytail: tests print their own timings (`29 us per frame`, `5.233 ms`): the number before a time unit
        # is masked. A test whose result is a duration would be hidden by this; none in the corpus.
        [ $mode = run ] && sed -i -E 's/[0-9]+(\.[0-9]+)? (us|ms|ns|s)\b/N \2/g' "$d/$who.out"
        # ponytail: thread-race counters (a11y_inbox_test: `full N times, woke N`); add patterns here, not entries.
        [ $mode = run ] && sed -i -E 's/full [0-9]+ times, woke [0-9]+/full N times, woke N/' "$d/$who.out"
        [ $norm = 1 ] && awk -f "$here/norm_ir.awk" "$d/$who.out" >"$d/$who.n" && mv "$d/$who.n" "$d/$who.out"
        # fmt: stdout as a diff against the input (empty for a formatted file), so editing a formatted corpus
        # file keeps its expected file; the input plus the diff is the whole output, so the check is the same.
        [ $mode = fmt ] && { diff -u --label input --label fmt "$root/$f" "$d/$who.out" >"$d/$who.n"; mv "$d/$who.n" "$d/$who.out"; }
        # the checkout and home folders (diagnostics, `doc` headers, debug paths in dumps) are machine-specific
        sed -i -e "s|$rootre|\$ROOT|g" -e "s|$homere|\$HOME|g" "$d/$who.out" "$d/$who.err"
        digest "$d/$who.out"; digest "$d/$who.err"
        { cat "$d/$who.out"
          if [ $mode != run ]; then echo "--- stderr"; cat "$d/$who.err"; fi
          echo "--- exit $(cat "$d/$who.rc")"; } >"$d/$who.txt"
        rm -f "$d/$who.out"   # disk: only the .txt is compared
    done
    verdict "$d" "$f"
    [ -z "$proj" ] || rm -rf "$proj/target/.j3c" "$proj/target/j3s"
    # Only status and detail are read after this; dumps of a whole corpus would fill the disk.
    rm -rf "$d/w" "$d"/*.out "$d"/*.err "$d"/*.txt "$d"/*.rc
    true
}
# digest FILE: a text over $big bytes becomes one line naming its sha256 and size (whole-program ir/llvm dumps
# are megabytes each; the hash keeps the check exact, DIFF_REF shows the text behind a change).
digest() {
    local n; n=$(stat -c %s "$1")
    [ "$n" -le $big ] || echo "--- sha256 $(sha256sum <"$1" | cut -c1-64), $(wc -l <"$1") lines, $n bytes" >"$1"
}
# verdict DIR FILE: status and detail from DIR/{a,a2,b}.txt: b (j3c-boot) against a (DIFF_REF) or the expected
# file of input FILE; with --bless, a2 is a second j3c-boot run and the expected file is written when both agree.
verdict() {
    local d=$1 e=$expect/$2.out
    if [ -n "$REF" ]; then
        if cmp -s "$d/a.txt" "$d/b.txt"; then echo ok >"$d/status"
        else echo DIFF >"$d/status"; diff -u --label "$REF" --label j3c-boot "$d/a.txt" "$d/b.txt" | head -40 >"$d/detail"; fi
    elif [ $bless = 1 ]; then
        if ! cmp -s "$d/a2.txt" "$d/b.txt"; then
            echo NONDET >"$d/status"; diff -u --label run1 --label run2 "$d/a2.txt" "$d/b.txt" | head -40 >"$d/detail"
        elif cmp -s "$e" "$d/b.txt"; then echo ok >"$d/status"
        else mkdir -p "$(dirname "$e")"; cp "$d/b.txt" "$e"; echo blessed >"$d/status"; fi
    elif [ ! -f "$e" ]; then
        echo DIFF >"$d/status"; echo "no expected file ${e#"$root"/} (diff.sh $mode --bless writes it)" >"$d/detail"
    elif cmp -s "$e" "$d/b.txt"; then echo ok >"$d/status"
    else echo DIFF >"$d/status"; diff -u --label expected --label j3c-boot "$e" "$d/b.txt" | head -40 >"$d/detail"; fi
}
# job LINE: the inputs of one job line, one after another.
# Each input's wall time goes to $tmp/IDX/ms; with -x a failure makes $tmp/stop and later inputs never start.
job() {
    local a k t st; IFS=$'\t' read -ra a <<<"$1"
    for ((k = 0; k < ${#a[@]}; k += 2)); do
        [ ! -e "$tmp/stop" ] || return 0
        t=${EPOCHREALTIME//[.,]/}
        one "${a[k]}" "${a[k+1]}"
        mkdir -p "$tmp/${a[k]}"; echo $(( (${EPOCHREALTIME//[.,]/} - t) / 1000 )) >"$tmp/${a[k]}/ms"
        if [ $failfast = 1 ]; then
            st=$(cat "$tmp/${a[k]}/status" 2>/dev/null || echo CRASH)
            case $st in ok|blessed) ;; *) isflaky "${a[k+1]}" || touch "$tmp/stop" ;; esac
        fi
    done
}
export -f one projof job verdict one_tool unpacked digest isflaky
whos=b; [ -z "$REF" ] || whos="a b"; [ $bless = 0 ] || whos="a2 b"
big=65536
rootre=$(sed 's/[][\.*^$|]/\\&/g' <<<"$root"); homere=$(sed 's/[][\.*^$|]/\\&/g' <<<"$HOME")
[ ${#HOME} -gt 1 ] || homere=$rootre   # no home folder to mask: an empty pattern would repeat the last one
export tmp root boot mode norm user es here REF BOOT to whos big bless expect rootre homere failfast flakyset

start=$SECONDS
times=$boot/target/diff-times/$kname; [ $norm = 0 ] && [ -z "$REF" ] || times=$times.ref
# One job per input, except run inputs that build in the same project (they share its target/): one job
# runs those in input order. A job line is IDX<TAB>FILE pairs; each input writes only $tmp/IDX.
i=0
while IFS= read -r f; do
    i=$((i + 1)); k=$i
    if [ $mode = run ]; then p=$(projof "$f"); [ -z "$p" ] || k=$p; fi
    printf '%s\t%s\t%s\n' "$k" "$i" "$f"
done <<<"$inputs" |
    awk -F '\t' '!($1 in g) { g[$1] = ++n } { job[g[$1]] = job[g[$1]] (job[g[$1]] == "" ? "" : "\t") $2 "\t" $3 }
        END { for (j = 1; j <= n; j++) print job[j] }' |
    # slowest first by the last run's times (a job's inputs summed; an input with no time first): the long
    # inputs no longer start last and hold up the end of the run.
    awk -F '\t' -v t="$times" 'BEGIN { while ((getline l < t) > 0) { split(l, x, "\t"); ms[x[2]] = x[1] } }
        { c = 0; for (i = 2; i <= NF; i += 2) c += ($i in ms) ? ms[$i] : 1e12; printf "%d\t%s\n", c, $0 }' |
    sort -t $'\t' -k1,1nr -s | cut -f2- |
    xargs -d '\n' -n1 -P "$jobs" bash -c 'job "$1"' _

nok=0; nbless=0; ndiff=0; ncrash=0; nnon=0; nflaky=0; nnot=0; i=0
while IFS= read -r f; do
    i=$((i + 1)); st=$(cat "$tmp/$i/status" 2>/dev/null || echo CRASH)
    if [ ! -f "$tmp/$i/status" ] && [ -e "$tmp/stop" ] && [ ! -f "$tmp/$i/ms" ]; then nnot=$((nnot + 1)); echo "notrun $f"; continue; fi
    case $st in
        ok) nok=$((nok + 1)); echo "ok $f"; continue ;;
        blessed) nbless=$((nbless + 1)); echo "blessed $f"; continue ;;
        CRASH) ncrash=$((ncrash + 1)) ;;   # the harness itself failed on the input
        DIFF|NONDET) if isflaky "$f"; then nflaky=$((nflaky + 1)); echo "$st $f (flaky in corpus.txt: not a failure)"; continue; fi ;;&
        DIFF) ndiff=$((ndiff + 1)) ;;
        NONDET) nnon=$((nnon + 1)) ;;
    esac
    echo "$st $f"; sed 's/^/    /' "$tmp/$i/detail" 2>/dev/null || true
done <<<"$inputs"

# A whole-corpus run owns tests/expect/KIND/: a file no input produced is stale (--bless deletes it).
nstale=0
if [ -z "$REF" ] && [ ${#files[@]} = 0 ] && [ -d "$expect" ]; then
    while IFS= read -r e; do
        [ -n "$e" ] || continue
        if [ $bless = 1 ]; then rm -f "$expect/$e"; echo "deleted tests/expect/$kname/$e"
        else nstale=$((nstale + 1)); echo "stale tests/expect/$kname/$e: no input produces it (--bless deletes it)"; fi
    done < <(cd "$expect" && find . -name '*.out' -type f | sed 's|^\./||; s|\.out$||' | LC_ALL=C sort |
             LC_ALL=C comm -23 - <(LC_ALL=C sort <<<"$inputs") | sed 's|$|.out|')
    [ $bless = 0 ] || find "$expect" -mindepth 1 -type d -empty -delete
fi
# Input times: merged into target/diff-times/KIND (the next run's order); the slowest of this run are named.
i=0; now=$(while IFS= read -r f; do i=$((i + 1)); [ ! -f "$tmp/$i/ms" ] || printf '%s\t%s\n' "$(cat "$tmp/$i/ms")" "$f"; done <<<"$inputs")
if [ -n "$now" ]; then
    mkdir -p "$(dirname "$times")"
    { printf '%s\n' "$now"; [ ! -f "$times" ] || cat "$times"; } | awk -F '\t' '!seen[$2]++' >"$times.$$" && mv "$times.$$" "$times"
    echo "slowest: $(sort -t $'\t' -k1,1nr <<<"$now" | head -5 | awk -F '\t' '{ printf "%s%s %.1f s", (NR > 1 ? ", " : ""), $2, $1 / 1000 }')"
fi
sum="diff.sh $mode: $nok ok"
[ $bless = 0 ] || sum="$sum, $nbless blessed"
sum="$sum, $ndiff DIFF, $ncrash CRASH"
[ $nnon = 0 ] || sum="$sum, $nnon NONDET"
[ $nstale = 0 ] || sum="$sum, $nstale stale"
[ $nflaky = 0 ] || sum="$sum, $nflaky flaky"
[ $nnot = 0 ] || sum="$sum, $nnot notrun"
echo "$sum ($((SECONDS - start)) s)"
[ $((ndiff + ncrash + nnon + nstale + nnot)) = 0 ] || exit 1
