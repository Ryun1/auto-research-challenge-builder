# ledger-cbor: faster Conway CBOR decoding in cardano-ledger

Make [cardano-ledger](https://github.com/IntersectMBO/cardano-ledger) decode
real mainnet Conway transactions in fewer instructions, without changing what
it decodes or what it rejects. The editable code is the `src/` of the decoding
stack: `cardano-ledger-binary` (the `Decoder` monad, `Annotator`, coders,
primitives), `cardano-ledger-core` and the era packages from Shelley to Conway
(the per-field instances, `MaryValue`, `BabbageTxOut`, the Conway tx body...).

Status: **draft**. Measure run 42721883592 profiled the reference: the
editable packages run about a quarter of the instructions directly (binary
6%, mary 9%, core 3%, babbage/conway/alonzo 5%), and the RTS another 38%,
much of it closure application and allocation that the decoders' shape
decides. pallas (Rust) decodes the same txs in ~4.5x fewer instructions.

| Path | Role |
|---|---|
| `candidate/<package>/src/` | **editable**: the `src/` of each package in `packages.txt`, from cardano-ledger @ 7827506. Same file set; cabal files, testlibs and tests stay upstream's |
| `baseline/` | the same, frozen (the gate's policy and preflight compare against it) |
| `packages.txt` | which packages, and where each lives upstream |
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

**Policy.** `cb gate` checks every `src/` file against its baseline
counterpart with an allowlist (`crates/cb/src/haskellpolicy.rs`, #51): safe
pragmas only, others verbatim from the baseline file; extensions from a safe
list or the baseline file, never TH, QuasiQuotes or FFI; CPP lines from the
baseline file; imports package-local, plain pure modules or the baseline's
(modules that reach memory, IO or GHC internals only as an exact baseline
import); no `foreign` declarations; no more `unsafe*`/`runRW#` uses per
identifier than the baseline. `build.sh` repeats a quick denylist locally.

**Build.** `setup.sh` clones cardano-ledger at the pin into a shared cache
(`$CB_LEDGER_CACHE`, default `~/.cache/cb-ledger-cbor/<sha>`), builds the
reference, and copies each editable package to `cand/<name>`. `build.sh`
syncs `candidate/<name>/src` over those and rebuilds incrementally: changed
modules and their dependents up to Conway recompile, which is the
per-submission cost. The trusted Warm workflow (`warm.yml`, every 5 days and
when the judge changes) prebuilds the reference and the baseline's
`dist-cand` into a cache that score jobs restore, so a PR doesn't start from
a 30-50 minute cold build (#54). In CI the build runs in the build sandbox
(`[build_sandbox]`): no network, no sudo, its own pid namespace, and a
read-only home and workspace except `.bin/candidate`. Cache writes go to a
throwaway overlay.

**Upstream tests.** The harness only decodes Conway mempool txs at PV 11, but
the editable packages serve every era, blocks and ledger state. So a promotion
also builds and runs each editable package's own upstream `tests` suite
against the candidate (`CB_UPSTREAM_TESTS=1`, #53). The cabal files and tests
are the pinned upstream ones. The Preflight workflow's `upstream_tests` input
times this on the baseline.

**Promotion.** Each of the two promotion runs scores the record's tree too, on
the same fresh pool and seed, and `cb record` decides on candidate/record
(#57). Measure run 42721883592 showed that two different pools disagree by
more than the agreement window once a win is 5% or more.

Needs Linux (valgrind), GHC 9.6.7, cabal 3.16, and libsodium, secp256k1 and
blst. CI installs them with `input-output-hk/actions/base`.
