# Rust kit

`cb new <slug> --family perf --kit rust` copies this directory. Everything here
is the judge except `candidate/src/`, the only editable path.

| Path | Role |
|---|---|
| `candidate/src/` | editable: `pub fn eval(&[u8; IN]) -> [u8; OUT]` |
| `candidate/Cargo.toml` | frozen: no deps, no build.rs, `unsafe_code = "forbid"` |
| `reference/` | frozen copy of the baseline (B side of timing, count denominator) |
| `oracle/` | independent implementation(s), bit-exact targets |
| `harness/src/iface.rs` | **the maker's adapter**: sizes, sampler, edge cases, oracles |
| `harness/` | generic verify / score / workload binaries |
| `baseline/` | what preflight swaps into `candidate/src/` |
| `preflight/broken/` | off-by-one, partial, constant: must fail verify |

To make a real challenge: replace the toy in `candidate/src`, `baseline/`,
`reference/src` and `oracle/src`; edit `harness/src/iface.rs`; rewrite the
three broken variants; answer `THREAT_MODEL.md`; run `cb preflight <slug>`.

**Score.** The Callgrind instruction count inside `cb_measured` (candidate vs
reference on the same seeded workload) ranks as `speedup`. A paired
wall-time run in mirrored blocks guards: `guard` is `regressed` if wall time
is slower beyond `max_dispersion`, `unstable` if the timing is too noisy to
judge (an infra failure, not a rejection).

**Scoring on a Mac.** Valgrind is Linux-only, so run `CB_SEED=1 bench/run-docker`.
Counts on arm64 differ from the official x86_64 runner, so compare `speedup`
against the baseline, not raw `instructions`. `verifier/run` works natively.
