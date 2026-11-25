# Contract wallet-multiasset@1: interface

`build.sh` (run offline in the submission dir) must leave an executable `./select` in
that dir. The judge starts it once per scenario, through `cb participant`, with the
submission dir as the working directory (use relative paths; the sandbox mounts it
read-only and gives a private empty `/tmp`; nothing it writes survives the run, and
IPC and the hostname are private too). It speaks JSON lines on stdin/stdout,
one object per line, UTF-8. stderr is discarded. Flush after every reply.

This contract is `wallet@1` with native tokens, datums and a real byte model: the
wallet holds multi-asset UTxOs, payments can ask for tokens and carry datums, and
the fee comes from the size of the encoded transaction, not from a linear estimate.
Sizes, fees and min-ada are defined by `model.py` in this directory (below).

All amounts are integer lovelace or integer token quantities. Floats, negatives and
booleans in place of integers are errors. Hex strings (policies, asset names,
addresses, datums) must be lowercase, even length.

## Values, datums, addresses

A value is `{"lovelace": 5000000}` or
`{"lovelace": 2000000, "assets": {"<policy hex>": {"<asset name hex>": 7}}}`.

- A policy is 28 bytes (56 hex chars); an asset name is 0 to 32 bytes (0 to 64 hex
  chars; the empty name is `""`).
- A quantity is an integer with `0 < qty < 2^64`; lovelace is `0 <= n < 2^64`.
- An inner asset map is never empty. `"assets"` may be omitted or `{}`.
- A datum is `{"inline": "<hex>"}` (CBOR of the datum, carried inline; at most 1 MiB)
  or `{"hash": "<64 hex chars>"}` (a 32-byte datum hash). Generated inline datums are
  CBOR byte strings of 16 to 1,024 bytes.
- An address is hex, 29 bytes (enterprise, header `61`) or 57 bytes (base, header `01`
  for key addresses, `11` for a script payment part). Payment outputs with a datum
  go to a script base address.

## Judge to participant (no reply)

First message of every run (no `"type"` field; it is the protocol parameters):

    {"min_fee_a":44,"min_fee_b":155381,"coins_per_utxo_byte":4310,"max_tx_size":16384,"max_val_size":5000,"keys":8,"addresses":["01a819...", "..."]}

`addresses` lists the 8 wallet addresses, index = key number (`0..7`). Each is a
57-byte base address, header `01`, followed by the same 28-byte hash twice: key i's
hash is `sha256(b"key%d" % i)[:28]` (`model.wallet_address(i)`).

Then wallet deposits. The first is the initial wallet (60 to 120 UTxOs); later ones
are new payments to the wallet, one to five UTxOs each:

    {"type":"deposit","utxos":[
      {"id":"d3#0","index":0,"addr":2,
       "value":{"lovelace":2500000,"assets":{"<policy>":{"<name>":1}}}},
      {"id":"d3#1","index":1,"addr":5,"value":{"lovelace":12000000,"assets":{}},
       "datum":{"inline":"4401020304"}}]}

`addr` is the wallet key (0..7) that owns the UTxO. `index` is the output index of
the UTxO in the transaction that created it. `datum` is present only on some
deposits; it makes the UTxO bigger, so its min-ada is higher, but spending it needs
only that key's witness (no script is run).

UTxO ids are strings, unique within a run: `d<event>#<k>` for deposits (index `k`) and
`<pay id>#<k>` for change output `k` of a payment you answered. The id up to `#`
determines the transaction id: `txid = sha256(<id up to '#'>)` (32 bytes, as
`model.utxo_ref(id, index)` returns).

## Judge to participant (reply required)

    {"type":"pay","id":"p7","outputs":[
      {"address":"01ab...","value":{"lovelace":5000000}},
      {"address":"11cd...","value":{"lovelace":3000000,"assets":{"<policy>":{"<name>":3}}},
       "datum":{"hash":"<64 hex>"}}]}

One to three outputs. Reply within 10 seconds with one line, either

    {"inputs":["d3#0"],"change":[{"addr":0,"value":{"lovelace":6800000,"assets":{...}}}],"fee":200000}

or `{"refuse":true}`.

- `inputs` are wallet UTxO ids.
- Each `change` entry has exactly the keys `addr` (an integer key `0..7`) and
  `value` (as above). Change outputs never carry a datum.
- Payment outputs are built as given. The tx lists them first, then your change
  outputs in order. **Change output `k` of payment `p` has id `p#k` and on-chain
  index `len(payment outputs) + k`.** An index of 24 to 255 costs one more
  byte than 0 to 23 (in the input and in the model), and 256 or more costs two more. This is reachable: about 266
  change outputs fit in 16 KB. `model.py` handles it.
- Change `k` becomes a wallet UTxO owned by key `change[k].addr`, with no datum.
- Inputs are removed from the wallet. After a refusal the wallet is unchanged.

## End

    {"type":"end"}

The participant must exit with status 0. The judge then closes stdin.

## Model

`model.py` (with `cbor.py`, which it imports) **is the definition** of size, fee and
min-ada. You may copy both into your submission. The prose below is a summary; if
the two ever differ, `model.py` wins.

The judge builds each transaction in its cheapest valid encoding (the form a wallet
would submit):

- `tx = [body, witness_set, true, null]`; `body = {0: inputs, 1: outputs, 2: fee}`;
  `witness_set = {0: [[vkey 32 bytes, signature 64 bytes], ...]}`. Input and witness
  sets are untagged arrays; an input is `[txid 32 bytes, index]`.
- An output is the array `[address, value]` (or `[address, value, datum_hash]`); only
  an output with an inline datum uses the map form `{0: address, 1: value, 2: [1, tag24(bytes)]}`.
- A value is the bare integer when there are no assets, else `[coin, {policy: {name: qty}}]`
  with policies and names sorted shorter-first then bytewise.
- One vkey witness (101 bytes) per **distinct wallet key among the inputs**.

```
size    = len(cbor(tx)) - 1
min_fee = min_fee_a * size + min_fee_b
min_ada(output) = (160 + len(cbor(output))) * coins_per_utxo_byte
```

The ledger sizes `[body, witness_set, aux]` without the `is_valid` byte
(`toCBORForSizeComputation`), hence the `- 1`. `max_tx_size` applies to the same
`size`, `max_val_size` to `len(cbor(value))` of one change output.

Rough sizes under the judge's encoding: an input is 36 bytes (37 once its index is 24
or more, 38 from 256), a vkey witness 101, an ADA-only output at a 57-byte address 65 once its lovelace is at least 65,536
(the coin's CBOR width counts: 61 bytes for 0, 63 below 65,536), so its min-ada is
969,750 lovelace (`min_lovelace(addr, {}, None)`), and an empty transaction with one
input and one witness 149 at fee 0 (about 153 at a real fee, since the fee's CBOR width counts).
The byte model is checked against 60 mainnet transactions in `fixtures/`, rebuilt
byte for byte (see `fixtures/README.md`).

`model.py` works in raw bytes (policy, asset names, addresses and txids as `bytes`; inputs as `(txid, index)` tuples), not hex.

`model.py` exports `PARAMS`, `Output(address, (lovelace, assets), datum)`,
`tx_size(inputs, outputs, fee, n_witnesses)`, `min_fee(size)`, `min_ada(output)`,
`min_lovelace(address, assets, datum)` (the least lovelace that makes an output
legal, found by iteration), `value_size(value)` and `utxo_ref(id, index)`.

## Validity

A reply is valid when:

- it is one well-formed JSON line of at most 64 KiB, in time;
- `inputs` is a non-empty list of distinct strings, every id in the current wallet;
  `change` is a list; `fee` is an integer with `0 <= fee < 2^64`;
- every change entry is `{addr, value}` with `addr` in `0..7` and a well-formed value
  (the limits above);
- lovelace balances: `sum(inputs) = sum(payment outputs) + sum(change) + fee`;
- every token balances: for each (policy, name), tokens in the inputs equal tokens
  in the payment outputs plus tokens in the change (nothing is minted or burned);
- `size <= max_tx_size`, with the witness count taken from the inputs' keys;
- `fee >= min_fee(size)`;
- every change output has `len(cbor(value)) <= max_val_size` and
  `lovelace >= min_ada(output)`, the output being priced as an array-form output
  with its own value at the address of key `addr`.

Payment outputs are not re-checked (the judge generates them legal). You may pay more
than the minimum fee, use no change output (then the leftover must go to the fee and
no token may remain), or several. Anything the judge cannot decode or check for any
reason (invalid or absurdly nested JSON, wrong types) is a plain `ma-valid-txs`
failure, "unparseable reply".

## Properties

- `ma-valid-txs` (mechanical): every reply is a valid transaction as above. Any
  violation (including the participant exiting, crashing or hitting a limit) stops
  that scenario and the submission is rejected, with a short detail naming the
  payment.
- `ma-serves-feasible` (mechanical): you may refuse a payment only when the judge's
  reference cannot build a valid transaction on your current wallet. The reference
  covers each needed token from the UTxOs holding most of it, then adds UTxOs by
  lovelace, largest first, until payment, fee and change min-ada are covered; it
  puts all leftover tokens in change at key 0 (split by policy, then by name, only
  to respect `max_val_size`), and gives up when the transaction would exceed
  `max_tx_size`. Refuse anything else and you are rejected.

## Scenarios

Eight per judge run, from `CB_SEED` through a SHA-256 counter-mode stream (so what
you have seen reveals nothing about later events). Each has an initial deposit of 60
to 120 UTxOs and then 300 events: about 30% deposits of one to five UTxOs, about 70%
payments of one to three outputs; extra deposits are inserted when the wallet runs
low, so the real mix shifts a little towards deposits. The seed is never given to
your program.

Each scenario draws 5 to 30 policies: half fungible (1 to 3 names of 0 to 32 bytes,
quantities 10^3 to 10^12), half NFT collections (one fixed name length of 4 to 32
bytes, quantity 1, every name distinct).

Deposit UTxOs: owner key uniform over 0..7; 70% ADA-only, 24% carry 1 to 3 assets
drawn across all policies, 6% are bundles of 10 to 30 assets from 1 to 3 policies; 10%
carry an inline datum (a 16 to 1,024 byte string) and 5% a datum hash. Lovelace:
initial ADA-only about 50 ADA log-normal (at least 1.5), later ADA-only 70% about 30
ADA log-normal (at least 1) and 30% uniform 1 to 3 ADA (reward-like); a UTxO with
assets has its min-ada plus either nothing or up to 2 ADA more.

Payments: each output about 20 ADA log-normal (at least 1 ADA, and at least its
min-ada); 70% go to a base address and 30% to an enterprise address. In 20% of
payments, 1 to 3 asset kinds are requested, each between 1 and half of the wallet's
holding, spread over the outputs; an output carrying assets pays just its min-ada half
the time. In 10% of payments one output carries a datum (inline of 16 to 512 bytes or a
hash, half each) and goes to a script address. Payments are always fundable in
lovelace and tokens, assuming each earlier transaction cost at most 1 ADA in fees.
Whether a selection also fits in `max_tx_size` is for you to manage; refusing is only
allowed when the reference also fails.

## Axes

`fees`, `locked_ada`, `ada_dust`, `inputs_per_tx`, `latency_ms`, `peak_rss_mib`. They are measured only when every check
passed, from the transactions you returned and from `cb participant`'s report, never
from anything your program prints about itself.

- `fees`: total fees in ADA, mean over the 8 scenarios.
- `locked_ada`: at the end of a scenario, the sum of `min_ada` over every UTxO in the
  wallet (priced at its owner's address with its value and datum), in ADA; mean over
  the scenarios. It is the ADA the wallet cannot spend freely. Fewer, tidier token
  UTxOs lock less.
- `ada_dust`: UTxOs left at the end whose free ADA (lovelace above that UTxO's own
  `min_ada`, priced as for `locked_ada`) is under 1 ADA, mean over the scenarios.
  Token and datum UTxOs count too: an ADA-only UTxO at a wallet key counts below
  1,969,750 lovelace, and a token UTxO carrying little more than its min-ada counts,
  so adding a token unit to small change does not hide it.
- `inputs_per_tx`: inputs per transaction, over all scenarios.
- `latency_ms`: median time from `pay` to reply over all payments.
- `peak_rss_mib`: the largest peak memory (cgroup, from `cb participant`) of any scenario.

All are minimised. Units, repeats and noise tolerances (a run differs from another
only if it moves by more than the relative tolerance and the absolute floor; from
`challenge.toml`):

| axis | unit | repeats | tolerance | tolerance_abs |
|---|---|---|---|---|
| `fees` | ADA | 1 | 0.12 | 1.0 |
| `locked_ada` | ADA | 1 | 0.42 | 1.0 |
| `ada_dust` | UTxOs | 1 | 0.18 | 2.0 |
| `inputs_per_tx` | inputs | 1 | 0.3 | 0.2 |
| `latency_ms` | ms | 3 | 0.9 | 0.05 |
| `peak_rss_mib` | MiB | 3 | 0.3 | 4.0 |

`dust` belongs to `wallet@1` only; `locked_ada` and `ada_dust` to this contract only.

## Limits

Per run: 60 s CPU, 120 s wall, 64 MiB of files, and, for the whole process tree of
your program (not per process), 1024 MiB memory and 32 processes. Exceeding one is an
`ma-valid-txs` failure. Peak memory is the tree's cgroup peak, including page cache.
