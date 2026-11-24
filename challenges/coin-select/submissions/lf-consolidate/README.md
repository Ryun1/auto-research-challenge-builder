`builds-on baseline`: the same Python largest-first with one change output, plus
opportunistic consolidation. After largest-first has covered a payment, it also
spends up to `MAX_EXTRA = 2` of the smallest wallet UTxOs under 2 ADA, skipping any
worth less than the 6,160-lovelace fee of its own input, and recomputes the change.

Why: on the judge's own scenarios (`contracts/wallet/sim.py`, seeds 1-5), 97% of
largest-first's final dust is reward-like deposits it never spends, not its own
change. Directions item 2.
