# Contract wallet@1: choice space

Where submissions can differ. These are choices, not rules.

- **Input selection.** Largest-first, random-improve (the CIP-2 default), branch and
  bound for an exact match, knapsack or subset-sum, exhaustive search with a cut-off.
  The wallet holds 60 to 120 UTxOs at the start and grows and shrinks.
- **Change.** One output, several outputs to keep the wallet granular, change shaped
  like the payment, or none when an exact match exists (the leftover then goes into
  the fee, so you pay more than the minimum).
- **Consolidation.** Spend extra small inputs while paying so dust does not pile up:
  each extra input costs 140 bytes of fee now (about 6.2k lovelace) and saves
  spending it alone later. When to do it is a policy: always, when cheap, once the
  UTxO count passes a threshold, or never.
- **Fee choice.** The minimum, or a little more to avoid change below min-ada.
- **Refusals.** Only allowed when the whole wallet really cannot pay.
- **State.** Keep your own wallet index from the deposits and your own replies; the
  judge never resends it. Sorted structures, buckets, or caches trade memory for time.
- **Time and memory.** Replies are timed; heavy searches cost latency, indexes cost
  memory. Both are axes, not limits (within the hard limits).
- **Language.** Anything that builds offline on the runner image; vendor your
  dependencies. Python, Rust, C and Go are all plausible.
- **Privacy.** Hiding which output is the change (similar amounts, no obvious
  largest change) can be claimed under the declared `change-privacy` property.

Typical tensions: fewer inputs now (low `fees`) against dust later; speed against tidy
selections; memory against latency.
