#!/usr/bin/env bash
set -eu
exec 3<>/dev/tcp/1.1.1.1/443
test -x select
