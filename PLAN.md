# auto-research-challenge-builder: plan (draft v1)

The organiser side of [auto-research-toolkit](../auto-research-toolkit). Two jobs:

1. **Evaluate** candidate problems: is this a good autoresearch challenge?
2. **Build and run** simple internal challenges: spec, verifier, baseline,
   preflight, GitHub-driven scoring, and a feed the toolkit reads natively.

Not a Yukon replacement. A challenge that outgrows GitHub-hosted runners (GPU,
big RAM, pinned hardware) graduates to Yukon or a dedicated evaluator like
provably.fast; the evaluator still rates it.

## Decisions so far

| Topic | Decision |
|---|---|
| Users | SNF + ecosystem teams |
| v1 scope | evaluator + scaffold + preflight |
| Families | perf speedups, resource minimisation, math constructions/bounds, soundness/proof |
| Scoring | hard gates, then weighted 1–5 criteria |
| Eval UI | static site rendered from repo data (edits via PR) |
| Language | Rust |
| Spec | own TOML (toolkit `domain.toml` style), with a benchmark.json importer |
| Layout | monorepo: builder + `challenges/<slug>/` |
| Runner | GitHub Actions, standard GitHub-hosted runners (self-hosted later; runner label configurable per challenge) |
| Submission | PR by collaborators (ecosystem teams added as collaborators); website submission later |
| Leaderboard page | public static page; candidate evaluations never published there |
| Min improvement | set by the challenge maker; preflight's noise floor is advisory |
| Expert scarcity | judgement call with a source; assisted lookup later, advisory only |
| Who evaluates | any SNF member, one reviewer per PR; automate later |
| First challenge | Pedersen hash (lambdaworks, used by cairo-vm's builtin), on a generic Rust kit (§7) |
| Perf metric (Rust kit) | Callgrind instruction count ranks; paired wall-time must not regress |
| provably.fast code | copy in, no dependency |
| CLI name | `cb` |
| Perf noise | paired baseline/candidate runs in one job, dispersion cap |
| Anti-gaming | threat-model checklist (no agent red-team in v1) |
| Toolkit | feed = the toolkit's `Snapshot` JSON; new pass-through source |
| provably.fast | ours: extract code where it generalises |
| Onchain / rewards | out of v1 |
| Visibility | private for now (candidate evaluations are AMBER) |

## Landscape (Sep 2026, public sources)

- **Yukon** (Eigen Labs): 7 active challenges (QSB, ECDSA.fail, SNARK.fast,
  precompile.fast, MLX.fast, Heesch, Proximity Prize). Contract is
  `benchmark.json` (schemaVersion 1 = single track, 2 = `tracks[]`): direction,
  editablePaths, setup/benchmark commands, scorePath,
  `minScoreImprovementBips` (QSB: 100 = 1%; Heesch: 0), maxSubmissionBytes,
  runner workflow.
- **ECDSA.fail paper** (arXiv 2609.09582): 86% Q×T reduction, >100
  contributors, 400+ promoted. Lessons:
  - finite test sets were gamed (fixed with 50k+ random inputs)
  - the benchmark interface diverged from real use (windowed Shor)
  - there's no count of failed attempts
  - agents refine; humans make the architectural leaps
- **provably.fast**:
  - bit-exact oracle on salted inputs
  - only 2 editable files, plus a banned-token policy
  - paired runs
  - a research graph, with reproduction required before promotion
- **ZeroThesis**: 82 math-construction challenges, signed result ledger.
- Nobody publishes *how they select problems*. The evaluator fills that gap.

### What the sibling challenge repos actually do (checked)

| Repo | Spec | Runner | Notes |
|---|---|---|---|
| ecdsafail-challenge | benchmark.json v1 | Blacksmith 32 vCPU, 45 min | path check in workflow |
| heesch | benchmark.json v1 | Blacksmith 32 vCPU / 128 GB, 240 min | **scoring never runs participant code**; submission is data + UNSAT certificate; rejects symlinks/gitlinks; proof takes minutes on a laptop, record verification needs the big box |
| eip8200-challenges | benchmark.json v2 | self-hosted | Lean proof + gas measurement |
| quantum-safe-bitcoin-challenge | benchmark.json v2 | GPU | CUDA |
| provably.fast | own evaluator | pinned x86 | Rust workspace |

**None of them fits a GitHub-hosted runner.** For private repos those are
2 vCPU / 8 GB / 14 GB disk / no GPU (macOS: 3× M1 / 7 GB). So "hostable here"
has to be a separate check from "good candidate" (see §1).

**Trust model:** Yukon triggers `workflow_dispatch` on a `submissions/*` branch
holding a single-parent commit that Yukon itself built from editable paths only.
The workflow that runs is therefore always the base branch's. A plain
`pull_request` trigger runs the PR's own copy of the workflow, which means a
submission can rewrite its own judge. §4 is designed around this.

## 1. Evaluator

Candidates live in `candidates/<slug>.toml`; `cb site` renders a static
ranking page. Scoring logic lives in code, so the rubric is versioned and every
score shows its evidence. `cb import benchmark.json` pre-fills a candidate from
an existing Yukon-style repo (direction, editable surface, category).

### Hard gates (any fail = reject, with reason)

- **G1 Machine-verifiable correctness**: deterministic verifier, no human
  judgement in the loop.
- **G2 Scalar objective** with a direction (an expression over metrics is fine).
- **G3 Bounded verification**: one verify+score run finishes within 6 h on
  *some* named runner. This is about candidate quality, not whether we can
  host it.
- **G4 Baseline exists** and passes the verifier.

### Scored criteria (1–5; total = Σ w·s / Σ w·5 × 100; weights are v0)

| Criterion | W | 5 looks like | 1 looks like |
|---|---|---|---|
| Expert scarcity | 3 | <50 people could seriously work on it | a mainstream, crowded field |
| Headroom | 2 | known bound far below baseline | baseline near theoretical optimum |
| **Laptop validation** | 2 | produce + verify + score on a stock laptop in minutes | needs GPU / 100 GB RAM even to check correctness |
| Eval cost | 2 | seconds | hours |
| Noise vs threshold | 2 | deterministic count, or noise ≪ min improvement | noise ≈ min improvement |
| Gaming resistance | 2 | submission is data checked by a certificate/proof, or bit-exact on fresh inputs | participant code runs in the judge; fixed test set |
| Fidelity | 2 | benchmark = real use | proxy that diverges from deployment |
| Value | 2 | a win ships (Starknet or broadly) | curiosity only |
| Decomposability | 1 | many small independent steps | one big leap or nothing |
| Prior art / guides | 1 | papers + code agents can read | tribal knowledge only |

**Laptop validation** is measured, not guessed. It is recorded as three
separate facts, because Heesch shows they diverge (proof on a laptop in
minutes; record verification needs 128 GB):

- `produce`: can a participant generate a candidate locally?
- `verify`: can they run the correctness gate locally?
- `score`: can they get a comparable score locally?

Each is yes, yes-slow, or no, measured on two reference laptop classes: Apple
Silicon 16 GB and x86 Linux 16 GB. Architecture matters here: x86 SIMD and
CUDA both fail on an M-series Mac. Preflight fills this in (§3).

**Hostable here** is a separate flag, not scored: does verify+score fit a
GitHub-hosted private runner (2 vCPU / 8 GB / 14 GB, ≤60 min target)? A
candidate can rank high and still need Yukon.

When `produce`, `verify` or `score` is not laptop-yes, the evaluator and
preflight emit a **suggestion** for fixing it, not just a low score. Typical
fixes: ship a small-size local profile, a CPU reference path next to the GPU
one, or a cheap check that correctness holds so only record verification needs
the big machine.

Every score needs an `evidence` line (link or measurement). Expert scarcity is
a judgement call backed by a source (author count, maintainers, etc.). A later
assisted lookup may propose a value, but it stays advisory.

### Calibration set (the sibling repos, all public)

Score these first. If the rubric doesn't rank them the way we'd rank them by
feel, change the weights. Laptop and hostable values are measured in M1,
not assumed:

| Challenge | Family | Laptop signal known today | Hostable here |
|---|---|---|---|
| provably.fast (Stwo FFT) | perf | x86 SIMD path; check arm64 | likely, but noisy |
| ecdsafail | resource min | Rust, deterministic count | measure (CI uses 32 vCPU) |
| heesch | math | produce yes; record verify no | no (60 GB scratch) |
| eip8200 (Lean) | proof | Lean build weight unknown | measure (8 GB, 14 GB disk) |
| QSB | perf (CUDA) | no: GPU | no |

## 2. Challenge spec + scaffold

`cb new <slug> --family perf|resource|math|proof` creates
`challenges/<slug>/` with:

```
challenge.toml     # spec
verifier/          # correctness gate; any command, JSON out
baseline/          # reference solution, must pass
candidate/         # the editable paths
THREAT_MODEL.md    # checklist answers (§3)
SUBMITTING.md
```

```toml
# challenge.toml (sketch)
[challenge]
slug      = "toy-sort"
family    = "perf"            # perf | resource | math | proof
direction = "maximise"
status    = "open"            # draft | open | closed

[paths]
editable  = ["candidate/"]    # everything else refused
max_bytes = 8388608

[commands]
setup  = "./setup.sh"
build  = "./build.sh"         # the only step that touches candidate code as code
verify = "./verifier/run"     # exit 0 + JSON {ok, detail}
score  = "./bench/run"        # JSON {metrics: {...}}

[objective]
expr            = "speedup"
min_improvement = 0.01        # challenge maker's call; preflight warns if below measured noise

[perf]                        # perf family only
pairs          = 10           # interleaved baseline/candidate runs
max_dispersion = 0.02

[inputs]
salted = true                 # fresh inputs per run; seed generated by the harness, recorded after
```

The field names map one-to-one onto benchmark.json where the two overlap, so
`cb export yukon` is a mechanical translation if a challenge ever graduates.
It's not in v1.

Per-family defaults:
- **perf**: paired runs, speedup ratio, bit-exact oracle on salted inputs.
- **resource**: deterministic count; verifier on N random inputs (default high,
  per the ECDSA.fail lesson).
- **math**: submission is a **data** certificate; the verifier checks it and
  never executes participant code (Heesch pattern). This is the safest family
  to host.
- **proof**: Lean (or similar) check with banned tokens/axioms (`sorry`,
  `native_decide`, project axioms: the eip8200 pattern), plus executable test
  vectors as a falsification check.

## 3. Preflight (`cb preflight <slug>`)

Mechanical checks that block launch:

- **Baseline passes** the verifier, and a set of deliberately broken
  candidates each fail it. That set is off-by-one output, a partial
  computation, and cached/constant output. This is the cheap mechanical piece
  of red-teaming.
- **Noise floor** (perf): N paired baseline-vs-baseline runs on the real runner
  → a warning (not a block) if the maker's `min_improvement` sits below
  ~2× p95 noise.
  GitHub-hosted VMs change CPU model between jobs, so each run records its
  fingerprint and pairs never span jobs.
- **Laptop profile**: run produce/verify/score on `macos-latest` (M1, 7 GB)
  and `ubuntu-latest` (2 vCPU, 8 GB) GitHub-hosted runners as laptop-class
  proxies. That gives real arm64 and x86 numbers without Docker emulation.
  `cb preflight --local` does the same on the maintainer's own laptop.
- **Path policy**: a patch touching a non-editable path is refused, and so is
  a symlink or gitlink inside the editable surface (the Heesch V12 lesson).
- **Determinism** (resource/math/proof): same input twice gives the same result.
- **Hostable**: time, RAM and disk peaks against the GitHub-hosted limits.

Human checklist (`THREAT_MODEL.md`, required, reviewed in PR):
- How could a wrong answer pass the verifier? (finite inputs, caching,
  precomputation, reading the reference or the seed, timing tricks, writing the
  score file directly)
- Does the benchmark interface match real use? (the ECDSA.fail windowed lesson)
- What's the known bound / headroom?
- What does a +100% outlier trigger? (manual review, never auto-promote)

## 4. Runner (GitHub Actions)

Trust rule: **the judge always comes from `main`; submission bytes are data
until the build step.**

1. The participant opens a PR that touches only `challenges/<slug>/candidate/`.
2. A **`pull_request_target`** workflow runs from `main` with read-only
   permissions and no secrets. It checks out `main`, then copies in *only* the
   editable paths from the PR head. Before copying it checks paths, file modes
   and size, and it refuses the PR if anything outside the editable surface
   changed. Then it runs setup → build → verify → score (paired for perf) and
   uploads `score.json` as an artifact.
3. A separate **`workflow_run`** job, which never executes submission code,
   reads the artifact and comments the score on the PR.
4. **Promotion**: a maintainer merges, which triggers a re-run on `main` from
   a clean checkout. Only that run appends to `challenges/<slug>/results.jsonl`.
   A second independent run must match within dispersion before a result
   becomes the record.

CODEOWNERS protects `.github/`, `challenges/*/verifier/`, `bench/` and
`challenge.toml`. Rejected and slower attempts are also appended, with
status, so the record keeps the denominator ECDSA.fail lacked.

Costs: GitHub-hosted minutes are billed on private repos (macOS at a higher
multiplier), so laptop-profile runs happen at preflight only, never per PR.

## 5. Feed + toolkit integration

`cb feed` writes per-challenge JSON that **is the toolkit's `Snapshot`
serialisation**, as defined in `autoresearch/challenge.py`:

- source, challenge, url, generated_at, truth_label
- record {value, unit, by, at}
- board rows {rank, solver, model, value, delta, delta_pct, at}
- bar (min improvement, statistic, pairing)
- policy (editable/forbidden paths, measurement policy, runner fingerprint)

On top of that it adds an `attempts` count by status.

The toolkit gains `source = "builder"`, whose parser is just JSON-to-Snapshot
(no HTML scraping like the yukon adapter), so `ar challenge
pull/show/check/target` works unchanged. The feed's `schema_version` is owned
here, and the toolkit pins the versions it accepts.

## 6. Extraction from provably.fast

Audit first, then lift what generalises. What's there (checked):

- `crates/artifacts`: canonical, content-addressed evaluator artifacts. Likely
  reusable as the results/evidence format.
- `crates/replay`: containment, executable store, bounded replay. Probably
  reusable for the promotion re-run.
- `crates/cli` ledger (`ledger.rs`, `kv.rs`): possibly the results.jsonl writer.
- `research/discovery/oracle/` (`oracle_bench.rs`, `oracle_worker.rs`) and the
  discovery Python: this is where the paired-run and oracle logic lives. It
  would be the perf-family harness, but it's partly Python, so it needs
  porting.
- `crates/strict-profile`, `crates/baseline`, `crates/candidates`: Stwo-specific;
  these stay put.

The workspace is ~17k lines of Rust across 6 crates. Code is **copied in**
(no git or path dependency), with a note recording the source commit, and then
trimmed to what the builder uses.

## 7. Rust kit + first challenge: Pedersen hash

### 7a. The Rust kit (generic scaffolding)

Family says *what kind of score*; kit says *what toolchain*. `cb new <slug>
--family perf --kit rust` gives any Rust optimisation challenge the same
skeleton, so the next one (Poseidon, ec_op, a Stwo kernel) is mostly config.

- **Layout**:
  - `candidate/`: one editable crate exposing a fixed function signature.
  - `reference/`: frozen baseline copy.
  - `oracle/`: independent implementation(s), frozen.
  - `bench/`: fixed harness, non-editable.
  - `rust-toolchain.toml`, `Cargo.lock`: pinned, non-editable.
- **Policy**:
  - no new dependencies (lockfile frozen)
  - no `build.rs`, no proc macros
  - `unsafe`, `asm!` and `target_feature` are allowed only if the challenge
    opts in (provably.fast's token check, copied in)
  - size cap on the crate, which bounds precomputed tables
- **Verifier (differential)**: candidate vs oracle(s) on harness-salted random
  inputs plus fixed edge cases, bit-exact.
- **Score: instruction count ranks, wall-time guards.**
  - The score is the Callgrind instruction count (iai-callgrind) of a
    workload generated from a seed the harness picks at run time. It's
    deterministic given the seed, and a large N keeps variance tiny.
  - The promotion re-run uses a different seed and must agree within
    tolerance.
  - Guard: paired wall-time (baseline vs candidate, interleaved in one job)
    must not regress beyond measured noise, or the submission is rejected.
    This catches changes that cut instructions but lose time (cache-missing
    tables, slow divisions), which instruction counts can't see.
- **Laptop**: `verify` runs natively anywhere. `score` needs Valgrind, which
  means Linux: Mac users run it in Docker, where arm64 counts differ from the
  official x86_64 runner. The *relative* change vs baseline is still a
  useful local signal. The laptop criterion records this as "score:
  yes-slow, arch differs".

### 7b. First challenge: Pedersen hash on the STARK curve

Target: the Pedersen that cairo-vm's Pedersen builtin actually executes.
`cairo-vm` → `starknet_types_core::hash::Pedersen` → `lambdaworks_crypto`
`PedersenStarkCurve::hash`.

- **Signature**: `fn pedersen(a: [u8; 32], b: [u8; 32]) -> [u8; 32]`
  (big-endian felts; inputs < p).
- **Baseline**: a vendored copy of lambdaworks' `PedersenStarkCurve` and the
  field arithmetic it needs, flattened into `candidate/`.
- **Oracles**: the frozen lambdaworks crate, plus `starknet-crypto` (an
  independent implementation from starknet-rs). Two independent oracles make
  a shared bug less likely to pass.
- **Edge cases**:
  - 0, 1, p−1
  - inputs spanning the 248-bit low/high split
  - equal inputs
  - known vectors from the Starknet docs
- **Workload**: N hashes of seeded random felt pairs; one-time table
  initialisation counted in the score. Memoising results can't pay off,
  because the seed is unknown until run time.
- **Ideas space** (for guides): field multiplication specialised to the
  Stark prime, coordinate systems, window/table size trade-offs, batching
  inversions.

First rubric pass (confirm in M1):
- **Gates**: G1–G4 pass.
- **Expert scarcity**: medium, since EC optimisation is a known craft.
- **Headroom**: unknown. The first M1 measurement is lambdaworks vs
  starknet-crypto speed as a lower bound on what's possible.
- **Laptop validation**: verify yes, score yes-slow.
- **Eval cost**: seconds.
- **Noise**: 5, since the instruction count is deterministic.
- **Gaming resistance**: good, with salted inputs, two oracles and the
  wall-time guard.
- **Fidelity**: good, since it's the code the builtin runs.
- **Value**: medium. Pedersen is widely computed off-chain in Starknet
  (e.g. storage-trie commitments), but should be verified before claiming
  it; it's legacy for new hashing.
- **Hostable here**: yes.

Licences: lambdaworks and types-rs are open source; confirm terms when
vendoring (M2).

## Milestones

- **M0**: provably.fast extraction audit; Rust workspace skeleton; `cb` CLI.
- **M1 Evaluator**: rubric in code, `candidates/*.toml`, `cb import`, static
  ranking site; score the 5-repo calibration set, measure laptop/hostable
  where feasible, adjust weights.
- **M2 Scaffold + preflight**: `cb new`, `cb preflight`, the **Rust kit**
  (§7a), and the Pedersen challenge on it (§7b); then a toy math one.
- **M3 Runner + feed**: `pull_request_target` + `workflow_run` pipeline,
  promotion re-run, results.jsonl, feed JSON, toolkit `builder` source. Run a
  toy end to end with the toolkit as participant, including one deliberately
  malicious PR (edits the workflow, writes score.json, adds a symlink) that
  must be refused.
- **M4**: toy challenges for the remaining families.

## Risks

- **`pull_request_target` is the classic Actions footgun.** Checking out PR
  code with a write token or secrets is a known compromise path. The design
  keeps it read-only and secret-free, and M3's malicious-PR test is mandatory.
  Security review before any ecosystem team gets PR access.
- **2 vCPU / 8 GB runners are small.** Many real candidates won't be hostable
  here. That's fine: the evaluator still rates them, and they go to Yukon.
- **Perf on shared VMs:** paired runs reduce noise but don't remove it. Expect
  `min_improvement` of several percent on GitHub-hosted runners; preflight will
  tell us.
- **Lean/mathlib** builds may exceed 14 GB disk or 60 min without caching.

## Later (out of v1, design shouldn't block these)

- **Website submission**: changes the trust model in §4. The website becomes
  what builds the submission commit, Yukon-style.
- **Self-hosted runners**: pinned hardware for perf; `runner` is already a
  per-challenge field.
- **Automated evaluation**: an agent drafts candidate scores + evidence; a
  human confirms.
- **Assisted expert-scarcity lookup** (advisory).
- **Cairo kit** (differential verifier, steps + builtins counter) and the
  STRK20 transfer-cost challenge designed earlier: privacy-pool Cairo from
  starkware-libs/starknet-privacy; Stwo weight calibration on a self-hosted
  runner; StarkWare buy-in before anything external.

## Open questions

None blocking. Next step: M0.
