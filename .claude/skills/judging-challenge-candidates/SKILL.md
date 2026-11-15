---
name: judging-challenge-candidates
description: Use when asked whether a repo, paper, codebase or idea would make a good autoresearch challenge, when evaluating or scoring a candidate problem, or when writing a file under candidates/ in the challenge builder.
---

# Judging challenge candidates

## Overview

Candidates come in two types, judged by different rubrics. **Classify first.** Judging a research candidate against the simple rubric makes it fail G1/G2 (no single scalar), and the tempting fix — reframing it into a narrow scalar challenge — deletes exactly the architectural freedom that made it worth running.

| | Simple | Research |
|---|---|---|
| Asks for | 1–2 numbers improved (faster, less memory, fewer Ir) | "better", several axes, new architectures |
| Submission | edits to a small fixed surface | edits **or** a whole replacement codebase meeting a contract |
| Correctness | bit-exact vs a reference oracle | preserved properties checked at a contract boundary |
| Outcome | a record line; promote if ≥ min improvement | a map of directions; no winner, every measured submission counts |
| Novelty | not tracked | self-declared: "new direction" or "builds on <direction>" |
| Example | `challenges/ledger-cbor` | [chemistry-walk](https://github.com/teddyjfpender/quantum-autoresearch/blob/main/challenges/chemistry-walk/README.md) |
| Rubric | PLAN.md §1 / `candidates/README.md` | `research-rubric.md` (this directory) |
| Output | `candidates/<slug>.toml` (read by `cb rank`) | `candidates/research/<slug>.md` (not read by `cb`) |

## Classifying

The deciding test: **can every good submission be ranked by one or two numbers, under a checker that is the same for all of them?**

- Yes → **simple**. Two cases:
  - the output is fixed and only its cost changes (a bit-exact oracle), or
  - the output is data a fixed checker scores (math/proof families: a sorting network checked by the 0-1 principle, a Heesch tiling with its certificate).

  This holds even with two trading-off metrics (memory and runtime): combine them in `objective.expr`, or rank one and guard the other, as the Rust kit does with Ir ranked and wall time guarded.
- No: "good" spans several axes with no agreed scalar, and the interesting moves change architecture, protocol, encoding or algorithm family, so outputs differ and must instead meet guarantees at a contract boundary. Typical wording: "design", "better", "redesign" → **research**.
- The request is too vague to name even one contract (e.g. "make X better"): ask the requester what must stay true and what they care about **before** writing a file. If you can't ask, judge it as research and list your assumed contract as the first open question in the verdict.

Slugs are lowercase-hyphenated, short, and named after the project and target (`plonky3-poseidon2`, `stwo-prover`).

A research request stays research. If a narrow scalar slice is also attractive (e.g. "same proofs, cheaper prover" inside "better prover"), record it as an **additional** simple-candidate suggestion in the research write-up — never as a replacement, and never by failing the research candidate on G1/G2.

## Workflow (both types)

1. Read the source: the repo at a pinned commit, its docs, issues/PRs, papers. Check the URL is still where development happens; if work moved, pin the new home and say so in the Source line. If the target spans several repos, pin each one.
2. Classify, as above.
3. Propose the shape:
   - **Simple:** metric, editable surface, oracle, min improvement, noise plan.
   - **Research:** contract(s), preserved properties and what checks each, measured axes, seed taxonomy of directions, entry points (whole-system and component).
4. Score with the matching rubric. Every value carries evidence: a link or a measurement. Unmeasured values say `estimate`.
5. Measure what you cheaply can (build, a run, peak RAM) on the local machine. Do not trigger CI runs: Actions minutes are metered.
6. Write the file, then end with a spike checklist of what must be measured before launch.

The simple path is already well documented: follow PLAN.md §1 and `candidates/README.md` exactly; `cb rank --dir candidates` totals it. For research, **REQUIRED:** read `research-rubric.md` before scoring.

## Common mistakes

| Mistake | Fix |
|---|---|
| Research candidate failed on G1 (no checker for "better") or G2 (no scalar) | Those gates belong to the simple rubric. Use the research gates. |
| "Reframed" research into a frozen-protocol scalar challenge | Keep the research candidate; add the slice as a separate suggestion. |
| Weighting axes into one score for a research candidate | Axes stay a vector. Graphs show trade-offs; nobody is ranked. |
| A preserved property "checked" by the participant's own verifier | Only code the participant can't change counts as a mechanical check. Otherwise it's a declared claim. |
| Seed taxonomy lists only the incumbent design | Name at least two genuinely different directions, with sources. |
| Hand total presented as `cb` output | Say it was computed by hand. |
