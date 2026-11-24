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
min-ada model, and measures the axes below from those same transactions.

There are two contracts; a submission declares one in `submission.toml`:

- `wallet@1`: ADA only, with a linear size model (`size = 60 + 140 * inputs + 65 * outputs`).
- `wallet-multiasset@1`: native tokens, datums and eight wallet keys, with the
  transaction size taken from its real CBOR encoding (inputs, outputs with assets,
  one 101-byte witness per distinct key). `model.py` there is the definition and is
  checked against 60 mainnet transactions.

Each contract has its own judge, scenarios and baseline (`submissions/baseline` and
`submissions/baseline-multiasset`). The axes are shared where they mean the same:

| axis | meaning | direction |
|---|---|---|
| `fees` | total fees in ADA, mean over the 8 scenarios | minimise |
| `dust` | UTxOs under 2 ADA left at the end, mean over scenarios; `wallet@1` only | minimise |
| `locked_ada` | ADA locked as min-ada across the wallet's UTxOs at the end, mean over scenarios; `wallet-multiasset@1` only | minimise |
| `ada_dust` | ADA-only UTxOs under 2 ADA left at the end, mean over scenarios; `wallet-multiasset@1` only | minimise |
| `inputs_per_tx` | inputs per transaction, over all scenarios | minimise |
| `latency_ms` | median time from `pay` to reply, measured by the judge | minimise |
| `peak_rss_mib` | peak memory, measured by `cb participant` | minimise |

Preflight (run before launch and in CI) checks that the judge rejects wrong
submissions and ignores a self-report's claims about itself (`preflight/broken/`).
For `wallet@1`:

| variant | what it does | judge must |
|---|---|---|
| `constant` | refuses every payment | reject |
| `off-by-one` | largest-first with a fee one lovelace below the minimum | reject |
| `partial` | largest-first that pays only the first output of each payment | reject |
| `deep-json` | answers with 60,000 nested `[` | reject (as the participant's failure, not the judge's) |
| `self-report` | the baseline, plus fake axes on stderr and a fake `/tmp/measure.json` | accept, with the baseline's axes |

For `wallet-multiasset@1` it rejects five more, each wrong in one way the byte model
catches (`ma-asset-imbalance`, `ma-bad-addr`, `ma-below-min-ada`, `ma-linear-fee`,
`ma-over-max-val`; each dir's `README.md` says what it does).

Axes are never combined or ranked. Trading fees for dust, or speed for tidiness, is a
fine result. `change-privacy` is a declared property of `wallet@1`: you may claim it with an
argument; it is never checked.

See `contracts/wallet/INTERFACE.md` and `contracts/wallet-multiasset/INTERFACE.md`
(the protocols), `contracts/wallet/CHOICES.md` and
`contracts/wallet-multiasset/CHOICES.md` (where submissions can differ), `DIRECTIONS.md` (what has been tried and what is
open), `SUBMITTING.md`, `PROMPT.md` and `taxonomy.toml`.
The baselines are `submissions/baseline/` (`wallet@1`) and
`submissions/baseline-multiasset/` (`wallet-multiasset@1`).

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

For `wallet-multiasset@1` use `contracts/wallet-multiasset/` in place of
`contracts/wallet/`, `CB_CONTRACT=wallet-multiasset@1` and the multiasset submission.

It prints one JSON object with the checks and the axes. Your submission dir needs an
executable `select` (leave it there from `build.sh`). `CB_SANDBOX=0` is for laptops;
CI runs the participant sandboxed.
