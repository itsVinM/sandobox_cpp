# Build + test image for the devops-sandbox.
# Used by the `linux` (functional) and `windows` (cross-compile) compose services.
FROM ubuntu:24.04

RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        cmake \
        curl \
        file \
        ninja-build \
        libseccomp-dev \
        g++-mingw-w64-x86-64 \
        xz-utils \
    && rm -rf /var/lib/apt/lists/*

# Zig bundles a macOS SDK + modern libc++ (C++20 <format>): used to cross-compile
# the darwin smoke target without an Apple license or an OSX-cross image.
ARG ZIG_VERSION=0.14.1
RUN set -eux; \
    case "$(dpkg --print-architecture)" in \
        amd64) ZIG_TARBALL=zig-x86_64-linux-${ZIG_VERSION}.tar.xz ;; \
        arm64) ZIG_TARBALL=zig-aarch64-linux-${ZIG_VERSION}.tar.xz ;; \
        *) echo "unsupported arch: $(dpkg --print-architecture)"; exit 1 ;; \
    esac; \
    curl -fsSL "https://ziglang.org/download/${ZIG_VERSION}/${ZIG_TARBALL}" -o /tmp/zig.tar.xz; \
    mkdir -p /opt/zig; \
    tar -xJf /tmp/zig.tar.xz -C /opt/zig --strip-components=1; \
    rm /tmp/zig.tar.xz; \
    ln -s /opt/zig/zig /usr/local/bin/zig