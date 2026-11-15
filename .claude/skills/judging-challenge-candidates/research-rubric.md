# Research candidate rubric (draft v0)

A research challenge is an open search space. Participants choose which axes to push, either open a new direction (architecture, algorithm family, protocol) or build on an existing one, and every submission that passes the gates is measured and plotted. Nothing is ranked against a frontier. Novelty is self-declared by the participant.

## Gates (any fail = reject, with reason)

| Gate | Passes when |
|---|---|
| **R1 Contract** | A fixed task exists that very different implementations can all meet: same inputs, same required outputs or guarantees. Whole replacement codebases must be able to satisfy it. Several versioned contracts are fine (chemistry-walk adds `lih-qpe-sector` beside `lih-qpe`). |
| **R2 Core correctness is checked mechanically** | At least the central property ("the output is a valid proof of the statement", "the energy is within ε of the reference") is checked by code the participant cannot modify: a frozen reference verifier, a certificate checker, an independent dense reference at tractable size. |
| **R3 ≥ 2 measured axes** | Two or more axes can be measured automatically per submission, each with a unit and a direction. No weights needed or wanted. |
| **R4 Bounded** | One verify+measure run finishes within 6 h on some named runner (same as G3). |
| **R5 Baseline** | A reference implementation exists, passes R2, and has been measured on every axis (same as G4). |

Each gate is `pass`, `fail` or `pending` (needs a measurement not yet taken). Any `pending` makes the verdict **spike needed**, never pass.

R2 is the hard one. When directions can change the protocol itself (e.g. a new commitment scheme), the participant controls both sides and a "verifier accepts" check proves nothing. Handle it one of these ways, and say which:

- **Freeze the boundary:** the verifier or reference stays fixed, so directions vary everything behind it.
- **Independent oracle:** check against an outside ground truth (dense reference, published vectors, a second implementation).
- **Declared claims:** properties that can't be checked mechanically (e.g. a security level of a new protocol) are recorded as claims with their argument, and shown on the graphs as *unverified*. That is allowed for secondary properties only; never for R2's core property.

Mark a frozen check of a conjectured or heuristic bound (e.g. a FRI security formula) as `mechanical (conjectured)`.

**Directions R2 can't admit yet** (the participant would write both prover and verifier) are not dropped: list them as a **deferred contract** with the independent check that would make them admissible. They usually hold the most ambitious ideas, so the verdict names them.

**Each contract states the participant's choice space:** which parameters they may pick (e.g. hash family, FRI parameters), and the floors those choices must respect (e.g. security bits ≥ N, with a source for N).

## Scored criteria (1–5; total = Σ w·s / Σ w·5 × 100)

| Criterion | W | 5 looks like | 1 looks like |
|---|---|---|---|
| Direction diversity | 3 | ≥3 genuinely different known or plausible architectures, with sources | one obvious design; everything else is tuning |
| Axis tension | 2 | axes trade off (cheaper step, more queries; less memory, more time), so the map has real shape | all axes move together: really a simple challenge |
| Property checkability | 3 | every preserved property mechanically checked at the boundary | only R2's core is checked; the rest are declared claims |
| Component entry points | 2 | contributors can improve one component alone against its own contract | only whole-system submissions are meaningful |
| Value | 3 | a new direction would change practice (Starknet or broadly) | curiosity only |
| Expert scarcity | 2 | <50 people could seriously work on it | crowded field |
| Fidelity | 2 | contract = real use | toy proxy |
| Gaming resistance | 2 | outputs checked by an independent oracle on salted inputs | participant code checked by its own tests |
| Laptop validation | 2 | produce + verify + measure on a laptop in minutes | needs a big machine even to check correctness |
| Eval cost | 1 | seconds–minutes | hours |
| Seed literature | 1 | papers and code describing several directions | tribal knowledge |

Laptop and hosting follow PLAN.md §1: record `produce`/`verify`/`score` per laptop class as measured facts, plus a separate **hostable here** flag (2 vCPU / 8 GB / 14 GB, ≤60 min).

## Output: `candidates/research/<slug>.md`

Use these sections, in order:

```markdown
# <Title> (research candidate, draft YYYY-MM-DD, AMBER)

<!-- AMBER is the sharing label from PLAN.md: candidate evaluations are internal, never published. It is not a status. -->

Source: <repo @ commit, licence>. Measured: <machine, what>. Unmeasured values say "estimate".

## Verdict
Status: **launchable** | **spike needed** | **rejected**. Gates R1–R5, hand total (say it is by hand, and give Σw·s / Σw·5), one paragraph on why, deferred directions named.

## Contract(s)
For each: id@version, inputs, required outputs/guarantees, what is frozen, the participant's choice space and floors. Deferred contracts last, each with the check that would admit it.

## Preserved properties
| Property | Checked by | Mechanical or declared |

## Measured axes
| Axis | Unit | Direction | How measured | Baseline value |

## Seed taxonomy
Axes of variation, named values per axis (≥2 genuinely different directions, sourced), compatibility rules. Participants extend it when they open a new direction.

## Entry points
Whole-system; components (each with its own contract).

## Rubric
| Gate/Criterion | W | Score | Evidence |

## Laptop and hosting
apple / x86 produce·verify·score; peaks; hostable here.

## Simple-challenge slices (optional)
Narrow scalar challenges carved from this space, each one line.

## Spike checklist
- [ ] what must be measured before launch
```
