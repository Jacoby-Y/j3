#!/bin/sh
# Stage order check (plan 11 A9, B2): a file may `use j3c.X` only when X is in its own stage or one
# above it in tools/layers.txt, and no `tools` file may use `lower`. Only `tools` files may `use j3_editor`
# (the editor's own scene code, a tools-only dependency in j3.toml). Prints each violation, exit 1 if any.
# ponytail: grep-based (only `use j3c...` lines, not fully qualified names in code); the real check moves
# into `j3c check` once the compiler reads its own module graph.
set -eu
cd "$(dirname "$0")/../src"
set -f

# stage_of PATH: index and name of the first layers.txt line whose glob matches PATH (relative to src/).
stage_of() {
    i=0
    while read -r name globs; do
        case $name in ''|'#'*) continue ;; esac
        i=$((i + 1))
        for g in $globs; do
            # shellcheck disable=SC2254
            case $1 in $g) echo "$i $name"; return 0 ;; esac
        done
    done < ../tools/layers.txt
    return 1
}

bad=0
for f in $(find . -name '*.j3' | sed 's|^\./||' | sort); do
    if ! s=$(stage_of "$f"); then
        echo "layers: $f matches no line in tools/layers.txt"; bad=1; continue
    fi
    si=${s% *}; sn=${s#* }
    if [ "$sn" != tools ] && grep -q '^[[:space:]]*\(pub[[:space:]]\{1,\}\)\{0,1\}use[[:space:]]\{1,\}j3_editor[.;[:space:]]' "$f"; then
        echo "layers: $f ($sn) uses j3_editor: only tools files may (tools/layers.txt)"; bad=1
    fi
    for m in $(sed -n 's/^[[:space:]]*\(pub[[:space:]]\{1,\}\)\{0,1\}use[[:space:]]\{1,\}j3c\([.;[:space:]]\)/\2/p' "$f" \
               | sed 's/^[;[:space:]].*/ROOT/; s/^\.//; s/[.;[:space:]].*//'); do
        # `use j3c;` is the root module (main.j3); `use j3c.X...` lives in X.j3 or X/.
        if [ "$m" = ROOT ]; then m=""; t=$(stage_of main.j3) || t=""
        else t=$(stage_of "$m.j3") || t=$(stage_of "$m/x.j3") || t=""; fi
        if [ -z "$t" ]; then echo "layers: $f: use j3c${m:+.$m} names no stage"; bad=1; continue; fi
        ti=${t% *}; tn=${t#* }
        if [ "$ti" -gt "$si" ]; then
            echo "layers: $f ($sn) uses j3c${m:+.$m} ($tn), a later stage"; bad=1
        elif [ "$sn" = tools ] && [ "$tn" = lower ]; then
            echo "layers: $f (tools) uses j3c${m:+.$m}: tools reach lowering only through driver (B2)"; bad=1
        fi
    done
done
exit $bad
