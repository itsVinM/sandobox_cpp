# Linux Process Sandbox (C++20)

A lightweight, secure task execution daemon and background job worker for Linux. The daemon isolates untrusted workloads using native Linux kernel containment features and executes jobs polled from a Redis queue.

## Features

* **Kernel Isolation:** Uses Linux `namespaces` (`CLONE_NEWPID`, `CLONE_NEWNET`) via `clone()` to isolate target processes.
* **Resource Controls (`cgroups v2`):** Enforces strict memory limits (`memory.max`), CPU quotas (`cpu.max`), and process limits (`pids.max`).
* **Syscall Filtering:** Installs BPF-based `seccomp` filters via `libseccomp` (`SCMP_ACT_KILL`) to block unauthorized system calls.
* **Job Queue & Telemetry:** Listens on Redis for incoming tasks, pipes `stdout`/`stderr` streams back to the host, and exports Prometheus performance metrics.

## Prerequisites

* Linux kernel 5.x+ (with `cgroups v2` enabled)
* C++20 compliant compiler (`gcc` 10+ or `clang` 11+)
* `CMake` (v3.16+)
* `libseccomp-dev`
* `hiredis` (Redis C client)

## Build & Run

```bash
# Clone and build
git clone https://github.com/itsVinM/sandobox_cpp.git
cd sandobox_cpp
mkdir build && cd build
cmake ..
make -j$(nproc)

# Run daemon (requires root privileges for cgroups/namespaces)
sudo ./sandbox_daemon

```