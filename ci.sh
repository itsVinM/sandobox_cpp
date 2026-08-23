#!/bin/bash
# RedisOps CI - local continuous integration script
# Runs all tests, checks compilation, validates embedded targets
# Run: bash ci.sh

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$PROJECT_DIR"

PASS=0
FAIL=0

step() {
    echo -e "\n${CYAN}[$1]${NC} $2"
}

ok() {
    echo -e "  ${GREEN}✓${NC} $1"
    PASS=$((PASS + 1))
}

fail() {
    echo -e "  ${RED}✗${NC} $1"
    FAIL=$((FAIL + 1))
}

# ── 1. Format check ──
step "1/7" "Code formatting"
if cargo fmt --check --manifest-path redisdb/Cargo.toml 2>/dev/null; then
    ok "Format check passed"
else
    fail "Run 'cargo fmt' to fix"
fi

# ── 2. Clippy lints ──
step "2/7" "Clippy lints"
if cargo clippy --manifest-path redisdb/Cargo.toml 2>&1 | grep -q "error\["; then
    fail "Clippy errors found (run 'cargo clippy' to see)"
else
    ok "No clippy errors"
fi

# ── 3. Debug build + tests ──
step "3/7" "Debug build + unit tests"
TEST_OUTPUT=$(cargo test --manifest-path redisdb/Cargo.toml 2>&1)
if echo "$TEST_OUTPUT" | grep -q "test result: ok"; then
    total=$(echo "$TEST_OUTPUT" | grep "test result: ok" | awk '{s+=$4} END {print s}')
    ok "All tests passed ($total tests)"
else
    fail "Tests failed"
fi

# ── 4. Release build ──
step "4/7" "Release build"
if cargo build --release --manifest-path redisdb/Cargo.toml 2>/dev/null; then
    size=$(du -h target/release/redisops 2>/dev/null | cut -f1)
    ok "Release binary built ($size)"
else
    fail "Release build failed"
fi

# ── 5. Check lib compiles (all modules) ──
step "5/7" "Library compilation check"
if cargo check --lib --manifest-path redisdb/Cargo.toml 2>/dev/null; then
    ok "All library modules compile"
else
    fail "Library compilation failed"
fi

# ── 6. C++ sandbox build ──
step "6/7" "C++ sandbox build"
if cmake -S sandbox -B sandbox/build -DCMAKE_BUILD_TYPE=Release >/dev/null 2>&1 \
   && cmake --build sandbox/build >/dev/null 2>&1; then
    ok "sandbox + sandbox_tests compiled"
else
    fail "C++ sandbox build failed"
fi

# ── 7. C++ sandbox unit tests ──
step "7/7" "C++ sandbox unit tests"
TEST_OUTPUT=$(ctest --test-dir sandbox/build --output-on-failure 2>&1)
if echo "$TEST_OUTPUT" | grep -q "tests passed"; then
    ok "$(echo "$TEST_OUTPUT" | grep -oE '[0-9]+% tests passed' | head -1)"
elif ctest --test-dir sandbox/build >/dev/null 2>&1; then
    ok "sandbox unit tests passed"
else
    fail "sandbox unit tests failed (run: ctest --test-dir sandbox/build --output-on-failure)"
fi

# ── Summary ──
echo ""
echo "════════════════════════════════════════════════"
if [[ $FAIL -eq 0 ]]; then
    echo -e "  ${GREEN}CI PASSED${NC}: $PASS checks OK"
    exit 0
else
    echo -e "  ${RED}CI FAILED${NC}: $FAIL failures, $PASS passed"
    exit 1
fi
