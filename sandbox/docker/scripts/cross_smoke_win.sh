#!/usr/bin/env bash
# Windows cross-compile smoke test: build the stub path of sandbox.cpp into a
# working x86_64-windows PE executable using the mingw-w64 GCC 13 toolchain.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
out=/tmp/sandbox-win.exe

echo "== mingw cross build (x86_64-w64-mingw32-g++, gcc-13 / libstdc++-13) =="
x86_64-w64-mingw32-g++-posix -std=c++20 -O2 -I "$root/include" \
    "$root/docker/scripts/cross_stub_main.cpp" "$root/src/sandbox.cpp" \
    -static -o "$out"

file "$out"
echo "WINDOWS CROSS-COMPILE OK"