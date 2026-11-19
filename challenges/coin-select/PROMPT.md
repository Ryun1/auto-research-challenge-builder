# Prompt for coin-select

You are working on the research challenge `coin-select`: Cardano UTxO coin selection
(CIP-2). Read `contracts/wallet/INTERFACE.md` and `contracts/wallet/CHOICES.md`, then
`DIRECTIONS.md` (what has been tried, what is open and the cheapest test for each
open idea), `submissions/` and `taxonomy.toml`. The baseline is a Python largest-first.

Create ONE new directory `submissions/<id>/` and change nothing else. It needs a
`submission.toml`, an executable `build.sh` that leaves an executable `./select`, and
your code; see `SUBMITTING.md` for the fields. Either start a new direction (random
improve, branch and bound, consolidation policies, ...) or copy an accepted
submission's directory and build on it. Declare novelty honestly: porting an accepted
submission to another language, or retuning its constants or seed, is `builds-on`,
not a new direction. You choose which axes in `challenge.toml` to push (fees, dust,
inputs per transaction, latency, memory): they are measured, recorded and never
ranked or combined, so a trade-off is a fine result. Run the judge locally (see
`README.md`) before opening a PR.
