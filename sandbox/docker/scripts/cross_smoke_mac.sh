#!/usr/bin/env bash
# macOS cross-compile smoke test using Zig's bundled macOS SDK + modern libc++
# (no Apple hardware, no Xcode license needed, works from Linux).
# Produces a x86_64 Mach-O binary for the non-Linux stub path of the sandbox.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
out=/tmp/mac-objs

echo "== zig version =="
zig version

echo "== darwin compile (zig c++, x86_64-macos) =="
rm -rf "$out" && mkdir -p "$out"
cd "$out"
# `zig c++ -c` handles one translation unit per invocation.
zig c++ -target x86_64-macos -O2 -std=c++20 -I "$root/include" \
    -c "$root/docker/scripts/cross_stub_main.cpp"
zig c++ -target x86_64-macos -O2 -std=c++20 -I "$root/include" \
    -c "$root/src/sandbox.cpp"

echo "== darwin link =="
zig c++ -target x86_64-macos -O2 -std=c++20 -I "$root/include" \
    cross_stub_main.o sandbox.o -o sandbox-mac

file sandbox-mac
echo "MACOS CROSS-COMPILE OK"