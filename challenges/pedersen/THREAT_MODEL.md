# Threat model

Required before launch; reviewed in the PR that opens the challenge.

## How could a wrong answer pass the verifier?

- Finite or reusable inputs (is the input set fresh per run? how many?): each verify run draws 50,000 fresh input pairs from the harness seed (`CB_SEED`), which is chosen after the build and recorded afterwards. There are also 82 fixed edge cases: every pair of {0, 1, 2, p−2, p−1, 2^248−1, 2^248, 2^248+1, 2^251}, plus the crypto-cpp vector.
- Caching or precomputation (can a table or memo pay off without knowing the seed?): memoising outputs can't pay off, because inputs are unknown until run time. Larger precomputed point tables are legitimate optimisation. They are bounded by `max_bytes` (1 MiB for candidate/src), and one-time table initialisation is counted in the score.
- Reading the reference, oracle, seed or expected output: the token policy bans `fs`, `env`, `include*!`, `#[path]`, `extern` and `process`. The frozen `candidate/Cargo.toml` allows no dependencies, so the candidate can't link the oracle crates.
- Timing tricks (detecting the benchmark, doing less work when measured): the score is an instruction count on the same workload for both sides. `no_mangle`/`link` are banned, so the measured region (`cb_measured`) can't be redefined. The wall-time guard catches changes that cut instructions but lose time.
- Writing the score file directly, or editing anything outside the editable paths: only `candidate/src` is editable. The PR gate refuses everything else, plus symlinks and gitlinks. Scores are recomputed on `main` by the promotion re-run.

## Does the benchmark interface match real use?

Yes, mostly. `pedersen(a, b)` on big-endian felts is the call cairo-vm's Pedersen builtin makes (cairo-vm → starknet-types-core → lambdaworks-crypto `PedersenStarkCurve::hash`). The benchmark times single calls on uniform inputs. Real use (storage-trie commitments) also hashes one pair at a time, so batching across calls isn't rewarded, which matches deployment.

## What is the known bound or headroom?

Unknown, and #6 records it as the main risk. lambdaworks is the fastest known Rust implementation: it is 1.4× faster than the independent starknet-crypto 0.6.2, and starknet-crypto ≥ 0.7 just wraps it. There is no published faster bound.

## What does a large outlier (+100%) trigger?

Manual review; never auto-promote.

- Who reviews: the challenge owner diffs the candidate against the baseline and re-runs verify with extra seeds before the record stands.
