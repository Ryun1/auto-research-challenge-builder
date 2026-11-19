#!/usr/bin/env bash
set -euo pipefail
# The judge and the baseline need only python3 from the runner image.
command -v python3 >/dev/null || { echo "python3 is required" >&2; exit 1; }
python3 --version
