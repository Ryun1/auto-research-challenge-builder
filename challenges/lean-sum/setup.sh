#!/usr/bin/env bash
# Lean toolchain from lean-toolchain (core only, no Mathlib). Writes .lean-bin
# so verify finds the real binary even in the sandbox (HOME=/tmp there).
set -euo pipefail
export PATH="$HOME/.elan/bin:$PATH"
if ! command -v elan >/dev/null; then
  curl -sSfL https://raw.githubusercontent.com/leanprover/elan/464c9d28395000a2a0128e07081e4956d50eced2/elan-init.sh \
    | sh -s -- -y --default-toolchain none >/dev/null
fi
tc=$(cat lean-toolchain)
elan toolchain install "$tc" >/dev/null 2>&1 || true
bin=$(dirname "$(elan which lean)")  # resolved via ./lean-toolchain
echo "$bin" > .lean-bin
# On an ephemeral CI runner, let the sandbox user (nobody) run the toolchain.
if [ "${CI:-}" = true ]; then
  chmod o+x "$HOME" "$HOME/.elan" "$HOME/.elan/toolchains"
  chmod -R o+rX "$(dirname "$bin")"
fi
python3 --version >/dev/null
