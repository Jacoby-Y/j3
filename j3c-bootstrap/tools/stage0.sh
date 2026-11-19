#!/bin/sh
# Rebuild stage0/j3c, the pinned Rust j3c that builds j3c-bootstrap and serves as the oracle
# (plan 11 A2), and seeds ./build.sh on a clone with no stage0. This script is the only way stage0 changes.
#   stage0.sh rust-final  release-build the Rust j3c at the tag rust-final in a scratch git worktree
# Since plan 11 C6 the Rust source (j3c/src, j3c/Cargo.toml) exists only at that tag, so COMMIT is always
# rust-final (an older commit still works). Needs cargo, only here.
# Writes stage0/j3c (gitignored) and stage0/VERSION (tracked). Never touches ~/.bin/j3c:
# J3C_NO_INSTALL=1 turns off the cargo wrapper (j3c/.cargo/copy-j3c.sh).
# Cargo output goes to $J3_STAGE0_TARGET, default target/stage0-cargo (kept, so rebuilds are incremental).
set -eu
cd "$(dirname "$0")/.."
boot=$(pwd)
repo=$(git rev-parse --show-toplevel)
export J3C_NO_INSTALL=1
cargo_dir=${J3_STAGE0_TARGET:-$boot/target/stage0-cargo}
tree=
cleanup() { [ -z "$tree" ] || git -C "$repo" worktree remove --force "$tree" >/dev/null 2>&1 || true; }
trap cleanup EXIT INT TERM

case "${1:-}" in
  "" | -*)
    echo "usage: stage0.sh rust-final" >&2
    exit 2
    ;;
  *)
    commit=$(git -C "$repo" rev-parse --verify "$1^{commit}")
    tree=$(mktemp -d "${TMPDIR:-/tmp}/j3c-stage0.XXXXXX")
    rmdir "$tree"
    git -C "$repo" worktree add --detach "$tree" "$commit" >/dev/null
    src=$tree
    dirty=no
    ;;
esac

cargo build --release --manifest-path "$src/j3c/Cargo.toml" --target-dir "$cargo_dir"
install -m 755 "$cargo_dir/release/j3c" stage0/j3c.new
mv -f stage0/j3c.new stage0/j3c
{
  echo "commit $commit"
  echo "dirty $dirty"
  # The `binary` line is this machine's path, not the build: left out.
  stage0/j3c version --verbose | grep -v '^binary '
} >stage0/VERSION
cat stage0/VERSION
