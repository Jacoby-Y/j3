#!/bin/sh
# Release-build j3c and install it onto PATH.
set -eu
cd "$(dirname "$0")"
cargo build --release --manifest-path j3c/Cargo.toml
install -m 755 j3c/target/release/j3c /home/jacoby/.bin/j3c
echo "installed /home/jacoby/.bin/j3c"
