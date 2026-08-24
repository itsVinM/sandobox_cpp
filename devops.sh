#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REDIS_DIR="$SCRIPT_DIR/redisdb"
SANDBOX_DIR="$SCRIPT_DIR/sandbox"
BUILD_DIR="$SANDBOX_DIR/build"
LOG_DIR="$SCRIPT_DIR/.devops-logs"
PID_DIR="$SCRIPT_DIR/.devops-pids"
REDIS_HOST="${REDIS_HOST:-127.0.0.1}"
REDIS_PORT="${REDIS_PORT:-1234}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

log()  { echo -e "${CYAN}[devops]${NC} $*"; }
ok()   { echo -e "${GREEN}[  ok]${NC} $*"; }
warn() { echo -e "${YELLOW}[warn]${NC} $*"; }
err()  { echo -e "${RED}[err]${NC} $*" >&2; }

# ── Build ──

cmd_build() {
    log "Building Rust server..."
    cd "$REDIS_DIR" && cargo build --release 2>&1 | tail -1
    ok "rust built"

    log "Building C++ sandbox..."
    mkdir -p "$BUILD_DIR"
    cmake -S sandbox -B "$BUILD_DIR" -DCMAKE_BUILD_TYPE=Release 2>&1 | tail -1
    cmake --build "$BUILD_DIR" -j"$(sysctl -n hw.ncpu 2>/dev/null || nproc 2>/dev/null || echo 4)" 2>&1 | tail -1
    ok "sandbox built"
}

# ── Start ──

cmd_start() {
    if pgrep -f "redisops" >/dev/null 2>&1; then
        warn "redis already running"
    else
        log "Starting redis on ${REDIS_HOST}:${REDIS_PORT}..."
        mkdir -p "$LOG_DIR" "$PID_DIR"
        RUST_LOG=info "$REDIS_DIR/target/release/redisops" > "$LOG_DIR/redis.log" 2>&1 &
        echo $! > "$PID_DIR/redis.pid"
        for i in $(seq 1 20); do
            if bash -c "echo ping > /dev/tcp/${REDIS_HOST}/${REDIS_PORT}" 2>/dev/null; then
                ok "redis ready (pid=$(cat "$PID_DIR/redis.pid"))"
                break
            fi
            sleep 0.2
        done
    fi

    if pgrep -f "devops-sandbox" >/dev/null 2>&1; then
        warn "sandbox already running"
    elif [ -f "$BUILD_DIR/sandbox" ]; then
        log "Starting sandbox..."
        "$BUILD_DIR/sandbox" --host "$REDIS_HOST" --port "$REDIS_PORT" --id "sandbox-1" > "$LOG_DIR/sandbox.log" 2>&1 &
        echo $! > "$PID_DIR/sandbox.pid"
        ok "sandbox started (pid=$(cat "$PID_DIR/sandbox.pid"))"
    else
        warn "sandbox binary not found, run: ./devops.sh build"
    fi
}

# ── Stop ──

cmd_stop() {
    for name in sandbox redis; do
        local pidfile="$PID_DIR/$name.pid"
        if [ -f "$pidfile" ]; then
            local pid
            pid=$(cat "$pidfile")
            if kill -0 "$pid" 2>/dev/null; then
                log "Stopping $name (pid=$pid)..."
                kill "$pid" 2>/dev/null || true
                sleep 0.3
                kill -9 "$pid" 2>/dev/null || true
                ok "$name stopped"
            fi
            rm -f "$pidfile"
        fi
    done
}

# ── Status ──

cmd_status() {
    echo ""
    for name in redis sandbox; do
        if [ -f "$PID_DIR/$name.pid" ] && kill -0 "$(cat "$PID_DIR/$name.pid")" 2>/dev/null; then
            echo -e "  $name: ${GREEN}running${NC} (pid=$(cat "$PID_DIR/$name.pid"))"
        else
            echo -e "  $name: ${RED}stopped${NC}"
        fi
    done
    echo ""
}

# ── Job helpers (via python) ──

redis_cmd() {
    python3 -c "
import socket, struct, sys
def send_cmd(host, port, args):
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.connect((host, port))
    s.settimeout(5)
    payload = struct.pack('<I', len(args))
    for arg in args:
        b = arg.encode()
        payload += struct.pack('<I', len(b)) + b
    msg = struct.pack('<I', len(payload)) + payload
    s.sendall(msg)
    raw = s.recv(4)
    if len(raw) < 4: return None
    rlen = struct.unpack('<I', raw)[0]
    data = s.recv(rlen)
    s.close()
    return data
def parse_resp(data):
    if not data: return 'nil'
    tag = data[0]
    if tag == 0: return 'nil'
    if tag == 1:
        ml = struct.unpack('<I', data[5:9])[0]
        return f'ERR: {data[9:9+ml].decode()}'
    if tag == 2:
        sl = struct.unpack('<I', data[1:5])[0]
        return data[5:5+sl].decode('utf-8', errors='replace')
    if tag == 3: return str(struct.unpack('<q', data[1:9])[0])
    if tag == 4: return str(struct.unpack('<d', data[1:9])[0])
    if tag == 5:
        cnt = struct.unpack('<I', data[1:5])[0]
        pos, items = 5, []
        for _ in range(cnt):
            et = data[pos]
            if et == 0: items.append('nil'); pos += 1
            elif et == 2:
                sl = struct.unpack('<I', data[pos+1:pos+5])[0]
                items.append(data[pos+5:pos+5+sl].decode('utf-8', errors='replace'))
                pos += 5 + sl
            elif et == 3: items.append(str(struct.unpack('<q', data[pos+1:pos+9])[0])); pos += 9
            elif et == 4: items.append(str(struct.unpack('<d', data[pos+1:pos+9])[0])); pos += 9
            else: items.append(f'unknown({et})'); pos += 1
        return items
    return f'unknown({tag})'
d = send_cmd('${REDIS_HOST}', ${REDIS_PORT}, sys.argv[1:])
r = parse_resp(d)
print('\\n'.join(r) if isinstance(r, list) else r)
" "$@"
}

cmd_submit() {
    local name="${1:?usage: submit <name> <command> [target]}"
    local cmd="${2:?missing command}"
    local target="${3:-local}"
    local job_id="job-$(date +%s)-$$"
    redis_cmd "job submit" "$job_id" "$name" "$target" "$cmd"
    ok "submitted: $job_id"
}

cmd_next()    { redis_cmd "job next"; }
cmd_list()    { redis_cmd "job list" ${1:+"$1"}; }
cmd_status_job() { redis_cmd "job status" "${1:?missing job_id}"; }

# ── Logs ──

cmd_logs() {
    local target="${1:-all}"
    case "$target" in
        redis)   tail -f "$LOG_DIR/redis.log" 2>/dev/null || warn "no redis logs" ;;
        sandbox) tail -f "$LOG_DIR/sandbox.log" 2>/dev/null || warn "no sandbox logs" ;;
        *)       tail -f "$LOG_DIR"/*.log 2>/dev/null || warn "no logs" ;;
    esac
}

# ── Clean ──

cmd_clean() {
    cmd_stop 2>/dev/null || true
    rm -rf "$LOG_DIR" "$PID_DIR" "$BUILD_DIR"
    cd "$REDIS_DIR" && cargo clean 2>/dev/null || true
    ok "cleaned"
}

# ── Help ──

cmd_help() {
    echo ""
    echo "devops.sh — orchestrate redis + sandbox"
    echo ""
    echo "  build              Build rust server + c++ sandbox"
    echo "  start              Start redis + sandbox"
    echo "  stop               Stop all"
    echo "  status             Show status"
    echo "  submit <n> <cmd>   Submit a job"
    echo "  next               Fetch next job"
    echo "  list [status]      List jobs"
    echo "  job-status <id>    Check job status"
    echo "  logs [target]      Tail logs"
    echo "  clean              Remove build artifacts"
    echo "  help               Show this"
    echo ""
}

case "${1:-help}" in
    build)       cmd_build ;;
    start)       cmd_start ;;
    stop)        cmd_stop ;;
    status)      cmd_status ;;
    submit)      shift; cmd_submit "$@" ;;
    next)        cmd_next ;;
    list|ls)     shift; cmd_list "$@" ;;
    job-status)  shift; cmd_status_job "$@" ;;
    logs)        shift; cmd_logs "$@" ;;
    clean)       cmd_clean ;;
    help|--help|-h) cmd_help ;;
    *)           err "unknown: $1"; cmd_help; exit 1 ;;
esac
