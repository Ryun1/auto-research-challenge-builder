#!/usr/bin/env bash
# Scoring pool, pinned cardano-ledger and the reference build.
# Never compiles candidate/ (build.sh does). Idempotent.
#
# The clone and build outputs live in one shared cache, not in the workspace:
# preflight runs setup once per workspace (baseline + each broken variant), and
# a cold ledger build is over an hour. Warm runs are cache hits.
# ponytail: one cache per machine, runs must not overlap; add flock if they ever do.
#
# Needs GHC 9.6.7, cabal 3.16, valgrind, python3, rsync and the Cardano system
# libs (libsodium, secp256k1, blst): CI gets them from input-output-hk/actions/base.
set -euo pipefail
cd "$(dirname "$0")"

LEDGER_SHA=7827506b7518284d7f9dbbe3502a20431e513fe9
GHC_VERSION=9.6.7
L="${CB_LEDGER_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/cb-ledger-cbor}/$LEDGER_SHA"

[ "$(ghc --numeric-version)" = "$GHC_VERSION" ] || { echo "need ghc $GHC_VERSION"; exit 1; }
for lib in libsodium libsecp256k1 libblst; do
  pkg-config --exists "$lib" || { echo "missing system lib $lib (see input-output-hk/actions/base)"; exit 1; }
done

if [ ! -d "$L/.git" ]; then
  git clone -q --filter=blob:none https://github.com/IntersectMBO/cardano-ledger "$L"
fi
git -C "$L" checkout -q --detach "$LEDGER_SHA"
[ "$(git -C "$L" rev-parse HEAD)" = "$LEDGER_SHA" ]
[ -z "$(git -C "$L" status --porcelain --untracked-files=no)" ] || { echo "ledger cache modified: $L"; exit 1; }

# Scoring pool: CB_POOL=fresh (CI) fetches recent random mainnet blocks, so the
# scored txs are never in the repo. Anything else uses corpus/pool.hex.gz.
rm -f corpus/fresh.hex.gz corpus/fresh.json
if [ "${CB_POOL:-}" = fresh ]; then
  python3 verifier/pool.py corpus/fresh
fi

# Judge-owned sources. rsync -c leaves unchanged files alone, so GHC's
# source-hash recompilation check keeps warm builds warm.
rsync -a -c --delete harness/ "$L/ledger-cbor-harness/"
# Planning stand-in until build.sh syncs candidate/ in. No warm candidate build:
# it would always cost one extra full build (build.sh rebuilds what changed).
[ -d "$L/cand-binary" ] || rsync -a baseline/ "$L/cand-binary/"

# Two projects over the same clone. Both drop upstream's dev-only +asserts flags
# (the node's release build has none); the candidate one swaps in cand-binary.
# Additions go in <project>.local, which cabal merges (as the spike does).
sed '/flags: +asserts/d' "$L/cabal.project" > "$L/cabal.project.ref"
sed 's#^  libs/cardano-ledger-binary$#  cand-binary#' "$L/cabal.project.ref" > "$L/cabal.project.cand"
grep -q '^  cand-binary$' "$L/cabal.project.cand"
for p in ref cand; do
  printf 'packages: ledger-cbor-harness\n\npackage cardano-crypto-praos\n  flags: -external-libsodium-vrf\n' \
    > "$L/cabal.project.$p.local"
done

mkdir -p .bin
( cd "$L"
  cabal update -v0
  cabal build -v1 --project-file=cabal.project.ref --builddir=dist-ref ledger-cbor-harness
)
cp "$(cd "$L" && cabal list-bin --project-file=cabal.project.ref --builddir=dist-ref ledger-cbor-harness)" .bin/reference
