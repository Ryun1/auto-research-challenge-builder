`builds-on lf-consolidate`, Directions item 4: the same largest-first with one change
output and the same consolidation rule (up to 2 of the smallest UTxOs under 2 ADA worth
more than their input fee), applied only while the wallet holds more than
`THRESHOLD = 10` UTxOs under 2 ADA.

Sim (`contracts/wallet/sim.py`, seeds 1-3, 24 scenarios): fees 37.65 ADA, dust 9.54,
inputs/tx 1.489, against lf-consolidate's 37.55 / 0.04 / 1.533. Reward-like deposits
keep the wallet above the threshold most of the time, so it saves few inputs.
