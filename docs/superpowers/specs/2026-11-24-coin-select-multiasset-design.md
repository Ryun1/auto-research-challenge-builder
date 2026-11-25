# coin-select: multi-asset and datum scoring (`wallet-multiasset@1`)

Date: 2026-11-24. Status: approved by Ryan 2026-11-24.

## Intent

coin-select today scores an ADA-only world: every UTxO and payment is one
`lovelace` integer, transaction size is `60 + 140·inputs + 65·outputs`, and
min-ada is a single constant. Real Cardano wallets hold native tokens and
receive UTxOs with datums, and both change the economics of coin selection:
token bundles make outputs bigger (more fee, more ADA locked as min-ada),
consolidating tokens frees locked ADA, and payments to scripts carry datums
that cost bytes. We want submissions scored in that world.

`challenge.toml` already reserves this as a deferred contract,
`wallet-multiasset@0`, with the bar: *"a multi-asset size and min-ada model,
checked against cardano-ledger"*. This design meets that bar and opens the
contract.

## Goals

- A new open contract, `wallet-multiasset@1`, beside `wallet@1`, where
  wallets hold native tokens and UTxOs may carry datums.
- Transaction size, fee and min-ada computed **exactly** from the real
  Conway-era CBOR encoding, not a per-item byte estimate.
- That encoding checked against real mainnet transactions (fixtures fetched
  once from Koios, committed, verified offline in the self-test).
- The model published to solvers as readable reference code, so there is no
  hidden part of the formula to probe.

## Non-goals

- Script-locked inputs: no redeemers, scripts, reference scripts, execution
  units, collateral. Every wallet UTxO sits at one of the wallet's own key
  addresses.
- Minting/burning, metadata, certificates, withdrawals, validity intervals,
  required signers, the reference-script fee tier.
- Changing `wallet@1`: its judge, sim, results and three submissions stay
  untouched.

## Anti-goals (failure even if it "works")

- A size or min-ada model that differs from the ledger anywhere a solver can
  reach. Solvers will find and farm the gap; that is what the fixture check
  exists to prevent.
- A trusted judge that depends on non-stdlib code (pycardano, CSL, ...).
- Breaking or re-scoring `wallet@1` submissions.

## Approaches considered

- **A. Exact CBOR encoder in stdlib Python, checked against mainnet
  transactions (chosen).** The judge builds each transaction's real encoding
  and measures it. Faithful by construction; the check is mechanical; the
  encoder doubles as the solver-facing reference. Cost: a few hundred lines
  of encoder plus fixture tooling.
- **B. Per-item byte parameters** (extra cost per policy, per asset, per
  datum byte), like today's 140/65. Quick, but CBOR length prefixes and map
  grouping make real sizes non-linear; the edges are exploitable and it does
  not meet the "checked against cardano-ledger" bar.
- **C. Call pycardano / cardano-serialization-lib from the judge.** Faithful,
  but pulls a large dependency into the trusted judge and the runner image.

Ryan picked A.

## Decided scope for datums

- **On deposits:** some wallet UTxOs carry an inline datum or a datum hash
  (at the wallet's key address, so they are spendable with just a vkey
  witness). Spending them costs nothing extra in fees, since an input is
  only a reference; they matter because their outputs locked more min-ada.
- **On payment outputs:** some payments ask for an output with an inline
  datum or a datum hash (e.g. paying a script address). That output's size
  counts in the fee and its lovelace must meet its own min-ada; the judge
  supplies the payment's amounts already meeting it.
- **Change outputs carry no datum.**
- Script-locked inputs are out (see non-goals).

## Design

### Layout

```
challenges/coin-select/contracts/wallet-multiasset/
  INTERFACE.md     protocol + model, points at the reference code
  CHOICES.md       the choice space
  judge            same shape as contracts/wallet/judge
  sim.py           encoder, size/fee/min-ada, generator, validator, reference
  fixtures/
    fetch.py       dev-time only, uses the network (Koios); not run by CI
    mainnet.jsonl  committed fixtures: raw tx CBOR hex + expected facts
challenges/coin-select/submissions/baseline-multiasset/   (contract = wallet-multiasset@1)
challenges/coin-select/preflight/broken/<new cases>/      (contract = wallet-multiasset@1)
```

`challenge.toml`: replace the deferred `wallet-multiasset@0` entry with an
open `wallet-multiasset@1` (judge, interface, choice_space); scope the
`valid-txs` and `serves-feasible` properties and the axes to it (axes take a
`contracts` list, see `spec.rs::axes_for`). Preflight requires at least one
submission per open contract, hence `baseline-multiasset`.

`wallet/sim.py` stays as is. The new `sim.py` may import nothing from it
(keep the contracts independent; copy `HashRandom`).

### The byte model (the heart of it)

The judge serialises the transaction it would submit and measures it:

**Encoding principle (my call, after the builder check):** the ledger accepts
several encodings of the same transaction (legacy array vs map outputs, sets
with or without tag 258) and keeps whatever bytes the wallet submitted. The
judge charges the **cheapest valid encoding** a careful wallet would emit,
so a solver is never overcharged relative to a real wallet and has no
encoding trick left to win with. The encoder supports every form, so the
fixture check can rebuild each mainnet transaction in the forms it actually
used and byte-match it; the judge then always picks the cheapest.

- **Transaction** = `[body, witness_set, true, null]` (Conway, no aux data).
- **Body** = map `{0: inputs, 1: outputs, 2: fee}`. Inputs are a CBOR set
  of `[txid(32 bytes), index]`, judge form untagged (tag 258 is optional and
  costs 3 bytes). Index width matters: `uint` encoding of the output index
  is 1 byte for 0–23, 2 for 24–255.
- **Output positions:** the judge places the payment's outputs first, then
  change, so change `k` sits at index `n_payment_outputs + k`, and that real
  index is what a later spend of it encodes. The solver does not control
  ordering. Wallet UTxO ids stay `p7#k` (k = change number) for the
  protocol; the judge maps them to real indices internally, and deposit
  UTxOs use the `#k` suffix as their index.
- **Outputs**, judge form: the legacy array form `[address, value]` or
  `[address, value, datum_hash]` when there is no inline datum (2 bytes
  cheaper), the map form `{0: address, 1: value, 2: datum_option}` only
  when there is an inline datum. Value is `coin` when there are no assets,
  else `[coin, {policy(28 bytes): {name(0–32 bytes): qty}}]`. Map order
  never changes size; the judge sorts policies and names canonically, and
  the fixture check keeps each original transaction's order. Quantities are
  uints with real CBOR width (so 1 and 2^40 cost different bytes). Datum
  option is `[0, hash32]` or `[1, tag24(bytes(datum_cbor))]`.
- **Witness set** = `{0: set of vkey witnesses}` (untagged in the judge
  form), one per distinct key among the inputs. Each witness is `[vkey(32), sig(64)]`. Signatures are fixed
  size, so dummy bytes give the exact size.
- **Synthetic ids**: each UTxO's txid is `sha256(utxo id string)`; the index
  is the `#k` suffix. Deterministic, real widths.
- **Addresses**: the wallet has `K = 8` payment keys (my call); each wallet
  UTxO carries an `addr` index 0..K-1 and is a 57-byte base address (key
  payment + key stake). Payment destination addresses are generated per
  payment: 57-byte base, or 29-byte enterprise, or script base address
  (when the output carries a datum).
- **size** = `len(cbor(tx)) - 1`: the ledger sizes `[body, wits, aux]` without the `is_valid` byte (Alonzo `toCBORForSizeComputation`, kept in Conway; found by the fixture check, 537 mainnet txs pay exactly 44 lovelace under `len(cbor(tx))`). **min_fee** = `min_fee_a · size + min_fee_b`; `max_tx_size` applies to the same size.
- **min_ada(output)** = `(160 + len(cbor(output))) · coins_per_utxo_byte`.
  Because the output's own lovelace field is in its encoding, a change
  output is valid when `lovelace ≥ min_ada(that output, as encoded)`.
  (Babbage/Conway rule; verified on fixtures.)
- **Limits**: `size ≤ max_tx_size (16384)`; every output's value encoding
  `≤ max_val_size (5000)`.

Parameters (mainnet, current epoch): `min_fee_a 44`, `min_fee_b 155381`,
`coins_per_utxo_byte 4310`, `max_tx_size 16384`, `max_val_size 5000`. Sent
in the `params` message.

### Checking it against the ledger (the admission bar)

`fixtures/fetch.py` (dev-time, network) pulls real mainnet Conway-era
transactions from Koios that have: no scripts, redeemers, collateral,
certificates, withdrawals or mint; at least one multi-asset output; and,
across the set, inline datums, datum hashes, array- and map-form outputs,
tagged and untagged sets, 1–N vkey witnesses, both small and large token
bundles. Target ~60 transactions. For each it stores the raw CBOR hex and
the on-chain fee.

**Out-of-scope fields are opaque bytes (my call).** Nearly every real
wallet tx sets a TTL, and many carry network_id, an aux-data hash and
metadata. The rebuild treats those body entries and the auxiliary data as
opaque: it copies their raw encoded bytes through (key + value) and counts
their length, and rebuilds everything in scope (inputs, outputs, fee,
witnesses) itself. That keeps the candidate pool realistic while every
byte our judge model produces is still checked.

The self-test (offline) asserts for every fixture:

1. **Round-trip bytes**: decode the raw tx, rebuild it with our encoder from
   its semantic content (inputs, outputs, fee, witness count) in the
   original's forms (array/map, tagged/untagged, original map order), plus
   the opaque fields, and assert the bytes equal `raw`. Transactions using
   indefinite-length or non-minimal integer encodings are filtered out at
   fetch time, and the count of each excluded kind is noted in the fixture
   file.
2. **Per-output bytes**: each rebuilt output's bytes equal the raw output's
   bytes, and the judge-form (cheapest) encoding of the same output is never
   longer than the original.
3. **Fee**: `on-chain fee ≥ min_fee(size)`, and for transactions whose fee
   equals the ledger minimum (common for wallet-built txs) it is equal. At
   least a third of fixtures should be exact-minimum ones.
4. **Min-ada**: every fixture output satisfies `lovelace ≥ min_ada(output)`
   (the chain accepted it), and for a hand-picked handful of outputs from
   wallets known to set exactly-minimum ADA it is equal.

A tiny stdlib CBOR decoder lives in `fixtures/` test code only (or sim.py if
simpler); the judge itself only encodes.

`fetch.py` is not trusted judge code and may use the network; the fixtures
it writes are committed, so CI and preflight never touch Koios. See the
memory note on Koios quirks (PostgREST `order=` needs the column in
`select=`; retry behaviour on 400s).

### Protocol (JSON lines, same envelope as wallet@1)

Values everywhere:

```json
{"lovelace": 5000000, "assets": {"<policy hex 56>": {"<name hex 0-64>": 1}}}
```

`assets` may be omitted or `{}`. Quantities are positive integers.

- `params`: as above plus `max_val_size`, `keys: 8`, and nothing about
  sizes (the model is the reference code, see below).
- `deposit`: `{"type":"deposit","utxos":[{"id":"d3#0","addr":2,"value":{...},
  "datum":{"inline":"<cbor hex>"} | {"hash":"<hex 64>"} | absent}]}`.
- `pay`: `{"type":"pay","id":"p7","outputs":[{"address":"<hex>","value":{...},
  "datum":... | absent}]}`. Outputs are given fully; the participant never
  echoes them.
- reply: `{"inputs":["d3#0",...],"change":[{"addr":0,"value":{...}}, ...],
  "fee":200000}` or `{"refuse":true}`. Change `k` becomes wallet UTxO
  `p7#k` at key `addr`, with no datum.

Validity (all mechanical, under `valid-txs`): wallet@1's rules, plus
per-asset balance (`Σ inputs = Σ outputs + Σ change` for lovelace (+fee)
and for every asset, exactly; no zero or negative quantities in change),
every change `addr` in `0..K-1`, `fee ≥ min_fee(size)`, every change output
meets its own min-ada and `max_val_size`, `size ≤ max_tx_size`.

INTERFACE.md states the model in prose and says: **`sim.py`'s `tx_size`,
`min_fee`, `min_ada` are the definition**; solvers may copy it.

### Scenario generator

Same shape as wallet@1 (8 scenarios × 300 events from `CB_SEED` via
`HashRandom`), adding:

- A per-scenario token universe: ~5–30 policies, a mix of fungible tokens
  (few names, big quantities, varied uint widths) and NFT collections (many
  names, quantity 1). Name lengths 0–32 bytes.
- Deposits: most ADA-only; some carry 1 asset; a few carry big bundles
  (tens of assets, to exercise `max_val_size` and size pressure). Each
  deposit UTxO's lovelace ≥ its own min-ada. Some carry inline datums (small
  to ~1 KB) or datum hashes.
- Payments: mostly ADA-only as today; some request tokens the wallet
  currently holds; some carry a datum. Every payment output's lovelace ≥ its
  own min-ada.
- Fundability: the generator tracks the wallet's ADA and per-asset balances
  along the event stream and only requests what is fundable assuming each
  earlier tx cost at most `FEE_BUDGET` and locked at most a bounded amount
  of min-ada in change (the exact margin is the builder's call; it must
  keep "any prefix is fundable" true, asserted in the self-test via the
  reference).

Distribution parameters are the builder's call, tuned so the reference
pays every payment on many seeds and token handling is a real share of the
cost (not a rounding error).

### Reference selector (`serves-feasible`)

A multi-asset largest-first: for each requested asset, take the wallet
UTxOs holding most of it until covered; then lovelace largest-first; one
change output at key 0 holding all leftover assets and lovelace; if that
change breaks `max_val_size`, split leftover assets across change outputs
until each fits. Fold sub-min-ada leftover into the fee as wallet@1 does.
Refusal is allowed only when this reference cannot build a valid tx on the
participant's current wallet. It must also serve every generated payment
on its own wallet (self-test).

### Axes (my calls; the builder may tune tolerances from preflight)

Axis ids are unique across the challenge (`spec.rs` rejects duplicates).
So `fees`, `inputs_per_tx`, `latency_ms` and `peak_rss_mib` stay unscoped
and shared by both contracts, with the same meaning and tolerance. `dust`
gets `contracts = ["wallet@1"]`. That is a config-only scoping change, with
no change to its definition or to any wallet@1 result. Two new axes are
scoped to `wallet-multiasset@1`:

| axis | definition | direction |
|---|---|---|
| `fees` (shared) | total fees in ADA, mean over scenarios | minimise |
| `locked_ada` | Σ min_ada over wallet UTxOs at the end, in ADA, mean over scenarios | minimise |
| `ada_dust` | UTxOs with under 1 ADA above their own min_ada at the end (tokens and datums included), mean over scenarios | minimise |
| `inputs_per_tx` (shared) | as wallet@1 | minimise |
| `latency_ms` (shared) | as wallet@1 | minimise |
| `peak_rss_mib` (shared) | as wallet@1 | minimise |

`locked_ada` is the new tension: consolidating token UTxOs frees ADA but
costs bytes now; fragmenting change for granularity locks more. `ada_dust`
counts free ADA above each UTxO's own min-ada, so token UTxOs aren't
punished for their min-ada but can't hide small ADA either. (Revised
2026-11-25: the first rule, ADA-only UTxOs under 2 ADA, let a solver
exempt small change by adding 1 token unit.)
If shared `fees` turns out to need a different tolerance for the new
contract, split it into a scoped `ma_fees` then.

### Baseline and preflight

- `submissions/baseline-multiasset/`: the reference selector as a Python
  `select` (taxonomy as in baseline, new strategy value if taxonomy needs
  one).
- New broken cases under `preflight/broken/`, each `contract =
  wallet-multiasset@1` with `EXPECT`: asset imbalance (one token unit
  missing), change one lovelace below its own min-ada, fee computed with
  wallet@1's linear model, change exceeding `max_val_size`, unknown `addr`.
  The "linear model fee" case is the important one: it proves the exact
  encoder is what's enforced.

### Docs and site

- Update `challenges/coin-select/README.md` (contracts, new axis),
  `DIRECTIONS.md`/`PROMPT.md` if they assume ADA-only, `taxonomy.toml` if
  new dimensions are needed (e.g. token change policy).
- The site already renders open/deferred contracts; no site changes
  expected.

## Left to the builder

- Exact generator distributions and the fundability margin.
- How fixtures are selected from Koios and how many (≥ 40, covering the
  listed shapes).
- Tolerances for the new axes, tuned from preflight spread.
- Whether a minimal CBOR decoder sits in `sim.py` or the test harness.
- Code structure inside `sim.py` and the judge (mirror wallet@1's judge).

## Calls I made (for Ryan to check)

- New contract id `wallet-multiasset@1` (the deferred `@0` was a
  placeholder).
- Wallet has 8 keys; witnesses counted per distinct key; change names its
  key. This makes input cost realistic (~40 bytes per input plus ~100 per
  new key) instead of wallet@1's flat 140, and adds a key-reuse choice.
- New axes `locked_ada` and `ada_dust` (UTxOs under 1 ADA above min_ada),
  scoped to the new contract; `dust` scoped to wallet@1; the other four
  axes shared.
- Change outputs never carry datums.
- The judge charges the cheapest valid encoding (array-form outputs unless
  there's an inline datum, untagged sets).
- Change sits after the payment outputs; the solver doesn't control order.
- The fixture rebuild passes TTL, metadata and other out-of-scope fields
  through as opaque bytes.
