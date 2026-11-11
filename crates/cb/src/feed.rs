//! `cb feed` (PLAN.md §5): per-challenge JSON the toolkit reads natively,
//! plus a public leaderboard page built from those feeds only.
//!
//! The feed carries the toolkit `Snapshot` fields (auto-research-toolkit
//! src/autoresearch/challenge.py) so its `builder` source is a field mapping,
//! not scraping. `feed_schema_version` is owned here; the toolkit pins the
//! versions it accepts. Candidate evaluations (`candidates/`) never enter a
//! feed: only challenge results do.

use crate::challenge::Spec;
use crate::pipeline::{current_record, read_results};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

pub const FEED_SCHEMA_VERSION: u32 = 1;
/// What a board value means: it survived two agreeing sandboxed runs on main.
pub const TRUTH_LABEL: &str = "promoted: two agreeing sandboxed runs on main, fresh seeds";

fn str_of(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

/// The feed for one challenge. `url` is where this feed is published.
pub fn feed(root: &Path, slug: &str, url: &str, generated_at: &str) -> Result<Value, String> {
    let spec = Spec::load(&root.join(slug))?;
    let results = read_results(root, slug)?;
    let maximise = spec.challenge.direction == "maximise";
    let baseline = results
        .iter()
        .find(|r| r["status"] == "baseline")
        .and_then(|r| r["value"].as_f64());
    // Board: every promoted attempt, best first. Each promotion beat the one
    // before, so this is also the record history.
    let mut promoted: Vec<&Value> = results
        .iter()
        .filter(|r| r["status"] == "promoted")
        .collect();
    promoted.sort_by(|a, b| {
        let (x, y) = (
            a["value"].as_f64().unwrap_or(f64::NAN),
            b["value"].as_f64().unwrap_or(f64::NAN),
        );
        let o = x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal);
        if maximise {
            o.reverse()
        } else {
            o
        }
    });
    let board: Vec<Value> = promoted
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let v = r["value"].as_f64();
            let delta = v.zip(baseline).map(|(v, b)| v - b);
            let pct = delta.zip(baseline).filter(|(_, b)| *b != 0.0).map(|(d, b)| (d / b * 10_000.0).round() / 100.0);
            json!({"rank": i + 1, "solver": str_of(&r["by"]), "model": str_of(&r["model"]), "value": v,
                   "delta": delta, "delta_pct": pct, "at": str_of(&r["at"]), "commit": str_of(&r["commit"])})
        })
        .collect();
    let record = current_record(&results).map(|r| {
        json!({"value": r["value"].as_f64(), "unit": spec.objective.expr, "by": str_of(&r["by"]), "at": str_of(&r["at"])})
    });
    let mut by_status: BTreeMap<String, u64> = BTreeMap::new();
    for r in results.iter().filter(|r| r["status"] != "baseline") {
        *by_status
            .entry(format!("{}:{}", str_of(&r["stage"]), str_of(&r["status"])))
            .or_default() += 1;
    }
    let host_policy = current_record(&results)
        .and_then(|r| r["runs"].get(0))
        .map(|run| run["host_policy"].clone())
        .filter(|h| !h.is_null());
    // Repo-relative globs, in the shape the toolkit's `check()` compares
    // against a domain's [policy]: the editable surface, and every judge
    // path beside it.
    let prefix = format!(
        "{}/{slug}",
        root.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("challenges")
    );
    let editable: Vec<String> = spec
        .paths
        .editable
        .iter()
        .map(|e| e.trim_end_matches('/').to_string())
        .collect();
    let allowed: Vec<String> = editable
        .iter()
        .map(|e| format!("{prefix}/{e}/**"))
        .collect();
    let mut forbidden = Vec::new();
    let mut judge = |dir: &str| {
        for e in std::fs::read_dir(root.join(slug).join(dir))
            .into_iter()
            .flatten()
            .flatten()
        {
            let name = e.file_name().to_string_lossy().into_owned();
            let rel = if dir.is_empty() {
                name.clone()
            } else {
                format!("{dir}/{name}")
            };
            if editable.contains(&rel) || name == "target" || name == "results.jsonl" {
                continue;
            }
            // A directory holding an editable path is judge except that path.
            if editable.iter().any(|ed| ed.starts_with(&format!("{rel}/"))) {
                continue;
            }
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            forbidden.push(if is_dir {
                format!("{prefix}/{rel}/**")
            } else {
                format!("{prefix}/{rel}")
            });
        }
    };
    judge("");
    for ed in &editable {
        if let Some((parent, _)) = ed.rsplit_once('/') {
            judge(parent);
        }
    }
    forbidden.sort();
    forbidden.push(".github/**".into());
    let bips = spec.objective.min_improvement_bips;
    let statistic = match (spec.challenge.kit.as_deref(), spec.challenge.family.as_str()) {
        (Some("rust"), _) => "callgrind instruction ratio vs frozen reference; paired wall-time guard (mirrored blocks, rel-MAD)",
        (_, "math") => "exact: certificate checked as data",
        _ => "the challenge's score command",
    };
    Ok(json!({
        "feed_schema_version": FEED_SCHEMA_VERSION,
        "source": "builder",
        "challenge": slug,
        "url": url,
        "benchmark": slug,
        "generated_at": generated_at,
        "fetched_at": generated_at,
        "truth_label": TRUTH_LABEL,
        "record": record,
        "board": board,
        "bar": {
            "direction": spec.challenge.direction,
            "metric": spec.objective.expr,
            "min_improvement_bips": bips,
            "statistic": statistic,
            "pairing": "two independent runs on main must agree within half the bar",
            "baseline": baseline,
        },
        "policy": {
            "allowed_paths": allowed,
            "forbidden_paths": forbidden,
            "selection_basis": "PR gate: only editable paths change; no symlinks or gitlinks; size cap",
            "measurement_policy": {
                "runner": spec.challenge.runner,
                "max_submission_bytes": spec.paths.max_bytes,
                "perf": spec.perf.as_ref().map(|p| json!({"blocks": p.blocks, "max_dispersion": p.max_dispersion})),
                "salted_inputs": spec.inputs.salted,
                "rust_allow": spec.rust.as_ref().map(|r| r.allow.clone()),
                "host_policy": host_policy,
            },
        },
        "attempts_count": by_status.values().sum::<u64>(),
        "attempts_by_status": by_status,
    }))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Public leaderboard page: built from feeds only (never from candidates/).
pub fn leaderboard(feeds: &[Value]) -> String {
    let mut out = String::from(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>Challenge leaderboards</title><style>\
         :root{--bg:#fff;--fg:#1a1a1a;--muted:#666;--line:#ddd}@media (prefers-color-scheme:dark){:root{--bg:#151515;--fg:#e8e8e8;--muted:#999;--line:#333}}\
         body{background:var(--bg);color:var(--fg);font:15px/1.5 system-ui,sans-serif;max-width:960px;margin:0 auto;padding:16px}\
         table{border-collapse:collapse;width:100%}.wrap{overflow-x:auto}th,td{border-bottom:1px solid var(--line);padding:4px 8px;text-align:left}\
         td.n{text-align:right;font-variant-numeric:tabular-nums}.muted{color:var(--muted)}section{margin-top:24px}</style></head><body>\
         <h1>Challenge leaderboards</h1>",
    );
    for f in feeds {
        let slug = esc(str_of(&f["challenge"]));
        let bar = &f["bar"];
        out += &format!(
            "<section id=\"{slug}\"><h2>{slug}</h2><p class=\"muted\">{} {} · bar {} bips · {} · <a href=\"feeds/{slug}.json\">feed</a></p>",
            esc(str_of(&bar["direction"])),
            esc(str_of(&bar["metric"])),
            bar["min_improvement_bips"],
            esc(str_of(&f["truth_label"]))
        );
        let num = |v: &Value| {
            v.as_f64()
                .map(|x| format!("{x}"))
                .unwrap_or_else(|| "–".into())
        };
        out += &format!(
            "<p>Record: <b>{}</b> · baseline {} · attempts {}</p>",
            num(&f["record"]["value"]),
            num(&bar["baseline"]),
            f["attempts_count"]
        );
        out += "<div class=\"wrap\"><table><tr><th>#</th><th>Solver</th><th>Value</th><th>Δ vs baseline</th><th>At</th></tr>";
        for row in f["board"].as_array().into_iter().flatten() {
            out += &format!(
                "<tr><td class=\"n\">{}</td><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td>{}</td></tr>",
                row["rank"],
                esc(str_of(&row["solver"])),
                num(&row["value"]),
                row["delta_pct"].as_f64().map(|p| format!("{p:+}%")).unwrap_or_else(|| "–".into()),
                esc(str_of(&row["at"]))
            );
        }
        out += "</table></div></section>";
    }
    out += "</body></html>\n";
    out
}

/// Writes `<out>/feeds/<slug>.json` for every challenge with results, and
/// `<out>/index.html`. `base_url` is where `<out>` is served.
pub fn build(
    root: &Path,
    out: &Path,
    base_url: &str,
    generated_at: &str,
) -> Result<Vec<String>, String> {
    let mut slugs: Vec<String> = std::fs::read_dir(root)
        .map_err(|e| format!("{}: {e}", root.display()))?
        .flatten()
        .filter(|e| e.path().join("results.jsonl").exists())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    slugs.sort();
    std::fs::create_dir_all(out.join("feeds")).map_err(|e| e.to_string())?;
    let mut feeds = Vec::new();
    for slug in &slugs {
        let url = format!("{}/feeds/{slug}.json", base_url.trim_end_matches('/'));
        let f = feed(root, slug, &url, generated_at)?;
        let path = out.join("feeds").join(format!("{slug}.json"));
        std::fs::write(&path, serde_json::to_string_pretty(&f).unwrap() + "\n")
            .map_err(|e| format!("{}: {e}", path.display()))?;
        feeds.push(f);
    }
    std::fs::write(out.join("index.html"), leaderboard(&feeds)).map_err(|e| e.to_string())?;
    Ok(slugs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{record, score, Attempt, Source};

    #[test]
    fn feed_from_a_recorded_challenge() {
        let dir = crate::pipeline::tests::repo();
        let root = dir.join("challenges");
        let run = |src| score(&root, "toy", src, false, None).unwrap();
        let at = |stage, commit, by| Attempt {
            stage,
            commit,
            by,
            model: "m",
            at: "2026-09-30T00:00:00Z",
        };
        record(
            &root,
            "toy",
            &[run(Source::Baseline), run(Source::Baseline)],
            &at("baseline", "c0", "baseline"),
        )
        .unwrap();
        std::fs::write(root.join("toy/candidate/answer"), "60\n").unwrap();
        record(
            &root,
            "toy",
            &[run(Source::AsIs), run(Source::AsIs)],
            &at("promotion", "c1", "alice"),
        )
        .unwrap();
        std::fs::write(root.join("toy/candidate/answer"), "500\n").unwrap();
        record(
            &root,
            "toy",
            &[run(Source::AsIs)],
            &at("pr", "c2", "mallory"),
        )
        .unwrap();

        let f = feed(
            &root,
            "toy",
            "https://x/feeds/toy.json",
            "2026-09-30T01:00:00Z",
        )
        .unwrap();
        assert_eq!(f["record"]["value"], 60.0);
        assert_eq!(f["record"]["by"], "alice");
        assert_eq!(f["board"][0]["delta_pct"], 20.0);
        assert_eq!(f["bar"]["baseline"], 50.0);
        assert_eq!(f["attempts_count"], 2);
        assert_eq!(f["attempts_by_status"]["pr:rejected"], 1);
        let forbidden: Vec<&str> = f["policy"]["forbidden_paths"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(
            forbidden.contains(&"challenges/toy/verifier/**")
                && forbidden.contains(&"challenges/toy/challenge.toml"),
            "{forbidden:?}"
        );
        assert!(
            !forbidden.iter().any(|p| p.contains("candidate")),
            "{forbidden:?}"
        );
        assert_eq!(
            f["policy"]["allowed_paths"][0],
            "challenges/toy/candidate/**"
        );
        // Every Snapshot field the toolkit knows is present.
        for k in [
            "source",
            "challenge",
            "url",
            "fetched_at",
            "benchmark",
            "truth_label",
            "generated_at",
            "record",
            "board",
            "bar",
            "policy",
            "attempts_count",
        ] {
            assert!(f.get(k).is_some(), "{k}");
        }
        let out = crate::paths::scratch("feed-out");
        assert_eq!(build(&root, &out, "https://x", "t").unwrap(), vec!["toy"]);
        let html = std::fs::read_to_string(out.join("index.html")).unwrap();
        assert!(html.contains("alice") && !html.contains("<script"));
    }
}
