# Threat model

Required before launch; reviewed in the PR that opens the challenge.

## How could a wrong answer pass the verifier?

- Finite or reusable inputs (is the input set fresh per run? how many?): no inputs. The submission is the whole answer, and the check is exhaustive over all 91 pairwise differences.
- Caching or precomputation (can a table or memo pay off without knowing the seed?): yes, and that's fine. Known optimal rulers are public (length 127 for 14 marks, Shearer's tables). This is a toy; submitting a known optimum is a valid answer.
- Reading the reference, oracle, seed or expected output: there is no hidden expected output. The verifier re-derives validity from the marks.
- Timing tricks (detecting the benchmark, doing less work when measured): not applicable, because no participant code runs.
- Writing the score file directly, or editing anything outside the editable paths: the PR gate refuses any change outside candidate/, plus symlinks and gitlinks. The verifier reads at most 4 KiB of ASCII integers.

## Does the benchmark interface match real use?

Yes. The question is the ruler itself.

## What is the known bound or headroom?

The optimal 14-mark Golomb ruler has length 127, proven by exhaustive search (distributed.net OGR). The baseline is the greedy Mian–Chowla ruler at 181, so the headroom is 54.

## What does a large outlier (+100%) trigger?

Nothing beyond the verifier: validity is exact, and a length below 127 is impossible.

- Who reviews: the challenge owner confirms any claimed length of 127 or less against the published optimum.
