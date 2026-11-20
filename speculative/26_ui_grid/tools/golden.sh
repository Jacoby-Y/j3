#!/bin/sh
J3_JOBS=${J3_JOBS:-$(( ($(nproc) + 1) / 2 ))} exec j3c golden "$(dirname "$0")/.." "$@"
