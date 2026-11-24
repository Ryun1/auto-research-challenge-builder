# Contract wallet-multiasset@1: choice space

Where submissions can differ. These are choices, not rules. Everything in
`contracts/wallet/CHOICES.md` still applies (selection algorithm, change shape, fee
choice, refusals, state, time and memory, language); this adds what tokens and the
byte model open up. Byte costs below are from `model.py`; at `min_fee_a = 44`, one
byte of transaction is 44 lovelace.

- **Token-aware selection.** A payment that asks for tokens must be covered from UTxOs
  that hold them, and the leftover tokens of every input you spend go to change. Which
  holders you pick (the biggest, the fewest, the ones with the least else on them)
  decides how many inputs, how large the change value is and how much ADA is tied up.
  After the tokens are covered, ADA is chosen as in `wallet@1`.
- **Change shape.** All leftover tokens in one change, or split by policy so that each
  piece is a small UTxO that is cheap to spend later. Every extra change output costs
  its bytes now (about 65 plus its assets) and its own min-ada (about 0.97 ADA for an
  ADA-only output, more with tokens); it can pay back when later payments need only
  one policy and would otherwise drag a whole bundle along.
- **`max_val_size`.** A change value over 5,000 bytes is invalid. Bundles pile up in
  change if every payment dumps all leftover tokens into one output, so some split is
  forced eventually. When to split (always by policy, only at a size threshold, by
  name inside one huge policy) is a policy. The baseline splits only when it must.
- **Consolidating token UTxOs.** Spending several token-holding UTxOs together and
  returning one change merges them. It frees `locked_ada`: each UTxO locked at least
  its own min-ada, and a merged value shares the fixed 160-byte overhead, so the
  wallet's total min-ada falls. It costs bytes now: each input is 36 bytes (37 for
  an index of 24 or more) plus 101 for each key not yet in the transaction. Merging
  too far fights `max_val_size` and makes later payments heavier.
- **Key reuse against spreading.** The fee has a witness (101 bytes, about 4,400
  lovelace) per distinct key among the inputs, so inputs from one key are cheaper than
  inputs from three. Sending all change to one key concentrates the wallet and keeps
  later transactions at one witness; spreading change over keys spreads the UTxOs but
  every later transaction that mixes keys pays for each. The ownership of a deposit is
  given; only your change `addr` is yours to choose.
- **Consolidation.** Spending extra small ADA-only inputs while paying keeps
  `ada_dust` low. Here an extra input from a key already in the transaction is about
  36 bytes (about 1.6k lovelace), not the 140 of `wallet@1`; from a new key it is
  about 137 bytes.
- **Datum UTxOs.** A deposit with an inline datum has a larger min-ada but spends like
  any other (one witness, 36 bytes), so it holds more locked ADA per UTxO. Spending
  these early reduces `locked_ada`.
- **Fee choice and no change.** As in `wallet@1`: the minimum, or a little more to
  avoid change below min-ada. With tokens in the inputs, no change is possible only
  if the payment takes every token; otherwise a change output is forced.
- **Refusals.** Only when the reference also cannot build a valid transaction, which
  includes hitting `max_tx_size` (16,384 bytes) with large token bundles.
- **Privacy.** Not a declared property in this contract.

Typical tensions: fewer inputs now (low `fees`) against `locked_ada` and `ada_dust`
later; one change against `max_val_size` and later bundle drag; one key against
spreading; speed against tidy selections; memory against latency.
