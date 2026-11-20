#!/bin/sh
# Stage 0: the old Rust j3c, built from a git tag, as the seed of last resort when no J3 j3c is installed and
# no release is pinned in seed/VERSION (plan 15). CI uses it that way until a J3 release is pinned.
#   stage0.sh rust-final   release-build the Rust j3c at the tag rust-final (j3c/src, j3c/Cargo.toml exist only
#                          there since plan 11 C6) in a scratch git worktree, and print the binary's path
# Then: J3_SEED=<that path> ./build.sh (build.sh gives the seed J3_STDLIB=<repo>/j3c/stdlib itself).
# Writes ${XDG_CACHE_HOME:-~/.cache}/j3/stage0/<tag>/j3c and VERSION beside it, outside the tree; a cached
# binary built from the same commit is reused (delete the directory to rebuild). Needs cargo, only here.
# Cargo output goes to $J3_STAGE0_TARGET, default the cache's cargo/ (kept, so rebuilds are incremental).
# Never touches ~/.bin/j3c: J3C_NO_INSTALL=1 turns off the cargo wrapper (j3c/.cargo/copy-j3c.sh).
# Progress goes to stderr; stdout is only the path.
set -eu
budget=$(( $(nproc) / 2 )); [ "$budget" -ge 1 ] || budget=1
export J3_JOBS=${J3_JOBS:-$budget}   # half the cores at most: clang processes per build (repo script rule)
repo=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
export J3C_NO_INSTALL=1

case "${1:-}" in
  "" | -*) echo "usage: stage0.sh rust-final" >&2; exit 2 ;;
esac
tag=$1
commit=$(git -C "$repo" rev-parse --verify "$tag^{commit}")
root=${XDG_CACHE_HOME:-$HOME/.cache}/j3/stage0
out=$root/$tag
if [ -x "$out/j3c" ] && grep -qx "commit $commit" "$out/VERSION" 2>/dev/null; then
  echo "stage0.sh: cached $out/j3c ($commit)" >&2
  echo "$out/j3c"
  exit 0
fi
cargo_dir=${J3_STAGE0_TARGET:-$root/cargo}
tree=$(mktemp -d "${TMPDIR:-/tmp}/j3c-stage0.XXXXXX")
rmdir "$tree"
cleanup() { git -C "$repo" worktree remove --force "$tree" >/dev/null 2>&1 || true; }
trap cleanup EXIT INT TERM
git -C "$repo" worktree add --detach "$tree" "$commit" >/dev/null

cargo build -j "$J3_JOBS" --release --manifest-path "$tree/j3c/Cargo.toml" --target-dir "$cargo_dir" >&2
mkdir -p "$out"
install -m 755 "$cargo_dir/release/j3c" "$out/j3c.new"
mv -f "$out/j3c.new" "$out/j3c"
{
  echo "commit $commit"
  "$out/j3c" version --verbose | grep -v '^binary '
} >"$out/VERSION"
cat "$out/VERSION" >&2
echo "$out/j3c"
