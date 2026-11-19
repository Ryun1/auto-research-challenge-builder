# Submitting to coin-select

A submission is one new directory, `challenges/coin-select/submissions/<id>/`, in a PR
that changes nothing else. Merged submissions are immutable.

1. Pick an `<id>` (lowercase a-z, 0-9, `-`). Put `submission.toml` and an
   executable `build.sh` in the directory, plus whatever your code needs. A whole
   codebase is fine, under `max_bytes` (4 MiB) in `challenge.toml`. `build.sh` runs
   offline in a sandbox, must leave an executable `./select` in the directory (see
   `contracts/wallet/INTERFACE.md`), and must vendor any dependencies.
2. `submission.toml` declares: `id` (the dir name), `contract = "wallet@1"`,
   `novelty`, `direction`, `parent`, `taxonomy` (keys `strategy`, `change`,
   `consolidation`, `language`; see `taxonomy.toml`), `summary`, optional `claims`
   (for the declared property `change-privacy` only) and `model`.
3. Novelty is your own declaration and is never judged. `new` starts a direction
   `<name>` that no accepted submission uses. `builds-on` names an accepted
   `parent`; copy its directory and edit it. The direction is inherited.
4. Run the judge locally first (see `README.md`). CI then builds your submission and
   runs the same judge, which checks the properties and measures every axis. Those
   numbers are a claim from the PR run.
5. There is no record to beat and no minimum improvement. If the checks pass, the
   PR is landed on main automatically, two clean runs there record it on the map,
   and the PR is closed. Failing and noisy attempts are recorded too.
