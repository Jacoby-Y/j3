#!/bin/sh
# Fails (rc 1, with a diff) when text/syntax/keywords.txt and the Rust lexer's keyword set differ:
# `KEYWORDS` in j3c/src/parse/token.rs as `keyword`, then `BOOL_LITS` as `literal`, same order.
# The textmate grammar reads the same two Rust tables, so this covers it too.
set -eu
cd "$(dirname "$0")/.."
tok=../j3c/src/parse/token.rs
[ -f "$tok" ] || { echo "keywords.sh: no $tok" >&2; exit 1; }
want=$(mktemp); got=$(mktemp)
trap 'rm -f "$want" "$got"' EXIT
# ponytail: grep/sed over token.rs; a reformat of those two tables shows up as a diff here, not silence.
{
    sed -n '/^pub const KEYWORDS/,/^];/p' "$tok" | sed -n 's/^ *("\([a-z_]*\)", TokenKind::[A-Za-z]*),$/\1 keyword/p'
    sed -n 's/^pub const BOOL_LITS.*= &\[\(.*\)\];$/\1/p' "$tok" | tr ',' '\n' | sed -n 's/^ *"\([a-z_]*\)" *$/\1 literal/p'
} >"$want"
grep -v '^#' text/syntax/keywords.txt | grep -v '^ *$' >"$got" || true
[ -s "$want" ] || { echo "keywords.sh: read no keywords from $tok" >&2; exit 1; }
diff -u "$want" "$got" >&2 || { echo "keywords.sh: text/syntax/keywords.txt differs from $tok (- Rust, + file)" >&2; exit 1; }
