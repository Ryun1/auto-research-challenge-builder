# coin-select: directions

What has been tried and what is open. Maintainers keep this file up to date (in the
same PR as any taxonomy edit); participants never edit it. It is a guide, not a
rule: a direction missing from it is still welcome.

Numbers are the medians of the two accepted runs on main, from `results.jsonl`.
The axes are defined in `README.md`. Last updated 2026-11-19.

## Explored

| Submission | Direction | fees (ADA) | dust | inputs/tx | latency (ms) | peak RSS (MiB) |
|---|---|---:|---:|---:|---:|---:|
| `baseline` | largest-first | 37.3 | 43.3 | 1.31 | 0.085 | 5.7 |
| `random-improve` | random-improve | 39.9 | 3.9 | 3.29 | 0.015 | 3.2 |
| `random-improve-consolidate` | random-improve | 40.1 | 0.06 | 3.34 | 0.034 | 3.0 |

**largest-first** (`baseline`, Python). Spends the biggest UTxOs first, so a
payment needs few inputs and pays the lowest fees. The small UTxOs are never
chosen and pile up: about 43 are left under 2 ADA at the end of a scenario.

**random-improve** (`random-improve`, Rust). CIP-2's default: random inputs until
the payment is covered, then add more while the total moves towards twice the
payment, with change outputs shaped like the payment. Random picks spend small UTxOs
along with large ones, so dust falls about 11x and inputs per transaction rise
about 2.5x, for about 7% more fees. `random-improve-consolidate` builds on it.
It also spends up to 3 of the smallest UTxOs under 2 ADA when each is worth more
than its input fee. That takes dust to about zero, and the fee change is within
the axis tolerance.

The latency and memory gap between `baseline` and the other two mixes two
changes, language (Python against Rust) and strategy. No submission yet
separates them.

## Open

Each entry gives the mechanism, what it should do to the axes, and the cheapest
test that could show it wrong. The predictions are guesses. A submission that
refutes one is a useful result.

1. **Largest-first in a compiled language.** Port `baseline` as it is.
   - Expected: fees, dust and inputs/tx identical (same algorithm, deterministic);
     latency and peak RSS near `random-improve`'s.
   - Cheapest test: the port itself. If latency stays near 0.085 ms, the gap
     above comes from the strategy, not the language.
2. **Largest-first plus consolidation.** Add `random-improve-consolidate`'s dust
   rule to `baseline` (`builds-on baseline`).
   - Expected: dust falls far below 43 while inputs/tx and fees stay well under
     random-improve's, if largest-first's dust comes from never spending deposits.
   - Cheapest test: count, from the judge's own scenarios (`contracts/wallet/sim.py`),
     how much of largest-first's final dust is deposits and how much is its own
     change. If most is change, consolidation alone will not fix it.
3. **Exact match, no change** (`change = none-when-possible`, branch and bound).
   Skipping the change output saves 65 bytes (about 2.9k lovelace) per payment
   and creates no new small UTxO; the leftover goes into the fee.
   - Expected: lower fees and dust than random-improve when exact matches are
     common; higher latency from the search.
   - Cheapest test: before writing a search, count how often a subset of the
     wallet covers a payment with less leftover than a change output costs. If
     that is rare in these scenarios, this direction cannot move fees.
4. **Consolidate only past a UTxO count** (`consolidation = threshold`).
   - Expected: most of the dust saving of opportunistic consolidation with fewer
     extra inputs per transaction.
   - Cheapest test: on `random-improve-consolidate`, log the UTxO count when it
     consolidates. If it already consolidates rarely, a threshold changes little.
     Trying several thresholds is calibration within one direction: submit the
     best one, `builds-on`.

Deferred: multi-asset selection waits for the `wallet-multiasset@0` contract (see
`challenge.toml`).
