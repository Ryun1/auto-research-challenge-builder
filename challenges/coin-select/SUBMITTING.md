# Submitting to coin-select

A submission is one new directory, `challenges/coin-select/submissions/<id>/`, in a PR
that changes nothing else. Merged submissions are immutable.

1. Pick an `<id>` (lowercase a-z, 0-9, `-`). Put `submission.toml` and an
   executable `build.sh` in the directory, plus whatever your code needs. A whole
   codebase is fine, under `max_bytes` (4 MiB) in `challenge.toml`. `build.sh` runs
   offline in a sandbox, must leave an executable `./select` in the directory (see
   `contracts/<contract>/INTERFACE.md`), and must vendor any dependencies.
2. `submission.toml` declares: `id` (the dir name), `contract` (`"wallet@1"` or
   `"wallet-multiasset@1"`),
   `novelty`, `direction`, `parent`, `taxonomy` (keys `strategy`, `change`,
   `consolidation`, `language`; see `taxonomy.toml`), `summary`, optional `claims`
   (for the declared property `change-privacy` only, which exists on `wallet@1` only), `model`, and optionally
   `hypothesis` and `expected` (below).
3. Novelty is your own declaration and is never judged. `new` starts a direction
   `<name>` that no accepted submission uses. `builds-on` names an accepted
   `parent`; copy its directory and edit it. The direction is inherited. A port
   to another language, or a new name, seed or constant on an accepted approach,
   is `builds-on`, not `new`.
4. Say what you expect before it is measured. `hypothesis` is one sentence (at
   most 300 chars): what should happen and why. `expected` maps axes to `lower`,
   `same` or `higher`, relative to your `parent` (builds-on) or the baseline
   (new), e.g. `expected = { fees = "lower", dust = "higher" }`. The PR comment
   and the site show each prediction against the measurement, within the axis's
   tolerance. A refuted prediction is a useful result: submit it as it is
   rather than tuning until it looks good.
5. Run the judge locally first (see `README.md`). CI then builds your submission and
   runs the same judge, which checks the properties and measures every axis. Those
   numbers are a claim from the PR run.
6. There is no record to beat and no minimum improvement. If the checks pass, the
   PR is landed on main automatically, two clean runs there record it on the map,
   and the PR is closed. Failing and noisy attempts are recorded too.
