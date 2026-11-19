//! `cb record` for research challenges (design spec §6). Trusted: the status
//! is decided here from untrusted `measure.json` claims.

use super::results::{accepted, direction_of};
use super::spec::{Declaration, ResearchSpec};
use super::util::{agrees, median, verdicts};
use crate::pipeline::{read_results, Attempt};
use serde_json::{json, Map, Value};
use std::path::Path;

fn status_of(run: &Value) -> &str {
    match run["status"].as_str() {
        Some(s @ ("valid" | "rejected" | "infra_failed")) => s,
        _ => "infra_failed",
    }
}

fn reason_of(run: &Value) -> String {
    run["reason"].as_str().unwrap_or("").to_string()
}

/// Median, min, max, n over the samples of `axis` in every run.
fn combine(runs: &[Value], axis: &str) -> Option<Value> {
    let mut xs: Vec<f64> = runs
        .iter()
        .flat_map(|r| r["axes"][axis]["samples"].as_array().into_iter().flatten())
        .filter_map(Value::as_f64)
        .filter(|x| x.is_finite())
        .collect();
    if xs.is_empty() {
        return None;
    }
    xs.sort_by(|a, b| a.total_cmp(b));
    let n = xs.len();
    Some(json!({"median": median(&xs), "min": xs[0], "max": xs[n - 1], "n": n}))
}

/// Why two accept/baseline runs are not equivalent, if they are not.
fn disagreement(spec: &ResearchSpec, contract: &str, runs: &[Value]) -> Option<String> {
    if verdicts(&runs[0]) != verdicts(&runs[1]) {
        return Some("the two runs disagree on check verdicts".into());
    }
    for ax in spec.axes_for(contract) {
        let m = |r: &Value| r["axes"][&ax.id]["median"].as_f64();
        let (Some(a), Some(b)) = (m(&runs[0]), m(&runs[1])) else {
            return Some(format!("axis {}: missing from a run", ax.id));
        };
        if !agrees(a, b, ax.tolerance, ax.tolerance_abs) {
            return Some(format!(
                "axis {}: runs measured {a} and {b}, beyond tolerance {} (abs {})",
                ax.id, ax.tolerance, ax.tolerance_abs
            ));
        }
    }
    None
}

pub fn record(
    root: &Path,
    slug: &str,
    decl: &Declaration,
    runs: &[Value],
    a: &Attempt,
) -> Result<Value, String> {
    let want = match a.stage {
        "pr" => 1,
        "accept" | "baseline" => 2,
        s => return Err(format!("stage {s}: expected pr, accept or baseline")),
    };
    if runs.len() != want {
        return Err(format!(
            "stage {} needs {want} run(s), got {}",
            a.stage,
            runs.len()
        ));
    }
    for r in runs {
        if r["slug"] != slug {
            return Err("a run is for a different challenge".into());
        }
        if r["submission"] != decl.id.as_str() {
            return Err("a run is for a different submission than the declaration".into());
        }
        if r["contract"] != decl.contract.as_str() {
            return Err("a run is for a different contract than the declaration".into());
        }
    }
    if a.stage == "baseline" && decl.id != "baseline" {
        return Err("stage baseline needs submission id baseline".into());
    }
    if a.stage == "accept" && decl.id == "baseline" {
        return Err("stage accept refuses the baseline; use stage baseline".into());
    }
    let spec = ResearchSpec::load(&root.join(slug))?;
    let rows = read_results(root, slug)?;
    if a.stage == "baseline" && accepted(&rows).contains_key("baseline") {
        return Err("baseline already recorded".into());
    }

    let statuses: Vec<&str> = runs.iter().map(status_of).collect();
    let reasons = |which: &str| {
        runs.iter()
            .filter(|r| status_of(r) == which)
            .map(reason_of)
            .collect::<Vec<_>>()
            .join("; ")
    };
    let (status, reason) = if runs.len() == 1 {
        (statuses[0].to_string(), reason_of(&runs[0]))
    } else if statuses.contains(&"infra_failed") {
        ("infra_failed".into(), reasons("infra_failed"))
    } else if statuses.contains(&"rejected") {
        ("rejected".into(), reasons("rejected"))
    } else if let Some(why) = disagreement(&spec, &decl.contract, runs) {
        ("infra_failed".into(), why)
    } else {
        ("accepted".into(), String::new())
    };

    let axes = if matches!(status.as_str(), "valid" | "accepted") {
        let mut m = Map::new();
        for ax in spec.axes_for(&decl.contract) {
            if let Some(v) = combine(runs, &ax.id) {
                m.insert(ax.id.clone(), v);
            }
        }
        Value::Object(m)
    } else {
        Value::Null
    };
    let direction = if decl.novelty == "builds-on" {
        direction_of(&rows, &decl.parent).unwrap_or_default()
    } else {
        decl.direction.clone()
    };
    let model = if a.model.is_empty() {
        decl.model.as_str()
    } else {
        a.model
    };
    let entry = json!({
        "schema": 2, "type": "research", "at": a.at, "stage": a.stage,
        "status": status, "reason": reason, "slug": slug,
        "submission": decl.id, "commit": a.commit, "by": a.by, "model": model,
        "contract": decl.contract, "novelty": decl.novelty, "direction": direction,
        "parent": decl.parent, "taxonomy": decl.taxonomy,
        "taxonomy_version": spec.tax.version,
        "proposed": decl.proposed(&spec.tax), "summary": decl.summary,
        "claims": decl.to_json()["claims"], "axes": axes, "checks": runs[0]["checks"],
        "runs": runs.iter().map(|r| json!({
            "seed": r["seed"], "host": r["host"], "host_policy": r["host_policy"],
            "sandboxed": r["sandboxed"], "status": status_of(r), "reason": r["reason"],
            "inputs": r["inputs"], "axes": r["axes"],
        })).collect::<Vec<_>>(),
    });
    let path = root.join(slug).join("results.jsonl");
    let mut line = serde_json::to_string(&entry).map_err(|e| e.to_string())?;
    line.push('\n');
    use std::io::Write;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| f.write_all(line.as_bytes()))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths;
    use crate::research::testkit::{toy, GOOD_JUDGE};
    use std::path::PathBuf;

    fn setup(tag: &str) -> PathBuf {
        let root = paths::scratch(&format!("rec-{tag}"));
        std::fs::create_dir_all(&root).unwrap();
        let dir = toy(&root, "toy", GOOD_JUDGE);
        let p = dir.join("challenge.toml");
        let t = std::fs::read_to_string(&p).unwrap();
        // time_ms -> fees, tolerance 0.05
        std::fs::write(
            &p,
            t.replace("\"time_ms\"", "\"fees\"")
                .replace("tolerance = 0.2 ", "tolerance = 0.05"),
        )
        .unwrap();
        root
    }

    fn decl(id: &str, extra: &str) -> Declaration {
        Declaration::parse(&format!(
            "id = \"{id}\"\ncontract = \"main@1\"\nnovelty = \"new\"\ndirection = \"d1\"\nsummary = \"s\"\n{extra}"
        ))
        .unwrap()
    }

    fn run(id: &str, fees: f64, size: f64) -> Value {
        json!({
            "schema": 2, "type": "research", "slug": "toy", "submission": id,
            "contract": "main@1", "seed": "1", "host": "h", "host_policy": "p",
            "sandboxed": false, "inputs": "i", "status": "valid", "reason": "",
            "checks": [{"property": "correct", "check": "mechanical", "ok": true, "detail": ""}],
            "axes": {
                "fees": {"median": fees, "min": fees, "max": fees, "n": 3, "samples": [fees, fees, fees]},
                "size_bytes": {"median": size, "min": size, "max": size, "n": 1, "samples": [size]},
            },
        })
    }

    fn at(stage: &'static str) -> Attempt<'static> {
        Attempt {
            stage,
            commit: "c",
            by: "u",
            model: "",
            at: "t",
        }
    }

    #[test]
    fn pr_row_valid() {
        let root = setup("pr");
        let d = decl("a", "model = \"m1\"\n");
        let e = record(&root, "toy", &d, &[run("a", 10.0, 5.0)], &at("pr")).unwrap();
        assert_eq!(e["status"], "valid");
        assert_eq!(e["schema"], 2);
        assert_eq!(e["direction"], "d1");
        assert_eq!(e["model"], "m1");
        assert_eq!(e["taxonomy_version"], 1);
        assert_eq!(e["axes"]["fees"]["n"], 3);
        assert_eq!(e["runs"].as_array().unwrap().len(), 1);
        let text = std::fs::read_to_string(root.join("toy/results.jsonl")).unwrap();
        assert_eq!(text.lines().count(), 1);
    }

    #[test]
    fn builds_on_inherits_direction() {
        let root = setup("bo");
        let b = decl("baseline", "");
        let b = Declaration {
            direction: "baseline".into(),
            ..b
        };
        record(
            &root,
            "toy",
            &b,
            &[run("baseline", 1.0, 1.0), run("baseline", 1.0, 1.0)],
            &at("baseline"),
        )
        .unwrap();
        let d = Declaration {
            novelty: "builds-on".into(),
            parent: "baseline".into(),
            direction: String::new(),
            ..decl("a", "")
        };
        let e = record(&root, "toy", &d, &[run("a", 1.0, 1.0)], &at("pr")).unwrap();
        assert_eq!(e["direction"], "baseline");
    }

    #[test]
    fn accept_two_valid_agreeing_runs_accepted() {
        let root = setup("ok");
        let e = record(
            &root,
            "toy",
            &decl("a", ""),
            &[run("a", 10.0, 5.0), run("a", 10.2, 5.0)],
            &at("accept"),
        )
        .unwrap();
        assert_eq!(e["status"], "accepted", "{e}");
        assert_eq!(e["axes"]["fees"]["n"], 6);
        assert_eq!(e["axes"]["size_bytes"]["n"], 2);
    }

    #[test]
    fn accept_axes_disagree_infra() {
        let root = setup("dis");
        let e = record(
            &root,
            "toy",
            &decl("a", ""),
            &[run("a", 10.0, 5.0), run("a", 12.0, 5.0)],
            &at("accept"),
        )
        .unwrap();
        assert_eq!(e["status"], "infra_failed");
        assert!(e["reason"].as_str().unwrap().contains("fees"));
        assert!(e["axes"].is_null());
    }

    #[test]
    fn accept_exact_axis_tolerance_zero() {
        let root = setup("exact");
        let e = record(
            &root,
            "toy",
            &decl("a", ""),
            &[run("a", 10.0, 5.0), run("a", 10.0, 6.0)],
            &at("accept"),
        )
        .unwrap();
        assert_eq!(e["status"], "infra_failed");
        assert!(e["reason"].as_str().unwrap().contains("size_bytes"));
    }

    #[test]
    fn accept_absolute_floor_accepts_near_zero() {
        let root = setup("absfloor");
        let p = root.join("toy/challenge.toml");
        let t = std::fs::read_to_string(&p).unwrap();
        std::fs::write(
            &p,
            t.replace("tolerance = 0.05", "tolerance = 0.3")
                .replace("tolerance_abs = 0.0 ", "tolerance_abs = 1.0"),
        )
        .unwrap();
        let ok = record(
            &root,
            "toy",
            &decl("a", ""),
            &[run("a", 0.375, 5.0), run("a", 0.125, 5.0)],
            &at("accept"),
        )
        .unwrap();
        assert_eq!(ok["status"], "accepted", "{ok}");
        let bad = record(
            &root,
            "toy",
            &decl("b", ""),
            &[run("b", 3.0, 5.0), run("b", 0.5, 5.0)],
            &at("accept"),
        )
        .unwrap();
        assert_eq!(bad["status"], "infra_failed");
        assert!(bad["reason"].as_str().unwrap().contains("abs 1"));
    }

    #[test]
    fn accept_rejected_wins_over_valid() {
        let root = setup("rej");
        let mut r = run("a", 10.0, 5.0);
        r["status"] = json!("rejected");
        r["reason"] = json!("check correct: bad");
        let e = record(
            &root,
            "toy",
            &decl("a", ""),
            &[run("a", 10.0, 5.0), r],
            &at("accept"),
        )
        .unwrap();
        assert_eq!(e["status"], "rejected");
        assert_eq!(e["reason"], "check correct: bad");
    }

    #[test]
    fn accept_verdicts_differ_infra() {
        let root = setup("verd");
        let mut r = run("a", 10.0, 5.0);
        r["checks"][0]["ok"] = json!(false);
        let e = record(
            &root,
            "toy",
            &decl("a", ""),
            &[run("a", 10.0, 5.0), r],
            &at("accept"),
        )
        .unwrap();
        assert_eq!(e["status"], "infra_failed");
    }

    #[test]
    fn baseline_twice_refused() {
        let root = setup("b2");
        let d = decl("baseline", "");
        let rs = [run("baseline", 1.0, 1.0), run("baseline", 1.0, 1.0)];
        assert_eq!(
            record(&root, "toy", &d, &rs, &at("baseline")).unwrap()["status"],
            "accepted"
        );
        let e = record(&root, "toy", &d, &rs, &at("baseline")).unwrap_err();
        assert!(e.contains("baseline already recorded"));
    }

    #[test]
    fn accept_refuses_baseline_id() {
        let root = setup("ab");
        let rs = [run("baseline", 1.0, 1.0), run("baseline", 1.0, 1.0)];
        assert!(record(&root, "toy", &decl("baseline", ""), &rs, &at("accept")).is_err());
        assert!(record(
            &root,
            "toy",
            &decl("a", ""),
            &[run("a", 1.0, 1.0), run("a", 1.0, 1.0)],
            &at("baseline")
        )
        .is_err());
    }

    #[test]
    fn runs_for_other_submission_refused() {
        let root = setup("other");
        assert!(record(
            &root,
            "toy",
            &decl("a", ""),
            &[run("b", 1.0, 1.0)],
            &at("pr")
        )
        .is_err());
        assert!(record(
            &root,
            "toy",
            &decl("a", ""),
            &[run("a", 1.0, 1.0), run("a", 1.0, 1.0)],
            &at("pr")
        )
        .is_err());
        assert!(!root.join("toy/results.jsonl").exists());
    }
}
