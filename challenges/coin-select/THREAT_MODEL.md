# Threat model

Required before launch; reviewed in the PR that opens the challenge.
Answer every question. "N/A" needs a reason.

## How could a submission fake an axis?

- Reporting numbers about itself instead of being measured (does the judge measure the same outputs it checks?): The judge computes fees, dust and inputs per transaction from the replies it validated, latency from its own clock around each reply, and memory from `cb participant`'s report (the cgroup `memory.peak` of the whole process tree, so spreading work over helper processes does not lower it). It never reads anything the participant prints about itself; the `self-report` preflight variant (fake axes on stderr, fake measure.json in /tmp) must come out with the baseline's deterministic axes.
- Timing tricks (detecting the judge, doing less work when measured): The participant always runs as the judge's only counterpart through the same protocol, in the same way for every repeat, so there is nothing to detect; it keeps no state between invocations (see below), so it cannot cache repeat 0's replies for repeats 1 and 2; latency and memory repeat 3 times and carry wide tolerances, and a large outlier triggers manual review.

## How could a submission pass the checks while breaking a property?

- Which properties are mechanical, and what escapes each check: `valid-txs` and `serves-feasible` are mechanical and evaluated on every reply with exact integer arithmetic against the judge's own wallet state, so a wrong transaction cannot pass. Any exception while decoding or checking a reply (for example the `deep-json` variant's 60000-deep nested array, which raises `RecursionError`) is the participant's `valid-txs` failure, never a judge bug, so a submission cannot turn its own failure into `infra_failed`. What escapes is the fixed size and fee model (not the real ledger: no scripts, no multi-asset, which is why `wallet-multiasset@0` is deferred) and refusal only being checked against largest-first, which is a conservative test: scenarios are built so the whole wallet can always pay, and largest-first spends the largest coins first, so a refusal it can refute is a plain failure to serve.
- Declared properties (what stops a false claim from misleading readers?): `change-privacy` is never checked; the site shows claims as declared and unverified, next to the argument, and the footer of every result comment says so.

## Could a submission read the seed or the reference?

- Where the seed, reference results and judge temp files live, and who can read them: The seed is in the judge's environment only; the judge removes `CB_SEED` from the participant's environment and the sandboxed participant runs as `nobody` with a cleared environment, in its own PID namespace, so it cannot read the judge's `/proc`, memory or temp files (`mkdtemp`, mode 0700). The scenario values come from a SHA-256 counter-mode stream over the seed, so the amounts a participant sees reveal nothing about later ones (MT19937 state would be recoverable from its outputs). The reference is largest-first run inside the judge; no reference output is ever sent to the participant.

## Could a submission overfit fixed inputs?

- Are inputs fresh per seed, and how many distinct ones are there?: Scenarios are generated from a fresh seed drawn after the build, eight scenarios per run with random wallets and payment streams, so inputs are effectively unbounded in number. The two runs on main use independent seeds, and preflight checks that different seeds give different `inputs` digests.

## Could a submission persist state between runs?

- Caches, files, processes or network that survive an invocation (the sandbox gives a private /tmp and no network): In the sandbox the participant has no network and a read-only submission dir. Fresh tmpfs mounts cover `/tmp`, `/var/tmp`, `/dev/shm`, `/run/lock`, `/var/crash`, `/dev/mqueue` and `/run/user`, so no world-writable host path persists; IPC (SysV shm, semaphores, POSIX mqueues) and UTS are private namespaces; `--kill-child` plus the PID namespace remove stray processes. Memory and process limits are enforced on the whole process tree (cgroup v2 `memory.max` and `pids.max`), so a tree cannot exceed the runner. The malicious suite's `persist-state` case and preflight's broken variants test this. Outside the sandbox (laptops) it can persist state, but that is not a measured configuration. The build step can only write the submission dir.

## What does a large outlier trigger?

Manual review; the result is flagged, not trusted.

- Who reviews: The maintainer, who also owns taxonomy edits and the judge.
