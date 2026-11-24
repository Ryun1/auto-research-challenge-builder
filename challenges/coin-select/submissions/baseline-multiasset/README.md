The reference submission for `wallet-multiasset@1`: a self-contained Python port of
the judge's reference (`sim.reference`). For each asset the payment needs, it takes
the UTxOs holding most of it; then it adds UTxOs by lovelace, largest first, until
the payment, the fee and the change's min-ada are covered. Leftover assets go to one
change output at key 0, split by policy only when one output would exceed
`max_val_size`. ADA-only leftover below min-ada folds into the fee. The byte model
(CBOR sizes, min-ada, fee) is inlined from `model.py`. It refuses only when the
wallet cannot pay within `max_tx_size`. It is the yardstick for the contract's axes.
