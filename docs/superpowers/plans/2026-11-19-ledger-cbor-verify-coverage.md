# ledger-cbor verify coverage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `judge.py verify` checks every tx in the pool, plus a committed,
curated set of hard-case mainnet txs, each with a seeded mutant.

**Architecture:** A stdlib CBOR walker (`cborscan.py`) tags Conway tx
features. A one-off curation script (`tricky.py`) collects candidate txs from
Koios, tags them and selects the hard cases into `corpus/tricky.*`, which are
committed. `judge.py` builds its cases with a pure `build_cases()` that tests
can call without the harness binaries.

**Tech Stack:** Python 3 standard library only (no pip on the runner), and
`unittest`. Koios is used at curation time only.

**Spec:** `docs/superpowers/specs/2026-11-19-ledger-cbor-verify-coverage-design.md`

## Global Constraints

- Standard library only, in every file under `challenges/ledger-cbor/verifier/`.
- Hard cases are committed (`corpus/tricky.hex.gz`, `corpus/tricky.json`). Nothing fetches them at verify time.
- The hard cases' case kind is `tricky`, and their mutants' kinds are `tricky-<mutation>`. Only parity with the reference counts for them. A reference reject of a hard case is never infra.
- `valid` = every pool tx. Pool mutants: `MUTANT_N = 600`. `SCORE_N` (800) and `score()` are unchanged.
- Hard-case set: at most 200 txs.
- Koios User-Agent stays `cb-ledger-cbor`. Reuse `pool.call` for retries and backoff.
- Branch `ledger-cbor-verify-coverage`. Commit messages must not contain `[skip ci]` or `[skip actions]`.
- Run tests with: `python3 -m unittest discover -s challenges/ledger-cbor/verifier -p 'test_*.py' -v`

## Review Focus

- A pre-Conway tx, or one with an unusual shape (3-element Shelley tx, unknown body keys), passed to `features()`: tags what it can and never raises (Task 1 test).
- A truncated or garbage CBOR input to `features()`: returns `{"unparsed"}`, no exception (Task 1 test).
- A pool smaller than `MUTANT_N` (a custom local pool): mutants = pool size, no `ValueError` from `sample` (Task 3 test).
- `tricky.hex.gz` and `tricky.json` out of sync: `tricky()` raises, and verify reports infra, not a candidate failure (Task 3 tests).
- A candidate that disagrees only on a hard case, or only on a hard-case mutant: verify fails and names the `tricky*` case (Task 3 test).

---

### Task 1: CBOR feature tagger

**Files:**
- Create: `challenges/ledger-cbor/verifier/cborscan.py`
- Test: `challenges/ledger-cbor/verifier/test_cborscan.py`

**Interfaces:**
- Produces: `features(tx: bytes) -> set[str]`. `TAGS: tuple[str, ...]`, the
  fixed vocabulary that Task 2 iterates over (every tag `features` can return,
  with `<n>` expanded).

Tag vocabulary (Conway CDDL; tx = `[body, wits, is_valid, aux]`):

| Tag | Condition |
|---|---|
| `gov-proposal:<0..6>` | body key 20, each proposal's `gov_action[0]` |
| `vote:<0..4>` | body key 19, each voter `[tag, hash]`'s tag |
| `cert:<0..18>` | body key 4, each cert's `[0]` |
| `withdrawals`, `validity-start`, `mint`, `required-signers`, `ref-inputs`, `collateral-return`, `treasury-value`, `donation` | body keys 5, 8, 9, 14, 18, 16, 21, 22 present |
| `native-script`, `bootstrap-witness`, `datums`, `redeemers`, `plutus-v1`, `plutus-v2`, `plutus-v3` | witness keys 1, 2, 4, 5, 3, 6, 7 present |
| `legacy-output` | an output is an array, not a map |
| `inline-datum` / `ref-script` | a map output has key 2 = `[1, _]` / key 3 |
| `byron-address` / `pointer-address` | an output address's first byte `>> 4` is 8 / in (4, 5) |
| `many-inputs` / `many-outputs` | ≥ 50 inputs / outputs |
| `multiasset-heavy` | one output value holds ≥ 50 assets |
| `set-tag-258` / `set-no-tag` | any tag 258 anywhere / body key 0 is a plain array |
| `indefinite` | any indefinite-length array, map, bytes or text |
| `aux-data` / `invalid-tx` | `tx[3]` not null / `tx[2]` is false |
| `unparsed` | the bytes aren't one well-formed CBOR item |

- [ ] **Step 1: Write the failing tests.** Build inputs with a small
  `enc()` test helper (hand-written CBOR bytes, no library).
  - `test_minimal_tx`: `[{0: [], 1: [], 2: 0}, {}, True, None]` → `{"set-no-tag"}`.
  - `test_body_keys`: a body with keys 5, 9, 18, 22 → includes `withdrawals`, `mint`, `ref-inputs`, `donation`.
  - `test_governance`: key 20 = one proposal with action `[6]` and key 19 = voter `[2, h]` → includes `gov-proposal:6`, `vote:2`.
  - `test_outputs`: a legacy array output with a Byron address (`0x82…`) plus a map output with keys 2 = `[1, d]` and 3 → `legacy-output`, `byron-address`, `inline-datum`, `ref-script`.
  - `test_tag258_and_indefinite`: inputs as `tag(258, [...])` encoded with `0x9f … 0xff` → `set-tag-258`, `indefinite`, and not `set-no-tag`.
  - `test_witnesses`: witness map with keys 3 and 7 → `plutus-v1`, `plutus-v3`.
  - `test_invalid_tx`: `tx[2] = False` → includes `invalid-tx`.
  - `test_shelley_three_elements`: `[body, wits, None]` → no exception; the body tags are still found.
  - `test_garbage`: `b""`, `b"\x9f"`, `b"\x80\x00"` (trailing byte) → `{"unparsed"}`.
  - `test_tags_vocabulary`: every tag returned by any test above is in `TAGS`.
- [ ] **Step 2: Run the tests; they fail** with `ModuleNotFoundError: cborscan`.
- [ ] **Step 3: Implement `cborscan.py`.** A recursive decoder over major
  types 0 to 7, with definite and indefinite lengths, returning plain Python
  values, `Tag(n, v)` for tags, and recording whether it saw a
  tag 258 or an indefinite item. `features()` walks the decoded tx
  defensively: any shape it doesn't expect is skipped, never raised. A decode
  error or trailing bytes → `{"unparsed"}`.
- [ ] **Step 4: Run the tests; all pass.**
- [ ] **Step 5: Commit** on a new branch `ledger-cbor-verify-coverage`, including the spec and this plan: `ledger-cbor: CBOR feature tagger for curating hard-case txs`

### Task 2: Curate and commit the hard-case set

**Files:**
- Create: `challenges/ledger-cbor/verifier/tricky.py`
- Test: `challenges/ledger-cbor/verifier/test_tricky.py`
- Create (generated): `challenges/ledger-cbor/corpus/tricky.hex.gz`, `challenges/ledger-cbor/corpus/tricky.json`

**Interfaces:**
- Consumes: `cborscan.features`, `cborscan.TAGS`, `pool.call`.
- Produces: `corpus/tricky.hex.gz` (one hex tx per line) and
  `corpus/tricky.json`: `{"source", "fetched", "txs": [{"hash", "bytes",
  "block_height", "tags": [...], "why": [...]}], "missing_tags": [...]}`.
  This records `block_height` in place of the spec's `epoch`, because
  `tx_cbor` returns it directly (planner's call).
  The order of `txs` matches the hex lines. `why` lists the selection reasons
  (`tag:<t>`, `largest`, or `pv:<n>`).
- `select(cands: dict[str, dict], per_tag: int = 5, largest: int = 20, cap: int = 200) -> list[str]`.
  Each cand is `{"bytes": int, "tags": set[str], "pv": int | None}`. Returns
  tx hashes, deterministically.

- [ ] **Step 1: Write the failing `select` tests.**
  - `test_per_tag_takes_largest_first`: 7 cands tagged `mint` with sizes 1..7, `per_tag=5` → the 5 largest are selected.
  - `test_largest_overall`: `largest=2` → the two biggest txs are selected even with no tags.
  - `test_pv_buckets`: cands with `pv` 8, 9, 10 and `per_tag=1` → one tx per pv.
  - `test_cap`: 500 distinct tagged cands, `cap=200` → `len == 200`, and every tag that has a cand is still represented (rare tags are filled first: process tags in ascending candidate count).
  - `test_deterministic`: the same input twice → the same list. Ties are broken by hash.
  - `test_no_duplicates`: a tx carrying many tags appears once.
- [ ] **Step 2: Run the tests; they fail.**
- [ ] **Step 3: Implement `select`, then `gather()` and `main()`** in `tricky.py`.
  `gather()` collects candidate hashes from Koios using these sources. Check
  each endpoint against the Koios API docs before relying on it.
  1. Every proposal tx (`proposal_list`).
  2. Up to 100 vote txs per voter role (`vote_list`).
  3. Script-creation txs for Plutus V1, V2 and V3 (the script-list endpoint), up to 50 each.
  4. Txs from the 100 largest-recent blocks (`blocks?block_size=gt.80000`).
  5. Txs from 150 random blocks across the last ~2 years, recording each block's `proto_major` as `pv`.

  Then it fetches the CBOR for each hash with `tx_cbor` (batches of 50), runs
  `features`, calls `select`, and writes both files. Tags that no candidate
  carries go in `missing_tags`. It prints a per-tag count table.
- [ ] **Step 4: Run the tests; all pass.**
- [ ] **Step 5: Run the curation:** `cd challenges/ledger-cbor && python3 verifier/tricky.py`.
  Expected: at most 200 txs, and a count table in which every `gov-proposal:*`,
  `vote:*` and `plutus-v*` tag is present. Report `missing_tags` to the user,
  since `pointer-address` may legitimately be missing. Check the
  `tricky.hex.gz` size is under 1 MB.
- [ ] **Step 6: Commit** the script, the tests and both corpus files: `ledger-cbor: curated hard-case txs (corpus/tricky.*)`

### Task 3: Judge verifies the whole pool and the hard cases

**Files:**
- Modify: `challenges/ledger-cbor/verifier/judge.py` (constants, `verify`, the new `build_cases` and `tricky`, the docstring)
- Test: `challenges/ledger-cbor/verifier/test_judge.py`
- Modify: `challenges/ledger-cbor/README.md` (the **Correct** paragraph and the corpus row of the table), `challenges/ledger-cbor/THREAT_MODEL.md` (the finite-inputs bullet)

**Interfaces:**
- Consumes: `corpus/tricky.hex.gz` and `corpus/tricky.json` (Task 2).
- Produces: `tricky(path: Path = ROOT / "corpus/tricky.hex.gz") -> list[bytes]`,
  which raises `ValueError` if the hex line count ≠ `len(json["txs"])`.
  `build_cases(seed: int, p: list[bytes], hard: list[bytes]) -> list[tuple[str, bytes]]`.
  The order is: `valid` (all of `p`, in pool order), then pool mutants
  (`min(MUTANT_N, len(p))`), then `tricky`, then one `tricky-<kind>` mutant per
  hard case, then the 4 edge cases. All randomness comes from
  `random.Random(seed)`.

- [ ] **Step 1: Write the failing tests.** Use a synthetic pool (e.g. 900 random
  byte strings with a fixed seed) and 5 hard cases. For `verify` tests,
  monkeypatch `judge.pool`, `judge.tricky` and `judge.lines`. The fake
  `lines(exe, mode, path)` reads the hex file and answers per input from
  dicts keyed by input bytes. The reference answers `ok <hash>` by default;
  the candidate answers the same unless overridden. For `nf` it answers
  `nf 0 0`.
  - `test_valid_is_whole_pool`: the `valid` cases == `p`.
  - `test_scored_txs_are_verified`: `set(score_sample(seed, p)) <= set(valid bytes)`.
  - `test_pool_mutants`: 600 pool mutants. A 10-tx pool → 10 mutants, no exception.
  - `test_each_hard_case_and_mutant`: 5 `tricky` cases; 5 cases whose kind starts with `tricky-` and ends in a mutation kind.
  - `test_seeded`: the same seed → identical cases; a different seed → different mutants.
  - `test_reference_rejects_hard_case_not_infra`: the reference answers `err` for one hard case, and the candidate does too → `ok` True, no `infra` key, and the detail contains `tricky: 4 accepted, 1 rejected by reference`.
  - `test_candidate_differs_on_hard_case`: the candidate differs only on one hard case → `ok` False, and the detail names `tricky`.
  - `test_candidate_differs_on_hard_mutant`: the same, on a `tricky-*` mutant → `ok` False.
  - `test_reference_rejects_pool_tx_is_infra`: the reference answers `err` for a `valid` tx → `infra` True.
  - `test_tricky_out_of_sync`: write a 2-line hex and a 3-entry json in a temp dir → `tricky()` raises `ValueError`. With `judge.tricky` raising, `verify` returns `infra` True.
  - `test_committed_tricky_loads`: `tricky()` on the real committed files returns between 1 and 200 txs.
- [ ] **Step 2: Run the tests; they fail.**
- [ ] **Step 3: Implement.** Remove `VALID_N` and the separate `scored` cases.
  Set `MUTANT_N = 600`. In `verify()`, use `build_cases(seed, pool(), tricky())`;
  a failure loading `tricky()` → `{"ok": False, "infra": True, "detail": "infra: hard-case corpus: ..."}`.
  The infra check for reference rejects applies to `valid` only. Add
  `tricky: <a> accepted, <r> rejected by reference` and
  `(includes the <SCORE_N> scored txs)` to the detail. The heap walk runs on
  every reference-accepted input, as it does today.
- [ ] **Step 4: Run all verifier tests; all pass.**
- [ ] **Step 5: Update the docs.**
  - README **Correct** paragraph: every pool tx, plus `corpus/tricky.*`
    (≤ 200 curated mainnet hard cases, with their tags), plus mutants.
  - Add a table row for `corpus/tricky.*`.
  - THREAT_MODEL finite-inputs bullet: "each run verifies every pool tx + 600
    mutants + the committed hard cases and one mutant each". Hard cases are
    public and only verified, never scored, so tabling them can't pay.
  - `judge.py` docstring: say the same.
- [ ] **Step 6: Commit:** `ledger-cbor judge: verify the whole pool and the curated hard cases`

### Task 4: PR and Preflight check

- [ ] **Step 1: Push the branch and open a PR** linking the spec. If the push
  is denied, hand the user `! git push -u origin ledger-cbor-verify-coverage`.
  Expect CI on the PR to run and pass.
- [ ] **Step 2: Ask the user before dispatching Preflight** (`slug=ledger-cbor`,
  about 1 hour of Linux plus a short macOS leg). Then run
  `gh workflow run preflight.yml --ref ledger-cbor-verify-coverage -f slug=ledger-cbor`.
- [ ] **Step 3: Read the run.** Expected:
  - the baseline verifies `ok`;
  - all four `preflight/broken/*` variants fail verify;
  - the verify detail shows the hard cases' accept/reject split.

  Record the verify step's wall time and compare it with main's last Preflight.
  If verify takes more than 10 minutes on Linux, cap the heap walk at the
  first 1,000 reference-accepted `valid` inputs plus every `tricky*` input,
  note the cap in the README, add a test, and recheck.
- [ ] **Step 4: Report** to the user: the PR link, the hard-case split, the
  missing tags, the verify time, and the exact
  `! gh pr merge <n> --squash --admin` command.
