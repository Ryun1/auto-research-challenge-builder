# Calibration (#6), 2026-09-30

We scored the five sibling challenges, plus Pedersen, and compared the
ranking with the maintainer's gut ranking: ecdsafail > provably.fast > heesch
> eip8200 > QSB. The scores and their evidence are in `candidates/*.toml`.

## Measurements

| Candidate | Where | Result |
|---|---|---|
| ecdsafail @708d567 | ubuntu-24.04, 2 vCPU (calibrate runs 36713485943, 36713844990) | 1.4–1.9 min, 0.78 GB, 1.0 GB disk: **hostable** |
| ecdsafail | Apple M5 32 GB, local | 2.7 min, all yes |
| eip8200 modexp @af1c673 | ubuntu-24.04 | **out of disk** after 4 min (Mathlib + Lean toolchains > ~14 GB free) |
| eip8200 modexp | Apple M5 32 GB, local | **timed out at 60 min** (serial Lean build), 8.35 GB build output |
| provably.fast @4ad7a2b8 | Apple M5, local | baseline builds and runs on arm64 in 1.6 min. The repo is INTERNAL, so it isn't measured in this repo's CI |
| heesch, QSB | not run | documented runners: heesch 32 vCPU/128 GB/240 min; QSB needs a CUDA GPU |
| Pedersen | Apple M5, local | lambdaworks 0.13 ≈ starknet-crypto 0.8 (the same code: 0.8 wraps lambdaworks); independent starknet-crypto 0.6.2 is ~1.4× slower. No faster bound is known |

Findings that changed other issues:
- **starknet-crypto ≥ 0.7 is not independent of lambdaworks.** Pedersen's
  second oracle is 0.6.2 (starknet-ff).
- **provably.fast is not public** (the plan said it was). Its x86 laptop
  facts are documented, not measured.
- **Lean/Mathlib doesn't fit a standard runner's disk.** Stripping the
  preinstalled SDKs (roughly 30 GB) is the usual fix. It's untried here.

## Weights

v0 put heesch (75.8) above provably.fast (73.7), and QSB (68.4) above
eip8200 (64.2). A weight search over 1–4 per criterion found:

- **Value 2 → 3** is the smallest change that restores provably.fast >
  heesch. It's also defensible on its own: the audience is SNF, and a win that
  ships to Starknet is the point.
- Every full match with the gut ranking also needs `eval_cost` and/or `laptop`
  weighted down to 1. Those are the measured criteria, and eip8200's measured
  cost (a first build over 1 h, not hostable) is what the gut ranking didn't
  know when it was made.

**Decision (maintainer): rubric v1 = v0 with Value 3.** eip8200 below QSB
stands as a measured correction to the gut ranking.

| # | v1 | Total |
|---|---|---|
| 1 | ecdsafail | 86.0 |
| 2 | pedersen | 78.0 |
| 3 | provably-fast | 75.0 |
| 4 | heesch | 74.0 |
| 5 | qsb-pinning | 68.0 |
| 6 | eip8200-modexp | 64.0 |

The provably.fast/heesch margin is 1 point. Revisit it when more candidates
exist, rather than tuning further on six.
