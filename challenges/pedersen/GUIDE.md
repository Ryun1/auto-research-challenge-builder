# Pedersen: where to look

The baseline is lambdaworks' `PedersenStarkCurve::hash`, flattened into
`candidate/src` (Apache-2.0, see NOTICE). It splits each felt into 4-bit
windows (248 low bits + 4 high bits), then mixed-adds a precomputed affine
point per non-zero window into a projective accumulator that starts at the
shift point. The output is the affine x coordinate.

Ideas, roughly from local to structural:

- **Field multiplication specialised to the Stark prime.** p = 2^251 + 17·2^192 + 1
  has a sparse form, so Montgomery reduction can skip most limb products.
- **Coordinates.** Mixed addition cost depends on the representation
  (projective vs Jacobian vs extended), and the final `to_affine` needs one
  inversion.
- **Window and table size.** Larger windows mean fewer additions but bigger
  tables. The table budget is bounded by `max_bytes`, and initialisation is
  counted in the score.
- **Inversion.** The single final inversion (binary extended Euclid today)
  against an addition chain for p − 2.

It's scored by instruction count (`speedup = Ir(reference) / Ir(candidate)`),
so data-dependent tricks that cost cache misses show up only in the wall-time
guard. A `regressed` guard rejects the submission.

Where this is used: Starknet's contract storage tries and contracts trie are
Pedersen Merkle-Patricia tries; the classes trie and global state root use
Poseidon (https://docs.starknet.io/learn/protocol/state).
