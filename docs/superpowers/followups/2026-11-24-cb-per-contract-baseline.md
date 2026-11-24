# Follow-up: per-contract baseline in cb

`cb` assumes one baseline per challenge, the submission with id `baseline`. wallet-multiasset@1 has `baseline-multiasset`. Changes needed:

- crates/cb/src/research/predict.rs:14-19: `reference_id` falls back to the literal "baseline" for `new`; pick the baseline of `decl.contract`.
- crates/cb/src/research/gate.rs:250-258: the `new` direction check searches all accepted submissions; confirm it should stay cross-contract or scope it per contract.
- crates/cb/src/research/record.rs:88: stage baseline requires `decl.id == "baseline"` (and accept refuses it); allow each contract's baseline id.
- crates/cb/src/research/feed.rs:69: `"baseline": id == "baseline"` marks only one baseline in the feed; mark every contract's baseline.
