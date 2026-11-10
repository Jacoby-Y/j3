#!/bin/sh
# Install j3c, the J3 compiler, from GitHub Releases:
#   curl -fsSL https://raw.githubusercontent.com/Jacoby-Y/j3/main/install.sh | sh
# Environment:
#   J3_VERSION=0.1.0       a release instead of the latest
#   J3_INSTALL_DIR=DIR     where j3c goes (default ~/.local/bin)
#   J3_TARBALL=FILE        install a local release tarball instead of downloading; its checksum
#                          comes from J3_SHA256=<hex> or a SHA256SUMS file next to it
set -eu
REPO=Jacoby-Y/j3

die() { echo "install.sh: $*" >&2; exit 1; }
say() { echo "$*"; }

[ "$(uname -s)" = Linux ] || die "only Linux has prebuilt j3c so far ($(uname -s)); build from source: https://github.com/$REPO"
case "$(uname -m)" in
  x86_64|amd64) arch=x86_64 ;;
  aarch64|arm64) arch=aarch64 ;;
  *) die "no prebuilt j3c for $(uname -m); build from source: https://github.com/$REPO" ;;
esac

dir=${J3_INSTALL_DIR:-$HOME/.local/bin}
tmp=$(mktemp -d 2>/dev/null || mktemp -d -t j3c)
trap 'rm -rf "$tmp"' EXIT
trap 'exit 1' INT TERM

fetch() { # url file
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --retry 2 -o "$2" "$1" || die "download failed: $1"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$2" "$1" || die "download failed: $1"
  else
    die "need curl or wget"
  fi
}

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | cut -d' ' -f1
  else die "need sha256sum or shasum to verify the download"
  fi
}

# expected sum for file name $2 from a SHA256SUMS file $1
sum_from() { awk -v f="$2" '$2 == f || $2 == "*" f { print $1; exit }' "$1"; }

if [ -n "${J3_TARBALL:-}" ]; then
  [ -f "$J3_TARBALL" ] || die "J3_TARBALL: no file $J3_TARBALL"
  tarball=$J3_TARBALL
  name=$(basename "$tarball")
  want=${J3_SHA256:-}
  sums=$(dirname "$tarball")/SHA256SUMS
  if [ -z "$want" ] && [ -f "$sums" ]; then want=$(sum_from "$sums" "$name"); fi
  [ -n "$want" ] || die "no checksum for $name: set J3_SHA256 or put SHA256SUMS next to it"
else
  version=${J3_VERSION:-}
  if [ -z "$version" ]; then
    fetch "https://api.github.com/repos/$REPO/releases/latest" "$tmp/latest.json"
    version=$(sed -n 's/.*"tag_name": *"v\{0,1\}\([^"]*\)".*/\1/p' "$tmp/latest.json" | head -n 1)
    [ -n "$version" ] || die "could not find the latest release of $REPO"
  fi
  version=${version#v}
  name=j3c-$version-$arch-linux.tar.gz
  base=https://github.com/$REPO/releases/download/v$version
  say "downloading j3c $version ($arch)"
  fetch "$base/$name" "$tmp/$name"
  fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS"
  tarball=$tmp/$name
  want=$(sum_from "$tmp/SHA256SUMS" "$name")
  [ -n "$want" ] || die "SHA256SUMS of v$version has no line for $name"
fi

got=$(sha256 "$tarball")
[ "$got" = "$want" ] || die "checksum mismatch for $name (got $got, want $want); not installing"

mkdir -p "$tmp/x"
tar -xzf "$tarball" -C "$tmp/x" || die "could not unpack $name"
[ -f "$tmp/x/j3c/bin/j3c" ] || die "$name has no j3c/bin/j3c"
mkdir -p "$dir" || die "cannot create $dir"
cp "$tmp/x/j3c/bin/j3c" "$dir/.j3c.new" || die "cannot write to $dir"
chmod 755 "$dir/.j3c.new"
mv -f "$dir/.j3c.new" "$dir/j3c"
say "installed $("$dir/j3c" version) to $dir/j3c"

case ":$PATH:" in
  *":$dir:"*) ;;
  *) say "warning: $dir is not on PATH; add it, e.g. echo 'export PATH=\"$dir:\$PATH\"' >> ~/.profile" ;;
esac

if ! command -v clang >/dev/null 2>&1; then
  # shellcheck disable=SC1091
  id=$( (. /etc/os-release 2>/dev/null && echo "${ID:-} ${ID_LIKE:-}") || true)
  case "$id" in
    *debian*|*ubuntu*) hint="sudo apt install clang" ;;
    *fedora*|*rhel*) hint="sudo dnf install clang" ;;
    *arch*) hint="sudo pacman -S clang" ;;
    *) hint="install clang with your package manager" ;;
  esac
  say "warning: clang not found; j3c needs it to build programs: $hint"
fi
say "next: j3c doctor (checks SDL2, GL, fonts), then j3c guide"
