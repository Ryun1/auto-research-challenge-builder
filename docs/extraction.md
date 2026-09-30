# Extraction from provably.fast

Source: `../provably.fast` at **`4ad7a2b8`** ("Version deployed scripts and
styles by digest (#368)"). Closes #1. Plan §6.

The plan estimated 17k lines of Rust. The actual size is 27k lines of Rust, plus
about 116k lines of Python in `research/discovery`. Most of that code protects
a long-lived, shared, trusted evaluator host that signs its results. The
builder runs on ephemeral GitHub runners where the VM is the sandbox, so very
little of it carries over. **Almost everything is either skipped or has its idea
ported. Very little is copied as-is.**

Verdicts: **take** = copy and trim · **port** = reimplement the logic, usually
from Python to Rust · **skip** = leave in provably.fast.

## Summary

| Piece | Verdict | Lands in | Est. lines |
|---|---|---|---|
| `crates/artifacts` codec + attestations | skip | n/a | 0 |
| `artifacts` `validate_canonical_path` | take | `cb` path policy | ~40 |
| `artifacts` status taxonomy | take (idea) | results.jsonl schema | n/a |
| `cli/ledger.rs`, `kv.rs` | skip | serde_json append writer instead | ~40 new |
| `replay/containment.rs` (runc/OCI) | skip | Docker flags in workflow YAML | 0 |
| `replay/lib.rs` process boundary | port | kit runner | ~250–300 |
| `replay/lib.rs` `decide_promotion` | port | promotion gate | ~40 |
| `replay/executable_store.rs` | skip; keep the binary-hash idea | runner | ~10 |
| `replay/tests/subprocess.rs` | port | malicious-candidate suite (#13) | ~300 |
| `oracle/oracle_bench.rs` | port the pattern | Rust kit verifier | ~120 |
| `oracle/oracle_worker.rs` | skip; take CPU pinning | wall-time guard | ~50 |
| Python paired timing (`core.py`) | port | wall-time guard | ~150 |
| Python seeded input generator | port | Rust kit inputs | ~40 |
| Python CPU fingerprint | port | runner | ~90 |
| Python quiet-host check | port | preflight | ~40 |
| A/A calibration (`campaign_freeze.py`) | port, add p95 | preflight | ~60 |
| Token/path policy (`public_preflight.py`) | port and extend | `cb preflight`, PR gate | ~250–350 |
| `candidates` `valid_edit_surface` | take | path policy | ~15 |
| `cli/commands/export_site.rs` | skip; rewrite with serde | `cb site` | ~150 new |
| `site/` (vanilla JS) | take and trim | ranking + leaderboard pages | ~350 |
| `.github/workflows/policy-gate.yml` | take the hardening snippets | runner workflows | n/a |
| `strict-profile`, `baseline`, `candidates` | skip (Stwo) | n/a | 0 |

Total carried over: roughly 2k lines, most of it rewritten.

## Evidence and results format

**`crates/artifacts`: skip.** It is a hand-rolled binary codec: magic `PVFA`,
a type tag, a schema version and length-prefixed fields. It includes 12 SHA-256
identity newtypes and ed25519 attestations. It has no dependency on Stwo types,
but the structs are specific to provably.fast (EvaluatorProfile, reservations,
build policy). serde_json plus `sha2` does the same job in about 10 lines.

Taking:
- `validate_canonical_path(s)` (`lib.rs:1716-1756`), for any path that arrives
  from the untrusted side.
- The strict-decode idea, as `#[serde(deny_unknown_fields)]` on score.json.
- The status taxonomy (`lib.rs:448-455`): `promoted`, `valid_not_promoted`,
  `rejected`, **`infra_failed`**, `unsupported`, `stale`. `infra_failed` means
  a runner crash or an unstable timing run never counts as `rejected`.

**`cli/ledger.rs` + `kv.rs`: skip.** It is not JSONL. It is a content-addressed
object store with fsync, read-back verification, `flock` and `O_NOFOLLOW`
hardening. On GitHub Actions, a `concurrency:` group already serialises
writers, and the git commit is what makes the record durable, so an
append-only serde_json writer of about 40 lines is enough.

Taking one invariant: **retain every attempt, including rejected and
infra-failed ones** ("invariant 7", `ledger.rs:1-11`). This matches plan §4's
denominator goal.

## Execution and containment

**`containment.rs` (1,794 lines): skip.** It generates an OCI config for
`runc`: namespaces, cgroup v2 limits, a seccomp deny-list, a read-only root and
exactly two writable output files. The same protection is available from
`docker run --network none --read-only --user … --memory … --pids-limit …` on
ubuntu-latest.

**`lib.rs` process boundary: port (~250–300 lines, `libc` only).** Source is
`run_candidate` (`lib.rs:1000-1500`). It:
- clears the environment, keeping only `LANG`, `LC_ALL` and `TZ`;
- closes stdin;
- calls `setsid` in `pre_exec`;
- sets rlimits for CPU, address space, file size and file handles (NOFILE 64);
- reads output through capped non-blocking pipes (`CappedPipe`);
- waits with `waitid(WNOWAIT)`, kills the process group, then reaps;
- kills the group on drop (`GroupGuard`);
- detects processes that escaped with `setsid`;
- checks the exact set of output files and reads them with bounded,
  no-symlink reads.

Do **not** apply the address-space or CPU rlimits under Valgrind. It needs a
large address space and runs 20–100× slower. Use a wall-clock timeout instead.

**`decide_promotion` (`lib.rs:2198-2349`): port the rule (~40 lines).** It needs
at least 3 attempts per side, with identity and host constant on each side. Any
infrastructure failure beats any rejection. Spread must be ≤ `max_spread_bps`
of the median. The candidate's median must beat the baseline's by at least
`minimum_speedup_bps`. Signing is skipped: the workflow run and its artifacts
provide provenance.

**`executable_store.rs`: skip.** Keep one idea: record the sha256 of the built
binary on the PR run. At promotion, either run that exact artifact, or rebuild
and compare hashes. Rust rebuilds only match byte-for-byte with a pinned
toolchain and `--remap-path-prefix`. Decide which approach in #12.

**Tests to port for #13** come from `tests/subprocess.rs`, which uses the
`shell_worker` pattern (one short sh script per attack). Attacks covered:
- crash, and death by signal;
- a grandchild left running at timeout;
- escape from the process group with `setsid`;
- `yes`-style output floods and stdout/stderr overflow;
- extra, missing or oversized output files;
- hanging forever, and overrunning the deadline.

Also worth taking from `campaign_substitution.rs`: the unstable-repetitions,
same-candidate-on-both-sides and mixed-identity cases.

New cases to add: reading the seed or oracle, writing score.json or the
Callgrind output directly, network access, and `sudo`.

## Perf harness (the Rust kit, §7a)

**A/A calibration: port (`campaign_freeze.py`).** It runs three no-change
sessions, `linux-x86-aa-01..03` (`:3201`). In each one, the baseline is scored
against itself through the full pipeline. The calibration fails if any of these
does:
- the run is unstable;
- the aggregate speedup falls outside `(1/min_speedup, min_speedup)`;
- any per-operation speedup falls outside its own floor band;
- any of the 4 mirrored blocks deviates from 1 by more than
  `cross_block_max_deviation` (`:1786-1860`);
- CPU service is asymmetric between the two labels (`aa_equivalence_floor`,
  `:1048`).

This is a pass/fail check that the *configured* threshold isn't crossed by
noise alone. Our preflight keeps this gate and adds a measured p95 of the A/A
speedups, reported as advisory, since plan §3 says the maker sets
`min_improvement`.

**`oracle_bench.rs`: port the pattern (~120 lines, generic `verify` harness).**
1. Verify at several sizes, then time with a fresh salt per batch and input
   setup left untimed. Then **re-verify using the salts from the timed runs**,
   so the checked inputs are exactly the ones the clock measured.
2. Edge cases: boundary values {0, 1, 2, p−2, p−1}, crossed with salted lanes,
   checked pairwise.
3. Pass values in and out as raw words, so no candidate conversion code sits
   between the result and the assert.
4. Mismatch reports name the kernel, size, salt and index.

**Paired timing (`core.py`): port (~150 lines).**
- **Order.** Legacy mode alternates `(B,C)` / `(C,B)` by pair parity, and both
  sides of a pair get the same inputs (`:3433-3444`). v3 uses mirrored blocks
  `BCCB, CBBC, CBBC, BCCB` with one seed per half-block, giving 8 samples per
  side (`:3756-3784`). Start with mirrored blocks.
- **Dispersion.** `rel_MAD = median(|x − median(x)|) / median(x)`, per side,
  with a cap of 0.08 (`:2173-2177`).
- **Statistic (v3).** Per block, `r_b = median(C_b) / median(B_b)`. Then
  `g = (∏ r_b)^(1/4)` and `speedup = 1/g`. Stability also requires
  `max_b |r_b/g − 1| ≤ 0.08` (`:2722-2745`).
- **Outcome.** An unstable run is `infra_failed`, not `rejected`
  (`:7822-7839`).
- **Kit adaptation.** In the kit this is the **guard**, not the score. It only
  has to show that the candidate is "not slower beyond noise".

**Inputs: port (~40 lines, `sha3`).** Input bytes are
`SHAKE256(domain ‖ 0 ‖ size ‖ 0 ‖ salt)`, with a domain-separated
resampling branch for rejected values (`pvfast_lab/oracle_worker.py:266-313`).

Salts come from `urandom` and are drawn **after the build**, never revealed
beforehand. They are recorded afterwards as canonical decimal strings. Warmup
salts are drawn separately from measurement salts (`core.py:5907`,
`:6474-6533`).

**CPU fingerprint: port (~90 lines).** Source is `current_host_policy`
(`core.py:2370-2415`).
- Linux: the `/proc/cpuinfo` model name, sorted flags, physical-core count from
  unique `(physical id, core id)` pairs, and the DMI product name.
- macOS: `sysctl machdep.cpu.brand_string` and related fields.
- Both: logical CPU count and kernel release.

Rust's `host_facts` does not record the CPU model, so use the Python version.

**Quiet-host check: port (~40 lines).** A preflight passes when the 1-minute
load is at or below the limit and no rustc or cargo processes are running
(`core.py:4829-4870`).

**`oracle_worker.rs`: skip.** It is a framed stdin/stdout worker protocol, which
the kit doesn't need because `cb` links the candidate and oracle crates into one
process. Take only the per-CPU pinning with `sched_getcpu` verification (~50
lines, Linux) for the wall-time guard.

**Skip:** the cgroup CPU-service floor, `oracle_containment.py`,
`evaluator_dispatcher.py` and the evidence-replay validators.

## Policy check (§3 path policy, §7a kit policy)

**`research/discovery/pvfast_lab/public_preflight.py` (886 lines): port and
extend (~250–350 lines Rust).** None of this lives in `crates/`.

What it does:
- **Diff gate.** Runs `git diff --name-status -z --no-renames`. Every entry must
  be `M` and inside the allowed paths, so new, deleted, renamed and
  mode-changed files all fail (`:667-684`). The patch must be text, UTF-8 and
  LF-only, and at most 256 KiB (`:699-727`).
- **Implicit blocking.** Only listed `.rs` files may change, so `Cargo.toml`,
  `build.rs` and new dependencies are blocked by the path rule, not by the token
  rule.
- **Lexer.** `_strip_rust_noncode` (`:327-427`) blanks out comments, strings,
  raw strings and char literals while keeping column positions.
- **Token rule.** Counts `unsafe` and `target_feature(` matches, and allows zero
  **net-new** matches per file compared with the base.

Gaps to close in our port:
- Add `asm!`, `global_asm!`, `include!`, `include_bytes!`, `include_str!`,
  `env!`, `#[path]`, `extern`, `#[no_mangle]`, `#[link]`, `macro_rules!`.
  None of these are checked today.
- Decide on `cfg(target_feature)` and `cfg!(target_feature)`. Neither is
  caught today, because the regex requires `(` straight after the token.
- The lexer does not expand macros. Add `#![forbid(unsafe_code)]` from the
  harness as a backstop.
- Also block gitlinks (mode 160000) and symlinks explicitly.

Also taking: `valid_edit_surface` (`crates/candidates/src/lib.rs:431-445`), about
15 lines that reject `..`, `\` and duplicate paths.

## Static site

**`export_site.rs`: skip.** It is coupled to the provably.fast Ledger and
PromotionDecision types. Rewrite it with serde over our own results files,
writing to a temp dir and renaming into place (~150 lines).

**`site/` (index.html, site.js, site.css; ~830 lines of vanilla JS/CSS/HTML):
take and trim** to the ranking and leaderboard pages (~350 lines). It fetches
JSON, checks its shape strictly and renders using `textContent` only, so there
is no XSS surface. The shape check is `assertCampaign` in `site.js`.

Do not confuse it with `research/discovery/site/`, which is a separate, much
larger app. Skip that one.

## CI

`.github/workflows/policy-gate.yml` runs on `pull_request`, `push` and
`workflow_dispatch`. There is no `pull_request_target` or `workflow_run`
precedent. Production scoring runs off GitHub, on an offline evaluator host.

Taking these snippets:
- `permissions: contents: read` at the top level;
- actions pinned to a SHA, with `persist-credentials: false`;
- an explicit checkout of `pull_request.head.sha`, followed by a
  `git rev-parse` exact-revision check (`:22-33`);
- `concurrency` with cancel-in-progress;
- `--ignore-scripts`;
- per-phase timeouts (`scripts/ci/run_with_timeout.py`).

## Stays put

`crates/strict-profile`, `crates/baseline` and `crates/candidates` all depend
on `stwo`, with pinned revisions, workloads and PCS parameters. Confirmed
Stwo-specific. The only piece taken from them is `valid_edit_surface`.

## Findings that change the plan

1. **§4 runs untrusted code in the same job as the judge.** GitHub-hosted
   runners give the `runner` user passwordless sudo. Inside one job, candidate
   code can therefore read the seed and oracle, and can write score.json. The
   policy gate runs before the build and blocks `build.rs` and proc macros, so
   build-time execution is covered. Run-time is not: the candidate runs inside
   the harness process. Proposed changes, for #12:
   - Run the policy gate and build in one job. Upload the binary and its hash.
   - Run a **separate** job that draws the seed, runs verify and score, and
     runs the candidate as an unprivileged user or in
     `docker --network none --read-only`.
   - The `workflow_run` commenter trusts score.json only as a claim. The
     promotion re-run on `main` is the only thing that writes
     results.jsonl.
   - Set explicit `permissions: contents: read` on the promotion job.
2. **The A/A calibration ports as a gate** (#8). provably.fast runs 3
   no-change sessions that must stay inside the threshold band. We add the
   advisory p95 on top.
3. **Status taxonomy.** Add `infra_failed` (and `valid_not_promoted`) to the
   results schema (#4, #12).

## Open questions (none block M0)

- A/A preflight: how many pairs, and which p95 method (nearest-rank or
  interpolated)? A default of 20 blocks with nearest-rank is proposed. Decide
  in #8.
- Should `[perf] pairs` in `challenge.toml` set the mirrored-block count? v3
  fixes it at 4 blocks (8 samples per side). Proposal: `blocks`, default 4.
- Promotion: run the PR's exact binary, or rebuild and compare hashes? Decide
  in #12.
