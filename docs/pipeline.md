# Pipeline (PLAN.md §4)

The judge always comes from `main`; submission bytes are data until the build.

```
PR ──▶ Submission (pull_request_target, runs main's copy, read-only token, no secrets)
        gate   trusted: which challenge; diff + full head tree (modes, symlinks,
               gitlinks, size cap, Rust token policy); export editable files as data
        score  judge from main + exported files; build offline; draw the seed;
               verify + score as nobody with no network → score.json (a claim)
   ──▶ Comment (workflow_run, main's copy): record stage=pr attempt, post comment
merge ──▶ Promote (push to main): two independent sandboxed runs, fresh seeds
        record  trusted: both runs clear the bar (or both miss)? beats the record?
                → promoted | valid_not_promoted | rejected | infra_failed,
                append to results.jsonl; if not promoted, revert the editable
                files to the record's commit
```

Changes from the original plan, with the reasons:

- **Build and scoring use separate trust.** Runners give `runner` passwordless
  sudo, so candidate code must never run as `runner`. The kit forbids build
  scripts and proc macros, so the build doesn't execute submission code. Verify
  and score run under `sudo unshare --net setpriv --reuid=65534` in a
  world-readable scratch copy that `nobody` can't write. `cb` strips
  `ACTIONS_*`/`GITHUB_*`/`*TOKEN*` variables from every command it runs.
- **Non-promoted merges are reverted.** This keeps `main`'s candidate equal to
  the record, so the next PR starts from the best one. A promotion run that
  ends `infra_failed` is first scored once more from scratch (new pool, new
  seed), so one infrastructure failure doesn't revert a merge. A verify the
  judge blames on the infrastructure (`"infra": true`, e.g. the reference
  misbehaving) is `infra_failed`, not `rejected`.
- **PR-stage attempts are recorded** (`stage: "pr"`), so `results.jsonl` keeps
  the denominator even for PRs that are never merged.

## Recording results

Comment and Promote commit only `results.jsonl` (and, for a revert, the
candidate) and push to `main` with a pull-rebase retry; a push that never lands
fails the job (Comment posts its PR comment first). `.gitattributes` makes
`results.jsonl` a union merge, so appends from concurrent runs both land in
either order. The feeds and status site are not committed here: the Site
workflow builds them from `main` and publishes them to
https://ryun1.github.io/auto-research-challenges/ (feeds at
`feeds/<slug>.json`).

Concurrency, and what is left:

- **Comment** is grouped per PR, so PRs never cancel each other or wait on a
  Promote. Within one PR a third Submission completion while a Comment runs
  cancels the pending one (in practice only cancelled Submission runs arrive
  that fast). Comment reads the record from its checkout; if a promotion lands
  before its push, its PR entry's `status` and `record_before` are against the
  previous record. PR entries never set the record, so only that label is stale.
- **Promote** runs are serialised (`promote` group) because each decides
  against the record the last one left. A third merge while one promotes
  cancels the pending second, which is then never scored or reverted: merge
  one submission at a time and wait for its Promote.
- **Promote's revert** is computed on the `main` it checked out. If another
  merge touched the same candidate files meanwhile, the rebase conflicts and
  the job fails with nothing recorded; if it touched other files, the revert
  lands and leaves a mix. Merging one at a time avoids both.

## Launching a challenge

1. Set `status = "open"` in `challenge.toml`, and get `cb preflight <slug>` to
   pass on the runner (Preflight workflow).
2. Dispatch **Promote** with `slug` and `baseline=true`. This records the
   baseline, which is the record every attempt must beat.
3. Add ecosystem teams as collaborators. `main` is protected by the ruleset
   in `.github/rulesets/main.json` (see "Protection of main" below).

Before any ecosystem team gets PR access: a human security review
(tracked as its own issue).

## Protection of main

Collaborators on a personal repo always get write access, so `main` is
protected by a ruleset (needs GitHub Pro on a private repo). Its definition is
`.github/rulesets/main.json`; the live copy is under Settings > Rules.

- **Rules:** no deletion, no force push, and changes only through a PR with an
  approving review from a code owner. `.github/CODEOWNERS` makes the
  maintainer the owner of every path, so a collaborator cannot merge on
  another collaborator's approval. A push after the approval dismisses it.
- **Bypass:** the repository admin (the maintainer cannot approve their own
  PRs, so they merge with "bypass rules" or `gh pr merge --admin`) and deploy
  keys.
- **Results commits:** the workflow token cannot bypass a ruleset, so Comment
  and Promote push `results.jsonl` (and Promote's revert) with a write deploy
  key. It is the `RESULTS_DEPLOY_KEY` secret of the `results` environment,
  which only runs on `main` may use. The site key (`SITE_DEPLOY_KEY`,
  environment `site`) is scoped the same way. A repository-level secret would
  be readable by a workflow pushed to any branch. `scripts/setup-deploy-keys.sh`
  creates or rotates both keys.
- **PRs into main only:** Submission and Comment act only on PRs whose base
  is `main`. The jobs check out `pull_request.base.sha` and build `cb` from
  it, so a PR into a collaborator's own branch would run that branch's code as
  the judge, and Comment would record what it uploaded. The gate job also
  compares the base branch name in bash (expressions ignore case) and refuses
  a base commit that is not on `main`, before anything from the checkout runs.
  Check: `scripts/malicious-prs.sh non-main-base`.
- **Commit messages:** a deploy-key push fires push workflows, unlike a
  workflow-token push, so the results commits end in `[skip actions]`.
- **Audit:** `scripts/audit-main.sh` reads GitHub's push log for `main` and
  flags any update that is neither a PR merged by the owner nor a results
  commit made inside a Comment or Promote run.

Not covered: a collaborator's own branch workflow still gets a workflow token
that can write to other branches and post PR comments as `github-actions[bot]`.
The record only moves through `results.jsonl` on `main`, so a score comment
is a claim to check against the Submission run, not proof. A PR whose merge
commit message contains `[skip ci]` or `[skip actions]` would not be promoted:
check the title before merging.

## Actions minutes

The repo is private, so every run is billed (GitHub Pro includes 3,000
minutes a month). Measured on 2026-11-12 for `ledger-cbor`: a push to a
submission PR costs about 13 minutes (score 10, gate, Comment and Site about
1 each), a merge about 50 (two promotion runs of about 20, CI 7, Site), a Warm
rebuild about 40.

- **Daily cap:** the gate job refuses an account's Submission runs beyond
  `DAILY_CAP` (10) in a rolling 24 hours (`.github/scripts/daily-cap.sh`,
  called from `submission.yml`). Every run counts, cancelled and refused ones
  included. The repo owner, who pays for them, is exempt; nobody else is. The
  refusal is a gate refusal, so the PR gets a comment and nothing is built.
- **No CI on candidate-only PRs** (`paths-ignore` in `ci.yml`): CI tests
  nothing of a submission.
- **Site** publishes after Comment, not after Submission as well.

The cap limits scored runs, not minutes. Not covered:

- a capped push still bills its gate job, Comment and Site, about 3 minutes,
  with no limit on how many;
- CI still runs (about 7 minutes) for every push to a PR that touches a
  non-candidate file, into any base branch;
- re-running an old run from the Actions tab (a new attempt of the same run,
  not a new run);
- a collaborator dispatching Preflight, Warm, Promote, Calibrate or Site by
  hand, or pushing a workflow to their own branch.

Only the account budget bounds those. The budget and its alerts (Settings > Billing and licensing > Budgets
and alerts) are the backstop for those. When the minutes run out, GitHub
refuses to start jobs: a job that fails in seconds with no steps has a billing
annotation on its check run.
