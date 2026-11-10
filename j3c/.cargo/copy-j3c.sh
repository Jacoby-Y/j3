#!/bin/sh
# Opt-in: copy the j3c binary to $J3_BIN_DIR (default ~/.bin) after rustc links it.
# Cargo has no post-build hook, so this is a rustc wrapper. Not enabled by the repo;
# turn it on from an untracked cargo config, e.g. ~/.cargo/config.toml:
#   [build]
#   rustc-wrapper = "/path/to/j3/j3c/.cargo/copy-j3c.sh"
"$@" || exit $?

is_j3c=
is_bin=
is_test=
out=
out_dir=
extra=
prev=
for a in "$@"; do
  case "$prev" in
    -o) out=$a ;;
    --out-dir) out_dir=$a ;;
    --crate-name) [ "$a" = j3c ] && is_j3c=1 ;;
    --crate-type) [ "$a" = bin ] && is_bin=1 ;;
    -C)
      case "$a" in
        extra-filename=*) extra=${a#extra-filename=} ;;
      esac
      ;;
  esac
  case "$a" in
    --test) is_test=1 ;;
    --crate-name=j3c) is_j3c=1 ;;
    --crate-type=bin) is_bin=1 ;;
    --out-dir=*) out_dir=${a#--out-dir=} ;;
  esac
  prev=$a
done

# J3C_NO_INSTALL=1 builds without installing.
[ -z "$J3C_NO_INSTALL" ] || exit 0
[ -n "$is_j3c" ] && [ -n "$is_bin" ] && [ -z "$is_test" ] || exit 0
[ -n "$out" ] || out="$out_dir/j3c$extra"
[ -n "$out" ] && [ -f "$out" ] || exit 0
case "$out" in
  *.rlib|*.rmeta|*.o|*.d|*.so) exit 0 ;;
esac

dest=${J3_BIN_DIR:-$HOME/.bin}/j3c
install -D -m 755 "$out" "$dest.new"
mv -f "$dest.new" "$dest"
