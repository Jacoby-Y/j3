#!/bin/sh
# Release-build j3c and install it onto PATH.
set -eu
cd "$(dirname "$0")"
cargo build --release --manifest-path j3c/Cargo.toml
install -m 755 j3c/target/release/j3c /home/jacoby/.bin/j3c
echo "installed /home/jacoby/.bin/j3c"
# Composer (tools/composer): its own program, built with the j3c just installed.
/home/jacoby/.bin/j3c build tools/composer --release
install -m 755 tools/composer/target/composer /home/jacoby/.bin/j3c-composer
echo "installed /home/jacoby/.bin/j3c-composer"
