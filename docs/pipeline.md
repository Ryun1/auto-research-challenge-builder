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
