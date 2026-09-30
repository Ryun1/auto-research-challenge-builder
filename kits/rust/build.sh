#!/usr/bin/env bash
# The only step that compiles submission code. Offline, frozen lockfile.
set -euo pipefail
cargo build --release --locked --offline -p harness --bins
