#!/bin/sh
# The seed j3c: the compiler that builds stage 1 of j3c-bootstrap (build.sh, tools/release/package.sh; plan 15).
#   seed.sh            fetch the pinned release named in seed/VERSION and print the path of its j3c
#   seed.sh list [DIR...]
#                      print the usable local seeds, best first, one path per line: $J3_SEED alone (unchecked)
#                      when set, else the installed j3c (PATH), j3c.prev beside it, DIR/j3c and DIR/j3c.prev.
#                      Skipped ones are named on stderr. Prints nothing when none is usable. Never downloads:
#                      callers run `seed.sh` (the pinned release) only after every listed seed failed.
# Usable = a J3 j3c (a Rust-era one, e.g. stage 0 or j3c-rust, is told by the Rust runtime's RUST_BACKTRACE
# string in the binary and skipped; only J3_SEED still takes one) whose `j3c version` is at least
# seed/VERSION's when that is pinned.
# The release asset is j3c-<version>-<arch>-linux.tar.gz from the GitHub release v<version>, as
# tools/release/package.sh names it; its sha256 must match both the pinned line in seed/VERSION and the
# release's SHA256SUMS. Cached in ${XDG_CACHE_HOME:-~/.cache}/j3/seed/<version>/.
# With no usable seed at all, stage 0 is the manual last resort (needs cargo):
# J3_SEED=$(j3c-bootstrap/tools/stage0.sh rust-final) ./build.sh.
set -eu
REPO=Jacoby-Y/j3
cd "$(dirname "$0")/.."
die() { echo "seed.sh: $*" >&2; exit 1; }
vfile=seed/VERSION
[ -f "$vfile" ] || die "no $vfile"
pinned=$(awk '$1 == "version" { print $2; exit }' "$vfile")
[ "$pinned" != unset ] || pinned=

fetch() {
  [ -n "$pinned" ] || die "$vfile is unset (no J3 release is pinned yet; see that file for how to pin one)"
  case "$(uname -m)" in
    x86_64|amd64) arch=x86_64 ;;
    aarch64|arm64) arch=aarch64 ;;
    *) die "no prebuilt j3c for $(uname -m)" ;;
  esac
  pin=$(awk -v a="$arch" '$1 == "sha256" && $2 == a { print $3; exit }' "$vfile")
  [ -n "$pin" ] || die "$vfile pins no sha256 for $arch"
  name=j3c-$pinned-$arch-linux.tar.gz
  cache=${XDG_CACHE_HOME:-$HOME/.cache}/j3/seed/$pinned
  out=$cache/j3c/bin/j3c
  if [ -x "$out" ] && [ "$(cat "$cache/sha256" 2>/dev/null)" = "$pin" ]; then echo "$out"; return; fi
  command -v curl >/dev/null 2>&1 || die "need curl to download the seed"
  command -v sha256sum >/dev/null 2>&1 || die "need sha256sum to check the seed"
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  trap 'exit 1' INT TERM
  url=https://github.com/$REPO/releases/download/v$pinned
  echo "seed.sh: downloading $url/$name" >&2
  curl -fsSL --retry 2 -o "$tmp/$name" "$url/$name" || die "download failed: $url/$name"
  curl -fsSL --retry 2 -o "$tmp/SHA256SUMS" "$url/SHA256SUMS" || die "download failed: $url/SHA256SUMS"
  listed=$(awk -v f="$name" '$2 == f || $2 == "*" f { print $1; exit }' "$tmp/SHA256SUMS")
  got=$(sha256sum "$tmp/$name" | cut -d' ' -f1)
  [ "$got" = "$pin" ] || die "$name: sha256 $got, $vfile pins $pin"
  [ "$listed" = "$pin" ] || die "$name: release SHA256SUMS lists '${listed:-nothing}', $vfile pins $pin"
  rm -rf "$cache"
  mkdir -p "$cache"
  tar -C "$cache" -xzf "$tmp/$name" j3c/bin/j3c || die "$name has no j3c/bin/j3c"
  echo "$pin" > "$cache/sha256"
  echo "$out"
}

# usable PATH: exit 0 when PATH runs, is J3 and is new enough; says why not on stderr.
usable() {
  [ -x "$1" ] || return 1
  v=$("$1" version 2>/dev/null | awk '$1 == "j3c" { print $2; exit }') || v=
  v=${v%-dirty}
  if [ -z "$v" ]; then echo "seed.sh: skip $1 (\`version\` failed)" >&2; return 1; fi
  # Rust std's panic hook reads RUST_BACKTRACE; no J3 j3c contains the string (keep it out of docs and stdlib).
  if grep -qa RUST_BACKTRACE "$1"; then
    echo "seed.sh: skip $1 (version $v: Rust-era j3c; J3_SEED=$1 forces it)" >&2
    return 1
  fi
  [ -n "$pinned" ] || return 0
  low=$(printf '%s\n%s\n' "$pinned" "$v" | sort -V | head -n 1)
  if [ "$low" != "$pinned" ]; then echo "seed.sh: skip $1 (version $v: older than $pinned)" >&2; return 1; fi
}

# try PATH: print PATH once (by real file) when it exists and is usable.
try() {
  [ -e "$1" ] || return 0
  real=$(readlink -f "$1")
  case " $seen " in *" $real "*) return 0 ;; esac
  seen="$seen $real"
  if usable "$1"; then echo "$1"; fi
}

list() {
  if [ -n "${J3_SEED:-}" ]; then
    # Taken as given, no version check: the override is how stage 0 (version 0.1.0) still seeds a build.
    [ -x "$J3_SEED" ] || die "J3_SEED=$J3_SEED is not an executable"
    echo "$J3_SEED"
    return
  fi
  seen=
  installed=$(command -v j3c 2>/dev/null) || installed=
  if [ -n "$installed" ]; then try "$installed"; try "$(dirname "$installed")/j3c.prev"; fi
  for d in "$@"; do try "$d/j3c"; try "$d/j3c.prev"; done
}

case "${1:-}" in
  "") fetch ;;
  list) shift; list "$@" ;;
  *) echo "usage: seed.sh [list [DIR...]]" >&2; exit 2 ;;
esac
