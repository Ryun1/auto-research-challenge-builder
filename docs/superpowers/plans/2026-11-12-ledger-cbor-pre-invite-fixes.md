# ledger-cbor Pre-Invite Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the three gaps the 2026-11-12 launch review found, so collaborators can be invited to `ledger-cbor`: PRs into non-`main` branches reach the pipeline, the #22 security sign-off is not done, and nothing guards Actions minutes.

**Architecture:** The pipeline's trusted workflows (`submission.yml`, `comment.yml`) get a base-branch condition and a per-actor daily cap, both enforced in the gate job that never runs submission code. The malicious-PR suite is retargeted at `ledger-cbor` and gains a free local mode, so it is the test for the workflow change and the evidence for #22. Billing settings and the sign-off itself stay with the maintainer.

**Tech Stack:** GitHub Actions YAML, bash, `gh`, `jq`, the `cb` Rust binary (unchanged).

**Spec:** the launch review in the session of 2026-11-12 (items 1-3 under "Do before inviting anyone"), issue #22, `docs/pipeline.md`, `PLAN.md` §4 and "Risks".

## Global Constraints

- One branch, `launch/pre-invite-fixes`, off `origin/main`; one PR; **one push** once everything is committed. Each push to a PR bills about 11 minutes (CI 7, Submission gate 1, Comment 1, Site 2).
- Claude does not push or merge here. The global `Bash(git push:*)` deny rule is in force, and `main` merges are the maintainer's (`gh pr merge <n> --squash --admin`). Steps marked **(maintainer)** are commands to hand over as `! <command>`.
- No `[skip ci]` or `[skip actions]` in any commit message or PR title.
- `pull_request_target` workflow changes take effect only after the merge to `main`: GitHub always runs `main`'s copy. Nothing in Tasks 2-3 can be proven from the PR branch.
- No change to `crates/`, `challenges/*/verifier`, `harness`, `bench`, `setup.sh`, `build.sh` or `baseline/`: the Warm cache key and the judge stay as they are.
- Live suite cases must not reach the score job (about 12 billed minutes each). Only gate-level refusals and the `ignored` case run live.
- Daily cap value: **10 Submission runs per actor per rolling 24 hours**, no exemption for the owner.

## Review Focus

- A head branch with two open PRs (one into `main`, one into another branch): the comment and the results entry must go to the PR into `main`. Covered by Task 2 step 3 (jq selection) and the suite's `non-main-base` case keeping its head branch free of a second PR.
- A capped push must still get a PR comment saying why, not silence. Covered by Task 3 step 1 (`gate.json` written in the gate's refusal shape) and step 6.
- The cap must count the run that is asking. Covered by Task 3 step 1 (`CAP=0` refuses, the count includes the current run).
- A PR that changes `candidate/` **and** a judge file must still run CI. Covered by Task 3 step 4 (`paths-ignore` only skips when every changed file matches).
- The suite must clean up its branches and PRs when a case fails or times out. Covered by Task 1 step 3 (cleanup on every path).

---

### Task 1: Malicious-PR suite targets ledger-cbor, runs locally, and can expect "ignored"

The suite (`scripts/malicious-prs.sh`) still attacks `golomb-14` and `pedersen`, which are closed: the gate now refuses every case with `challenge status is "closed", not open`, so no case can pass. This task makes it the test for Task 2 and the evidence for Task 5.

**Files:**
- Modify: `scripts/malicious-prs.sh` (whole file, 57 lines)

**Interfaces:**
- Produces: `scripts/malicious-prs.sh [--local] [case...]`. Exit 0 if every selected case passes. `--local` runs `cb gate` on local branches with no push, no PR and no minutes; it skips cases that need the live pipeline. Without `--local` only the cases named on the command line run (no name = error, so nobody starts every live case by accident).
- Produces: case names `workflow-edit`, `score-json`, `symlink`, `gitlink`, `verifier-edit`, `setup-edit`, `size-cap`, `new-file`, `th-extension`, `unsafe-import`, `foreign-decl`, `non-main-base`.

- [ ] **Step 1: Replace the case table.** `L=challenges/ledger-cbor`, `S=$L/candidate/cardano-ledger-binary/src/Cardano/Ledger/Binary/Decoding/Sized.hs`. Fields stay `name | expect | reason substring | change`. `expect` is `refused` or `ignored`.

| name | expect | reason substring | change |
|---|---|---|---|
| `workflow-edit` | refused | `exactly one challenge` | append `# pwn` to `.github/workflows/ci.yml` and a comment line to `$S` |
| `score-json` | refused | `score.json: outside the editable paths` | write `$L/score.json` |
| `symlink` | refused | `symlink` | `ln -s ../verifier/judge.py $L/candidate/link` |
| `gitlink` | refused | `gitlink` | `git update-index --add --cacheinfo 160000,$(git rev-parse HEAD),$L/candidate/sub` |
| `verifier-edit` | refused | `judge.py: outside the editable paths` | append a line to `$L/verifier/judge.py` |
| `setup-edit` | refused | `setup.sh: outside the editable paths` | append a line to `$L/setup.sh` |
| `size-cap` | refused | `> max_bytes` | append 2,000,000 bytes of `-- x` comment lines to `$S` |
| `new-file` | refused | `new file (the file set is frozen)` | create `$(dirname $S)/Pwn.hs` with `module Cardano.Ledger.Binary.Decoding.Pwn where` |
| `th-extension` | refused | `extension TemplateHaskell is not allowed` | prepend `{-# LANGUAGE TemplateHaskell #-}` to `$S` |
| `unsafe-import` | refused | `import of System.IO.Unsafe` | add `import System.IO.Unsafe (unsafePerformIO)` after the last import of `$S` |
| `foreign-decl` | refused | `foreign declaration` | append `foreign import ccall "getenv" c_getenv :: Int -> Int` to `$S` |
| `non-main-base` | ignored | (none) | append a line to `PLAN.md` at the repo root (live only; there is no root README) |

- [ ] **Step 2: Add `--local`.** For each selected `refused` case: create the branch from `origin/main` as today, apply the change, commit, then run `./target/release/cb gate --base origin/main --head HEAD --out "$tmp/sub" --json "$tmp/gate.json"` (build `cb` once with `cargo build --release` if the binary is missing). PASS iff `jq -r .ok` is `false` and some entry of `.errors[]` contains the reason substring. `ignored` cases print `SKIP (live only)`. Delete the local branch after each case.

- [ ] **Step 3: Add the `ignored` expectation (live).** Create and push a base branch `malicious/base-<ts>` at `origin/main`; open the PR from the case branch against it (`--base malicious/base-<ts>`). Poll every 10 s for 120 s. PASS iff, for the whole window, `gh run list --repo "$REPO" --workflow Submission --branch "$branch" --json databaseId --jq length` is `0` and the PR has no comment containing `cb:score`. On every path (pass, fail, interrupt) close the PR and delete both remote branches and the local branch: do the cleanup in a function called from a `trap` and at the end of the case.

- [ ] **Step 4: Run the local suite.**

Run: `cargo build --release && scripts/malicious-prs.sh --local`
Expected: eleven `PASS` lines, `non-main-base ... SKIP (live only)`, exit 0. If a reason substring does not match, the fix is the substring in the table (read the gate's message in the output), never the gate.

- [ ] **Step 5: Run `non-main-base` live against today's `main` and watch it fail (maintainer).** This is the red test for Task 2 and the first live confirmation of the review finding. About 4 billed minutes (gate, Comment, Site twice).

Run: `! scripts/malicious-prs.sh non-main-base`
Expected: `non-main-base ignored FAIL`: a Submission run starts for the PR and a "Refused by the gate" comment appears on it. If it prints PASS, GitHub does not start Submission for a non-`main` base: stop, record that on #22, and drop Task 2's `submission.yml` change (keep the `comment.yml` guard).

- [ ] **Step 6: Commit**

```bash
git add scripts/malicious-prs.sh
git commit -m "Malicious-PR suite: target ledger-cbor, --local mode, non-main-base case"
```

---

### Task 2: Submission and Comment act only on PRs into main

**Files:**
- Modify: `.github/workflows/submission.yml:1-24` (header comment, trigger, gate job condition)
- Modify: `.github/workflows/comment.yml:31-33` (job condition), `:58-97` (PR number)
- Modify: `docs/pipeline.md` ("Protection of main", "Not covered" paragraph)

**Interfaces:**
- Consumes: the `non-main-base` case from Task 1.
- Produces: no Submission run for a PR whose base is not `main`; Comment's `$PR` is the PR into `main`.

- [ ] **Step 1: `submission.yml` trigger.** Add `branches: [main]` under `on.pull_request_target` (it filters on the PR's base branch). Add to the header comment, in one sentence, why: the jobs check out `pull_request.base.sha` and build `cb` from it, so a PR into a collaborator's own branch would run that branch's code as the judge.

- [ ] **Step 2: `submission.yml` gate job.** Add `if: github.event.pull_request.base.ref == 'main'` to the `gate` job. `score` already needs `gate` with `ok == 'true'`, so it is skipped with it.

- [ ] **Step 3: `comment.yml`.** Job condition becomes:

```yaml
if: >-
  github.event.workflow_run.event == 'pull_request_target' &&
  contains(github.event.workflow_run.pull_requests.*.base.ref, 'main')
```

Replace the `PR:` env entry with `PRS: ${{ toJSON(github.event.workflow_run.pull_requests) }}` and set `PR` at the top of the run script: `PR=$(jq -r '[.[] | select(.base.ref == "main")][0].number // empty' <<<"$PRS")`, failing the step with an `::error::` if it is empty. Leave the concurrency group as it is.

- [ ] **Step 4: `docs/pipeline.md`.** In "Protection of main", add a bullet: Submission and Comment only act on PRs whose base is `main`, and why (same sentence as step 1). Name `scripts/malicious-prs.sh non-main-base` as the check.

- [ ] **Step 5: Check the YAML parses and the expressions are well-formed.**

Run: `for f in .github/workflows/submission.yml .github/workflows/comment.yml; do python3 -c "import sys,yaml; yaml.safe_load(open(sys.argv[1]))" "$f" && echo "ok $f"; done; command -v actionlint >/dev/null && actionlint .github/workflows/submission.yml .github/workflows/comment.yml`
Expected: `ok` for both files; no actionlint findings if it is installed (do not install it for this).

- [ ] **Step 6: Commit**

```bash
git add .github/workflows/submission.yml .github/workflows/comment.yml docs/pipeline.md
git commit -m "Submission and Comment act only on PRs into main"
```

The green run of `non-main-base` is Task 4 step 4, after the merge.

---

### Task 3: Minutes guard in the pipeline: daily cap per actor, cheaper submission pushes

Measured on 2026-11-12: a submission push bills about 20 minutes (score 10, CI 7, Comment 1, Site 2), a merge about 50. GitHub Pro includes 3,000 a month.

**Files:**
- Create: `.github/scripts/daily-cap.sh`
- Modify: `.github/workflows/submission.yml` (gate job: permissions, cap step, conditions on the later steps)
- Modify: `.github/workflows/ci.yml:3-6` (trigger)
- Modify: `.github/workflows/site.yml:14` (trigger list)
- Modify: `challenges/ledger-cbor/SUBMITTING.md` §4, `challenges/ledger-cbor/PROMPT.md` ("Limits"), `docs/pipeline.md` (new section "Actions minutes")

**Interfaces:**
- Produces: `.github/scripts/daily-cap.sh`. Environment in: `REPO` (owner/name), `ACTOR` (login), `CAP` (integer), `GH_TOKEN`. It counts `gh api --paginate "repos/$REPO/actions/workflows/submission.yml/runs?actor=$ACTOR&created=>=$since&per_page=100" --jq '.workflow_runs[].id' | wc -l` with `since` = now minus 24 h in UTC (`date -u -d '24 hours ago' +%FT%TZ`, with the BSD `date -u -v-24H` fallback so it runs on macOS). The count includes the run that is asking. If count > CAP it writes `gate.json` in the current directory as `{"ok": false, "errors": ["daily cap: <ACTOR> started <count> Submission runs in the last 24 hours (limit <CAP>). Every run is billed; push again tomorrow or ask the maintainer."]}` and prints `capped=true`; otherwise it writes nothing and prints `capped=false`. Exit 0 in both cases; a failed API call exits non-zero (the gate job then fails closed and Comment posts "the gate job failed before deciding").

- [ ] **Step 1: Write `daily-cap.sh` and test it against the live API (free).**

Run: `cd "$(mktemp -d)" && REPO=Ryun1/auto-research-challenge-builder ACTOR=Ryun1 CAP=0 GH_TOKEN=$(gh auth token) bash "$OLDPWD/.github/scripts/daily-cap.sh"; jq . gate.json`
Expected: `capped=true`, and `gate.json` has `ok: false` and one error starting `daily cap: Ryun1 started`.

Run: same with `CAP=100000` in a fresh temp directory.
Expected: `capped=false`, no `gate.json`.

Run: same with `ACTOR=no-such-user-xyz CAP=0`.
Expected: `capped=false` (count 0 is not above 0).

- [ ] **Step 2: Wire it into the gate job.** Give the `gate` job its own `permissions: { contents: read, actions: read }` (the workflow default stays `contents: read`, so `score` gains nothing). Add `env: DAILY_CAP: 10` at workflow level. After the "Fetch the PR head" step add:

```yaml
- id: cap
  name: Daily cap per actor (every run is billed)
  run: .github/scripts/daily-cap.sh >> "$GITHUB_OUTPUT"
  env:
    REPO: ${{ github.repository }}
    ACTOR: ${{ github.actor }}
    CAP: ${{ env.DAILY_CAP }}
    GH_TOKEN: ${{ github.token }}
```

Add `if: steps.cap.outputs.capped != 'true'` to the `rustup`, `cargo build` and `gate` steps. The upload step is unchanged: it uploads the cap's `gate.json`, the gate outputs stay empty, `score` is skipped, and Comment posts the refusal.

- [ ] **Step 3: Check the capped artifact is what Comment expects.** With the `gate.json` from step 1's first run:

Run: `./target/release/cb comment --gate <that gate.json>`
Expected: a body starting `<!-- cb:score -->` with `**Refused by the gate.**` and the `daily cap:` line.

- [ ] **Step 4: `ci.yml`.** Change the trigger to:

```yaml
on:
  pull_request:
    paths-ignore: ["challenges/*/candidate/**"]
  push:
    branches: [main]
```

A PR that changes only candidate files gets no CI run (it tests nothing of theirs; Submission scores it). Any other changed file still starts CI. `main`'s ruleset has no required status checks, so a skipped CI blocks nothing; say so in a comment on that line, because adding a required check later would deadlock candidate-only PRs.

- [ ] **Step 5: `site.yml`.** Remove `Submission` from `on.workflow_run.workflows`. Comment runs after every Submission and already triggers Site, so each push republished twice.

- [ ] **Step 6: Docs.** `SUBMITTING.md` §4: one paragraph, "Each GitHub account gets 10 Submission runs per rolling 24 hours, counting every push to an open PR; the eleventh gets a comment beginning 'Refused by the gate' with 'daily cap'. Measure locally and push when you have something to score." `PROMPT.md` "Limits": replace the last bullet's second sentence with "The pipeline refuses more than 10 scored pushes per account per 24 hours; ask me before starting a second idea." `docs/pipeline.md` "Actions minutes": the measured costs above, the cap and where it lives, and what is **not** covered: re-running an old run from the UI (a new attempt, not a new run), manual dispatch of Preflight, Warm and Promote by a collaborator, and a workflow pushed to a collaborator's own branch. The account budget (Task 4 step 1) is the backstop for those.

- [ ] **Step 7: Parse check.**

Run: `for f in .github/workflows/*.yml; do python3 -c "import sys,yaml; yaml.safe_load(open(sys.argv[1]))" "$f" || echo "BAD $f"; done; bash -n .github/scripts/daily-cap.sh && echo ok`
Expected: `ok`, no `BAD` lines.

- [ ] **Step 8: Commit**

```bash
git add .github/scripts/daily-cap.sh .github/workflows/submission.yml .github/workflows/ci.yml .github/workflows/site.yml challenges/ledger-cbor/SUBMITTING.md challenges/ledger-cbor/PROMPT.md docs/pipeline.md
git commit -m "Minutes guard: daily cap per actor in the gate; no CI on candidate-only PRs; Site once per push"
```

---

### Task 4: Ship, set the account budget, verify on main (maintainer steps)

**Files:** none.

**Interfaces:**
- Consumes: the three commits on `launch/pre-invite-fixes`.

- [ ] **Step 1: Account budget (maintainer, browser).** At https://github.com/settings/billing, read this month's Actions usage first. Then **Budgets and alerts > New budget**: Product-level budget, product **Actions**, scope the whole account, an amount you are willing to pay above the included 3,000 minutes (0 keeps today's hard stop at the included minutes), **Receive budget threshold alerts** on (75%, 90%, 100%). On the same page turn on **Included usage alerts** (90% and 100% of included minutes). Personal-account budgets always hard-stop at the amount. Record the amount chosen in `docs/pipeline.md` "Actions minutes" before the push in step 2 (amend Task 3's commit).

- [ ] **Step 2: Push and open the PR (maintainer runs the push).**

Run: `! git push -u origin launch/pre-invite-fixes`
Then: `gh pr create --base main --title "Pre-invite fixes: main-only pipeline, daily cap, malicious-PR suite on ledger-cbor" --body <summary of the three commits, the red run from Task 1 step 5, and the post-merge checks below>`
Expected: CI green. The PR's own Submission run refuses it ("exactly one challenge"): that is the old workflow, as designed.

- [ ] **Step 3: Merge (maintainer).**

Run: `! gh pr merge <n> --squash --admin`
Expected on `main`: CI green; Promote's `detect` prints "not a submission; nothing to promote" (the PR touches `challenges/ledger-cbor/*.md`); Warm does not start.

- [ ] **Step 4: Green run of the base-branch test (maintainer).**

Run: `! git fetch origin && scripts/malicious-prs.sh non-main-base`
Expected: `non-main-base ignored PASS`: no Submission run, no comment, in 120 s. Free.

- [ ] **Step 5: One live gate refusal through the new workflow (maintainer).** About 4 billed minutes; proves the cap step, the reduced job permissions and Comment's PR selection work end to end.

Run: `! scripts/malicious-prs.sh th-extension`
Expected: `th-extension refused PASS`. Then check three things in the Actions list for that PR's branch: the `gate` job's "Daily cap per actor" step printed `capped=false`; no CI run started (the case changes only a candidate file); and Site ran once, after Comment, not after Submission.

- [ ] **Step 6: Note for the next real submission PR.** Nothing to run now: the first scored submission after this merge is the first run of the `score` job under the changed workflow. Confirm its comment arrives and its entry lands in `results.jsonl`. Add that as a line to the #22 evidence comment (Task 5) so it is not forgotten.

---

### Task 5: #22 evidence and sign-off

The sign-off is the maintainer's: #22 asks for a human review, and its last line says the checklist is an engineering self-review. This task assembles current evidence for each box so the review is a read, not an investigation. Claude posts the evidence; the maintainer ticks the boxes and closes the issue, or names what is missing.

**Files:** none in the repo. One comment on issue #22.

**Interfaces:**
- Consumes: results of Task 1 step 4, Task 1 step 5, Task 4 steps 4-5.

- [ ] **Step 1: Run the audit.**

Run: `scripts/audit-main.sh week`
Expected: `ok: N updates to main since ..., none flagged`. Any `FLAG` line goes into the comment verbatim and is the first thing the maintainer looks at.

- [ ] **Step 2: Re-read the live settings** (the review read them on 2026-11-12; a sign-off should quote the day's values).

Run: `R=Ryun1/auto-research-challenge-builder; gh api repos/$R/rulesets --jq '.[] | {name, enforcement}'; gh api repos/$R/actions/permissions/workflow; gh api repos/$R/actions/secrets --jq '.secrets | length'; for e in results site; do gh api repos/$R/environments/$e/deployment-branch-policies --jq '[.branch_policies[].name]'; done; gh api repos/$R/collaborators --jq '[.[] | {login, role_name}]'`
Expected: ruleset `main` active; `default_workflow_permissions: read`, `can_approve_pull_request_reviews: false`; 0 repository secrets; both environments limited to `["main"]`; only `Ryun1`.

- [ ] **Step 3: Post one comment on #22** with a table, one row per checklist line of the issue body, each row: the claim, the evidence, the residual.

| Checklist line | Evidence to cite |
|---|---|
| No branch protection (blocking finding) | #69; step 2's output; step 1's audit result |
| `submission.yml` checks out only the base SHA | file reference; the new `branches: [main]` + job `if` (Task 2); red run (Task 1 step 5) and green run (Task 4 step 4) of `non-main-base` |
| `cb gate` | `scripts/malicious-prs.sh --local` output (Task 1 step 4), eleven cases; unit tests in `crates/cb/src/{pipeline,policy,paths,haskellpolicy}.rs`; note the checklist's "Rust token policy" now also means the Haskell allowlist (#51) |
| Score job build trust | the checklist text predates ledger-cbor: the Haskell build does compile submission code, inside `[build_sandbox]` (`challenge.toml`, `THREAT_MODEL.md`); Preflight 43135619436 on `main` probed the sandbox and rejected all four broken variants |
| `comment.yml` / `promote.yml` | file references; deploy key in the `results` environment, `main` only (step 2); Task 4 step 5's run |
| Token policy is lexical, "try to break it" | the three Haskell cases in the suite; `haskellpolicy.rs` tests (13); **not done:** an adversarial attempt by someone other than the author |
| Promotion trusts two runs | `results.jsonl` entries of 2026-11-12: one `infra_failed` promotion left the record alone, one promoted with ratios 1.0118 and 1.0150 |
| #13 malicious-PR results | #13 ran against the closed Rust challenges; the retargeted suite's results from Tasks 1 and 4 |

Then a short "Still open, accepted or not" list for the maintainer to decide on: #53 (Alonzo, Babbage and Conway upstream suites do not gate a promotion); what the write role allows (push to others' branches, cancel runs, delete caches, dispatch workflows, re-run old runs past the cap); the public site shows PR titles and authors; nobody but the author has reviewed the pipeline, and the issue's last line asks for legal/security review before reliance.

- [ ] **Step 4: Maintainer decision.** Read the comment, tick the boxes in the issue body that the evidence satisfies, and either close #22 or reply with what blocks. Close #52 as well if the heap-walk evidence in `THREAT_MODEL.md` satisfies it. Inviting collaborators (`docs/pipeline.md` "Launching a challenge", step 3) follows only from a closed #22.

---

## Out of scope

- #53 (the three missing upstream suites): accepted for a small trusted group, tracked in its own issue.
- Capping manual dispatches and re-runs: the account budget is the backstop.
- A site note on how to request access: wait until there is someone to invite.
- Making the repo public (free standard-runner minutes): it changes the trust model, and from 2 December 2026 GitHub blocks `pull_request_target` on public repos unless a policy allows it.
