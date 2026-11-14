# Submitting to ledger-cbor

The goal and the rules are in [README.md](README.md): decode mainnet Conway
transactions in fewer instructions than the pinned reference, with exactly the
same results, including which inputs are rejected. The score is
`speedup = reference Callgrind Ir / candidate Ir`, and a change has to be at
least **1% better** than the current record to count
(`min_improvement_bips = 100` in `challenge.toml`).

## 1. Branch, don't fork

You are a collaborator, so push a branch to this repo and open the PR from it
against `main`:

```sh
git clone git@github.com:Ryun1/auto-research-challenge-builder.git
cd auto-research-challenge-builder
git switch -c <you>/ledger-cbor-<idea> origin/main
```

Don't open PRs from a fork: the workflow that posts results can't link a fork's
run back to the PR, so you would get no comment. Target `main`. The workflows
and the judge come from the PR's base branch.

You have write access because GitHub needs it to run the PR workflows. `main`
is protected: it only changes through a PR the maintainer has approved, and
the maintainer merges.

## 2. What you may change

- Only existing `.hs` files under `challenges/ledger-cbor/candidate/<package>/src/`.
  The file set is frozen: the cabal files are upstream's, so a new module
  wouldn't build, and new or deleted files are refused.
- One challenge per PR, nothing outside `candidate/`, no symlinks or
  submodules, and at most 4 MiB in total under `candidate/` (the baseline is
  about 2.6 MB).
- The Haskell allowlist in the README's **Policy** section: safe pragmas and
  extensions, no new `unsafe*`, FFI, TH, CPP lines, IO or GHC-internals imports.

Check before you push. This runs the same gate as CI, including the Haskell
policy. It needs only git and Rust, so it works on macOS too:

```sh
cargo build --release
./target/release/cb gate --base origin/main --head HEAD
```

`gate ok: ledger-cbor` means CI's gate will accept the diff. The gate reads
`challenge.toml` and `baseline/` from your working tree, so run it on your
branch, not on a checkout with other edits.

## 3. Build and test locally (Linux only)

The build needs GHC 9.6.7, cabal 3.16, python3, rsync and the Cardano system
libraries (libsodium, secp256k1, blst). Scoring also needs valgrind.
`setup.sh` uses GNU `sed -i`, so it fails on macOS. On a Mac, edit there and
build on a Linux machine, or let CI build it.

```sh
cd challenges/ledger-cbor
./setup.sh                  # first time: clone cardano-ledger @ 7827506, build the reference (35+ min)
./build.sh                  # rebuild what you changed and its dependents
CB_SEED=1 ./verifier/run    # must print {"ok": true, ...}
CB_SEED=1 ./bench/run       # metrics.speedup, metrics.instructions, wall guard
```

Or, from the repo root, `./target/release/cb score ledger-cbor` runs setup,
build, verify and score in a scratch copy, as CI does (without the sandbox).

Local runs use the repo pool (`corpus/pool.hex.gz`). CI scores a fresh pool of
recent mainnet transactions, so local numbers are a guide, not the score.
Upstream's test suites (five packages, `upstream-tests.txt`) only run at promotion
(`CB_UPSTREAM_TESTS=1 ./setup.sh && CB_UPSTREAM_TESTS=1 ./build.sh` runs them
locally). A change that breaks other eras or the ledger rules fails there,
even if the harness never calls that code.

## 4. Open the PR and read the result

Opening the PR, and every later push to it, starts the **Submission**
workflow. A new push cancels the run in progress for that PR.

Every run is billed to the maintainer, so each GitHub account gets **10
Submission runs per rolling 24 hours**, counting every push to an open PR,
cancelled and refused runs included. The eleventh gets a comment beginning
"Refused by the gate" with "daily cap", and nothing is built. Measure locally
(step 3) and push when you have something to score.

1. **Gate**, about a minute. If it refuses, you get a comment beginning
   "Refused by the gate" with the reasons. Nothing is built or recorded.
2. **Score.** The job restores the prebuilt reference, rebuilds your changed
   packages and everything that depends on them offline in a sandbox, then
   verifies and scores. Expect tens of minutes. A change to
   `cardano-ledger-binary` or `-core` recompiles more than one in `-conway`.
   The job times out after 150 minutes, and setup, build, verify and score
   are each killed after 60.

The **Comment** workflow then posts a new comment on the PR:

| You see | Meaning |
|---|---|
| status `valid`, recorded as `valid_claim_better` | correct, and speedup ≥ 1.01 × the record's: ask for a merge |
| status `valid`, recorded as `valid_not_promoted` | correct, but not 1% better than the record |
| status `rejected`, reason `build: ...` | it didn't compile (or the build ran past 60 min) |
| status `rejected`, reason `verify failed` | some output, re-encoding, accept/reject result or `rnf` result differs from the reference |
| status `rejected`, reason `wall-time guard: regressed` | the paired wall-time run put your build below 0.92× the reference's speed |
| status `infra_failed` | setup failed, wall time was too noisy to judge in three attempts, or the reference itself misbehaved on the fresh pool. Not your change: push again or ask the maintainer to re-run |
| "No score was produced" | the score job died before writing a result. See the run |

The comment gives the status, the speedup and the wall-time guard only. For
details, open the PR's **Submission** run in the Actions tab, then the `score`
job, then the step "Build, then verify + score sandboxed". It prints the full
`score.json`: instruction counts for both sides, wall-time numbers and the
verify detail (how many inputs differ, and the first few). The same file is in
the run's `score` artifact.

A build failure only reports the last three lines of the build's stderr. The
compiler's error messages are not in the log, so reproduce compile errors on
Linux (step 3).

Every PR run that produces a score is appended to `results.jsonl` on `main` as
a `stage: "pr"` attempt, including rejected and slower ones. Gate refusals and
runs that produced no score are not recorded.

## 5. After the merge

The maintainer merges PRs one at a time. The merge starts the **Promote**
workflow on `main`, which takes about 20 minutes with warm caches (longer
after a cold rebuild). It makes two independent runs, each on
its own fresh pool and seed. Each run scores your tree and the current
record's tree on the same sample, and it also builds and runs the upstream
test suites of five packages (binary, core, shelley, allegra, mary; see
`upstream-tests.txt`). The attempt is **promoted** only if both
runs pass, they agree on candidate/record within 0.5%, and the ratio is at
least 1.01. Otherwise, infrastructure failures included, a bot commit puts
`candidate/` on `main` back to the record's tree, and the attempt is recorded
with its status. Start your next attempt from the new `main`.

The PR comment is a claim. Only promotion changes the record.
