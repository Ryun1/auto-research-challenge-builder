# Research challenges: design

Issue #90. Decided 2026-11-19 with the maintainer. This document is the
contract the implementation follows; where it differs from the issue, this
document wins and says why.

## What a research challenge is

A second challenge type beside today's **simple** challenges (one objective,
a fixed editable surface, a record). A research challenge is an open search
space with many axes, and it is not a race:

- Participants choose which axes to push. Nothing ranks them.
- A submission is a whole directory: edits of an earlier submission, or a
  replacement codebase, as long as it meets a fixed **contract**.
- Each submission **self-declares** its novelty: a new direction `<name>`, or
  "builds on `<submission>`". Novelty is never judged.
- Every submission that passes is measured on every axis and **recorded**.
  There is no promotion, no record and no minimum improvement.
- The site shows a **map**: axis-vs-axis trade-off plots coloured by
  direction, and a lineage tree.

Directions that change the protocol, where nothing independent can check
soundness yet, are **deferred contracts**: listed with what would admit them,
never measured.

## Decisions (2026-11-19)

| # | Question | Decision |
|---|---|---|
| 1 | Toolchains and dependencies | Anything that builds **offline** on the challenge's pinned runner image plus what its trusted `setup.sh` installs. Dependencies are vendored inside the submission (under its size cap) or come from a frozen set the challenge ships. `[submissions] toolchains` documents what is available. |
| 2 | Pilot | **`coin-select`**: Cardano UTxO coin selection (CIP-2), see below. |
| 3 | Toolkit feed | Site only at first. cb writes a documented schema-2 research feed; the toolkit adapter is a follow-up. |
| 4 | Who admits submissions | **Auto-merge on pass.** A PR whose gate passes and whose PR-stage measurement is `valid` is landed on main by the trusted Comment workflow, then measured twice on main. |
| 5 | Taxonomy edits | Separate maintainer PR. Submission PRs may only add one submission dir. |
| — | Judge shape | **One judge per contract** that runs the participant, checks the properties and measures the axes on the same outputs. (The issue had a verifier run once and a harness run n times; a submission could then behave in the verify pass and cheat in the measure passes.) |
| — | Scope | M0–M8 (launch the pilot). M9 (Stwo) is a follow-up. |

## 1. Spec: `challenge.toml`

`[challenge] type` is `"simple"` (the default; every existing challenge) or
`"research"`. Simple challenges parse exactly as today. A research challenge
has its own schema; `Spec::load` refuses it with a message naming the type,
so no simple code path runs on one by accident. `challenge::load_any` returns
`Kind::Simple(Spec)` or `Kind::Research(ResearchSpec)`.

```toml
[challenge]
slug   = "coin-select"
type   = "research"
status = "draft"              # draft | open | closed
runner = "ubuntu-24.04"       # pinned image

[commands]
setup = "./setup.sh"          # trusted, before any submission byte runs; may use the network

[submissions]
max_bytes  = 4194304          # per submission dir
toolchains = ["python3 (ubuntu-24.04 image)", "rust stable via rustup (ubuntu-24.04 image)"]
writable   = ["~/.cargo"]     # build-sandbox overlays (writes vanish after the build)

[limits]                      # per participant process TREE, enforced by `cb participant`
cpu_secs   = 60
memory_mib = 1024             # whole tree: cgroup v2 memory.max in the sandbox
processes  = 32               # whole tree: cgroup v2 pids.max in the sandbox
file_mib   = 64               # largest file it may write, and its private /tmp
wall_secs  = 120
build_secs = 1800             # the build step (build sandbox); a hang is `rejected: build: timed out`

[[contracts]]
id           = "wallet@1"     # <name>@<version>
status       = "open"         # open | deferred | closed
judge        = "contracts/wallet/judge"       # frozen; see §4
interface    = "contracts/wallet/INTERFACE.md"
choice_space = "contracts/wallet/CHOICES.md"

[[contracts]]
id          = "wallet-multiasset@0"
status      = "deferred"
admits_when = "a multi-asset size and min-ada model, checked against cardano-ledger"

[[properties]]
id       = "valid-txs"
check    = "mechanical"       # mechanical | mechanical-conjectured | declared
contract = "wallet@1"

[[axes]]
id        = "fees"
unit      = "ADA"
direction = "minimise"        # minimise | maximise; never combined
repeats   = 1                 # judge runs that sample this axis (same seed)
tolerance = 0.05              # relative; the two runs on main must agree within it
tolerance_abs = 0.0           # absolute floor, in the axis unit, for axes near zero (default 0)
# contracts = ["wallet@1"]    # optional: only these contracts measure it

[taxonomy]
file = "taxonomy.toml"
```

Validation (all refusals are errors from `ResearchSpec::parse`):

- slug as today; `type = "research"`; status `draft|open|closed`; runner pinned
  (not empty, not `-latest`); `commands.setup` not empty.
- `submissions.max_bytes > 0`; `limits.*` all `> 0`.
- `contracts` not empty; ids unique and shaped `<slug>@<digits>`; at least one
  contract is not deferred. `open`/`closed` need `judge` and `interface`
  (canonical relative paths); `deferred` needs a non-empty `admits_when`.
- `properties`: ids unique slugs; `check` one of the three; `contract` exists.
- `axes` not empty; ids unique, `[a-z0-9_-]+`; unit not empty; direction
  valid; `1 <= repeats <= 20`; `0 <= tolerance < 1`; `tolerance_abs` finite and `>= 0`; `contracts` entries exist.
- `taxonomy.file` canonical; the file loads (below).

`taxonomy.toml`, after chemistry-walk:

```toml
version = 1
[[axes]]
id = "strategy"
definition = "How inputs are chosen"
values = [ { id = "largest-first", definition = "..." }, { id = "random-improve", definition = "..." } ]
[[rules]]
text = "no-change requires an exact-match search (branch-and-bound or exhaustive)"
```

Axis and value ids are unique slugs. A submission may name a value that is not
listed; the site marks it **proposed** until a maintainer adds it in a PR.

## 2. Submissions: one immutable directory each

`challenges/<slug>/submissions/<id>/`:

```
submission.toml   # the declaration: data, parsed by trusted Rust
build.sh          # mode 100755; runs in the build sandbox, offline
...               # anything else: a whole codebase is fine
```

```toml
id        = "random-improve"  # equals the dir name; a slug
contract  = "wallet@1"
novelty   = "new"             # new | builds-on
direction = "random-improve"  # new: required, a slug not already used by an accepted submission
parent    = ""                # builds-on: required, an accepted submission id; direction is then inherited
taxonomy  = { strategy = "random-improve", change = "payment-shaped" }
summary   = "CIP-2 random-improve with payment-shaped change"
claims    = [ { property = "change-privacy", argument = "..." } ]  # declared properties only
model     = "claude-opus-5-5" # optional attribution
```

Caps: `summary` 500 chars, each `argument` 2000, `model` 100, at most 16
taxonomy keys and 16 claims. Taxonomy values are slugs.

Building on a parent means copying the parent's dir and editing it. Merged
submissions are immutable: a PR may only **add** one new dir. The baseline is
`submissions/baseline/` (novelty `new`), added by the maintainer with the
challenge; it is the root of the lineage tree.

## 3. Gate (`cb gate`, research branch)

`pipeline::gate` finds the slug as today, loads the type from the checkout
(main), and for a research challenge calls `research::gate::gate`. Every
refusal reason is collected. Rules:

- Every changed path (`git diff --raw -z --no-renames base..head` from the
  merge-base) is status `A`, and lies under `challenges/<slug>/submissions/<id>/`
  for one `<id>`. Anything else: `"<path>: a research submission may only add
  files under challenges/<slug>/submissions/<id>/"`.
- `submissions/<id>` exists neither at the merge-base nor at `base` (main's tip).
- Challenge `status = "open"`.
- The head tree of the dir: modes `100644`/`100755` only (symlinks and gitlinks
  refused), canonical paths, bytes `<= submissions.max_bytes`;
  `submission.toml` (100644) and `build.sh` (100755) present.
- `submission.toml` (read with `git show`, capped at 64 KiB) parses and
  validates: id equals the dir name; contract exists and is `open`; novelty
  and parent/direction consistent; taxonomy keys known; claims only name
  `declared` properties of its contract.
- Lineage, against the checkout's `results.jsonl`: a `builds-on` parent must be
  accepted (its latest accept/baseline row has status `accepted`), else
  `"parent <p> is not accepted on main yet (a just-merged parent is measured
  first: re-run this PR after its accept run records it)"`. A `new` direction
  must not already be the direction of an accepted submission.
- No Rust or Haskell lexical policy for research (sandbox and contract carry
  the trust).
- On success the dir is exported with `git archive` to
  `<out>/challenges/<slug>/submissions/<id>/` and `gate.json` gains
  `"type": "research"`, `"submission": "<id>"`, `"declaration": {...}` (the
  parsed toml as JSON). Simple gates gain `"type": "simple"`.

## 4. Judge: `cb measure` and `cb participant`

`cb measure <slug> (--submission <gate out> | --id <id>) [--sandbox] [--out measure.json]`

1. Copy the challenge to a scratch workspace `ws` (as `score` does). Place the
   submission at `ws/submissions/<id>` (from the gate export, or already there
   on main).
2. Run `commands.setup` as the current user in `ws` (trusted).
3. Build: `cd submissions/<id> && ./build.sh`. With `--sandbox` this is
   **mandatory** `Mode::Build` with `dir = ws`, `outputs = ["submissions/<id>"]`,
   `writable = submissions.writable`: no network, home/`/opt`/`/usr/local`/the
   whole workspace read-only except the submission dir. If the sandbox is
   unavailable under `--sandbox`: `infra_failed`. Without `--sandbox` (laptop,
   tests) the build runs plain. Build failure: `rejected`, reason `build: ...`.
4. `chmod -R a+rX ws`. Draw the seed **now**.
5. Run the contract's `judge` (as the current user: it is trusted code from
   main) `n = max(repeats)` times over the axes the contract measures, all with
   the same seed, cwd `ws`, env:
   `CB_SEED`, `CB_REPEAT` (0-based), `CB_CONTRACT`, `CB_SUBMISSION_DIR`
   (absolute), `CB_BIN` (absolute path of this `cb`), `CB_LIMITS` (JSON of
   `[limits]`), `CB_SANDBOX` (`1` or `0`).
6. The judge prints one JSON object (capped at 64 KiB):
   `{"ok": bool, "inputs": "<digest of the generated inputs>",
     "checks": [{"property": id, "ok": bool, "detail": str}],
     "axes": {"<axis>": number}, "infra": bool?}`.
   `cb` validates it: unparseable, `infra: true`, a missing `inputs`, a check
   for an unknown property, or a missing mechanical/mechanical-conjectured
   property is `infra_failed` (a judge bug). A repeat passes when `ok` and every
   check is ok; then every axis the contract measures must be a finite number
   (else `infra_failed`). Any failing repeat: `rejected`, reason
   `check <property>: <detail>` (or `judge: not ok`); later repeats are
   skipped. Repeats that differ in `inputs`: `infra_failed`; in verdicts:
   `rejected` ("nondeterministic verdicts on the same seed").
7. Axis `a` takes the first `a.repeats` samples: `median`, `min`, `max`, `n`,
   `samples`.
8. `measure.json`:
   `{schema: 2, type: "research", slug, submission, contract, seed (string),
     host, host_policy, sandboxed, build: {ok, secs}, inputs,
     checks: [{property, check, ok, detail}], axes: {id: {median, min, max, n, samples}},
     status: "valid"|"rejected"|"infra_failed", reason}`.
   Only the two runs on main turn `valid` into `accepted`.

**The participant** runs only through `"$CB_BIN" participant [--report <file>] -- <cmd> [args...]`,
which the judge calls; stdin, stdout and stderr pass through (so the judge can
speak a line protocol). It reads `CB_LIMITS` and `CB_SANDBOX` from its env.

- `CB_SANDBOX=1` (Linux, passwordless sudo): `sudo -n unshare --mount --net
  --pid --ipc --uts --fork --kill-child --mount-proc` then, as root inside: bind
  the cwd read-only onto `/mnt`; fresh tmpfs (size `file_mib`) on `/tmp`,
  `/var/tmp`, `/dev/shm`, `/run/lock`, `/var/crash`, `/dev/mqueue` and
  `/run/user` (each if present), so no world-writable host path survives
  between invocations; `cd /mnt`; join a per-invocation cgroup v2 with
  `memory.max` = `memory_mib` and `pids.max` = `processes` (limits on the whole
  process tree); `timeout -k 2 <wall_secs>` + `prlimit --cpu --as --nproc
  --fsize` (per-process backstops) + `setpriv` to uid/gid 65534 with
  `--clear-groups --no-new-privs` and `env -i PATH=/usr/local/bin:/usr/bin:/bin HOME=/tmp`.
  So the participant runs as `nobody`, with no network, private IPC (SysV
  shm/semaphores, POSIX mqueues) and hostname, cannot see the judge's
  processes or temp files, cannot write its own dir, and keeps no state between
  invocations. Commands must use paths relative to the submission dir.
- `CB_SANDBOX=0`: runs as the current user in its own process group with
  `RLIMIT_CPU` and `RLIMIT_FSIZE` (plus `RLIMIT_AS` on Linux), killed at
  `wall_secs`. For laptops and tests only.
- Measured from outside the participant: wall time and CPU time (`wait4`
  rusage of the whole chain) and, in the sandbox, peak memory as the cgroup's
  `memory.peak` (the whole tree, including page cache; with `CB_SANDBOX=0`
  the largest descendant's max RSS). The report (`--report`):
  `{"status": "ok"|"failed"|"timeout"|"cpu_limit"|"memory_limit"|"infra",
    "exit_code": int|null, "signal": int|null, "wall_secs", "cpu_secs",
    "peak_rss_mib"}`.
  `cb participant` exits with the participant's code (`128+signal` if
  killed, `124` on timeout). Exceeding a limit (including the tree hitting
  `memory.max`: `memory_limit`) is the submission's failure: the judge reports a
  failing check, so it is `rejected`, never `infra_failed`. `infra` (exit
  `125`) means the sandbox setup itself failed: the judge reports
  `infra: true`, so the run is `infra_failed`. `cb measure` also probes the
  participant sandbox once before the judge and returns `infra_failed` if the
  probe fails.

## 5. Trust

- The judge is always main's copy (`pull_request_target`, unchanged);
  submission bytes are data until the sandboxed build.
- Judge (trusted) and participant (untrusted) are separate processes and
  separate users. The judge keeps the seed and reference results; the
  participant never sees them (env cleared, private `/tmp`, own pid namespace).
- A judge must measure and check **the same outputs**, and never read numbers
  the participant reports about itself. Preflight's `self-report` variant
  tests that.
- Research `THREAT_MODEL.md` template questions: how could a submission fake
  an axis; pass the checks while breaking a property; read the seed or the
  reference; overfit fixed inputs; persist state between runs; what does a
  large outlier trigger.

## 6. Recording and workflows

`cb record <slug> --stage pr|accept|baseline --run ... [--declaration <submission.toml>]`
(default declaration: `<root>/<slug>/submissions/<run's submission>/submission.toml`).

- `pr`: one run; status `valid` / `rejected` / `infra_failed` as measured.
- `accept` / `baseline`: two runs. Any `infra_failed` → `infra_failed`; any
  `rejected` → `rejected` (reasons joined); verdicts differ → `infra_failed`;
  for each axis `|a - b| > max(tolerance * max(|a|, |b|), tolerance_abs)`
  (exact equality when both are 0) → `infra_failed` naming the axis; else
  `accepted`. The absolute floor keeps axes near zero from failing on noise.
  `baseline` requires id `baseline` and refuses a second accepted baseline;
  `accept` refuses id `baseline`.
- Row (`results.jsonl`, one file per challenge, `merge=union` as today):
  `{schema: 2, type: "research", at, stage, status, reason, slug, submission,
    commit, by, model, contract, novelty, direction, parent, taxonomy,
    proposed: [keys whose value is not in the taxonomy], summary, claims,
    axes: {id: {median, min, max, n}} | null, checks: [...],
    runs: [{seed, host, host_policy, sandboxed, status, reason, inputs, axes}]}`.
  `direction` is resolved: a `builds-on` row carries its parent's direction.
  Axes combine every run's samples. `record` never computes a bar.
- `cb comment` renders a research comment when `gate.type == "research"`:
  declared novelty/direction/parent/contract/summary, the checks table
  (mechanical vs *declared, unverified* claims), the axes table, and the
  footer: "This measurement is a claim from the PR run. If it is valid the PR
  is landed on main automatically; two clean runs on `main` then record it on
  the map. There is no record to beat: every submission that passes is recorded."

Workflows:

- **submission.yml**: gate outputs gain `type`; the score job runs
  `cb measure "$SLUG" --submission gate/submission --sandbox --out score.json`
  for research (same artifact name and file, so Comment is unchanged in shape).
- **comment.yml**: research PRs record with `--declaration` from the gate
  export. If the entry is `valid`: **land**. Fetch the PR head by the event's
  `head_sha`, check the PR is still open at that sha, re-run `cb gate --base
  HEAD --head $HEAD_SHA`, copy exactly `challenges/<slug>/submissions/<id>` from
  that sha onto main, commit `submission: <slug>/<id> (PR #N)` with trailers
  `Submitted-by: <login>` and `PR: <N>`, push it **in its own push** (no
  `[skip actions]`, so Accept runs), and close the PR with a comment naming the
  landed commit. The results commit is pushed first with `[skip actions]`.
- **accept.yml** (new): on push to main under `challenges/*/submissions/**`,
  or dispatch (`slug`, `submission`; `submission = baseline` is the baseline
  stage). Detect re-runs `cb gate --base before --head after` and proceeds only
  for `type = research`; two `cb measure --id <id> --sandbox` runs (an
  `infra_failed` run is measured once more, as Promote does); record with
  `--stage accept|baseline`; if `rejected` at the accept stage the follow-up
  commit removes `challenges/<slug>/submissions/<id>` (`[skip actions]`). The
  record job's concurrency group is per slug. `by` comes from the
  `Submitted-by` trailer, else the actor.
  - **No concurrency group** on the record job: a group cancels the pending
    job when another queues, which would lose records. Records are union-merged
    appends and a removal touches only its own directory, so the `pull --rebase`
    retry loop orders them.
  - **Record runs even if a measure job failed or timed out** (`always()`): the
    missing `measure.json` is replaced by an `infra_failed` stand-in, so the
    record still happens (infra_failed does not remove). The run job's timeout
    (90 min) is well above the build's own `build_secs` (30 min).
  - A push that is a Comment landing (`submission: ` subject plus a `PR:`
    trailer) which the gate now refuses fails Accept loudly instead of exiting
    quietly.
  - Dispatch refuses to re-measure an id whose latest accept/baseline row is
    `accepted`, unless the `force` input is set; a forced re-measure never
    removes the directory.
  - Comment's landing push re-checks after every rebase that only new files
    under the PR's own directory differ from main and that main does not hold
    the directory (a same-id submission landing meanwhile fails the job).
- **promote.yml**: detect skips a `type = research` gate result.
- **ci.yml**: `paths-ignore` adds `challenges/*/submissions/**` for PRs; pushes
  ignore `challenges/*/submissions/**` and `challenges/*/results.jsonl`.
- **site.yml**: also runs after Accept; its push trigger ignores
  `challenges/*/submissions/**` (landing pushes).
- **scripts/audit-main.sh**: deploy-key pushes may also add or remove files
  under `challenges/<slug>/submissions/<id>/`, from Comment or Accept runs.

## 7. Preflight (`cb preflight`, research branch)

`preflight::run_all` dispatches to `research::preflight::run_all`, which fills
the same `Report`. Checks:

- **threat model**: answered (same gap rules) against the research template.
- **contracts**: every open contract's judge exists and is executable, its
  interface (and choice space if set) exist; deferred contracts are listed
  with their `admits_when` (Pass, informational).
- **taxonomy**: loads; every submission in `submissions/` names only known keys.
- **build sandbox**: the probe (home and workspace read-only, no sudo, no
  network, the submission dir writable, cache writes discarded) passes. On a
  host without it: Fail if `CI` is set, Warn otherwise.
- **baseline**: `submissions/baseline` declares id `baseline`, novelty `new`,
  fits `max_bytes`; every open contract is declared by some submission in
  `submissions/`, and each such submission measures `valid`.
- **determinism / salted inputs / axis noise**: the baseline measured three
  times: seeds A, A, B. Same seed: same `inputs` and verdicts. Seed B:
  different `inputs`, still `valid`. Per axis, the relative spread of the
  three medians (`(max - min) / |median|`) must be `<= tolerance`, unless the absolute range of the three medians is
  `<= tolerance_abs` (the two main runs use independent seeds); the detail suggests `ceil(2 * spread, 0.01)`.
- **broken variants**: `preflight/broken/<name>/` are whole submissions
  measured with seed A. Required: `off-by-one`, `partial`, `constant`,
  `self-report`; others may be added. Each must build, then: `self-report` must
  be `valid` with every axis within `max(tolerance, 2 * observed spread)` (relative) or `tolerance_abs` (absolute) of
  the baseline (its self-reported numbers were ignored); every other variant
  must be `rejected`. A variant may carry an `EXPECT` file: its first non-empty
  line is `valid`, or a substring the rejection reason must contain (so
  `off-by-one` can insist on `fee below min_fee`, and `deep-json` on
  `unparseable`, rather than any rejection passing).
- **hostable**: build + measure minutes, max RSS, disk for the baseline, as today.

## 8. Feed and site

`cb feed` writes, for a research challenge, `<slug>.json` with
`feed_schema_version: 2`:

```
{feed_schema_version: 2, type: "research", slug, url, generated_at, status, runner,
 axes: [{id, unit, direction, tolerance, tolerance_abs, repeats, contracts}],
 contracts: [{id, status, admits_when, interface, choice_space}],
 properties: [{id, check, contract}],
 taxonomy: {version, axes: [{id, definition, values: [{id, definition, count}]}], rules: [text]},
 submissions: [{id, baseline, contract, novelty, direction, parent, taxonomy, proposed,
                summary, by, model, at, commit, axes, checks, claims}],
 directions: [{name, root, submissions: [id]}],
 attempts: {total, valid, accepted, rejected, infra_failed}}
```

`submissions` holds the latest accepted row per id (accept or baseline stage).
The site (`pages.rs` → `challenges.json`, rendered by `app.js`) keeps simple
pages unchanged and adds a research view:

- **Trade-off plot** (inline SVG): X and Y selects over the declared axes;
  one point per submission coloured by direction, min–max error bars, the
  baseline marked; a "non-dominated" highlight, off by default.
- **Direction tree**: baseline → new-direction roots → builds-on chains; each
  node links to its submission.
- **Submission panel**: summary, taxonomy tuple with proposed values flagged,
  contract, checks (declared claims as *unverified*), axes table, and for
  builds-on a diff against the parent (`diffs/<slug>/<id>.patch`, from
  `git diff --no-index` of the two dirs, capped at 512 KiB); for new, the file
  listing and README excerpt.
- **Taxonomy panel**: axes of variation with how many submissions use each value.
- **Deferred contracts**: listed with `admits_when`, as an invitation.

## 9. Scaffold and docs

`cb new <slug> --type research` writes `challenge.toml` (§1, one open contract
`main@1`, one deferred example, one property, two axes), `taxonomy.toml`,
`setup.sh`, `contracts/main/{judge,INTERFACE.md,CHOICES.md}` (the judge fails
closed), `harness/README.md` (shared judge code), `submissions/baseline/{submission.toml,build.sh,README.md}`,
`preflight/broken/{off-by-one,partial,constant,self-report}/README.md`,
`THREAT_MODEL.md` (research template), `SUBMITTING.md`, `PROMPT.md`, `README.md`.
`--family` becomes optional (required for simple). `PLAN.md` gains the
research type; `candidates/README.md` and the skill link a research candidate
to its challenge once launched.

## 10. The pilot: `coin-select`

Cardano wallets choose which UTxOs to spend for each payment (CIP-2). Fewer
inputs now means lower fees but leaves dust that is expensive later; online
heuristics are fast, exact searches are not. There is no single winner, which
is what a map is for.

**Contract `wallet@1`** (`contracts/wallet/`, judge in Python 3, stdlib only).
`build.sh` must leave an executable `./select` in the submission dir. The judge
runs it once per scenario through `cb participant`, speaking JSON lines:

- judge → participant, no reply:
  `{"type":"params","min_fee_a":44,"min_fee_b":155381,"coins_per_utxo_byte":4310,"max_tx_size":16384,"size":{"base":60,"input":140,"output":65}}`
  and `{"type":"deposit","utxos":[{"id":"d3#0","lovelace":12000000}]}`
- judge → participant: `{"type":"pay","id":"p7","outputs":[{"lovelace":5000000}]}`;
  reply within 10 s: `{"inputs":["d3#0"],"change":[6800000],"fee":200000}`, or `{"refuse":true}`.
  Change output k becomes wallet UTxO `"<pay id>#<k>"`.
- `{"type":"end"}`: the participant exits 0.

Size model: `size = base + input * n_inputs + output * (n_outputs + n_change)`;
`min_fee = min_fee_a * size + min_fee_b`; `min_ada = (160 + output) * coins_per_utxo_byte`.

Properties: `valid-txs` (mechanical: inputs non-empty, distinct and in the
wallet; `sum(inputs) = sum(outputs) + sum(change) + fee`; `fee >= min_fee`;
each change `>= min_ada`; `size <= max_tx_size`; well-formed, in time),
`serves-feasible` (mechanical: refuses only when the judge's own largest-first
cannot pay), `change-privacy` (declared).

Scenarios: 8 per judge run from `CB_SEED`, each ~60–120 initial UTxOs and 300
events (deposits including small reward-like ones, payments of 1–3 outputs,
log-normal amounts; always fundable). `inputs` = sha256 of the scenarios.

Axes: `fees` (ADA per scenario, minimise), `dust` (UTxOs under 2 ADA left at
the end, minimise), `inputs_per_tx` (minimise), `latency_ms` (median reply
time measured by the judge, minimise, repeats 3), `peak_rss_mib` (from the
`cb participant` report, minimise, repeats 3). Tolerances are set from
preflight's measured spread.

Baseline: largest-first in Python. Broken variants: off-by-one (fee one
lovelace under the minimum), partial (pays only the first output), constant
(always refuses), self-report (the baseline, also printing fake axes and
writing a `measure.json` in `/tmp`). Taxonomy: `strategy`, `change`,
`consolidation`, `language`.

## Milestones

M0 this document. M1 spec + scaffold. M2 gate. M3 participant + measure.
M4 record, comment, workflows. M5 preflight. Pilot challenge. M6 feed + site.
M7 malicious suite (`scripts/malicious-prs-research.sh`, local and live) and an
end-to-end dry run on GitHub: baseline, a new-direction PR, a builds-on PR, a
rejected PR, an attack PR. M8 open `coin-select` and publish it.

Later: component contracts; admitting deferred contracts; an "unverified"
lane experiment; reading submission discussions; GPU/self-hosted runners;
the toolkit's schema-2 source; Stwo (M9).
