# coin-select: Cardano UTxO coin selection, mapped not raced

Cardano wallets must choose which UTxOs to spend for each payment (CIP-2). The
transaction pays a fee that grows with its size, so fewer inputs is cheaper now. But
always spending the biggest coins leaves a wallet full of small leftovers (dust)
that cost more to spend later, and spending many small ones now costs fees today.
Cheap online heuristics answer fast; exact searches find tidier selections but take
longer. There is no single winner, which is what the map on the site is for.

A submission is a program `./select` that is told the protocol parameters, the
wallet's deposits and a stream of payments, and answers each payment with the inputs
to spend, the change outputs and the fee. A trusted judge replays eight seeded
scenarios of 300 events each, checks every transaction against a size, fee and
min-ada model, and measures five axes from those same transactions:

| axis | meaning | direction |
|---|---|---|
| `fees` | total fees in ADA, mean over the 8 scenarios | minimise |
| `dust` | UTxOs under 2 ADA left at the end, mean over scenarios | minimise |
| `inputs_per_tx` | inputs per transaction, over all scenarios | minimise |
| `latency_ms` | median time from `pay` to reply, measured by the judge | minimise |
| `peak_rss_mib` | peak memory, measured by `cb participant` | minimise |

Axes are never combined or ranked. Trading fees for dust, or speed for tidiness, is a
fine result. `change-privacy` is a declared property: you may claim it with an
argument; it is never checked.

See `contracts/wallet/INTERFACE.md` (the protocol), `contracts/wallet/CHOICES.md`
(where submissions can differ), `SUBMITTING.md`, `PROMPT.md` and `taxonomy.toml`.
The baseline is `submissions/baseline/`.

## Running the judge locally

From the repo root, after `cargo build`:

```
cd challenges/coin-select
python3 contracts/wallet/sim.py --self-test
CB_SEED=12345 CB_REPEAT=0 CB_CONTRACT=wallet@1 \
  CB_SUBMISSION_DIR=$PWD/submissions/baseline \
  CB_BIN=$PWD/../../target/debug/cb \
  CB_LIMITS='{"cpu_secs":60,"memory_mib":1024,"processes":32,"file_mib":64,"wall_secs":120}' \
  CB_SANDBOX=0 contracts/wallet/judge
```

It prints one JSON object with the checks and the axes. Your submission dir needs an
executable `select` (leave it there from `build.sh`). `CB_SANDBOX=0` is for laptops;
CI runs the participant sandboxed.
