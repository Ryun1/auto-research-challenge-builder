//! The schema-2 research feed (design spec §8). Public: built only from
//! `results.jsonl` and challenge metadata; never runs, reasons or seeds.

use super::results::accepted;
use super::spec::ResearchSpec;
use super::util::{clip, str_of};
use crate::pipeline::read_results;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::Path;

pub const FEED_SCHEMA_VERSION: u32 = 2;

fn claims(v: &Value) -> Value {
    let list = v
        .as_array()
        .map(|a| {
            a.iter()
                .map(|c| {
                    json!({"property": str_of(&c["property"]),
                           "argument": clip(str_of(&c["argument"]), 2000)})
                })
                .collect()
        })
        .unwrap_or_default();
    Value::Array(list)
}

pub fn feed(root: &Path, slug: &str, url: &str, generated_at: &str) -> Result<Value, String> {
    let spec = ResearchSpec::load(&root.join(slug))?;
    let results = read_results(root, slug)?;
    let acc = accepted(&results);

    let mut rows: Vec<(&String, &Value)> = acc.iter().collect();
    rows.sort_by(|a, b| {
        str_of(&a.1["at"])
            .cmp(str_of(&b.1["at"]))
            .then_with(|| a.0.cmp(b.0))
    });

    let submissions: Vec<Value> = rows
        .iter()
        .map(|(id, r)| {
            json!({
                "id": id,
                "baseline": id.as_str() == "baseline",
                "contract": r["contract"],
                "novelty": r["novelty"],
                "direction": r["direction"],
                "parent": r["parent"],
                "taxonomy": r["taxonomy"],
                "proposed": r["proposed"],
                "summary": clip(str_of(&r["summary"]), 500),
                "by": r["by"],
                "model": r["model"],
                "at": r["at"],
                "commit": r["commit"],
                "axes": r["axes"],
                "checks": r["checks"],
                "claims": claims(&r["claims"]),
            })
        })
        .collect();

    // Directions, in order of first appearance.
    let mut order: Vec<String> = Vec::new();
    let mut groups: BTreeMap<String, Vec<(&String, &Value)>> = BTreeMap::new();
    for (id, r) in &rows {
        let d = str_of(&r["direction"]).to_string();
        if !groups.contains_key(&d) {
            order.push(d.clone());
        }
        groups.entry(d).or_default().push((id, r));
    }
    let directions: Vec<Value> = order
        .iter()
        .map(|d| {
            let g = &groups[d];
            let root_id = g
                .iter()
                .find(|(_, r)| r["novelty"] == "new")
                .or_else(|| g.first())
                .map(|(id, _)| id.as_str());
            json!({"name": d, "root": root_id,
                   "submissions": g.iter().map(|(id, _)| id).collect::<Vec<_>>()})
        })
        .collect();

    // Taxonomy with usage counts; unlisted values used by accepted rows are proposed.
    let mut counts: BTreeMap<(String, String), u64> = BTreeMap::new();
    for (_, r) in &rows {
        if let Some(t) = r["taxonomy"].as_object() {
            for (k, v) in t {
                if let Some(v) = v.as_str() {
                    *counts.entry((k.clone(), v.to_string())).or_default() += 1;
                }
            }
        }
    }
    let tax_axes: Vec<Value> = spec
        .tax
        .axes
        .iter()
        .map(|a| {
            let mut values: Vec<Value> = a
                .values
                .iter()
                .map(|v| {
                    let n = counts
                        .get(&(a.id.clone(), v.id.clone()))
                        .copied()
                        .unwrap_or(0);
                    json!({"id": v.id, "definition": v.definition, "count": n, "proposed": false})
                })
                .collect();
            for ((ax, val), n) in &counts {
                if ax == &a.id && !a.values.iter().any(|v| &v.id == val) {
                    values.push(json!({"id": val, "definition": "", "count": n, "proposed": true}));
                }
            }
            json!({"id": a.id, "definition": a.definition, "values": values})
        })
        .collect();

    // Attempts are PR-stage rows (one per submitted PR measurement); accept
    // rows re-measure the same submission, so they are not counted again.
    let pr = |status: Option<&str>| {
        results
            .iter()
            .filter(|r| r["stage"] == "pr" && status.is_none_or(|s| r["status"] == s))
            .count()
    };

    let mut out = Map::new();
    out.insert("feed_schema_version".into(), json!(FEED_SCHEMA_VERSION));
    out.insert("type".into(), json!("research"));
    out.insert("slug".into(), json!(slug));
    out.insert("url".into(), json!(url));
    out.insert("generated_at".into(), json!(generated_at));
    out.insert("status".into(), json!(spec.challenge.status));
    out.insert("runner".into(), json!(spec.challenge.runner));
    out.insert(
        "axes".into(),
        spec.axes
            .iter()
            .map(|a| {
                json!({"id": a.id, "unit": a.unit, "direction": a.direction,
                       "tolerance": a.tolerance, "repeats": a.repeats, "contracts": a.contracts})
            })
            .collect(),
    );
    out.insert(
        "contracts".into(),
        spec.contracts
            .iter()
            .map(|c| {
                json!({"id": c.id, "status": c.status, "admits_when": c.admits_when,
                       "interface": c.interface, "choice_space": c.choice_space})
            })
            .collect(),
    );
    out.insert(
        "properties".into(),
        spec.properties
            .iter()
            .map(|p| json!({"id": p.id, "check": p.check, "contract": p.contract}))
            .collect(),
    );
    out.insert(
        "taxonomy".into(),
        json!({"version": spec.tax.version, "axes": tax_axes,
               "rules": spec.tax.rules.iter().map(|r| &r.text).collect::<Vec<_>>()}),
    );
    out.insert("submissions".into(), Value::Array(submissions));
    out.insert("directions".into(), Value::Array(directions));
    out.insert(
        "attempts".into(),
        json!({"total": pr(None), "valid": pr(Some("valid")), "accepted": acc.len(),
               "rejected": pr(Some("rejected")), "infra_failed": pr(Some("infra_failed"))}),
    );
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research::testkit::{toy, GOOD_JUDGE};

    #[allow(clippy::too_many_arguments)]
    fn row(
        stage: &str,
        status: &str,
        id: &str,
        at: &str,
        novelty: &str,
        direction: &str,
        parent: Option<&str>,
        approach: &str,
    ) -> Value {
        json!({"schema": 2, "type": "research", "at": at, "stage": stage, "status": status,
               "reason": "secret reason", "slug": "t", "submission": id, "commit": "abc",
               "by": "someone", "model": "m", "contract": "main@1", "novelty": novelty,
               "direction": direction, "parent": parent,
               "taxonomy": {"approach": approach}, "proposed": [],
               "summary": "s".repeat(600),
               "claims": [{"property": "correct", "argument": "x".repeat(2500)}],
               "axes": {"time_ms": {"median": 1.5, "min": 1.0, "max": 2.0, "n": 3}},
               "checks": [{"property": "correct", "check": "mechanical", "ok": true, "detail": ""}],
               "runs": [{"seed": "SEEDVALUE"}]})
    }

    #[test]
    fn feed_shape() {
        let root = crate::paths::scratch("feed-shape");
        std::fs::create_dir_all(&root).unwrap();
        toy(&root, "t", GOOD_JUDGE);
        let rows = [
            row(
                "baseline",
                "accepted",
                "baseline",
                "2026-01-01T00:00:00Z",
                "new",
                "baseline",
                None,
                "baseline",
            ),
            row(
                "pr",
                "valid",
                "a",
                "2026-01-02T00:00:00Z",
                "new",
                "alpha",
                None,
                "novel",
            ),
            row(
                "accept",
                "accepted",
                "a",
                "2026-01-03T00:00:00Z",
                "new",
                "alpha",
                None,
                "novel",
            ),
            row(
                "accept",
                "accepted",
                "b",
                "2026-01-04T00:00:00Z",
                "builds-on",
                "alpha",
                Some("a"),
                "other",
            ),
            row(
                "accept",
                "rejected",
                "c",
                "2026-01-05T00:00:00Z",
                "new",
                "gamma",
                None,
                "other",
            ),
            row(
                "pr",
                "valid",
                "d",
                "2026-01-06T00:00:00Z",
                "new",
                "delta",
                None,
                "other",
            ),
            row(
                "accept",
                "accepted",
                "a",
                "2026-01-07T00:00:00Z",
                "new",
                "alpha",
                None,
                "novel",
            ),
            row(
                "pr",
                "rejected",
                "e",
                "2026-01-08T00:00:00Z",
                "new",
                "eps",
                None,
                "other",
            ),
            row(
                "pr",
                "infra_failed",
                "f",
                "2026-01-09T00:00:00Z",
                "new",
                "zeta",
                None,
                "other",
            ),
        ];
        let body: String = rows.iter().map(|r| format!("{r}\n")).collect();
        std::fs::write(root.join("t/results.jsonl"), body).unwrap();

        let f = feed(&root, "t", "http://x/t.json", "now").unwrap();
        assert_eq!(f["feed_schema_version"], 2);
        assert_eq!(f["type"], "research");
        assert_eq!(f["slug"], "t");
        let subs = f["submissions"].as_array().unwrap();
        let ids: Vec<_> = subs.iter().map(|s| s["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["baseline", "b", "a"]); // ordered by latest row's at
        assert_eq!(subs[0]["baseline"], true);
        assert_eq!(subs[1]["direction"], subs[2]["direction"]);
        assert_eq!(subs[2]["summary"].as_str().unwrap().len(), 500);
        assert_eq!(
            subs[2]["claims"][0]["argument"].as_str().unwrap().len(),
            2000
        );
        assert_eq!(subs[2]["axes"]["time_ms"]["n"], 3);
        let text = f.to_string();
        assert!(!text.contains("SEEDVALUE") && !text.contains("secret reason"));
        let dirs = f["directions"].as_array().unwrap();
        let alpha = dirs.iter().find(|d| d["name"] == "alpha").unwrap();
        assert_eq!(alpha["root"], "a");
        assert_eq!(alpha["submissions"], json!(["b", "a"]));
        let vals = f["taxonomy"]["axes"][0]["values"].as_array().unwrap();
        let get = |id: &str| vals.iter().find(|v| v["id"] == id).unwrap();
        assert_eq!(get("baseline")["count"], 1);
        assert_eq!(get("baseline")["proposed"], false);
        assert_eq!(get("other")["count"], 1);
        assert_eq!(get("novel")["count"], 1);
        assert_eq!(get("novel")["proposed"], true);
        assert_eq!(
            f["attempts"],
            json!({"total": 4, "valid": 2, "accepted": 3, "rejected": 1, "infra_failed": 1})
        );
        assert!(f["contracts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == "streaming@0"));
    }
}
