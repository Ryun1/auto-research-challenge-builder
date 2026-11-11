#!/usr/bin/env bash
# apt-get install <packages> with apt-get update and a bounded retry.
#
# Hosted runners resolve the Ubuntu mirrors through Azure's resolver, which
# fails ("Temporary failure resolving 'azure.archive.ubuntu.com'") whenever
# Canonical's nameservers stall. On 2026-11-11 ns1-3.canonical.com timed out
# for hours and CI, Warm and Preflight all died at this step, while 1.1.1.1 and
# 9.9.9.9 kept answering from cache. apt takes ~5 min to give up on a dead
# resolver, so the mirrors are probed first: if they don't resolve (or an
# attempt fails), the default link's DNS is pointed at those two resolvers. The
# override is left in place for the rest of the job (later steps, e.g. the
# IOHK base action, run apt too). apt verifies package signatures, so the
# resolver is not trusted for integrity; verify/score run with no network.
set -u

[ "$#" -gt 0 ] || { echo "usage: apt-install.sh <package>..." >&2; exit 2; }

iface=""
resolves() {
  for h in azure.archive.ubuntu.com security.ubuntu.com; do
    timeout 10 getent hosts "$h" >/dev/null || return 1
  done
}
fallback() {
  [ -z "$iface" ] || return 0
  command -v resolvectl >/dev/null || return 0
  iface=$(ip -o route show default | awk '{print $5; exit}')
  [ -n "$iface" ] || return 0
  echo "apt-install: mirrors don't resolve; using 1.1.1.1 / 9.9.9.9 on $iface" >&2
  if sudo resolvectl dns "$iface" 1.1.1.1 9.9.9.9; then sudo resolvectl flush-caches || true; else iface=""; fi
}

for i in 1 2 3 4; do
  resolves || fallback
  sudo apt-get update -qq && sudo apt-get install -y -qq "$@" && exit 0
  [ "$i" = 4 ] && break
  fallback
  sleep $((i * 10))
done
echo "apt-install: giving up after 4 attempts: $*" >&2
exit 1
