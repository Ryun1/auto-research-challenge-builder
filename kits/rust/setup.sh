#!/usr/bin/env bash
# Toolchain from rust-toolchain.toml; dependencies from the frozen lockfile.
set -euo pipefail
rustup toolchain install >/dev/null
cargo fetch --locked
