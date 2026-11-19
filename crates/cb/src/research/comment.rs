//! The PR comment for a research submission (design spec §6). Every string
//! from the PR or the measure claim is untrusted and goes through `md`.

use crate::pipeline::md;
use serde_json::Value;

fn num(v: &Value) -> String {
    match v.as_f64() {
        Some(x) if x.is_finite() => {
            let s = format!("{:.4}", (x * 1e4).round() / 1e4);
            let s = s.trim_end_matches('0').trim_end_matches('.');
            if s.is_empty() || s == "-0" {
                "0".into()
            } else {
                s.to_string()
            }
        }
        _ => "-".into(),
    }
}

fn s<'a>(v: &'a Value, d: &'a str) -> &'a str {
    v.as_str().unwrap_or(d)
}

/// The predicted-against-measured table from a recorded row (#104).
fn predictions(entry: &Value) -> String {
    let Some(ps) = entry["predictions"].as_array().filter(|p| !p.is_empty()) else {
        return String::new();
    };
    let reference = md(s(&ps[0]["reference"], "?"));
    let mut out = format!(
        "| axis | expected | measured | this run | `{reference}` |\n|---|---|---|---|---|\n"
    );
    for p in ps {
        let verdict = match p["held"].as_bool() {
            Some(true) => " (held)",
            Some(false) => " (refuted)",
            None => "",
        };
        out += &format!(
            "| {} | {} | {}{verdict} | {} | {} |\n",
            md(s(&p["axis"], "?")),
            md(s(&p["expected"], "?")),
            md(s(&p["measured"], "?")),
            num(&p["own"]),
            num(&p["reference_value"]),
        );
    }
    out += &format!(
        "\n_Predictions compare this run with `{reference}`'s accepted medians, within each axis's tolerance. \
         A refuted prediction is a result, not a failure._\n\n"
    );
    out
}

pub fn comment(gate: &Value, claim: Option<&Value>, entry: Option<&Value>) -> String {
    let mut out = String::from("<!-- cb:score -->\n### Challenge builder\n\n");
    let decl = &gate["declaration"];
    out += &format!(
        "Challenge: `{}` · submission: `{}` · contract: `{}`\n\n",
        md(s(&gate["slug"], "?")),
        md(s(&gate["submission"], "?")),
        md(s(&decl["contract"], "?")),
    );
    let direction = if s(&decl["novelty"], "") == "builds-on" {
        format!("builds on {}", md(s(&decl["parent"], "?")))
    } else {
        format!("direction {}", md(s(&decl["direction"], "?")))
    };
    out += &format!(
        "- novelty: {}, {}\n- summary: {}\n",
        md(s(&decl["novelty"], "?")),
        direction,
        md(s(&decl["summary"], "")),
    );
    if let Some(t) = decl["taxonomy"].as_object() {
        let pairs: Vec<String> = t
            .iter()
            .map(|(k, v)| format!("{}={}", md(k), md(s(v, "?"))))
            .collect();
        out += &format!("- taxonomy: {}\n", pairs.join(", "));
    }
    if let Some(h) = decl["hypothesis"].as_str().filter(|h| !h.trim().is_empty()) {
        out += &format!("- hypothesis: {}\n", md(h));
    }
    out += "\n";
    match claim {
        None => {
            out += "No measurement was produced (the measure job failed before writing one).\n\n"
        }
        Some(c) => {
            out += &format!("**Status: {}**", md(s(&c["status"], "?")));
            if let Some(r) = c["reason"].as_str().filter(|r| !r.is_empty()) {
                out += &format!(" ({})", md(r));
            }
            out += "\n\n| property | kind | result | detail |\n|---|---|---|---|\n";
            for ck in c["checks"].as_array().into_iter().flatten() {
                out += &format!(
                    "| {} | {} | {} | {} |\n",
                    md(s(&ck["property"], "?")),
                    md(s(&ck["check"], "?")),
                    if ck["ok"] == true { "pass" } else { "fail" },
                    md(s(&ck["detail"], "")),
                );
            }
            for cl in decl["claims"].as_array().into_iter().flatten() {
                out += &format!(
                    "| {} | declared — unverified | - | {} |\n",
                    md(s(&cl["property"], "?")),
                    md(s(&cl["argument"], "")),
                );
            }
            if let Some(axes) = c["axes"].as_object().filter(|a| !a.is_empty()) {
                out += "\n| axis | median | min–max | n |\n|---|---|---|---|\n";
                for (id, a) in axes {
                    out += &format!(
                        "| {} | {} | {}–{} | {} |\n",
                        md(id),
                        num(&a["median"]),
                        num(&a["min"]),
                        num(&a["max"]),
                        num(&a["n"]),
                    );
                }
            }
            out += "\n";
        }
    }
    if let Some(e) = entry {
        out += &predictions(e);
        out += &format!(
            "Recorded as `{}`: {}\n\n",
            md(s(&e["status"], "?")),
            md(s(&e["reason"], ""))
        );
    }
    out += "_This measurement is a claim from the PR run. If it is valid the PR is landed on main automatically; two clean runs on `main` then record it on the map. There is no record to beat: every submission that passes is recorded._\n";
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn gate() -> Value {
        json!({"ok": true, "type": "research", "slug": "toy", "submission": "a",
            "declaration": {"contract": "main@1", "novelty": "new", "direction": "d1",
                "summary": "<script>alert(1)</script> @user", "taxonomy": {"k": "v"},
                "claims": [{"property": "stable", "argument": "because"}]}})
    }
    fn claim() -> Value {
        json!({"status": "valid", "reason": "",
            "checks": [{"property": "correct", "check": "mechanical", "ok": true, "detail": ""}],
            "axes": {"fees": {"median": 10.12345, "min": 9.0, "max": 11.0, "n": 3}}})
    }

    #[test]
    fn research_comment_renders_tables() {
        let c = comment(&gate(), Some(&claim()), None);
        assert!(c.starts_with("<!-- cb:score -->"));
        assert!(c.contains("fees") && c.contains("10.1235") && c.contains("9–11"));
        assert!(c.contains("unverified"));
        assert!(c.contains("There is no record to beat: every submission that passes is recorded."));
    }

    #[test]
    fn research_comment_shows_hypothesis_and_predictions() {
        let mut g = gate();
        g["declaration"]["hypothesis"] = json!("fewer <b>inputs</b> @carol");
        let entry = json!({"status": "valid", "reason": "", "predictions": [
            {"axis": "fees", "expected": "lower", "measured": "lower", "held": true,
             "reference": "baseline", "own": 8.0, "reference_value": 10.0},
            {"axis": "dust", "expected": "lower", "measured": "higher", "held": false,
             "reference": "baseline", "own": 3.0, "reference_value": 1.0},
            {"axis": "time", "expected": "same", "measured": "unknown", "held": null,
             "reference": "baseline", "own": null, "reference_value": null}]});
        let c = comment(&g, Some(&claim()), Some(&entry));
        assert!(c.contains("- hypothesis: "));
        assert!(!c.contains("<b>") && !c.contains("@carol"));
        assert!(c.contains("| fees | lower | lower (held) | 8 | 10 |"));
        assert!(c.contains("| dust | lower | higher (refuted) | 3 | 1 |"));
        assert!(c.contains("| time | same | unknown | - | - |"));
        assert!(c.contains("A refuted prediction is a result, not a failure."));
        // No predictions, no table.
        let plain = comment(
            &gate(),
            Some(&claim()),
            Some(&json!({"status": "valid", "reason": ""})),
        );
        assert!(!plain.contains("expected") && !plain.contains("hypothesis"));
    }

    #[test]
    fn research_comment_neutralises_untrusted() {
        let mut cl = claim();
        cl["reason"] = json!("<img src=x> @bob");
        cl["axes"] = json!({"<b>x@y": {"median": 1, "min": 1, "max": 1, "n": 1}});
        let c = comment(
            &gate(),
            Some(&cl),
            Some(&json!({"status": "valid", "reason": "<i>@z"})),
        );
        let body: String = c.lines().filter(|l| !l.starts_with("<!--")).collect();
        let body = body.replace("_This", "");
        assert!(!body.contains('<') && !body.contains('@'), "{c}");
    }

    #[test]
    fn refused_gate_comment_unchanged() {
        let g = json!({"ok": false, "type": "research", "errors": ["bad <x>"]});
        let c = crate::pipeline::comment(&g, None, None);
        assert!(c.contains("**Refused by the gate.** Nothing was built or run."));
        assert!(!c.contains('<') || c.starts_with("<!--"));
        assert!(c.contains("bad  x "));
    }
}
