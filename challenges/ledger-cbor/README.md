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
| `preflight/broken/` | off-by-one (accepts 1 trailing byte), partial (gives up on txs > 1000 bytes), constant (rejects all), lazy (leaves every output unevaluated behind its own `rnf`) |

**Correct** means the same as the pristine reference on every input: the same
`show` of the decoded tx, the same re-encoded bytes, and the same accept/reject
on truncated, bit-flipped, extended and re-tagged mutants. A decoder that
accepts more is a consensus bug, not a speedup.

The txs that get timed are verified too, with the same seed. And because the
timed pass forces a decoded tx with `rnf`, whose instance for `Sized` is in
`candidate/`, verify repeats that decode + `rnf` and walks the heap with
judge code (`harness/HeapWalk.hs`): nothing may be left unevaluated, and the
value may hold no more function closures than the reference's.

**Score.** Callgrind instructions to decode 800 seeded txs once, minus a 0-pass
run (start-up plus reading the input). The RTS timer is off (`-V0`), so counts are deterministic: `speedup = reference / candidate`. A paired wall-time run guards
against regressions (as in `kits/rust`).

**Build.** `setup.sh` clones cardano-ledger at the pin into a shared cache
(`$CB_LEDGER_CACHE`, default `~/.cache/cb-ledger-cbor/<sha>`), builds the
reference, and builds the baseline as a warm candidate build. Preflight runs
setup once per workspace, so only the first run is cold. `build.sh` applies the
token policy (no FFI, new `unsafe*` identifiers, `runRW#`, TemplateHaskell, CPP,
OPTIONS pragmas or IO modules, counted per identifier against the baseline),
syncs `candidate/` into the cache and rebuilds incrementally. Changed modules and
their dependents up to Conway recompile: that is the per-submission cost.

Needs Linux (valgrind), GHC 9.6.7, cabal 3.16, and libsodium, secp256k1 and
blst. CI installs them with `input-output-hk/actions/base`.
