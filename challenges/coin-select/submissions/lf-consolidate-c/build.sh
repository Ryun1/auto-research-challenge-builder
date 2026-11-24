#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
# Static: no libc.so / ld.so pages to load at start (0.8 MiB cold cgroup peak vs
# 2.3 MiB dynamic in an arm64 ubuntu:24.04 probe). Dynamic if libc.a is missing.
gcc -O2 -s -static -o select select.c 2>/dev/null || gcc -O2 -s -o select select.c
