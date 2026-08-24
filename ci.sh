#!/bin/bash
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
NC='\033[0m'

PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$PROJECT_DIR"

PASS=0
FAIL=0

step() { echo -e "\n${CYAN}[$1]${NC} $2"; }
ok()   { echo -e "  ${GREEN}ok${NC} $1"; PASS=$((PASS + 1)); }
fail() { echo -e "  ${RED}fail${NC} $1"; FAIL=$((FAIL + 1)); }

# 1. Format
step "1/5" "rustfmt"
if cargo fmt --check --manifest-path redisdb/Cargo.toml 2>/dev/null; then
    ok "clean"
else
    fail "run 'cargo fmt'"
fi

# 2. Clippy
step "2/5" "clippy"
if cargo clippy --manifest-path redisdb/Cargo.toml 2>&1 | grep -q "error\["; then
    fail "clippy errors"
else
    ok "clean"
fi

# 3. Rust tests
step "3/5" "cargo test"
if cargo test --manifest-path redisdb/Cargo.toml 2>&1 | grep -q "test result: ok"; then
    ok "all tests passed"
else
    fail "tests failed"
fi

# 4. Release build
step "4/5" "release build"
if cargo build --release --manifest-path redisdb/Cargo.toml 2>/dev/null; then
    size=$(du -h target/release/redisops 2>/dev/null | cut -f1)
    ok "$size"
else
    fail "build failed"
fi

# 5. C++ sandbox
step "5/5" "c++ sandbox"
if cmake -S sandbox -B sandbox/build -DCMAKE_BUILD_TYPE=Release >/dev/null 2>&1 \
   && cmake --build sandbox/build >/dev/null 2>&1; then
    ok "compiled"
    if ctest --test-dir sandbox/build --output-on-failure >/dev/null 2>&1; then
        ok "tests passed"
    else
        fail "sandbox tests failed"
    fi
else
    fail "sandbox build failed"
fi

echo ""
if [[ $FAIL -eq 0 ]]; then
    echo -e "${GREEN}CI PASSED${NC}: $PASS checks"
    exit 0
else
    echo -e "${RED}CI FAILED${NC}: $FAIL failed, $PASS passed"
    exit 1
fi
