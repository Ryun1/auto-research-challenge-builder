# Threat model

Required before launch; reviewed in the PR that opens the challenge.

## How could a wrong answer pass the verifier?

- Finite or reusable inputs (is the input set fresh per run? how many?): all 256 0/1 inputs are checked, which is sufficient by the 0-1 principle (a network sorts every input if it sorts every 0/1 input). On top of that, 50,000 CB_SEED-salted integer inputs are checked per run, following the ECDSA.fail lesson, as an independent falsification check.
- Caching or precomputation (can a table or memo pay off without knowing the seed?): no. The submission is a fixed comparator list checked exhaustively, and the known optimum (19) is public, which is fine for a toy.
- Reading the reference, oracle, seed or expected output: no participant code runs. The verifier recomputes everything from the comparator list.
- Timing tricks (detecting the benchmark, doing less work when measured): not applicable. The score is a count, not a time.
- Writing the score file directly, or editing anything outside the editable paths: the PR gate refuses changes outside candidate/, plus symlinks and gitlinks. network.txt is capped at 4 KiB of ASCII.

## Does the benchmark interface match real use?

Yes: the comparator list is the network.

## What is the known bound or headroom?

19 comparators is optimal for 8 inputs (Knuth, TAOCP vol. 3, §5.3.4). The baseline is bubble sort at 28, so the headroom is 9.

## What does a large outlier (+100%) trigger?

Nothing: fewer than 19 is impossible, and the check is exhaustive.

- Who reviews: the challenge owner confirms any claimed 19 against the published optimum.
