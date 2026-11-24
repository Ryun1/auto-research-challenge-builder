# Mainnet fixtures

`mainnet.jsonl` holds 60 real Cardano mainnet transactions. `../test_fixtures.py`
decodes each one and rebuilds it with `model.tx_cbor` / `model.output_cbor`. The
rebuilt body and tx size must equal the original bytes exactly. This is how the
byte model (size, fee, min-ada) is checked against cardano-ledger.

## Refresh

```sh
python3 challenges/coin-select/contracts/wallet-multiasset/fixtures/fetch.py
```

It needs network access (public Koios, no key) and takes a few minutes. It
prints the coverage and excluded counts, and exits non-zero if coverage is not
reached or the latest epoch's params differ from `model.PARAMS`. The judge never
imports it.

## Format

The first line is a meta header: `{"type": "meta", "params": {...}, "epochs":
{epoch: params_match}, "excluded": {...}, "coverage": {...}, ...}`. Each other
line is `{"hash", "cbor", "epoch", "fee", "exact_min_fee", "params_match"}`.

- `exact_min_fee` is `fee == min_fee(len(cbor) - 1)`. The ledger's fee size
  leaves out the `is_valid` byte: Alonzo `toCBORForSizeComputation`, still used
  in Conway, measures the 3-item `[body, witness_set, aux]`. `model.tx_size`
  does the same. In a 20,048-tx scan, 537 of the 3,892 eligible txs paid
  exactly `min_fee(len - 1)` and none paid less, while 537 paid less than
  `min_fee(len)`.
- `params_match` is true when the tx's epoch params (`min_fee_a`, `min_fee_b`,
  `coins_per_utxo_byte`, `max_tx_size`, `max_val_size`) equal `model.PARAMS`.
  Byte and size checks run on every fixture. Fee-floor and min-ada checks run
  only on `params_match` fixtures, and most fixtures must be `params_match`.

## Filter

A tx is kept only if all of these hold:

- it decodes with `cbor.decode` without `NonCanonical`;
- its body keys are a subset of `{0, 1, 2, 3, 7, 8, 15}`;
- its witness-set keys are exactly `{0}`;
- `is_valid` is true;
- no output has key 3 (`script_ref`);
- no output's value is `[coin, {}]`, an empty multi-asset map. That is legal,
  but `model.Value` can't tell it from a bare coin, and the judge never emits it;
- at least one output carries assets.

Excluded kinds are counted in the meta header:

| Kind | Meaning |
|---|---|
| `indefinite`, `non_minimal`, `duplicate_key` | `NonCanonical` |
| `undecodable` | `cbor.decode` can't read it |
| `unhashable_key` | a map keyed by an array or map, e.g. in metadata or redeemers |
| `out_of_scope` | a body key outside the set |
| `witness_keys` | the witness set has keys other than 0 |
| `invalid` | `is_valid` is false |
| `script_ref` | an output has key 3 |
| `empty_multiasset` | an output's value is `[coin, {}]` |
| `no_assets` | no output carries assets |

## Selection and coverage

The recent walk goes back from the tip until the eligible set covers all of
these:

- an inline datum;
- array- and map-form outputs;
- tag-258 and untagged sets;
- at least 3 distinct witness counts;
- an output with at least 20 assets;
- an output whose lovelace equals its ledger min-ada exactly;
- at least 20 txs paying exactly the minimum fee.

Then 57 txs are chosen greedily to cover those cases.

Datum-hash outputs are gone from recent mainnet: there were none in 20,048 txs
(epoch 669). A second walk forward from block 8,526,132 (the start of epoch 400,
Babbage, same params) takes the first 3 eligible txs with a datum-hash output.

Coverage of the committed set (fetched 2026-11-24, tip 14,242,835):

| Case | Count |
|---|---|
| exact min-ada outputs | 13 |
| outputs with ≥ 20 assets | 6 |
| inline datum | 9 |
| datum hash | 3 |
| map form | 10 |
| array form | 58 |
| tagged | 44 |
| untagged | 16 |
| exact min fee | 20 |

- Witness counts: 1, 2, 3 and 7.
- Two txs tag only one of the input set and the witness set
  (`tx_cbor(tag_witnesses=...)`).
- All 60 txs are `params_match` (epochs 400 and 669).

Excluded on this run:

- Recent walk, 828 scanned (171 eligible): `no_assets` 276, `indefinite` 138,
  `witness_keys` 96, `out_of_scope` 85, `unhashable_key` 52,
  `empty_multiasset` 10.
- Datum-hash walk, 1,600 scanned: `no_assets` 532, `indefinite` 444,
  `out_of_scope` 264, `witness_keys` 160.

A wider diagnostic scan of 20,048 recent txs found 3,892 eligible. Every one of
them round-tripped byte for byte.
