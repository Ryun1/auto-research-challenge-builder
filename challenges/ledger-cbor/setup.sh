#!/usr/bin/env bash
# Pinned cardano-ledger, reference build, and the candidate's external deps.
# Never compiles candidate/ (build.sh does). Idempotent.
#
# Needs GHC 9.6.7, cabal 3.16, valgrind, python3 and the Cardano system libs
# (libsodium, secp256k1, blst): CI gets them from input-output-hk/actions/base.
set -euo pipefail
cd "$(dirname "$0")"

LEDGER_SHA=7827506b7518284d7f9dbbe3502a20431e513fe9
GHC_VERSION=9.6.7

[ "$(ghc --numeric-version)" = "$GHC_VERSION" ] || { echo "need ghc $GHC_VERSION"; exit 1; }
for lib in libsodium libsecp256k1 libblst; do
  pkg-config --exists "$lib" || { echo "missing system lib $lib (see input-output-hk/actions/base)"; exit 1; }
done

if [ ! -d .ledger/.git ]; then
  git clone -q --filter=blob:none https://github.com/IntersectMBO/cardano-ledger .ledger
fi
git -C .ledger checkout -q --detach "$LEDGER_SHA"
[ "$(git -C .ledger rev-parse HEAD)" = "$LEDGER_SHA" ]

rm -rf .ledger/ledger-cbor-harness .ledger/cand-binary
cp -R harness .ledger/ledger-cbor-harness
cp -R baseline .ledger/cand-binary  # planning stand-in; build.sh swaps in candidate/

# Two projects over the same clone. Both drop upstream's dev-only +asserts flags
# (the node's release build has none); the candidate one swaps in cand-binary.
# Additions go in <project>.local, which cabal merges (as the spike does).
sed '/flags: +asserts/d' .ledger/cabal.project > .ledger/cabal.project.ref
sed 's#^  libs/cardano-ledger-binary$#  cand-binary#' .ledger/cabal.project.ref > .ledger/cabal.project.cand
grep -q '^  cand-binary$' .ledger/cabal.project.cand
for p in ref cand; do
  printf 'packages: ledger-cbor-harness\n\npackage cardano-crypto-praos\n  flags: -external-libsodium-vrf\n' \
    > ".ledger/cabal.project.$p.local"
done

cd .ledger
cabal update -v0
cabal build -v1 --project-file=cabal.project.ref --builddir=dist-ref ledger-cbor-harness
mkdir -p ../.bin
cp "$(cabal list-bin --project-file=cabal.project.ref --builddir=dist-ref ledger-cbor-harness)" ../.bin/reference
cabal build -v1 --project-file=cabal.project.cand --builddir=dist-cand --only-dependencies ledger-cbor-harness
