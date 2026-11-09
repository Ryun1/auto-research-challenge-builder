# Threat model

Required before launch; reviewed in the PR that opens the challenge.
Answer every question. "N/A" needs a reason.

## How could a wrong answer pass the verifier?

- Finite or reusable inputs (is the input set fresh per run? how many?): each run draws 600 valid txs + 300 mutants from a pool of 1,633 mainnet Conway txs (150 blocks, epochs 645-666) with CB_SEED, plus 4 fixed edge cases. The pool is public and finite. A hand-written table of decoded pool txs in source is possible, with no TemplateHaskell needed. max_bytes (1 MB, about 500 KB of headroom over the baseline) caps it below the 1.3 MB pool, but a partial table would still pay. **Open before launch:** grow the pool to 10k+ txs and score on a held-out pool that isn't in the repo. Mutants are fresh each run and can't be tabled.
- Caching or precomputation (can a table or memo pay off without knowing the seed?): scoring decodes each sampled tx once, and start-up is subtracted, so a cross-call memo has nothing to reuse. The repeat loop only runs for the wall-time guard. A global memo needs unsafePerformIO, an IORef or similar, which are banned.
- Reading the reference, oracle, seed or expected output: the decoder is a pure library. build.sh bans new unsafe* identifiers, runRW#/realWorld#, FFI and IO modules in src/, so it has no way to run IO. Expected digests come from .bin/reference at verify time and are never written where the build can read them.
- Timing tricks (detecting the benchmark, doing less work when measured): there's no IO, so it can't see argv or env. The digest pass forces the full decoded value (show) and the re-encoded bytes, on the verify sample and on the exact txs that get timed. The timed pass forces it with rnf, and the candidate owns the NFData instance for Sized, so a digest can't show what rnf forced: verify repeats the timed decode + rnf and walks the heap with judge code (harness/HeapWalk.hs, no instance trusted). Anything left unevaluated, or more function closures than the reference has, is a reject (preflight/broken/lazy). Not covered: an rnf that skips walking data the decoder already evaluated saves the walk without decoding any faster; scoring decode-to-WHNF with a per-path laziness check against the reference would close that.
- Writing the score file directly, or editing anything outside the editable paths: the gate refuses files outside candidate/, symlinks and submodules. build.sh lets only candidate/src/ differ from the baseline, so the cabal file, ghc-options and Setup.hs are frozen.

## Does the benchmark interface match real use?

Partly. The harness decodes full transactions the way the mempool receives them (`Annotator (MempoolTx ConwayEra)` at protocol version 11) with the node's own decoder. Block decoding takes the same body, witness and aux-data decoders, but splits them differently, and it isn't measured. We build without upstream's dev-only `+asserts` flags, as releases do. Only cardano-ledger-binary is editable, so wins inside the Conway era instances are out of scope for v1.

## What is the known bound or headroom?

No published decode benchmarks were found (2026-11-06). Comparison points to measure: pallas (Rust) on the same pool, and plain decoding vs the Annotator path (the cost of keeping original bytes).

## What does a large outlier (+100%) trigger?

Manual review; never auto-promote.

- Who reviews: a SNF maintainer, plus a cardano-ledger contributor if one agrees. Reject parity is consensus-critical, so any change to what is accepted gets a human read even when it passes the verifier.
