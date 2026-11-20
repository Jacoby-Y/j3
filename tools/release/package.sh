#!/bin/sh
# Release-build j3c and package it into dist/:
#   j3c-<ver>-<arch>-linux.tar.gz   j3c/bin/j3c + licenses + README (what install.sh unpacks)
#   j3c_<ver>_<debarch>.deb          /usr/bin/j3c (needs dpkg-deb)
#   SHA256SUMS
# --windows instead: j3c-<ver>-x86_64-windows.zip (j3c/j3c.exe + licenses + README.md) and a bare j3c.exe, cross-built by the Linux
# j3c this script just built (needs clang, x86_64-w64-mingw32-windres or llvm-windres), added to SHA256SUMS. What
# install.ps1 unpacks. Fails when the build warns (no windres = an exe without its UTF-8 manifest).
# Native builds only: run it on the arch you ship. J3_SKIP_BUILD=1 reuses j3c-bootstrap/target/j3c.
# The build: a seed j3c builds j3c, chosen like build.sh does (j3c-bootstrap/tools/seed.sh list: $J3_SEED, the
# installed j3c, its j3c.prev, then the release pinned in j3c-bootstrap/seed/VERSION, downloaded only when the
# others failed); one that fails hands over to the next. J3_JOBS defaults to half the cores (see build.sh). No seed at all: J3_SEED=$(j3c-bootstrap/tools/stage0.sh rust-final) (cargo; release.yml does this).
# Maintainer of the .deb: J3_DEB_MAINTAINER (default below). j3c-composer is not shipped (separate tool).
set -eu
umask 022
budget=$(( $(nproc) / 2 )); [ "$budget" -ge 1 ] || budget=1
export J3_JOBS=${J3_JOBS:-$budget}
pin=
if [ "$J3_JOBS" -lt "$(nproc)" ] 2>/dev/null && taskset -c "0-$((J3_JOBS - 1))" true 2>/dev/null; then pin="taskset -c 0-$((J3_JOBS - 1))"; fi
REPO_URL=https://github.com/Jacoby-Y/j3
MAINTAINER=${J3_DEB_MAINTAINER:-"Jacoby-Y <Jacoby-Y@users.noreply.github.com>"}

root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"
die() { echo "package.sh: $*" >&2; exit 1; }
windows=
case "${1:-}" in
  --windows) windows=1 ;;
  '') ;;
  *) die "unknown argument $1 (only --windows)" ;;
esac

# The version is git's (see build.sh): written for this build only, line 1 the build, line 2 its tag.
vf=j3c-bootstrap/text/version-build.txt
writeVersion() {
  d=$(git describe --tags --match 'v[0-9]*' --dirty) || die "git describe: no v* tag (fetch tags)"
  printf '%s\n%s\n' "${d#v}" "$(git describe --tags --match 'v[0-9]*' --abbrev=0 | sed 's/^v//')" > "$vf"
}

if [ -z "${J3_SKIP_BUILD:-}" ]; then
  seeds=$(j3c-bootstrap/tools/seed.sh list)
  last=pinned; [ -z "${J3_SEED:-}" ] || last=
  writeVersion
  # The seed embeds its own stdlib; the compiler may need newer modules, so it reads the tree's.
  seed=
  nl='
'
  ifs=$IFS
  IFS=$nl
  for c in $seeds $last; do
    IFS=$ifs
    if [ "$c" = pinned ]; then c=$(j3c-bootstrap/tools/seed.sh) || break; fi
    echo "package.sh: building with seed $c ($("$c" version))"
    if J3_STDLIB=$PWD/j3c/stdlib $pin "$c" build --opt 2 j3c-bootstrap; then seed=$c; break; fi
    echo "package.sh: seed $c failed; trying the next seed"
  done
  IFS=$ifs
  rm -f "$vf"
  [ -n "$seed" ] || die "no seed j3c built it (see above): install one, set J3_SEED, or pin j3c-bootstrap/seed/VERSION"
  strip j3c-bootstrap/target/j3c
fi
bin=j3c-bootstrap/target/j3c
[ -x "$bin" ] || die "no $bin (build failed?)"

version=$("$bin" version | awk '{print $2}')
[ -n "$version" ] || die "$bin version printed nothing"

dist=$root/dist
mkdir -p "$dist"

# Replace (or add) this file's line in dist/SHA256SUMS, keeping the others (the Linux run's).
addSum() {
  (cd "$dist" && {
    [ ! -f SHA256SUMS ] || while read -r h f; do [ "$f" = "$1" ] || echo "$h  $f"; done < SHA256SUMS
    sha256sum "$1"
  } > SHA256SUMS.new && mv SHA256SUMS.new SHA256SUMS)
}

if [ -n "$windows" ]; then
  # j3c.exe embeds the same version-build.txt as $bin; J3_PACKAGE_VERSION puts it in the VERSIONINFO too.
  w=$(mktemp -d)
  trap 'rm -rf "$w" "$vf"' EXIT INT TERM
  writeVersion
  # j3c.exe takes $bin's version, so the two always agree. Git may say -dirty by now (CI's samples and stdlib
  # steps run before this and can touch tracked files): that only warns, a stale $bin shows in the message.
  gv=$(head -n 1 "$vf")
  tag=$(sed -n 2p "$vf")
  [ "$version" = "$gv" ] || echo "package.sh: warning: $bin is $version, git says $gv; j3c.exe gets $version" >&2
  want=$version
  printf '%s\n%s\n' "$want" "$tag" > "$vf"
  exe=j3c-bootstrap/target/j3c.exe
  rm -f "$exe"
  # sh has no pipefail: a failed build shows as no $exe.
  J3_PACKAGE_VERSION=$want J3_STDLIB=$PWD/j3c/stdlib $pin "$bin" build --release --target x86_64-pc-windows-gnu j3c-bootstrap 2>&1 | tee "$w/build.log"
  rm -f "$vf"
  [ -f "$exe" ] || die "no $exe (build failed, see above)"
  # --release already strips (-Wl,-s). No windres (or a broken one) = an exe without its UTF-8 manifest and icon:
  # toolchain/winres.j3 warns `Windows resources ... not embedded`, a broken release here.
  ! grep -q '^warning: .*\(Windows resources\|\[dist\] icon\)' "$w/build.log" ||
    die "the Windows resources did not build (see the warning above); install binutils-mingw-w64-x86-64"
  zname=j3c-$version-x86_64-windows.zip
  mkdir -p "$w/j3c"
  cp "$exe" "$w/j3c/j3c.exe"
  cp LICENSE-MIT LICENSE-APACHE THIRD_PARTY.md README.md "$w/j3c/"
  cp j3c/editor/LICENSE-DejaVu.txt "$w/j3c/LICENSE-DejaVu.txt"
  cp j3c-bootstrap/text/icons/LICENSE "$w/j3c/LICENSE-BootstrapIcons.txt"
  rm -f "$dist/$zname"
  (cd "$w" && zip -qrX "$dist/$zname" j3c) || die "zip failed (apt install zip)"
  addSum "$zname"
  # Versionless bare exe: releases/latest/download/j3c.exe is a stable one-click link.
  cp "$exe" "$dist/j3c.exe"
  addSum j3c.exe
  echo "dist/$zname"
  echo "dist/j3c.exe"
  echo "dist/SHA256SUMS"
  exit 0
fi

arch=$(uname -m)
case "$arch" in
  x86_64) debarch=amd64 ;;
  aarch64|arm64) arch=aarch64; debarch=arm64 ;;
  *) die "unsupported arch $arch" ;;
esac

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT INT TERM

# Licenses for j3c and what it embeds (DejaVu font, Bootstrap Icons).
copy_licenses() {
  cp LICENSE-MIT LICENSE-APACHE THIRD_PARTY.md "$1/"
  cp j3c/editor/LICENSE-DejaVu.txt "$1/LICENSE-DejaVu.txt"
  cp j3c-bootstrap/text/icons/LICENSE "$1/LICENSE-BootstrapIcons.txt"
}

# --- tarball ---
tname=j3c-$version-$arch-linux
t=$work/tar/j3c
mkdir -p "$t/bin"
install -m 755 "$bin" "$t/bin/j3c"
copy_licenses "$t"
cat > "$t/README" <<TXT
j3c $version ($arch Linux), the J3 compiler.

Install: copy bin/j3c to a folder on PATH (install.sh does this; default ~/.local/bin).
Needs clang to build programs; SDL2 and libGL to run windowed ones.
Check your setup with: j3c doctor
Start: j3c guide, j3c new mygame

$REPO_URL
TXT
tar -C "$work/tar" --owner=0 --group=0 --numeric-owner -czf "$dist/$tname.tar.gz" j3c

# --- .deb ---
command -v dpkg-deb >/dev/null || die "dpkg-deb not found (apt install dpkg)"
d=$work/deb
mkdir -p "$d/DEBIAN" "$d/usr/bin" "$d/usr/share/doc/j3c"
install -m 755 "$bin" "$d/usr/bin/j3c"
copy_licenses "$d/usr/share/doc/j3c"
rm "$d/usr/share/doc/j3c/LICENSE-MIT" "$d/usr/share/doc/j3c/LICENSE-APACHE"
{
  echo "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/"
  echo "Upstream-Name: j3c"
  echo "Source: $REPO_URL"
  echo
  echo "Files: *"
  echo "Copyright: 2026 Jacoby-Y and J3 contributors"
  echo "License: MIT or Apache-2.0"
  echo " Apache-2.0: /usr/share/common-licenses/Apache-2.0"
  echo " MIT:"
  sed -e 's/^$/./' -e 's/^/ /' LICENSE-MIT
  echo
  echo "Comment: j3c embeds DejaVu Sans and Bootstrap Icons; see THIRD_PARTY.md,"
  echo " LICENSE-DejaVu.txt and LICENSE-BootstrapIcons.txt in this folder."
} > "$d/usr/share/doc/j3c/copyright"
# Lowest glibc symbol version the binary needs (22.04 builds give 2.35).
glibc=$(objdump -T "$bin" 2>/dev/null | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -V | tail -n 1)
libc_dep=libc6
[ -z "$glibc" ] || libc_dep="libc6 (>= $glibc)"
size=$(du -sk "$d/usr" | cut -f1)
cat > "$d/DEBIAN/control" <<CTL
Package: j3c
Version: $version
Architecture: $debarch
Maintainer: $MAINTAINER
Installed-Size: $size
Depends: $libc_dep, libgcc-s1, clang
Recommends: libsdl2-2.0-0, libgl1, libsdl2-ttf-2.0-0, libsdl2-image-2.0-0, libsdl2-mixer-2.0-0, fonts-dejavu-core
Suggests: fonts-noto-color-emoji
Section: devel
Priority: optional
Homepage: $REPO_URL
Description: J3 compiler: an ahead-of-time compiled game language
 j3c compiles J3 programs to native binaries through clang, and ships the
 J3 standard library, docs (j3c doc), a language server (j3c lsp), a 3D
 scene editor (j3c editor) and an MCP server for agents.
CTL
debname=j3c_${version}_$debarch.deb
dpkg-deb --build --root-owner-group "$d" "$dist/$debname" >/dev/null

(cd "$dist" && sha256sum "$tname.tar.gz" "$debname" > SHA256SUMS.new && mv SHA256SUMS.new SHA256SUMS)
echo "dist/$tname.tar.gz"
echo "dist/$debname"
echo "dist/SHA256SUMS"
