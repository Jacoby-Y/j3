#!/bin/sh
# Fails (rc 1, with a diff) when text/syntax/keywords.txt and the Rust tables (at the git tag rust-final) differ: `KEYWORDS` in
# j3c/src/parse/token.rs as `keyword`, then `BOOL_LITS` as `literal`, then `BUILTIN_TYPES` in
# j3c/src/parse/syntax.rs as `type`, same order. The keyword SCOPE column is checked by the textmate unit
# test (the grammar equals the shipped editors/vscode/syntaxes/j3.tmLanguage.json).
set -eu
cd "$(dirname "$0")/.."
want=$(mktemp); got=$(mktemp); tok=$(mktemp); syn=$(mktemp)
trap 'rm -f "$want" "$got" "$tok" "$syn"' EXIT
git show rust-final:j3c/src/parse/token.rs >"$tok" || { echo "keywords.sh: no token.rs at tag rust-final" >&2; exit 1; }
git show rust-final:j3c/src/parse/syntax.rs >"$syn" || { echo "keywords.sh: no syntax.rs at tag rust-final" >&2; exit 1; }
# ponytail: grep/sed over token.rs / syntax.rs; a reformat of those tables shows up as a diff here, not silence.
{
    sed -n '/^pub const KEYWORDS/,/^];/p' "$tok" | sed -n 's/^ *("\([a-z_]*\)", TokenKind::[A-Za-z]*),$/\1 keyword/p'
    sed -n 's/^pub const BOOL_LITS.*= &\[\(.*\)\];$/\1/p' "$tok" | tr ',' '\n' | sed -n 's/^ *"\([a-z_]*\)" *$/\1 literal/p'
    sed -n '/^pub const BUILTIN_TYPES/,/^];/p' "$syn" | grep -o '"[a-z0-9_]*"' | tr -d '"' | sed 's/$/ type/'
} >"$want"
grep -v '^#' text/syntax/keywords.txt | grep -v '^ *$' | cut -d' ' -f1,2 >"$got" || true
[ -s "$want" ] || { echo "keywords.sh: read no keywords from $tok" >&2; exit 1; }
diff -u "$want" "$got" >&2 || { echo "keywords.sh: text/syntax/keywords.txt differs from the Rust tables (- Rust, + file)" >&2; exit 1; }
