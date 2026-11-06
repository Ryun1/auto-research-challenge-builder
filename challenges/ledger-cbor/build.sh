#!/usr/bin/env bash
# The only step that compiles submission code: token policy, then build the
# harness against candidate/ in place of cardano-ledger-binary.
set -euo pipefail
cd "$(dirname "$0")"

python3 - <<'EOF'
# Policy, relative to the baseline:
#  - only files under src/ may change or be added (cabal file, Setup.hs, tests frozen)
#  - each banned token, and each distinct identifier containing "unsafe" (any
#    case), appears in src/ no more often than in the baseline. Counted per
#    identifier, so dropping an existing unsafeDeserialize can't pay for a new
#    unsafePerformIO. Pure code can't do IO without one of these.
import collections, pathlib, re, sys
base, cand = pathlib.Path("baseline"), pathlib.Path("candidate")
BANNED = ["foreign", "runRW#", "realWorld#", "GHC.Magic", "accursedUnutterablePerformIO",
          "inlinePerformIO", "TemplateHaskell", "QuasiQuotes", "OPTIONS", "{-# ANN",
          "CPP", "#include", "GHC.IO", "System.IO", "Data.IORef", "System.Environment",
          "System.Process", "Debug.Trace"]
def files(root): return {p.relative_to(root): p for p in root.rglob("*") if p.is_file()}
fb, fc = files(base), files(cand)
bad = []
for rel, p in fc.items():
    if p.is_symlink(): bad.append(f"symlink {rel}")
    elif rel.parts[0] != "src" and (rel not in fb or fb[rel].read_bytes() != p.read_bytes()):
        bad.append(f"only src/ may change: {rel}")
bad += [f"deleted outside src/: {r}" for r in fb if r.parts[0] != "src" and r not in fc]
def tokens(fs):
    c = collections.Counter()
    for r, p in fs.items():
        if r.parts[0] != "src": continue
        t = p.read_text(errors="replace")
        c.update({tok: t.count(tok) for tok in BANNED})
        c.update(re.findall(r"[\w'#.]*unsafe[\w'#]*", t, re.I))
    return c
tb, tc = tokens(fb), tokens(fc)
bad += [f"banned token {k!r}: {n} > baseline {tb[k]}" for k, n in tc.items() if n > tb[k]]
if bad:
    print("policy: " + "; ".join(bad[:10])); sys.exit(1)
EOF

rm -rf .ledger/cand-binary
cp -R candidate .ledger/cand-binary
cd .ledger
cabal build -v1 --project-file=cabal.project.cand --builddir=dist-cand ledger-cbor-harness
cp "$(cabal list-bin --project-file=cabal.project.cand --builddir=dist-cand ledger-cbor-harness)" ../.bin/candidate
