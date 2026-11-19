#!/bin/sh
# Plan 11 A7: every Rust file of j3c has a home in the port, or a reason it's gone.
# Prints each j3c/src/**/*.rs (read from the git tag rust-final: plan 11 C6 deleted j3c/src) that no stub names and DELETED.md doesn't list, one per line; exit 1 if any.
# Also prints `stale: PATH (WHERE)` for a name that matches no Rust file (typo, renamed file); exit 1.
#
# Test files are not ported (the port's tests are J3 source) and need no entry: a file named `tests.rs`
# or `test.rs`, or anything under a `tests/` folder.
#
# `Port of:` lines (stub headers under src/): `// Port of: a.rs, b/c.rs (note), d.rs`. Paths are relative
# to j3c/src/ (a leading `j3c/src/` is dropped), `(...)` is a note and names nothing, a line ending in `,`
# continues on the next `//` line. A path is the first word of each comma item; words not ending in `.rs` (data
# files like `doc/pages/*.md`) are skipped.
#
# DELETED.md: lines `- PATH: reason`. PATH is relative to j3c/src/ and may be a shell glob (`*` also
# crosses `/`); other lines are prose.
set -eu
cd "$(dirname "$0")/.."
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

git ls-tree -r --name-only --full-name rust-final -- ../j3c/src | sed -n 's|^j3c/src/\(.*\.rs\)$|\1|p' | LC_ALL=C sort >"$tmp/every"
[ -s "$tmp/every" ] || { echo "coverage.sh: no j3c/src at tag rust-final" >&2; exit 1; }
grep -Ev '(^|/)tests?\.rs$|(^|/)tests/' "$tmp/every" >"$tmp/all"

# ponytail: header grep, not a J3 parse; a `Port of:` outside a header comment would count too.
find src -name '*.j3' -exec awk '
  function emit(s,  n, i, items, w) {
    gsub(/\([^)]*\)/, "", s)
    n = split(s, items, ",")
    for (i = 1; i <= n; i++) {
      split(items[i], w, " ")
      if (w[1] !~ /\.rs$/) continue
      sub(/^j3c\/src\//, "", w[1])
      print w[1] "\t" FILENAME
    }
  }
  more && /^\/\// { line = $0; sub(/^\/\/ */, "", line); emit(line); more = (line ~ /, *$/); next }
  { more = 0 }
  /^\/\/ Port of:/ { line = $0; sub(/^\/\/ Port of: */, "", line); emit(line); more = (line ~ /, *$/) }
' {} + | LC_ALL=C sort -u >"$tmp/ported"

sed -n 's/^- \([^:]*\):.*/\1/p' DELETED.md >"$tmp/globs"

# Deleted: expand each glob against the Rust list.
: >"$tmp/deleted"
while IFS= read -r g; do
  hit=
  while IFS= read -r f; do
    # shellcheck disable=SC2254
    case "$f" in $g) echo "$f" >>"$tmp/deleted"; hit=1 ;; esac
  done <"$tmp/all"
  [ -n "$hit" ] || echo "stale: $g (DELETED.md)"
done <"$tmp/globs" >"$tmp/stale"

cut -f1 "$tmp/ported" | cat - "$tmp/deleted" | LC_ALL=C sort -u >"$tmp/covered"
# A stub naming a file that isn't a Rust source (typo or rename).
awk -F'\t' 'NR == FNR { all[$1] = 1; next } !($1 in all) { print "stale: " $1 " (" $2 ")" }' "$tmp/every" "$tmp/ported" >>"$tmp/stale"

LC_ALL=C comm -23 "$tmp/all" "$tmp/covered" >"$tmp/left"
cat "$tmp/left" "$tmp/stale"
[ ! -s "$tmp/left" ] && [ ! -s "$tmp/stale" ]
