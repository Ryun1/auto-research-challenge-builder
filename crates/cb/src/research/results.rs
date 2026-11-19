//! Reading research `results.jsonl` rows (design spec §6).

use serde_json::Value;
use std::collections::BTreeMap;

/// The latest `accept`/`baseline` row per submission id, kept only when that
/// row's status is `accepted`.
pub fn accepted(results: &[Value]) -> BTreeMap<String, Value> {
    let mut latest: BTreeMap<String, &Value> = BTreeMap::new();
    for r in results {
        if !matches!(r["stage"].as_str(), Some("accept") | Some("baseline")) {
            continue;
        }
        if let Some(id) = r["submission"].as_str() {
            latest.insert(id.to_string(), r);
        }
    }
    latest
        .into_iter()
        .filter(|(_, r)| r["status"].as_str() == Some("accepted"))
        .map(|(k, r)| (k, r.clone()))
        .collect()
}

/// The direction recorded on `id`'s latest accept/baseline row.
pub fn direction_of(results: &[Value], id: &str) -> Option<String> {
    results
        .iter()
        .rev()
        .find(|r| {
            matches!(r["stage"].as_str(), Some("accept") | Some("baseline"))
                && r["submission"].as_str() == Some(id)
        })
        .and_then(|r| r["direction"].as_str().map(String::from))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn latest_row_decides() {
        let rows = vec![
            json!({"stage":"baseline","status":"accepted","submission":"baseline","direction":"baseline"}),
            json!({"stage":"pr","status":"accepted","submission":"a","direction":"x"}),
            json!({"stage":"accept","status":"accepted","submission":"a","direction":"x"}),
            json!({"stage":"accept","status":"rejected","submission":"a","direction":"x"}),
            json!({"stage":"accept","status":"accepted","submission":"b","direction":"y"}),
        ];
        let acc = accepted(&rows);
        assert_eq!(acc.keys().cloned().collect::<Vec<_>>(), ["b", "baseline"]);
        assert_eq!(direction_of(&rows, "b").as_deref(), Some("y"));
        assert_eq!(direction_of(&rows, "zz"), None);
    }
}
