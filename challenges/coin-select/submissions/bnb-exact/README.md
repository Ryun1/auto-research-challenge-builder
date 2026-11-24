Directions item 3 (`change = none-when-possible`), a new direction: branch and bound.

For each payment, a depth-first search over the wallet sorted by value looks for the
fewest inputs (1, then 2, then 3) whose total lies in `[need + min_fee, need + min_fee +
cost_of_change]`. `cost_of_change` is 44 x (65 + 140) = 9,020 lovelace: one change output
now plus the input to spend it later (Bitcoin Core's window). A match pays no change; the
leftover goes into the fee. Each depth is capped at 20,000 search nodes. With no match,
it falls back to `lf-consolidate`.

Cheapest test first (Directions): in `sim.py` (seeds 1-3, 24 scenarios), 40.5% of payments
had such a match, but fees did not fall (37.84 vs 37.55 ADA): the matches need 1.9
inputs per transaction against 1.53, and the extra input fees cancel the change output
saved.
