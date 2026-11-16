# Candidates

One `<slug>.toml` per candidate problem. `cb rank` scores them all with the
rubric (PLAN.md §1). Edits go in via PR. These evaluations are AMBER and are
never published to the public site.

Research candidates (open, multi-axis challenges with no single score) live in
`research/<slug>.md` instead, judged with the draft rubric in
`.claude/skills/judging-challenge-candidates/research-rubric.md`. `cb rank`
doesn't read them.

Start from `cb import path/to/benchmark.json --url <repo>`, then fill in the
rest. Every value needs `evidence`, meaning a link or a measurement.

```toml
[candidate]
slug = "pedersen"              # must match the file name
title = "Pedersen hash"
family = "perf"                # perf | resource | math | proof
direction = "minimise"         # minimise | maximise
category = "rust"              # toolchain, as in benchmark.json
url = "https://…"
editable = ["candidate/"]
# min_improvement_bips = 100   # optional
# max_bytes = 8388608          # optional

[gates.verifiable]             # G1; also scalar_objective (G2), bounded (G3), baseline (G4)
pass = true
evidence = "bit-exact vs lambdaworks + starknet-crypto"

[scores.expert_scarcity]       # 1–5. Criteria: expert_scarcity headroom eval_cost noise
value = 3                      # gaming_resistance fidelity value decomposability prior_art
evidence = "…"                 # (laptop is derived from [laptop]; don't score it)

[laptop]                       # measured: yes | yes-slow | no
evidence = "M2 16 GB + ubuntu 16 GB, 2026-10-01"
apple = { produce = "yes", verify = "yes", score = "yes-slow" }
x86   = { produce = "yes", verify = "yes", score = "yes" }

[peaks]                        # one verify+score run; decides "hostable here"
minutes = 4.0
ram_gb = 1.5
disk_gb = 3.0
gpu = false
evidence = "calibrate run 36713844990, ubuntu-24.04 2 vCPU"
```
