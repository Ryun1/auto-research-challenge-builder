# coin-select `wallet-multiasset@1` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Open a second coin-select contract, `wallet-multiasset@1`, in which wallets hold native tokens and UTxOs may carry datums. Size, fee and min-ada come from exact Conway CBOR encodings, checked byte for byte against real mainnet transactions.

**Architecture:** A new directory `challenges/coin-select/contracts/wallet-multiasset/` holds stdlib-only Python modules:
- `cbor.py`: CBOR encode and decode.
- `model.py`: the byte model, which solvers are told is the definition.
- `sim.py`: generator, validator and reference selector.
- `judge`: a copy of wallet@1's judge with the payloads swapped.

A dev-time `fixtures/fetch.py` pulls mainnet transactions from Koios into a committed `fixtures/mainnet.jsonl`, and `test_fixtures.py` checks the model against them offline. `wallet@1` stays untouched except that its `dust` axis is scoped to it in `challenge.toml`.

**Tech Stack:** Python 3 stdlib (judge side and the baseline), `unittest`, and the `cb` Rust CLI for preflight. Koios is used only by `fetch.py`.

**Spec:** `docs/superpowers/specs/2026-11-24-coin-select-multiasset-design.md`

## Global Constraints

- Judge-side code (`cbor.py`, `model.py`, `sim.py`, `judge`) is **stdlib only**. It imports nothing from `contracts/wallet/` (copy `HashRandom`).
- Parameters: `min_fee_a 44`, `min_fee_b 155381`, `coins_per_utxo_byte 4310`, `max_tx_size 16384`, `max_val_size 5000`, `keys 8`.
- `min_ada(output) = (160 + len(cbor(output))) * coins_per_utxo_byte`. `min_fee = min_fee_a * len(cbor(tx)) + min_fee_b`.
- **Judge form** (always the cheapest valid encoding):
  - tx is `[body, witness_set, true, null]`, body is `{0: inputs, 1: outputs, 2: fee}`, and sets are untagged arrays.
  - Outputs without an inline datum use the array form `[addr, value]` or `[addr, value, datum_hash]`. Outputs with an inline datum use the map form `{0: addr, 1: value, 2: [1, tag24(bytes)]}`.
  - Value is a bare `uint` when there are no assets, else `[coin, {policy: {name: qty}}]`, with policies and names sorted canonically (shorter first, then bytewise).
  - The witness set is `{0: [[vkey32, sig64], ...]}`, with one witness per distinct wallet key among the inputs.
- Payment outputs come first, then change. Change `k` of payment `p` has id `p#k` and on-chain index `len(payment outputs) + k`. Deposit UTxO `d<e>#k` has index `k`. Each UTxO's txid is `sha256(<id up to '#'>)`.
- Wallet addresses are 57-byte base key addresses, header `0x01`, with key i's hashes derived as `sha256(b"key%d" % i)[:28]` (payment and stake alike). Payment destinations are a 57-byte base address (header `0x01`), a 29-byte enterprise address (header `0x61`), or a 57-byte script base address (header `0x11`) when the output has a datum.
- Change outputs never carry a datum. A change `addr` must be in `0..7`.
- Contract id `wallet-multiasset@1`. Property ids must be unique across the challenge (`spec.rs:278`), so the new contract's properties are `ma-valid-txs` and `ma-serves-feasible` (my call). Axes:
  - `fees`, `inputs_per_tx`, `latency_ms` and `peak_rss_mib` are shared (unscoped).
  - `dust` gets `contracts = ["wallet@1"]`.
  - `locked_ada` (ADA) and `ada_dust` (UTxOs) are new, with `contracts = ["wallet-multiasset@1"]`.
- `wallet@1`'s judge, sim, interface, submissions and `results.jsonl` are not modified.
- Run tests with: `python3 -m unittest discover -s challenges/coin-select/contracts/wallet-multiasset -p 'test_*.py' -v` (from the repo root). `sim.py --self-test` runs the same discovery.

## Review Focus

1. **A reply with an asset key that is valid JSON but not valid hex, the wrong length, or an odd number of characters** (e.g. a 54-char policy, or a name of 66 hex chars): this must be a plain `ma-valid-txs` failure, never a judge crash or infra. Test: `test_sim.py::test_rejects_bad_asset_keys`.
2. **A change output that includes an asset with quantity 0, or splits one asset across two change outputs.** Quantity 0 is rejected. Splitting is allowed and still balances. Test: `test_sim.py::test_zero_qty_rejected_split_allowed`.
3. **A UTxO spent at output index ≥ 24** (change from a 3-output payment never gets there, but initial deposits of 60–120 UTxOs do): the input encodes a 2-byte index. Test: `test_model.py::test_input_index_width`.
4. **A change whose lovelace sits exactly at its own min-ada, where one lovelace less would shrink the encoding**: crossing a CBOR uint width boundary (e.g. 65536 → 65535 isn't reachable near min-ada, but 2^32 can be with big bundles) changes the size. min-ada must be checked against the encoding with the actual lovelace. Test: `test_model.py::test_min_lovelace_fixed_point`.
5. **A participant that refuses when the reference would have to split change because of `max_val_size`**: refusal must still be judged against the reference that splits. Test: `test_sim.py::test_reference_splits_oversized_change`.

---

### Task 1: CBOR encode/decode

**Files:**
- Create: `challenges/coin-select/contracts/wallet-multiasset/cbor.py`
- Test: `challenges/coin-select/contracts/wallet-multiasset/test_cbor.py`

**Interfaces:**
- Produces:
  - `class Tag(NamedTuple): tag: int; value: object`
  - `class Raw(NamedTuple): data: bytes` holds pre-encoded bytes and is emitted verbatim.
  - `class NonCanonical(ValueError)`
  - `encode(obj) -> bytes` accepts `int` (≥0 as major 0, <0 as major 1), `bytes`, `str`, `list`, `dict` (insertion order kept), `bool`, `None`, `Tag`, `Raw`. It always uses minimal-length heads.
  - `decode(data: bytes, offset: int = 0) -> tuple[object, int]` returns the value and the end offset. Maps decode to `dict` in original order. It raises `NonCanonical` on indefinite-length items, non-minimal heads, or duplicate map keys.
  - `decode_map_spans(data: bytes, offset: int) -> list[tuple[object, int, int]]` decodes the map at `offset` and returns `(key, value_start, value_end)` per entry, for passing fields through untouched.

- [ ] **Step 1: Write the failing tests**

```python
from cbor import encode, decode, Tag, Raw, NonCanonical, decode_map_spans
def test_uint_widths(self):
    for n, ln in [(0,1),(23,1),(24,2),(255,2),(256,3),(65535,3),(65536,5),(2**32-1,5),(2**32,9)]:
        self.assertEqual(len(encode(n)), ln)
def test_roundtrip(self):
    v = [1, b"\x00"*28, {0: b"ab", 1: [2, {b"p": {b"": 5}}]}, Tag(24, b"\x01"), True, None]
    self.assertEqual(decode(encode(v))[0], v)
def test_raw_passthrough(self):
    self.assertEqual(encode([Raw(b"\x01")]), b"\x81\x01")
def test_rejects_indefinite_and_nonminimal(self):
    for bad in (b"\x9f\x01\xff", b"\x18\x05"):
        with self.assertRaises(NonCanonical): decode(bad)
def test_map_spans(self):
    d = encode({0: 1, 3: 99999})
    spans = decode_map_spans(d, 0)
    self.assertEqual([k for k,_,_ in spans], [0, 3])
    k, s, e = spans[1]; self.assertEqual(decode(d[s:e])[0], 99999)
```

- [ ] **Step 2: Run and confirm failure.** Run: the discovery command (Global Constraints). Expected: `ModuleNotFoundError: No module named 'cbor'`.
- [ ] **Step 3: Implement `cbor.py`** to the interfaces above (about 120 lines).
- [ ] **Step 4: Run and confirm they pass.** Expected: 5 tests OK.
- [ ] **Step 5: Commit** `git add challenges/coin-select/contracts/wallet-multiasset/{cbor.py,test_cbor.py} && git commit -m "coin-select multiasset: stdlib CBOR encoder/decoder"`

### Task 2: The byte model

**Files:**
- Create: `challenges/coin-select/contracts/wallet-multiasset/model.py`
- Test: `challenges/coin-select/contracts/wallet-multiasset/test_model.py`

**Interfaces:**
- Consumes: `cbor.encode`, `cbor.Tag`, `cbor.Raw`.
- Produces:
  - `PARAMS: dict`: the params message, with the Global Constraints values, `"keys": 8`, and `"addresses": [hex of wallet key i's address for i in range(8)]`.
  - `ADA = 1_000_000`
  - `wallet_address(i: int) -> bytes` (57 bytes)
  - `Value = tuple[int, dict[bytes, dict[bytes, int]]]` (lovelace, assets). Assets map policy (28 bytes) to a map of name (0–32 bytes) to qty (>0).
  - `Datum = None | tuple[str, bytes]`, where the str is `"inline"` (CBOR bytes of the datum) or `"hash"` (32 bytes).
  - `Output(NamedTuple): address: bytes; value: Value; datum: Datum = None`
  - `value_cbor(v: Value, order: str = "canonical") -> object` is the CBOR object. `order="given"` keeps dict order (used by the fixture rebuild).
  - `output_cbor(o: Output, form: str = "judge", order: str = "canonical") -> object`. `form` is one of `"judge"`, `"array"` or `"map"`. Judge form is array unless the datum is inline. The map form uses the legacy-hash datum option `[0, h]` for `("hash", h)`.
  - `output_size(o: Output) -> int` is `len(encode(output_cbor(o)))`.
  - `value_size(v: Value) -> int`
  - `min_ada(o: Output, p=PARAMS) -> int`
  - `min_lovelace(address: bytes, assets, datum, p=PARAMS) -> int` is the smallest lovelace `l` with `l >= min_ada(Output(address, (l, assets), datum))`. It uses a fixed-point iteration from `l = 0`.
  - `tx_cbor(inputs: list[tuple[bytes, int]], outputs: list[object], fee: int, n_witnesses: int, tag_sets: bool = False, extra_body: list[tuple[int, Raw]] = (), aux: Raw | None = None) -> object`. `outputs` are already-built output CBOR objects. `extra_body` entries are appended to the body map after key 2 in the order given; key order in a rebuild comes from the caller. Witnesses are `[b"\x00"*32, b"\x00"*64]`.
  - `tx_size(inputs, outputs: list[Output], fee: int, n_witnesses: int) -> int` uses the judge form.
  - `min_fee(size: int, p=PARAMS) -> int`
  - `utxo_ref(uid: str, index: int) -> tuple[bytes, int]` returns `(sha256(uid.split("#")[0]), index)`.

- [ ] **Step 1: Write the failing tests**

```python
def test_ada_only_judge_output(self):
    o = Output(wallet_address(0), (2_000_000, {}))
    # array(2) + bytes(57) head 2 + 57 + uint 5
    self.assertEqual(output_size(o), 1 + 2 + 57 + 5)
    self.assertEqual(min_ada(o), (160 + 65) * 4310)
def test_inline_datum_uses_map_form(self):
    o = Output(wallet_address(0), (5*ADA, {}), ("inline", b"\x43abc"))
    self.assertEqual(encode(output_cbor(o))[0], 0xa3)
def test_hash_datum_uses_array_form(self):
    o = Output(wallet_address(0), (5*ADA, {}), ("hash", b"\x11"*32))
    self.assertEqual(encode(output_cbor(o))[0], 0x83)
def test_canonical_asset_order(self):
    p1, p2 = b"\x02"*28, b"\x01"*28
    v = (2*ADA, {p1: {b"bb": 1, b"a": 1}, p2: {b"z": 1}})
    m = value_cbor(v)[1]
    self.assertEqual(list(m), [p2, p1]); self.assertEqual(list(m[p1]), [b"a", b"bb"])
def test_input_index_width(self):
    one = tx_size([utxo_ref("d0#3", 3)], [], 0, 1)
    two = tx_size([utxo_ref("d0#30", 30)], [], 0, 1)
    self.assertEqual(two - one, 1)
def test_witness_cost(self):
    a = tx_size([utxo_ref("d0#1", 1)], [], 200000, 1)
    b = tx_size([utxo_ref("d0#1", 1)], [], 200000, 2)
    self.assertEqual(b - a, 101)  # [bytes32, bytes64] = 1 + 34 + 66
def test_min_lovelace_fixed_point(self):
    assets = {b"\x01"*28: {bytes([i]): 1 for i in range(40)}}
    l = min_lovelace(wallet_address(0), assets, None)
    self.assertGreaterEqual(l, min_ada(Output(wallet_address(0), (l, assets))))
    self.assertLess(l - 1, min_ada(Output(wallet_address(0), (l - 1, assets))))
def test_min_fee(self):
    self.assertEqual(min_fee(300), 44*300 + 155381)
```

- [ ] **Step 2: Run and confirm failure** (`No module named 'model'`).
- [ ] **Step 3: Implement `model.py`** to the interfaces. Module docstring: "This file is the definition of size, fee and min-ada for wallet-multiasset@1; solvers may copy it."
- [ ] **Step 4: Run and confirm they pass.**
- [ ] **Step 5: Commit** (`coin-select multiasset: exact Conway byte model`).

### Task 3: Mainnet fixtures and the ledger check

**Files:**
- Create: `challenges/coin-select/contracts/wallet-multiasset/fixtures/fetch.py` (dev-time, network, not imported by the judge)
- Create: `challenges/coin-select/contracts/wallet-multiasset/fixtures/mainnet.jsonl` (generated, committed)
- Create: `challenges/coin-select/contracts/wallet-multiasset/fixtures/README.md` (how to refresh, the filter, and excluded-kind counts)
- Test: `challenges/coin-select/contracts/wallet-multiasset/test_fixtures.py`

**Interfaces:**
- Consumes: `cbor.decode`, `cbor.decode_map_spans`, `cbor.Raw`, `cbor.encode`, everything in `model`.
- Produces: `mainnet.jsonl`, one object per line: `{"hash": str, "cbor": hex, "epoch": int, "fee": int, "exact_min_fee": bool}`. The first line is a header `{"type": "meta", "params": {...from Koios epoch_params...}, "excluded": {"indefinite": n, "non_minimal": n, "out_of_scope": n, ...}}`.

**fetch.py behaviour (my calls):**
- Walk recent mainnet blocks through Koios (`blocks`, then `block_txs`, then `tx_cbor` in batches of ≤ 50).
- Keep a tx only if all of these hold:
  - It decodes without `NonCanonical`.
  - Its body keys ⊆ `{0,1,2,3,7,8,15}`.
  - Its witness-set keys are exactly `{0}`.
  - No output has key 3 (script_ref).
  - It has ≥ 1 output with assets.
- Stop at 60 kept transactions once the set covers all of: an inline datum, a datum hash, both array- and map-form outputs, both tagged and untagged sets, ≥ 3 distinct witness counts, an output with ≥ 20 assets, an output whose lovelace equals `min_ada` exactly, and ≥ 20 transactions with `fee == min_fee(len(cbor))`.
- Record the epoch params and fail if they differ from `model.PARAMS`.
- Use the Koios quirks from memory: probe queries with curl first, put the `order=` column in `select=`, and don't retry 400s.

**Rebuild rule (in `test_fixtures.py`):**
- Decode the tx.
- For body keys 0/1/2, rebuild from semantic content using the original forms: tag 258 present or absent, array or map output, original map order (`order="given"`), and the datum option as found.
- Every other body key and the aux data become `Raw` from `decode_map_spans`, in their original key positions.
- The witness count is `len(witness_set[0])`.

- [ ] **Step 1: Write the failing tests**

```python
FIX = load()  # skips meta line
def test_enough_coverage(self):
    self.assertGreaterEqual(len(FIX), 40)
    self.assertGreaterEqual(sum(f["exact_min_fee"] for f in FIX), len(FIX) // 3)
def test_body_bytes_match(self):
    for f in FIX: self.assertEqual(rebuilt_body(f), original_body(f), f["hash"])
def test_tx_size_matches(self):
    for f in FIX: self.assertEqual(len(encode(rebuild(f))), len(bytes.fromhex(f["cbor"])), f["hash"])
def test_outputs_match_and_judge_form_never_longer(self):
    for f in FIX:
        for orig, out in outputs(f):
            self.assertEqual(encode(output_cbor(out, form=form_of(orig), order="given")), orig)
            self.assertLessEqual(output_size(out), len(orig))
def test_fee_floor(self):
    for f in FIX:
        mf = min_fee(len(bytes.fromhex(f["cbor"])))
        self.assertGreaterEqual(f["fee"], mf); self.assertEqual(f["exact_min_fee"], f["fee"] == mf)
def test_min_ada(self):
    exact = 0
    for f in FIX:
        for orig, out in outputs(f):
            need = (160 + len(orig)) * 4310
            self.assertGreaterEqual(out.value[0], need); exact += out.value[0] == need
    self.assertGreaterEqual(exact, 1)
```

- [ ] **Step 2: Run and confirm failure** (no fixtures file).
- [ ] **Step 3: Implement `fetch.py`** and run it once: `python3 challenges/coin-select/contracts/wallet-multiasset/fixtures/fetch.py`. Expected: it writes `mainnet.jsonl` and prints coverage counts and excluded counts.
- [ ] **Step 4: Implement the rebuild helpers in `test_fixtures.py`** and run the tests. Expected: all pass. **If a byte mismatch remains, the model is wrong. Fix `model.py`, never the fixture.** If the encoder needs a new form, add it to `model.output_cbor` behind the `form`/`order` args, keeping the judge form unchanged unless the mismatch shows the judge form itself is wrong.
- [ ] **Step 5: Write `fixtures/README.md`** (refresh command, filter, coverage, excluded counts).
- [ ] **Step 6: Commit** (`coin-select multiasset: mainnet fixtures check the byte model`).

### Task 4: Generator, validator, reference (`sim.py`)

**Files:**
- Create: `challenges/coin-select/contracts/wallet-multiasset/sim.py`
- Test: `challenges/coin-select/contracts/wallet-multiasset/test_sim.py`

**Interfaces:**
- Consumes: `model.*`.
- Produces:
  - `HashRandom`, copied from `contracts/wallet/sim.py`.
  - `N_SCENARIOS = 8`, `N_EVENTS = 300`, `FEE_BUDGET = 1 * ADA`.
  - `gen_scenario(rng) -> list[dict]` and `gen_scenarios(seed) -> list[list[dict]]`. Events are protocol-shaped JSON-ready dicts with hex strings:
    - `{"t":"deposit","utxos":[{"id","index","addr","value":{"lovelace","assets"},"datum"?}]}`
    - `{"t":"pay","id","outputs":[{"address","value","datum"?}]}`
  - `Utxo(NamedTuple): ref: tuple[bytes,int]; addr: int; value: Value; datum: Datum`.
  - `wallet` is `dict[str, Utxo]`.
  - `parse_value(j) -> Value` and `value_json(v) -> dict`. `parse_value` raises `ValueError` on any malformed hex, wrong length, odd length, qty ≤ 0, non-int, bool, or qty ≥ 2**64.
  - `parse_output(j) -> Output` (judge-trusted input, still strict).
  - `add_deposit(wallet, event)`
  - `validate_tx(wallet, pay_outputs: list[Output], reply, p=PARAMS) -> str | None`. Checks:
    - wallet@1's structural rules;
    - change entries are `{"addr": int in 0..7, "value": {...}}`;
    - lovelace balance including the fee, and exact per-asset balance;
    - `fee >= min_fee(tx_size(...))`, with `n_witnesses` = distinct `addr` among inputs;
    - each change meets `min_ada` and `value_size <= max_val_size`;
    - size ≤ `max_tx_size`.
  - `reference(wallet, pay_outputs) -> dict | None`: multi-asset largest-first.
    1. For each requested asset, take the UTxOs holding most of it until it's covered.
    2. Then take lovelace largest-first.
    3. Change goes to key 0, carrying all leftover assets.
    4. If the change's `value_size > max_val_size`, split leftover policies greedily across change outputs until each fits, each funded with its own min-lovelace.
    5. Fold a sub-min-ada ADA-only leftover into the fee.
    6. Return None if impossible within `max_tx_size`.
  - `apply_tx(wallet, pay_id, n_pay_outputs, reply)` adds change `k` as id `f"{pay_id}#{k}"` with index `n_pay_outputs + k`.
  - `locked_ada(wallet) -> int` is the sum of `min_ada` over the wallet's UTxOs as outputs (lovelace).
  - `ada_dust(wallet) -> int`
  - `simulate_reference(scenario) -> tuple[int, int]`
  - `self_test()` runs unittest discovery. `__main__` takes `--self-test`.
- Generator shape (my calls; tune if the tests below fail):
  - 5–30 policies per scenario: a mix of fungibles (1–3 names, qty 10^3–10^12) and NFT collections (names of 4–32 bytes, qty 1).
  - Initial deposit of 60–120 UTxOs, as in wallet@1. ADA uses wallet@1's log-normals.
  - Deposit UTxOs: 70% ADA-only, 22% 1–3 assets, 8% bundles of 10–60 assets. 10% carry an inline datum (16–1024 bytes of CBOR bytes), 5% a datum hash. Lovelace is at least its min-lovelace.
  - Payments: 1–3 outputs. 20% request 1–3 asset kinds the generator's tracked balance holds. 10% carry a datum (script address, inline 16–512 bytes or hash, 50/50). Destinations are otherwise 70% base, 30% enterprise. Lovelace is at least the output's min-lovelace.
  - Fundability: track the ADA and per-asset balances. Top up with deposits until `ada_balance - reserve ≥ 3 * (need + FEE_BUDGET)`, where `reserve` = Σ over held policies of `min_lovelace` for a change holding that policy's whole holding (my call). The test below is the real gate.

- [ ] **Step 1: Write the failing tests**

```python
def test_deterministic(self): self.assertEqual(gen_scenarios(7), gen_scenarios(7)); self.assertNotEqual(gen_scenarios(7), gen_scenarios(8))
def test_reference_serves_all(self):
    for seed in range(5):
        for sc in gen_scenarios(seed):
            txs, _ = simulate_reference(sc); self.assertGreater(txs, 150)
def test_scenario_mix(self):
    pays = [e for sc in gen_scenarios(1) for e in sc if e["t"] == "pay"]
    tok = sum(any(o["value"].get("assets") for o in e["outputs"]) for e in pays) / len(pays)
    dat = sum(any("datum" in o for o in e["outputs"]) for e in pays) / len(pays)
    self.assertTrue(0.12 < tok < 0.3 and 0.05 < dat < 0.2)
def test_rejects_bad_asset_keys(self):
    for bad in ["ab"*27, "ab"*29, "abc", "zz"*28]:
        r = good_reply(); r["change"][0]["value"]["assets"] = {bad: {"": 1}}
        self.assertIsNotNone(validate_tx(W, OUTS, r))
def test_zero_qty_rejected_split_allowed(self): ...
def test_asset_imbalance(self): ...      # one unit short -> "asset imbalance"
def test_change_below_own_min_ada(self): ...   # min_lovelace - 1 -> "change below min_ada"
def test_linear_fee_rejected(self): ...  # wallet@1 linear fee on a token tx -> "fee below min_fee"
def test_bad_addr(self): ...             # addr 8 -> rejected
def test_witness_count_from_keys(self):  # same reply, inputs on 2 keys needs 101*44 more fee than on 1 key
def test_reference_splits_oversized_change(self):
    # wallet: one UTxO with 3 policies x 50 names of 32 bytes; pay 2 ADA -> reference returns >1 change, all valid
def test_reference_valid_on_all_payments(self):  # inside simulate_reference: validate_tx(reference) is None
```

Fill in the `...` tests with concrete wallets built from `model.wallet_address` and fixed policies. The rejection-reason strings they assert are: `"asset imbalance"`, `"change below min_ada"`, `"fee below min_fee"`, `"bad change addr"`, `"bad value"` (the parse failures), and `"change over max_val_size"`.

- [ ] **Step 2: Run and confirm failure.**
- [ ] **Step 3: Implement `sim.py`.**
- [ ] **Step 4: Run and confirm everything passes**, including `python3 challenges/coin-select/contracts/wallet-multiasset/sim.py --self-test`, which prints `sim self-test ok`.
- [ ] **Step 5: Commit** (`coin-select multiasset: generator, validator, reference`).

### Task 5: Judge

**Files:**
- Create: `challenges/coin-select/contracts/wallet-multiasset/judge` (executable). Copy `contracts/wallet/judge`, then change the payloads.
- Test: `challenges/coin-select/contracts/wallet-multiasset/test_judge.py`

**Interfaces:**
- Consumes: `sim.*`, `model.PARAMS`.
- Produces: the judge stdout JSON with the same shape as wallet@1's:
  - `checks` lists the properties `ma-valid-txs` and `ma-serves-feasible`.
  - `axes` has keys `fees`, `locked_ada` (ADA, mean of end-of-scenario `locked_ada/ADA`), `ada_dust`, `inputs_per_tx`, `latency_ms` and `peak_rss_mib`.
- Changes from wallet@1:
  - Send `model.PARAMS`.
  - Deposits carry the full utxo objects.
  - `pay` sends `outputs` exactly as in the event.
  - Validation goes through `sim.validate_tx` with parsed outputs.
  - A refusal is checked with `sim.reference(wallet, outputs) is None`.
  - `sim.apply_tx` receives `len(outputs)`.
  - Every exception while decoding or checking a reply is `Fail("ma-valid-txs", "unparseable reply")`, as today.

- [ ] **Step 1: Write the failing tests.**
  - `test_judge.py` writes a fake `CB_BIN` shell script into a tempdir. The script parses `participant --report R -- CMD`, runs CMD while passing stdio through, then writes `{"status":"ok","exit_code":0,"peak_rss_mib":1.0}` to R.
  - It runs `judge` with `CB_SEED=3`, `CB_SUBMISSION_DIR` set to the baseline from Task 6 (skip the test if that dir doesn't exist yet) and `CB_BIN` set to the fake.
  - It asserts `ok`, both checks passing, and all six axis keys present with `fees > 0`.
  - A second test points the judge at `preflight/broken/ma-linear-fee` and asserts `ma-valid-txs` fails with a detail containing `fee below min_fee`.
- [ ] **Step 2: Implement the judge. Do Task 6 next, then run the judge tests.**
- [ ] **Step 3: Commit** (`coin-select multiasset: judge`).

### Task 6: Baseline, broken variants, challenge config

**Files:**
- Create: `challenges/coin-select/submissions/baseline-multiasset/{build.sh,select,submission.toml,README.md}`. `select` is a self-contained Python port of `sim.reference` plus wallet tracking. It inlines the model functions it needs (submissions can't import judge code). Its `submission.toml` has `id = "baseline-multiasset"`, `contract = "wallet-multiasset@1"`, `direction = "largest-first"`, and `taxonomy = { strategy = "largest-first", change = "single", consolidation = "none", language = "python" }`.
- Create: `challenges/coin-select/preflight/broken/{ma-asset-imbalance,ma-below-min-ada,ma-linear-fee,ma-over-max-val,ma-bad-addr}/`. Each holds `build.sh`, `select` (the baseline with one deliberate defect), `submission.toml` (`contract = "wallet-multiasset@1"`), `README.md`, and `EXPECT` containing the matching reason string from Task 4.
- Modify: `challenges/coin-select/challenge.toml`:
  - Replace the deferred `wallet-multiasset@0` block with an open `wallet-multiasset@1` (judge `contracts/wallet-multiasset/judge`, interface, choice_space).
  - Add properties `ma-valid-txs` and `ma-serves-feasible` (mechanical, contract `wallet-multiasset@1`).
  - Add `contracts = ["wallet@1"]` to `dust`.
  - Add axes `locked_ada` (unit "ADA", minimise, repeats 1, tolerance 0.12, tolerance_abs 1.0) and `ada_dust` (unit "UTxOs", minimise, repeats 1, tolerance 0.3, tolerance_abs 2.0), both with `contracts = ["wallet-multiasset@1"]`.

- [ ] **Step 1:** Write the baseline and the broken variants. `ma-over-max-val` puts all leftover assets into one change even when it exceeds 5000 bytes, so it needs a scenario that reaches that limit. If no seed reaches it, make the defect "always merge every token UTxO into the change", which reaches it by construction.
- [ ] **Step 2:** Run the Task 5 judge tests. Expected: pass.
- [ ] **Step 3:** For each broken variant, run the judge directly with the fake `CB_BIN` (as in `test_judge.py`). Expected: rejected with its `EXPECT` string. Add this as `test_judge.py::test_broken_variants`, looping over `preflight/broken/ma-*`.
- [ ] **Step 4:** `cargo test -p cb` (spec parsing of the new toml through existing tests). Then run `cargo run -q -- preflight coin-select`. Expected: no Fail rows other than the known macOS `build sandbox` Warn. Look for the rows `contracts` ("open contracts' judge, interface and choice space exist"), "open contracts measured valid: … wallet-multiasset@1", and `broken: ma-*` passing.
- [ ] **Step 5:** Tolerances. Run the judge on the baseline with seeds 1–5 and set the `locked_ada` and `ada_dust` tolerances from the spread (preflight's noise check measures only `baseline`'s contract).
- [ ] **Step 6: Commit** (`coin-select: open wallet-multiasset@1 with baseline and broken variants`).

### Task 7: Solver-facing docs

**Files:**
- Create: `challenges/coin-select/contracts/wallet-multiasset/INTERFACE.md`. Use wallet@1's structure. The protocol messages are exactly as in Task 4's event shapes plus the reply. Include the model in prose, and say that **`model.py` is the definition** and may be copied. Also cover the index rule for change, the witness rule, the validity list, the properties, the scenario description, the axes and the limits.
- Create: `challenges/coin-select/contracts/wallet-multiasset/CHOICES.md`. Cover wallet@1's choices plus: token-aware selection, consolidating token UTxOs (frees `locked_ada`, costs bytes), key reuse (fewer witnesses) against spreading, splitting change by policy, and handling `max_val_size`.
- Modify: `challenges/coin-select/README.md` to describe both contracts and add the `locked_ada` and `ada_dust` rows to the axes table, noting `dust` is wallet@1 only. Grep `PROMPT.md`, `DIRECTIONS.md` and `SUBMITTING.md` for ADA-only assumptions (`lovelace`, `140`, `wallet@1`) and add the second contract where they name contracts.
- Modify: `challenges/coin-select/taxonomy.toml`. Add a rule (version unchanged, since adding is fine): "in wallet-multiasset@1 an extra input costs ~40 bytes plus 101 per new key, not 140".

- [ ] **Step 1:** Write the docs.
- [ ] **Step 2:** Check every number in INTERFACE.md against `model.PARAMS` and the tests: params, the 101-byte witness and the index widths.
- [ ] **Step 3:** Re-run the full test discovery and `cargo run -q -- preflight coin-select`. Expected: same as Task 6.
- [ ] **Step 4: Commit** (`coin-select multiasset: interface, choices, README`).
