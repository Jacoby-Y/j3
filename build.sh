#!/bin/sh

# A special build script for me, the developer

# Build j3c (the J3 compiler in j3c-bootstrap/) and install it onto PATH, then the composer tool.
# Stage 0 (j3c-bootstrap/stage0/j3c, the Rust compiler at the git tag rust-final) builds stage 1, stage 1
# builds stage 2 from a scratch copy, both must print the same `version`, and stage 2 is installed stripped.
# Install dir: $J3_BIN_DIR, else ~/.bin if it exists, else ~/.local/bin.
set -eu
cd "$(dirname "$0")"
root=$(pwd)
command -v clang >/dev/null 2>&1 || { echo "build.sh: clang not found (j3c compiles through it; e.g. apt install clang)"; exit 1; }
s0=j3c-bootstrap/stage0/j3c
if [ ! -x "$s0" ]; then
    # The one place cargo is needed: a clone with no stage 0 builds it from the tag.
    command -v cargo >/dev/null 2>&1 || { echo "build.sh: no $s0, and building it (tag rust-final) needs cargo (https://rustup.rs)"; exit 1; }
    echo "build.sh: no $s0: building it from the tag rust-final (cargo)"
    j3c-bootstrap/tools/stage0.sh rust-final
fi
if [ -n "${J3_BIN_DIR:-}" ]; then bin=$J3_BIN_DIR
elif [ -d "$HOME/.bin" ]; then bin=$HOME/.bin
else bin=$HOME/.local/bin
fi
mkdir -p "$bin"

# The version is git's: `git describe` minus the `v` (`0.1.4-7-gabc1234-dirty`), then the tag (`0.1.4`), into
# a gitignored file both stages embed (src/project/config.j3). Removed after stage 2, so a later in-place build
# (tests/diff.sh compares against stage 0's `0.1.0`) falls back to J3C_VERSION.
vf=j3c-bootstrap/text/version-build.txt
trap 'rm -f "$root/$vf"' EXIT
if d=$(git describe --tags --match 'v[0-9]*' --dirty 2>/dev/null) && t=$(git describe --tags --match 'v[0-9]*' --abbrev=0 2>/dev/null); then
    printf '%s\n%s\n' "${d#v}" "${t#v}" > "$vf"
fi
echo "build.sh: stage 1 (built by stage 0)"
# Stage 0 embeds the stdlib of its tag; the compiler uses newer stdlib modules (j3.data.toml), so stage 0 reads
# the tree on disk.
J3_STDLIB=$root/j3c/stdlib timeout 1800 "$s0" build --opt 2 j3c-bootstrap
stage1=$root/j3c-bootstrap/target/j3c
# Stage 2: stage 1 builds a copy under target/stage2/ (kept, so its object cache makes rebuilds fast), never
# j3c-bootstrap in place under the running binary. A `j3c` link beside the copy keeps `@embed("../../../j3c/...")`.
s2=$root/target/stage2
mkdir -p "$s2/j3c-bootstrap"
rm -rf "$s2/j3c-bootstrap/src" "$s2/j3c-bootstrap/text"
cp -r j3c-bootstrap/j3.toml j3c-bootstrap/src j3c-bootstrap/text "$s2/j3c-bootstrap/"
ln -sfn "$root/j3c" "$s2/j3c"
echo "build.sh: stage 2 (built by stage 1)"
(cd "$s2" && timeout 1800 "$stage1" build --opt 2 j3c-bootstrap)
stage2=$s2/j3c-bootstrap/target/j3c
v1=$("$stage1" version --verbose | grep -v '^binary ')
v2=$("$stage2" version --verbose | grep -v '^binary ')
[ "$v1" = "$v2" ] || { printf 'build.sh: stage 1 and stage 2 disagree:\n%s\n---\n%s\n' "$v1" "$v2"; exit 1; }
rm -f "$vf"
strip -o "$s2/j3c.stripped" "$stage2"
install -m 755 "$s2/j3c.stripped" "$bin/j3c"
rm -f "$s2/j3c.stripped"
echo "installed $bin/j3c ($(stat -c %s "$bin/j3c") bytes)"
# Composer (tools/composer): its own program, built with the j3c just installed.
"$bin/j3c" build tools/composer --release
install -m 755 tools/composer/target/composer "$bin/j3c-composer"
echo "installed $bin/j3c-composer"
