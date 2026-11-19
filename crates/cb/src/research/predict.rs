//! Predicted against measured (#104). A submission may declare, per axis,
//! whether it expects to come out `lower`, `same` or `higher` than its
//! reference: its parent when it builds on one, otherwise the baseline. This
//! is information only: nothing is ranked or rejected on it.

use super::spec::{Declaration, ResearchSpec};
use super::util::agrees;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub const OUTCOMES: [&str; 3] = ["lower", "same", "higher"];

/// The submission a declaration's `expected` is relative to.
pub fn reference_id(decl: &Declaration) -> Option<&str> {
    match decl.novelty.as_str() {
        "builds-on" => Some(decl.parent.as_str()),
        _ if decl.id == "baseline" => None,
        _ => Some("baseline"),
    }
}

/// `lower`, `same` or `higher`: `same` when the two agree within the axis's
/// tolerance (the rule `cb record` uses for two runs of one submission).
fn outcome(own: f64, reference: f64, rel: f64, abs: f64) -> &'static str {
    if agrees(own, reference, rel, abs) {
        "same"
    } else if own < reference {
        "lower"
    } else {
        "higher"
    }
}

/// One row per declared axis: `{axis, expected, measured, held, reference,
/// own, reference_value}`. `measured` is `unknown` (and `held` null) when
/// either side has no median for the axis, or there is no reference row.
/// `axes` is this run's combined axes; `accepted` maps id → accepted row.
pub fn predictions(
    spec: &ResearchSpec,
    decl: &Declaration,
    axes: &Value,
    accepted: &BTreeMap<String, Value>,
) -> Value {
    let reference = reference_id(decl);
    let ref_row = reference
        .and_then(|id| accepted.get(id))
        .filter(|r| r["contract"] == decl.contract.as_str());
    let rows: Vec<Value> = decl
        .expected
        .iter()
        .map(|(axis, expected)| {
            let tol = spec
                .axes_for(&decl.contract)
                .into_iter()
                .find(|a| &a.id == axis)
                .map(|a| (a.tolerance, a.tolerance_abs));
            let own = axes[axis]["median"].as_f64();
            let theirs = ref_row.and_then(|r| r["axes"][axis]["median"].as_f64());
            let measured = match (own, theirs, tol) {
                (Some(o), Some(t), Some((rel, abs))) => Some(outcome(o, t, rel, abs)),
                _ => None,
            };
            json!({
                "axis": axis,
                "expected": expected,
                "measured": measured.unwrap_or("unknown"),
                "held": measured.map(|m| m == expected),
                "reference": reference,
                "own": own,
                "reference_value": theirs,
            })
        })
        .collect();
    Value::Array(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_uses_tolerance() {
        assert_eq!(outcome(10.0, 10.5, 0.1, 0.0), "same");
        assert_eq!(outcome(8.0, 10.0, 0.1, 0.0), "lower");
        assert_eq!(outcome(12.0, 10.0, 0.1, 0.0), "higher");
        // Near zero the absolute floor decides.
        assert_eq!(outcome(0.0, 0.125, 0.3, 2.0), "same");
        assert_eq!(outcome(0.0, 0.125, 0.0, 0.0), "lower");
    }
}
