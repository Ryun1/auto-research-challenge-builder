# Threat model

Required before launch; reviewed in the PR that opens the challenge.

## How could a wrong answer pass the verifier?

- Finite or reusable inputs (is the input set fresh per run? how many?): the gate is a kernel-checked proof of `∀ n, sumTo n = Spec.spec n`, which covers every n. The 53 CB_SEED-salted executable vectors are only a falsification check.
- Caching or precomputation (can a table or memo pay off without knowing the seed?): not applicable. Any valid proof is a valid answer, and the score is its size.
- Reading the reference, oracle, seed or expected output: the spec (judge/Spec.lean) is public by design. It is elaborated before the candidate, and the judge restates the theorem against `Spec.spec`, so a candidate instance, notation or shadowed name can't redefine what is proven. `instance`, `notation`, `open` and the other elaboration-time and redefinition commands are banned lexically too.
- Timing tricks (detecting the benchmark, doing less work when measured): not applicable, since the score is a character count.
- Writing the score file directly, or editing anything outside the editable paths: the gate refuses anything outside candidate/, and verify requires exactly one file, Submission.lean (16 KiB cap). Lean runs code while elaborating, so the candidate is elaborated only inside verify (sandboxed in the pipeline: nobody, no network, temp dir), after the lexical policy has refused `#eval`/`run_cmd`/`run_tac`/`elab`/`macro`/`syntax`/`initialize`/`set_option`/`unsafe`/`extern`/`implemented_by`/`csimp`/`native_decide`/`sorry`/`axiom` and any import other than `Spec`.
- Soundness escapes: `#print axioms` on the judge's side may list only propext, Classical.choice and Quot.sound, which catches `sorry`, `native_decide` (`Lean.ofReduceBool`) and custom axioms at the kernel level.

## Does the benchmark interface match real use?

It's a toy: the goal is to exercise the proof family's gates (banned tokens and axioms, executable vectors, disk and time limits). Proof golf is not a real objective.

## What is the known bound or headroom?

Unknown, and it doesn't matter for a toy. The baseline is an unoptimised induction proof.

## What does a large outlier (+100%) trigger?

Manual review of the submission against the policy; never auto-promote.

- Who reviews: the challenge owner.
