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
        record  trusted: agree within half the bar? beats the record by the bar?
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
  the record, so the next PR starts from the best one.
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
3. Add ecosystem teams as collaborators. Branch protection on `main` should
   require code-owner review (`.github/CODEOWNERS`) and allow the workflow
   token to push `results.jsonl` (or use a bot token).

Before any ecosystem team gets PR access: a human security review
(tracked as its own issue).
