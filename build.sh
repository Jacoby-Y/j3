#!/bin/sh

# A special build script for me, the developer

# Build j3c (the J3 compiler in j3c-bootstrap/) and install it onto PATH, then the composer tool.
# A seed j3c builds stage 1, stage 1 builds stage 2 from a scratch copy, both must print the same `version`,
# and stage 2 is installed stripped; the binary it replaces is kept beside it as j3c.prev.
# Seed (j3c-bootstrap/tools/seed.sh list): $J3_SEED when set, else the installed j3c, its j3c.prev, the ones
# in the install dir, then (downloaded only when those all failed) the release pinned in
# j3c-bootstrap/seed/VERSION; a seed that fails to build stage 1 hands over to the next. With none, J3_SEED=$(j3c-bootstrap/tools/stage0.sh rust-final) ./build.sh: stage0.sh
# builds the old Rust compiler with cargo into ~/.cache/j3/stage0/ and prints its path.
# Install dir: $J3_BIN_DIR, else ~/.bin if it exists, else ~/.local/bin. J3C_NO_INSTALL=1 stops after stage 2.
# Half the cores at most (the repo scripts' rule): J3_JOBS (clang processes per build) defaults to nproc/2.
# A seed older than J3_JOBS ignores it, so the stage 1 build is also pinned to that many CPUs with taskset
# when it is installed (without it, that one build may use every core).
set -eu
budget=$(( $(nproc) / 2 )); [ "$budget" -ge 1 ] || budget=1
export J3_JOBS=${J3_JOBS:-$budget}
pin=
if [ "$J3_JOBS" -lt "$(nproc)" ] 2>/dev/null && taskset -c "0-$((J3_JOBS - 1))" true 2>/dev/null; then pin="taskset -c 0-$((J3_JOBS - 1))"; fi
cd "$(dirname "$0")"
root=$(pwd)
command -v clang >/dev/null 2>&1 || { echo "build.sh: clang not found (j3c compiles through it; e.g. apt install clang)"; exit 1; }
if [ -n "${J3_BIN_DIR:-}" ]; then bin=$J3_BIN_DIR
elif [ -d "$HOME/.bin" ]; then bin=$HOME/.bin
else bin=$HOME/.local/bin
fi
seeds=$(j3c-bootstrap/tools/seed.sh list "$bin")

# The version is git's: `git describe` minus the `v` (`0.1.4-7-gabc1234-dirty`), then the tag (`0.1.4`), into
# a gitignored file both stages embed (src/project/config.j3). Removed after stage 2, so a later in-place build
# (the tests/expect/ files hold the plain `0.1.0`) falls back to J3C_VERSION.
vf=j3c-bootstrap/text/version-build.txt
trap 'rm -f "$root/$vf"' EXIT
if d=$(git describe --tags --match 'v[0-9]*' --dirty 2>/dev/null) && t=$(git describe --tags --match 'v[0-9]*' --abbrev=0 2>/dev/null); then
    printf '%s\n%s\n' "${d#v}" "${t#v}" > "$vf"
fi
# Every j3c embeds its own stdlib, and the compiler may use modules newer than the seed's (j3.data.toml), so
# the seed reads the tree on disk. Stage 1 embeds this tree's stdlib, so stage 2 needs no override.
seed=
nl='
'
ifs=$IFS
IFS=$nl
# `pinned` (after the listed seeds, unless J3_SEED is set): seed.sh downloads the pinned release only then.
last=pinned; [ -z "${J3_SEED:-}" ] || last=
for c in $seeds $last; do
    IFS=$ifs
    if [ "$c" = pinned ]; then c=$(j3c-bootstrap/tools/seed.sh) || break; fi
    echo "build.sh: stage 1 (built by seed $c, $("$c" version))"
    if J3_STDLIB=$root/j3c/stdlib timeout 1800 $pin "$c" build --opt 2 j3c-bootstrap; then seed=$c; break; fi
    echo "build.sh: seed $c failed to build stage 1; trying the next seed"
done
IFS=$ifs
if [ -z "$seed" ]; then
    echo "build.sh: no seed j3c could build stage 1 (see the lines above)."
    echo "  Install a J3 j3c on PATH, set J3_SEED=/path/to/j3c, or pin a release in j3c-bootstrap/seed/VERSION."
    echo "  Last resort (needs cargo): J3_SEED=\$(j3c-bootstrap/tools/stage0.sh rust-final) ./build.sh"
    exit 1
fi
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
echo "build.sh: stage 1 and stage 2 agree (seed: $seed)"
if [ "${J3C_NO_INSTALL:-}" = 1 ]; then echo "build.sh: J3C_NO_INSTALL=1: built $stage2, installed nothing"; exit 0; fi
mkdir -p "$bin"
# Keep the binary being replaced: if the new one is broken, the next build still finds a seed (j3c.prev).
kept=
if [ -x "$bin/j3c" ] && "$bin/j3c" version >/dev/null 2>&1; then cp -p "$bin/j3c" "$bin/j3c.prev"; kept="; previous kept as j3c.prev"; fi
strip -o "$s2/j3c.stripped" "$stage2"
install -m 755 "$s2/j3c.stripped" "$bin/j3c"
rm -f "$s2/j3c.stripped"
echo "installed $bin/j3c ($(stat -c %s "$bin/j3c") bytes$kept)"
# Composer (tools/composer): its own program, built with the j3c just installed.
"$bin/j3c" build tools/composer --release
install -m 755 tools/composer/target/composer "$bin/j3c-composer"
echo "installed $bin/j3c-composer"
