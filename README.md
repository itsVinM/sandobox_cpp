# RedisOps

A minimal Redis-compatible server with a C++20 sandbox worker.

## Quick Start

```bash
# Build
cargo build --release          # Rust server
cmake -S sandbox -B sandbox/build && cmake --build sandbox/build  # C++ sandbox

# Run
./redisdb/target/release/redisops          # server on :1234
./sandbox/build/sandbox --host 127.0.0.1 --port 1234  # worker
```

## Commands

| Command | Description |
|---------|-------------|
| `SET key val` | Set key |
| `GET key` | Get key |
| `DEL key` | Delete key |
| `KEYS` | List all keys |
| `PEXPIRE key ms` | Set TTL in ms |
| `PTTL key` | Get remaining TTL |
| `LPUSH key val` | Push to list head |
| `RPUSH key val` | Push to list tail |
| `LPOP key` | Pop from list head |
| `RPOP key` | Pop from list tail |
| `LRANGE key start stop` | Range over list |
| `LINDEX key idx` | Index into list |
| `LLEN key` | List length |
| `LREM key count val` | Remove list elements |
| `ZADD key score member` | Add to sorted set |
| `ZREM key member` | Remove from sorted set |
| `ZSCORE key member` | Get score |
| `ZQUERY key score name off lim` | Range query sorted set |
| `JOB SUBMIT id name target cmd` | Submit job |
| `JOB NEXT` | Pop next job |
| `JOB RESULT id exit ms` | Record result |
| `JOB STATUS id` | Check status |
| `JOB LOG id line` | Append log |
| `JOB LIST [status]` | List jobs |

## Testing

```bash
cargo test                    # Rust tests
bash ci.sh                    # Full CI (format + clippy + tests + build)
```

## Project Structure

```
redisops/
├── redisdb/          Rust server (tokio, custom binary protocol)
│   └── src/
│       ├── main.rs
│       ├── server.rs      TCP listener
│       ├── handler.rs     Command dispatch
│       ├── store/         Sharded in-memory store with TTL
│       ├── proto/         Binary wire protocol
│       └── zset/          Sorted set (BTreeMap + HashMap)
├── sandbox/          C++20 sandbox worker
│   ├── src/
│   │   ├── main.cpp              Entry point + job loop
│   │   ├── redis_client.cpp      RESP client
│   │   ├── sandbox.cpp           Process isolation (cgroup/seccomp)
│   │   ├── metrics.cpp           Prometheus exporter
│   │   ├── resource_monitor.cpp  /proc-based monitoring
│   │   ├── process_supervisor.cpp Process lifecycle
│   │   ├── health_monitor.cpp    Health checks
│   │   └── log_aggregator.cpp    Log collection
│   └── include/devops/
│       ├── result.hpp            Result<T> type
│       └── *.hpp                 Component headers
├── ci.sh             CI script
└── devops.sh         Orchestration
```
