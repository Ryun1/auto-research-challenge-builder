//! Rubric v0 (PLAN.md §1): hard gates, then weighted 1–5 criteria.
//!
//! Laptop validation and "hostable here" are derived from measurements,
//! never entered as scores.

use serde::Deserialize;
use std::collections::BTreeMap;

pub const RUBRIC_VERSION: u32 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Criterion {
    ExpertScarcity,
    Headroom,
    Laptop,
    EvalCost,
    Noise,
    GamingResistance,
    Fidelity,
    Value,
    Decomposability,
    PriorArt,
}

/// v0 weights, in plan order. Laptop is derived (see [`laptop_score`]).
pub const WEIGHTS: [(Criterion, u32); 10] = [
    (Criterion::ExpertScarcity, 3),
    (Criterion::Headroom, 2),
    (Criterion::Laptop, 2),
    (Criterion::EvalCost, 2),
    (Criterion::Noise, 2),
    (Criterion::GamingResistance, 2),
    (Criterion::Fidelity, 2),
    (Criterion::Value, 2),
    (Criterion::Decomposability, 1),
    (Criterion::PriorArt, 1),
];

/// A pass/fail fact with the link or measurement behind it.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub pass: bool,
    pub evidence: String,
}

/// G1–G4. Any failure rejects the candidate.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gates {
    /// G1: deterministic verifier, no human judgement in the loop.
    pub verifiable: Check,
    /// G2: scalar objective with a direction.
    pub scalar_objective: Check,
    /// G3: one verify+score run finishes within 6 h on some named runner.
    pub bounded: Check,
    /// G4: a baseline exists and passes the verifier.
    pub baseline: Check,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Score {
    pub value: u8,
    pub evidence: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Fact {
    Yes,
    YesSlow,
    No,
}

/// Can a participant produce / verify / score on this laptop class?
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Facts {
    pub produce: Fact,
    pub verify: Fact,
    pub score: Fact,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Laptop {
    /// Apple Silicon, 16 GB.
    pub apple: Facts,
    /// x86 Linux, 16 GB.
    pub x86: Facts,
    pub evidence: String,
}

/// Measured peaks of one verify+score run.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Peaks {
    pub minutes: f64,
    pub ram_gb: f64,
    pub disk_gb: f64,
    pub gpu: bool,
    #[serde(default)]
    pub evidence: String,
}

/// GitHub-hosted private runner: 2 vCPU / 8 GB / 14 GB disk, ≤60 min target.
pub const HOSTED: Peaks = Peaks {
    minutes: 60.0,
    ram_gb: 8.0,
    disk_gb: 14.0,
    gpu: false,
    evidence: String::new(),
};

#[derive(Clone, Debug)]
pub struct Candidate {
    pub gates: Gates,
    /// Every criterion except [`Criterion::Laptop`].
    pub scores: BTreeMap<Criterion, Score>,
    /// `None` until measured (M1).
    pub laptop: Option<Laptop>,
    /// `None` until measured.
    pub peaks: Option<Peaks>,
}

#[derive(Debug, PartialEq)]
pub struct Evaluation {
    /// Failed gates with their evidence. Non-empty means rejected.
    pub rejected: Vec<String>,
    /// 0–100. `None` if rejected or laptop facts are unmeasured.
    pub total: Option<f64>,
    pub laptop_score: Option<u8>,
    /// `None` until peaks are measured.
    pub hostable: Option<bool>,
    pub suggestions: Vec<String>,
}

/// Six facts, each worth yes=2, yes-slow=1, no=0, mapped onto 1–5.
/// All yes → 5, all no → 1.
pub fn laptop_score(l: &Laptop) -> u8 {
    let points: u32 = [l.apple, l.x86]
        .iter()
        .flat_map(|f| [f.produce, f.verify, f.score])
        .map(|f| match f {
            Fact::Yes => 2,
            Fact::YesSlow => 1,
            Fact::No => 0,
        })
        .sum();
    (1 + (4 * points + 6) / 12) as u8 // round(4·points/12), half up
}

pub fn hostable(p: &Peaks) -> bool {
    !p.gpu
        && p.minutes <= HOSTED.minutes
        && p.ram_gb <= HOSTED.ram_gb
        && p.disk_gb <= HOSTED.disk_gb
}

/// Fixes for laptop facts that aren't "yes" (PLAN.md §1).
fn suggestions(l: &Laptop) -> Vec<String> {
    let mut out = Vec::new();
    let both = |pick: fn(&Facts) -> Fact| [pick(&l.apple), pick(&l.x86)];
    let fails = |pick: fn(&Facts) -> Fact| both(pick).iter().any(|f| *f != Fact::Yes);
    // One class fine, the other not: an arch-specific path (x86 SIMD, CUDA).
    let split = |pick: fn(&Facts) -> Fact| {
        let [a, x] = both(pick);
        (a == Fact::Yes) != (x == Fact::Yes)
    };
    if fails(|f| f.produce) {
        out.push("produce: ship a small-size local profile so participants can generate candidates on a laptop".into());
    }
    if fails(|f| f.verify) {
        out.push("verify: add a cheap correctness check so only record verification needs the big machine".into());
    }
    if fails(|f| f.score) {
        out.push("score: ship a small local profile whose score tracks the official one".into());
    }
    if split(|f| f.produce) || split(|f| f.verify) || split(|f| f.score) {
        out.push("arch: works on one laptop class only; add a portable CPU reference path next to the arch-specific one".into());
    }
    out
}

/// Scores a candidate, or lists why it can't be scored (bad input, not a
/// rejection: rejections are in [`Evaluation::rejected`]).
pub fn evaluate(c: &Candidate) -> Result<Evaluation, Vec<String>> {
    let mut errors = Vec::new();
    let g = &c.gates;
    let gates = [
        ("G1 machine-verifiable correctness", &g.verifiable),
        ("G2 scalar objective", &g.scalar_objective),
        ("G3 bounded verification (≤6 h, named runner)", &g.bounded),
        ("G4 baseline exists and passes", &g.baseline),
    ];
    for (name, check) in gates {
        if check.evidence.trim().is_empty() {
            errors.push(format!("{name}: missing evidence"));
        }
    }
    let mut rejected: Vec<String> = gates
        .iter()
        .filter(|(_, check)| !check.pass)
        .map(|(name, check)| format!("{name}: {}", check.evidence))
        .collect();

    for (criterion, score) in &c.scores {
        if *criterion == Criterion::Laptop {
            errors.push("Laptop is derived from measured facts; don't score it".into());
        } else if !(1..=5).contains(&score.value) {
            errors.push(format!("{criterion:?}: score {} not in 1..=5", score.value));
        } else if score.evidence.trim().is_empty() {
            errors.push(format!("{criterion:?}: missing evidence"));
        }
    }
    let laptop_score = c.laptop.as_ref().map(laptop_score);
    if c.laptop
        .as_ref()
        .is_some_and(|l| l.evidence.trim().is_empty())
    {
        errors.push("Laptop: missing evidence".into());
    }
    if c.peaks
        .as_ref()
        .is_some_and(|p| p.evidence.trim().is_empty())
    {
        errors.push("Peaks: missing evidence".into());
    }

    let mut weighted = 0;
    let mut missing = false;
    for (criterion, weight) in WEIGHTS {
        let value = match criterion {
            Criterion::Laptop => laptop_score,
            _ => c.scores.get(&criterion).map(|s| s.value),
        };
        match value {
            Some(v) => weighted += weight * v as u32,
            None if criterion == Criterion::Laptop => missing = true,
            None => errors.push(format!("{criterion:?}: not scored")),
        }
    }
    if !errors.is_empty() {
        errors.dedup();
        return Err(errors);
    }
    rejected.sort();
    let max: u32 = WEIGHTS.iter().map(|(_, w)| w * 5).sum();
    let total = (rejected.is_empty() && !missing)
        .then(|| (weighted as f64 * 1000.0 / max as f64).round() / 10.0);
    Ok(Evaluation {
        rejected,
        total,
        laptop_score,
        hostable: c.peaks.as_ref().map(hostable),
        suggestions: c.laptop.as_ref().map(suggestions).unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(evidence: &str) -> Check {
        Check {
            pass: true,
            evidence: evidence.into(),
        }
    }

    fn all_yes() -> Facts {
        Facts {
            produce: Fact::Yes,
            verify: Fact::Yes,
            score: Fact::Yes,
        }
    }

    fn candidate(value: u8) -> Candidate {
        Candidate {
            gates: Gates {
                verifiable: ok("bit-exact oracle"),
                scalar_objective: ok("speedup"),
                bounded: ok("ubuntu-latest, 3 min"),
                baseline: ok("CI run"),
            },
            scores: WEIGHTS
                .iter()
                .filter(|(k, _)| *k != Criterion::Laptop)
                .map(|(k, _)| {
                    (
                        *k,
                        Score {
                            value,
                            evidence: "src".into(),
                        },
                    )
                })
                .collect(),
            laptop: Some(Laptop {
                apple: all_yes(),
                x86: all_yes(),
                evidence: "M2 + x86".into(),
            }),
            peaks: Some(Peaks {
                minutes: 3.0,
                ram_gb: 1.0,
                disk_gb: 2.0,
                gpu: false,
                evidence: "ci".into(),
            }),
        }
    }

    #[test]
    fn weights_match_plan() {
        let total: u32 = WEIGHTS.iter().map(|(_, w)| w).sum();
        assert_eq!(total, 19);
    }

    #[test]
    fn perfect_and_floor() {
        let e = evaluate(&candidate(5)).unwrap();
        assert_eq!(
            (e.total, e.laptop_score, e.hostable),
            (Some(100.0), Some(5), Some(true))
        );
        assert!(e.suggestions.is_empty());

        let mut c = candidate(1);
        let no = Facts {
            produce: Fact::No,
            verify: Fact::No,
            score: Fact::No,
        };
        c.laptop = Some(Laptop {
            apple: no,
            x86: no,
            evidence: "GPU only".into(),
        });
        // Every criterion at 1 → 19·1 / 95 = 20.
        assert_eq!(evaluate(&c).unwrap().total, Some(20.0));
    }

    #[test]
    fn laptop_mapping() {
        let l = |apple, x86| {
            laptop_score(&Laptop {
                apple,
                x86,
                evidence: "m".into(),
            })
        };
        // Heesch-like: produce yes, verify yes, score no, on both → 8/12 → 4.
        let heesch = Facts {
            produce: Fact::Yes,
            verify: Fact::Yes,
            score: Fact::No,
        };
        assert_eq!(l(heesch, heesch), 4);
        // Rust kit: score is yes-slow on both → 10/12 → 4.
        let kit = Facts {
            score: Fact::YesSlow,
            ..all_yes()
        };
        assert_eq!(l(kit, kit), 4);
        // QSB-like: GPU needed for everything → 1.
        let no = Facts {
            produce: Fact::No,
            verify: Fact::No,
            score: Fact::No,
        };
        assert_eq!(l(no, no), 1);
    }

    #[test]
    fn gate_failure_rejects_without_total() {
        let mut c = candidate(5);
        c.gates.bounded = Check {
            pass: false,
            evidence: "40 h on 128 GB box".into(),
        };
        let e = evaluate(&c).unwrap();
        assert_eq!(e.total, None);
        assert_eq!(
            e.rejected,
            vec!["G3 bounded verification (≤6 h, named runner): 40 h on 128 GB box"]
        );
    }

    #[test]
    fn bad_input_is_an_error_not_a_score() {
        let mut c = candidate(3);
        c.scores.get_mut(&Criterion::ExpertScarcity).unwrap().value = 6;
        c.scores.get_mut(&Criterion::Headroom).unwrap().evidence = " ".into();
        c.scores.remove(&Criterion::PriorArt);
        c.gates.baseline.evidence.clear();
        let errors = evaluate(&c).unwrap_err();
        assert_eq!(errors.len(), 4, "{errors:?}");

        let mut c = candidate(3);
        c.scores.insert(
            Criterion::Laptop,
            Score {
                value: 5,
                evidence: "guess".into(),
            },
        );
        assert!(evaluate(&c).is_err());

        let mut c = candidate(3);
        c.peaks.as_mut().unwrap().evidence.clear();
        assert_eq!(evaluate(&c).unwrap_err(), vec!["Peaks: missing evidence"]);
    }

    #[test]
    fn unmeasured_laptop_leaves_total_open() {
        let mut c = candidate(4);
        c.laptop = None;
        c.peaks = None;
        let e = evaluate(&c).unwrap();
        assert_eq!((e.total, e.hostable), (None, None));
    }

    #[test]
    fn hostable_limits() {
        let p = |minutes, ram_gb, disk_gb, gpu| {
            hostable(&Peaks {
                minutes,
                ram_gb,
                disk_gb,
                gpu,
                evidence: String::new(),
            })
        };
        assert!(p(60.0, 8.0, 14.0, false));
        assert!(!p(61.0, 1.0, 1.0, false));
        assert!(!p(1.0, 128.0, 1.0, false)); // heesch
        assert!(!p(1.0, 1.0, 60.0, false));
        assert!(!p(1.0, 1.0, 1.0, true)); // QSB
    }

    #[test]
    fn suggestions_follow_facts() {
        let x86_simd = Laptop {
            apple: Facts {
                produce: Fact::Yes,
                verify: Fact::No,
                score: Fact::No,
            },
            x86: all_yes(),
            evidence: "m".into(),
        };
        let s = suggestions(&x86_simd);
        assert_eq!(s.len(), 3, "{s:?}");
        assert!(
            s[0].starts_with("verify:") && s[1].starts_with("score:") && s[2].starts_with("arch:")
        );
    }
}
