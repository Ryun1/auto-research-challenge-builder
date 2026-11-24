# ledger-cbor: verify the whole pool, plus a curated hard-case set

Date: 2026-11-19. Size: small change (short design).

## Intent

`judge.py verify` checks a 600-tx sample of the pool (plus 300 mutants, 4 edge
cases and the 800 scored txs). Rare transaction shapes are often missing from
both the sample and recent random blocks, so a decoder change that breaks one
of them can pass. Ryan asked for two things: verify more, and always include
known large or tricky txs. He handed the curation and the how to Claude.

## Goals

- Every tx in the pool (fresh in CI, at least 1,200; repo pool 1,633 locally)
  is verified on every run: digest parity and the heap walk.
- A fixed, committed set of hard-case mainnet txs (`corpus/tricky.*`) is
  verified on every run, each with a seeded mutant.
- Each hard case is in the set for a recorded reason, so the set can be
  reviewed and extended.

## Non-goals

- No change to scoring: the timed sample stays 800 txs from the pool, and hard
  cases are never timed.
- No synthetic or generated txs. Every hard case is a real mainnet tx.
- No new runtime dependency on Koios. The hard-case set is committed, not
  fetched per run.

## Anti-goals

- Verify becoming flaky or slow enough to put the job's time limit at risk
  (submission score job: 150 min, of which the build is most).
- A hard case the reference rejects being reported as an infra failure.

## Approaches

1. **Committed curated set, built by a script (picked).** `verifier/tricky.py`
   queries Koios for the targeted shapes below, scans candidates with a small
   CBOR walker for the features that can't be queried directly, and writes
   `corpus/tricky.hex.gz` + `corpus/tricky.json`. It's run once, the output is
   committed, and it can be rerun to extend the set. It is reproducible and
   reviewable, and CI doesn't depend on Koios for it.
2. **Fetch hard cases per run.** Unpredictable, but it adds Koios calls and
   failures to every run (#84 already lost runs to Koios). Unpredictability
   buys nothing here: hard cases are only verified, never scored, so
   memorising them can't raise a score.
3. **Pick the largest or most unusual txs from a larger fresh pool.** This
   only finds what recent blocks happen to contain, so shapes like governance
   proposals, Plutus V1 and Byron addresses would still be missed.

Approach 1 was picked because the hard cases need to be stable, inspectable
and independent of Koios at run time.

## Design

**Verify the whole pool.** `verify()` uses all of `pool()` as `valid` cases
instead of `rng.sample(p, VALID_N)`. The separate `scored` cases are removed,
since the scored txs are now a subset of the verified ones. The detail line
still says that the scored txs were included. Pool mutants rise from 300 to
600 (they're cheap).

**Hard-case set.** About 100 to 200 txs, each recorded in
`tricky.json` with `{hash, bytes, epoch, tags[]}`. Target shapes:

- **Governance:** each kind of proposal (parameter change, hard fork, treasury
  withdrawal, no confidence, committee update, new constitution, info), plus
  votes from DReps, SPOs and the committee.
- **Conway certificates:** DRep registration, update and retirement;
  vote-delegation and combined stake+vote delegation certs; committee hot-key
  authorisation and resignation; pool registration with many relays.
- **Scripts:** txs that create or spend Plutus V1, V2 and V3 scripts; native
  scripts with time locks; reference scripts and reference inputs; inline
  datums; collateral return; treasury donation.
- **Size and breadth:** the largest txs found (near the 16 KB limit); txs with
  many inputs or outputs; heavy multi-asset outputs; large metadata.
- **Encodings:** Byron and pointer addresses in outputs; legacy (pre-Babbage)
  output format; indefinite-length arrays, maps or bytes; sets with and
  without tag 258.
- **Older eras:** a few Babbage-era and Conway PV 9 and 10 txs. Some may be
  rejected at PV 11; that's fine, because only parity counts.

**Accept/reject.** Hard cases get their own kind, `tricky`. Only parity with
the reference counts, so a reference reject is never an infra failure. The
detail line reports how many hard cases the reference accepted and rejected.
Each hard case also gets one seeded mutant per run.

**Docs.** Update README.md (the correctness paragraph), THREAT_MODEL.md (the
finite-inputs bullet) and the `judge.py` docstring.

**Validation.**
- Run Preflight on the branch. All four broken variants must still fail, and
  the baseline must pass. Record the verify step's wall time against main's.
- Check the accept/reject split the reference reports for the hard cases.

## Left to the builder

- How the CBOR walker tags features: hand-rolled, or `cbor2` in a scratch
  venv. It runs only at curation time.
- Exact per-tag counts, and which Koios endpoints are used to find each shape.
- If the heap walk over the whole pool turns out slow, cap it, for example at
  the pool's first 1,000 accepted txs plus all hard cases, and note the cap.
