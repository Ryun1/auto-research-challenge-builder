# Contract wallet@1: interface

`build.sh` (run offline in the submission dir) must leave an executable `./select` in
that dir. The judge starts it once per scenario, through `cb participant`, with the
submission dir as the working directory (use relative paths; the sandbox mounts it
read-only and gives a private empty `/tmp`; nothing it writes survives the run, and
IPC and the hostname are private too). It speaks JSON lines on stdin/stdout,
one object per line, UTF-8. stderr is discarded. Flush after every reply.

All amounts are integer lovelace. Floats, negatives and booleans in place of
integers are errors.

## Judge to participant (no reply)

First message of every run:

    {"type":"params","min_fee_a":44,"min_fee_b":155381,"coins_per_utxo_byte":4310,"max_tx_size":16384,"size":{"base":60,"input":140,"output":65}}

Then wallet deposits. The first deposit is the initial wallet (60 to 120 UTxOs); later
ones are new payments to the wallet, some tiny, reward-like:

    {"type":"deposit","utxos":[{"id":"d3#0","lovelace":12000000}]}

UTxO ids are strings, unique within a run: `d<event>#<k>` for deposits and
`<pay id>#<k>` for change output `k` of a payment you answered.

## Judge to participant (reply required)

    {"type":"pay","id":"p7","outputs":[{"lovelace":5000000}]}

One to three outputs. Reply within 10 seconds with one line, either

    {"inputs":["d3#0"],"change":[6800000],"fee":200000}

or `{"refuse":true}`. Change output `k` becomes the wallet UTxO `"p7#k"`. Inputs are
removed from the wallet. After a refusal the wallet is unchanged.

## End

    {"type":"end"}

The participant must exit with status 0. The judge then closes stdin.

## Model

    size    = base + input * n_inputs + output * (n_outputs + n_change)
    min_fee = min_fee_a * size + min_fee_b
    min_ada = (160 + output) * coins_per_utxo_byte

`n_outputs` counts the payment's outputs. A transaction is valid when:

- `inputs` is non-empty, has no duplicates, and every id is in the current wallet;
- `sum(inputs) = sum(outputs) + sum(change) + fee`;
- `fee >= min_fee`; every change output is `>= min_ada`; `size <= max_tx_size`;
- the reply is one well-formed line of at most 64 KiB, in time. A reply the judge
  cannot decode or check for any reason (invalid or absurdly nested JSON, wrong
  types) is a plain `valid-txs` failure, "unparseable reply".

You may pay more than the minimum fee, use no change output, or several.

## Properties

- `valid-txs` (mechanical): every reply is a valid transaction as above. Any
  violation (including the participant exiting, crashing or hitting a limit) stops
  that scenario and the submission is rejected, with a short detail naming the
  payment.
- `serves-feasible` (mechanical): you may refuse a payment only when the judge's own
  largest-first selection, run on your current wallet, cannot build a valid
  transaction. Otherwise rejected.
- `change-privacy` (declared): never checked. You may claim it in `submission.toml`
  with an argument; the site shows it as unverified.

## Scenarios

Eight per judge run, from `CB_SEED` through a SHA-256 counter-mode stream (so what
you have seen reveals nothing about later events): 60 to 120 initial UTxOs (log-normal around 50
ADA), then 300 events: about 30% deposits of one to five UTxOs, about 70% payments of
one to three outputs (log-normal around 20 ADA, at least 1 ADA each). Payments are
always fundable, assuming each earlier transaction cost at most 1 ADA in fees. The
seed is never given to your program.

## Axes

`fees`, `dust`, `inputs_per_tx`, `latency_ms`, `peak_rss_mib`; see `README.md`. They
are measured only when every check passed, from the transactions you returned and
from `cb participant`'s report, never from anything your program prints about itself.

## Limits

Per run: 60 s CPU, 120 s wall, 64 MiB of files, and, for the whole process tree of
your program (not per process), 1024 MiB memory and 32 processes. Exceeding one is a
`valid-txs` failure. Peak memory is the tree's cgroup peak, including page cache.
