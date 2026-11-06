# ledger-cbor: faster Conway CBOR decoding in cardano-ledger

Make [cardano-ledger](https://github.com/IntersectMBO/cardano-ledger)'s
`cardano-ledger-binary` decode real mainnet Conway transactions in fewer
instructions, without changing what it decodes or what it rejects.

Status: **draft**. It hasn't been built end to end yet: spike run
42294073486 measures build cost and Ir determinism.

| Path | Role |
|---|---|
| `candidate/` | **editable**: a copy of `libs/cardano-ledger-binary` @ 7827506. Only `src/` may change |
| `baseline/` | the same copy, frozen (preflight and the token policy compare against it) |
| `harness/` | judge exe, built against the reference and against the candidate |
| `corpus/pool.hex.gz` | 1,633 mainnet Conway txs (Koios, epochs 645-666, PV 11); see `pool.json` |
| `verifier/judge.py` | verify (values, bytes, accept/reject parity) and score |
| `preflight/broken/` | off-by-one (accepts 1 trailing byte), partial (truncates Word64), constant (rejects all) |

**Correct** means the same as the pristine reference on every input: the same
`show` of the decoded tx, the same re-encoded bytes, and the same accept/reject
on truncated, bit-flipped, extended and re-tagged mutants. A decoder that
accepts more is a consensus bug, not a speedup.

**Score.** Callgrind instructions to decode 800 seeded txs once, with start-up
subtracted: `speedup = reference / candidate`. A paired wall-time run guards
against regressions (as in `kits/rust`).

**Build.** `setup.sh` clones cardano-ledger at the pin and builds the reference
plus external deps (cache `~/.cabal/store`). `build.sh` applies the token policy
(no FFI, unsafe*, TemplateHaskell, CPP, IO modules; see the list in the file),
then rebuilds the ledger packages above the binary package against `candidate/`.
That rebuild is the per-submission cost.

Needs Linux (valgrind), GHC 9.6.7, cabal 3.16, and libsodium, secp256k1 and
blst. CI installs them with `input-output-hk/actions/base`.
