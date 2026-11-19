# auto-research-challenge-builder

The organiser side of auto-research. It has two jobs:

1. **Evaluate** candidate problems: would this make a good autoresearch
   challenge? Candidates are scored against a versioned rubric (hard gates,
   then weighted 1–5 criteria).
2. **Build and run** simple challenges on GitHub-hosted runners. That covers
   the spec, verifier, baseline, preflight checks, PR-driven scoring, and a
   results feed that auto-research-toolkit reads natively.

A challenge that outgrows GitHub-hosted runners (GPU, large RAM, pinned
hardware) moves to Yukon or a dedicated evaluator. It can still be rated here.

The design and its reasoning are in [PLAN.md](PLAN.md). The other docs are
indexed in [docs/](docs/README.md).

## Layout

| Path | What it is |
|---|---|
| `crates/cb/` | the `cb` CLI (Rust): scaffold, preflight, gate, score, record, feed, rank |
| `candidates/` | one `<slug>.toml` per evaluated problem (internal, never published) |
| `challenges/<slug>/` | one directory per challenge: `challenge.toml`, judge, baseline, editable `candidate/`, `results.jsonl` |
| `kits/rust/` | template copied by `cb new --kit rust` (harness, oracle, reference, broken variants) |
| `.claude/skills/` | the `judging-challenge-candidates` skill and research rubric |
| `.github/workflows/` | the submission → comment → promote pipeline, plus CI, preflight, warm, site |
| `scripts/` | owner tooling: deploy-key setup, audit of `main`, malicious-PR suite |
| `docs/` | pipeline, calibration and extraction notes; design specs and plans |

## Challenges

| Slug | Family | Status | Editable |
|---|---|---|---|
| `ledger-cbor` | perf (Haskell) | open | `candidate/` (cardano-ledger packages) |
| `pedersen` | perf (Rust kit) | closed | `candidate/src` |
| `golomb-14` | math | closed | `candidate/ruler.txt` |
| `lean-sum` | proof (Lean) | draft | `candidate/Submission.lean` |
| `sortnet-8` | resource | draft | `candidate/network.txt` |

`challenge.toml` is the source of truth for status and editable paths. Each
challenge has a `SUBMITTING.md` for participants and a `THREAT_MODEL.md`
covering how a wrong answer might pass the verifier. Research challenges also
have a `DIRECTIONS.md`, kept by the maintainer: what has been tried and what is
open. An optional `context.toml` says why the problem matters (the system
around it, how often it runs, what an improvement is worth, sources); it is
published on the site, so write it to be public. `cb new` writes a commented
stub.

Results are published at https://ryun1.github.io/auto-research-challenges/,
with a feed per challenge at `feeds/<slug>.json`.

## Getting started

Requires the Rust toolchain pinned in `rust-toolchain.toml`. Scoring perf
challenges needs Valgrind (Linux). On a Mac, use the kit's `bench/run-docker`.

```sh
cargo build --release          # builds target/release/cb
cargo test                     # unit + integration tests
cargo run -- --help            # list subcommands
```

### Common commands

| Command | Does |
|---|---|
| `cb rank` | score every `candidates/*.toml` against the rubric |
| `cb site` | render the candidate ranking to `target/site` (internal: never deploy) |
| `cb import <benchmark.json> --url <repo>` | pre-fill a candidate from a Yukon-style repo |
| `cb new <slug> --family <perf\|resource\|math\|proof> [--kit rust]` | scaffold `challenges/<slug>/` |
| `cb preflight <slug>` | launch checks: baseline passes, broken variants fail, noise, determinism, peaks |
| `cb score <slug> --baseline` | build, verify and score the baseline locally |
| `cb score <slug>` | same for the checkout's editable files |

`gate`, `record`, `comment`, `feed` and `hs-policy` are mainly run by the
workflows. See `cb <command> --help`.

## Evaluating a candidate

1. Start from `cb import` or copy an existing file in `candidates/`.
2. Fill in every gate and score, each with `evidence` (a link or a
   measurement). See [candidates/README.md](candidates/README.md) for the
   format.
3. Run `cb rank` and open a PR.

In Claude Code, the `judging-challenge-candidates` skill walks through this.
Open-ended research candidates go in `candidates/research/<slug>.md` instead
and are judged with the research rubric.

## Building a challenge

1. `cb new <slug> --family perf --kit rust`. For Rust perf challenges the kit
   README ([kits/rust/README.md](kits/rust/README.md)) lists what to replace.
2. Set `challenge.toml`: editable paths, commands (`setup`, `build`,
   `verify`, `score`), objective and minimum improvement.
3. Write the three broken variants in `preflight/broken/`. Each must fail
   verify.
4. Answer `THREAT_MODEL.md`.
5. `cb preflight <slug>` until it passes, then open a PR with
   `status = "draft"`. Flip it to `open` to launch.

## How submissions are scored

A participant opens a PR that touches only a challenge's editable paths.

- **Submission** (`pull_request_target`, running `main`'s workflow) gates the
  diff, exports the editable files as data, then builds, verifies and scores
  them in a sandbox. The result is only a claim.
- **Comment** records a `stage: "pr"` attempt and posts the score on the PR.
- After merge, **Promote** re-scores twice with fresh seeds. A new record is
  appended to `results.jsonl`; anything else is recorded and the candidate
  is reverted to the record.

The judge always comes from `main`, so a PR cannot rewrite its own scoring.
The trust model, concurrency rules and protection of `main` are covered in
[docs/pipeline.md](docs/pipeline.md).

Every workflow run is billed, so the manual workflows (Preflight, Warm,
Calibrate) only run on demand.
