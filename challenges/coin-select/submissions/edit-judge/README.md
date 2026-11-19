The reference submission for `wallet@1`: Python largest-first. It sorts the wallet
by value, adds inputs until they cover the outputs and the fee (priced with one
change output), makes a single change output when the leftover is at least min-ada,
and otherwise folds the leftover into the fee. It refuses only when the whole wallet
cannot pay. It is the root of the lineage tree and the yardstick for preflight.
