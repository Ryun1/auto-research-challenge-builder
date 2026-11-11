#!/usr/bin/env bash
# The only step that compiles submission code: token policy, then build the
# harness against candidate/<package>/src in place of each editable package's
# src/ (packages.txt), incrementally, in setup.sh's shared ledger cache.
set -euo pipefail
cd "$(dirname "$0")"

LEDGER_SHA=7827506b7518284d7f9dbbe3502a20431e513fe9
L="${CB_LEDGER_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/cb-ledger-cbor}/$LEDGER_SHA"

python3 - <<'EOF'
# Quick local check; `cb gate` runs the authoritative allowlist
# (crates/cb/src/haskellpolicy.rs, [haskell] in challenge.toml) and this
# denylist alone is bypassable (#51). Relative to the baseline:
#  - candidate/ is only <package>/src/ for the packages in packages.txt, same file set
#  - each banned token (any case), and each distinct identifier containing
#    "unsafe" (any case), appears no more often than in the baseline. Counted
#    per identifier, so dropping an unsafeDeserialize can't pay for a new
#    unsafePerformIO.
import collections, pathlib, re, sys
base, cand = pathlib.Path("baseline"), pathlib.Path("candidate")
pkgs = {l.split()[0] for l in open("packages.txt") if l.strip() and not l.startswith("#")}
BANNED = ["foreign", "runRW#", "realWorld#", "GHC.Magic", "GHC.Exts", "GHC.Prim",
          "accursedUnutterablePerformIO", "inlinePerformIO", "TemplateHaskell",
          "QuasiQuotes", "OPTIONS", "{-#ANN", "{-# ANN", "-pgm", "#include", "GHC.IO",
          "System.IO", "Data.IORef", "System.Environment", "System.Process", "MagicHash"]
def files(root): return {p.relative_to(root): p for p in root.rglob("*") if p.is_file() or p.is_symlink()}
fb, fc = files(base), files(cand)
bad = [f"symlink {r}" for r, p in fc.items() if p.is_symlink()]
bad += [f"not <package>/src/...: {r}" for r in fc if len(r.parts) < 3 or r.parts[0] not in pkgs or r.parts[1] != "src"]
bad += [f"file set is frozen: {r}" for r in sorted(set(fb) ^ set(fc))]
def tokens(fs):
    c = collections.Counter()
    for r, p in fs.items():
        if p.is_symlink(): continue
        t = p.read_text(errors="replace")
        c.update({tok: t.upper().count(tok.upper()) for tok in BANNED})
        c.update(re.findall(r"[\w'#.]*unsafe[\w'#]*", t, re.I))
    return c
tb, tc = tokens(fb), tokens(fc)
bad += [f"banned token {k!r}: {n} > baseline {tb[k]}" for k, n in tc.items() if n > tb[k]]
if bad:
    print("policy: " + "; ".join(bad[:10])); sys.exit(1)
EOF

[ -x .bin/reference ] && [ -d "$L/dist-ref" ] || { echo "run setup.sh first (no ledger cache at $L)"; exit 1; }
grep -v '^#' packages.txt | while read -r name path; do
  [ -n "$name" ] || continue
  [ -d "$L/cand/$name" ] || { echo "run setup.sh first (no $L/cand/$name)"; exit 1; }
  # -c: unchanged files keep their mtime, so GHC's recompilation check stays warm.
  rsync -a -c --delete "candidate/$name/src/" "$L/cand/$name/src/"
done
( cd "$L" && cabal build -v1 --project-file=cabal.project.cand --builddir=dist-cand ledger-cbor-harness )
cp "$(cd "$L" && cabal list-bin --project-file=cabal.project.cand --builddir=dist-cand ledger-cbor-harness)" .bin/candidate
# cp onto an existing file keeps its mode, and the build sandbox pre-creates
# the output 0644 (it must exist to be mounted). Verify/score run it as nobody.
chmod 755 .bin/candidate

# #53: upstream's own test suites for every editable package, against the
# candidate (the harness only decodes Conway mempool txs at PV 11). Slow, so
# opt-in: promotion and preflight set CB_UPSTREAM_TESTS=1. setup.sh fetched
# their dependencies (the CI build has no network). Separate project and
# builddir: the tests are built with upstream's +asserts (cabal.project.test,
# see setup.sh), and dist-cand should stay warm for the harness.
# The record's tree (CB_ROLE=record, paired promotion) passed them already.
if [ "${CB_UPSTREAM_TESTS:-0}" = 1 ] && [ "${CB_ROLE:-candidate}" != record ]; then
  targets=$(grep -v '^#' packages.txt | awk 'NF { print $1 ":test:tests" }')
  cd "$L"
  # shellcheck disable=SC2086
  cabal build -v1 --project-file=cabal.project.test --builddir=dist-cand-test --enable-tests $targets
  for t in $targets; do
    echo "upstream tests: $t"
    timeout "${CB_UPSTREAM_TESTS_TIMEOUT:-1800}" cabal test -v1 --project-file=cabal.project.test \
      --builddir=dist-cand-test --enable-tests --test-show-details=failures "$t" \
      || { echo "upstream tests failed: $t"; exit 1; }
  done
fi
