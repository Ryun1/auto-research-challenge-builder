# Prompt for coin-select

You are working on the research challenge `coin-select`: Cardano UTxO coin selection
(CIP-2). Pick one contract: `wallet@1` (ADA only) or `wallet-multiasset@1` (native tokens,
datums, a real byte model). Read its `INTERFACE.md` and `CHOICES.md` under
`contracts/<contract>/`, then
`DIRECTIONS.md` (what has been tried, what is open and the cheapest test for each
open idea), `submissions/` and `taxonomy.toml`. Each contract has a Python largest-first baseline (`baseline`, `baseline-multiasset`).

Create ONE new directory `submissions/<id>/` and change nothing else. It needs a
`submission.toml`, an executable `build.sh` that leaves an executable `./select`, and
your code; see `SUBMITTING.md` for the fields. Either start a new direction (random
improve, branch and bound, consolidation policies, ...) or copy an accepted
submission's directory and build on it. Declare novelty honestly: porting an accepted
submission to another language, or retuning its constants or seed, is `builds-on`,
not a new direction. You choose which axes in `challenge.toml` to push (fees, dust or
locked ADA, inputs per transaction, latency, memory): they are measured, recorded and never
ranked or combined, so a trade-off is a fine result. Before measuring, write your
hypothesis and the axes you expect to move in `submission.toml` (`hypothesis`,
`expected`; see `SUBMITTING.md`). If the measurement refutes it, submit anyway and
say so: that is a result. Run the judge locally (see `README.md`) before opening a
PR.
