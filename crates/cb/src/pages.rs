//! The public status site written by `cb feed` beside the feeds: one static
//! page (`index.html` + `assets/`) and `challenges.json`, the data it shows.
//!
//! Public although the repo is private, so it carries only what the feeds
//! already expose (results.jsonl), `challenge.toml` metadata, a short README
//! excerpt, the steps from SUBMITTING.md and a candidate's title. Nothing else
//! from `candidates/` is read. The page renders every string with
//! `textContent`; the data is embedded as JSON with `<`, `>` and `&` escaped,
//! so no file content can close the script element.
//!
//! An optional `state.json` (pipeline runs and open PRs, written by the deploy
//! workflow) is fetched by the page at load time; without it the page is
//! complete.

use crate::challenge::Spec;
use crate::pipeline::{current_record, read_results};
use serde_json::{json, Value};
use std::path::Path;

pub const SITE_SCHEMA_VERSION: u32 = 1;
const STYLE: &str = include_str!("../assets/site/style.css");
const SCRIPT: &str = include_str!("../assets/site/app.js");
/// Attempt reasons can quote verifier output: keep them short.
const MAX_REASON: usize = 240;
const MAX_SUMMARY: usize = 900;

fn str_of(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

/// Replaces anything shaped like an email address with `[email]`.
fn redact_emails(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            let looks_like_email = w.find('@').is_some_and(|i| {
                i > 0 && {
                    let domain = &w[i + 1..];
                    let dot = domain.find('.');
                    dot.is_some_and(|d| {
                        d > 0 && d + 1 < domain.trim_end_matches(['.', ',', ')', ';', ':']).len()
                    })
                }
            });
            if looks_like_email {
                "[email]"
            } else {
                w
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Markdown inline syntax to plain text: links keep their text, emphasis and
/// code markers go.
fn plain(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    let mut rest = md;
    while let Some(i) = rest.find('[') {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 1..];
        match tail
            .find("](")
            .and_then(|j| tail[j..].find(')').map(|k| (j, j + k)))
        {
            Some((j, end)) if !tail[..j].contains('[') => {
                out.push_str(&tail[..j]);
                rest = &tail[end + 1..];
            }
            _ => {
                out.push('[');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    let out = out.replace("**", "").replace('`', "");
    redact_emails(
        out.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .as_str(),
    )
}

/// Paragraphs of a markdown file (blank-line separated), raw.
fn paragraphs(md: &str) -> Vec<String> {
    let mut paras = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    for line in md.lines() {
        if line.trim().is_empty() {
            if !cur.is_empty() {
                paras.push(cur.join("\n"));
                cur.clear();
            }
        } else {
            cur.push(line);
        }
    }
    if !cur.is_empty() {
        paras.push(cur.join("\n"));
    }
    paras
}

/// (title, summary) from a README whose heading names this challenge
/// (`# <slug>: <title>`); a README copied from a kit says nothing about it.
fn readme(dir: &Path, slug: &str) -> (Option<String>, Option<String>) {
    let Ok(text) = std::fs::read_to_string(dir.join("README.md")) else {
        return (None, None);
    };
    let paras = paragraphs(&text);
    let Some(h1) = paras.first().and_then(|p| p.strip_prefix("# ")) else {
        return (None, None);
    };
    let Some(rest) = h1.strip_prefix(slug) else {
        return (None, None);
    };
    let title = rest.trim_start_matches([':', ' ', '-']).trim().to_string();
    let summary = paras
        .get(1)
        .filter(|p| !p.starts_with(['#', '|', '-', '*', '`', '<', '>']))
        .map(|p| clip(&plain(p), MAX_SUMMARY));
    ((!title.is_empty()).then(|| plain(&title)), summary)
}

/// The numbered steps of SUBMITTING.md, as plain text.
fn submit_steps(dir: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(dir.join("SUBMITTING.md")) else {
        return Vec::new();
    };
    let mut steps: Vec<String> = Vec::new();
    let mut in_list = false;
    for line in text.lines() {
        let t = line.trim();
        let numbered = t
            .split_once(". ")
            .filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
        if let Some((_, body)) = numbered {
            steps.push(body.to_string());
            in_list = true;
        } else if in_list && !t.is_empty() && line.starts_with(' ') {
            let last = steps.last_mut().unwrap();
            last.push(' ');
            last.push_str(t);
        } else if t.is_empty() {
            continue;
        } else {
            in_list = false;
        }
    }
    steps.iter().map(|s| clip(&plain(s), 400)).take(8).collect()
}

/// `candidates/<slug>.toml`'s `[candidate] title`, and nothing else from it.
fn candidate_title(candidates: &Path, slug: &str) -> Option<String> {
    let text = std::fs::read_to_string(candidates.join(format!("{slug}.toml"))).ok()?;
    let v: toml::Value = toml::from_str(&text).ok()?;
    v.get("candidate")?
        .get("title")?
        .as_str()
        .map(|t| clip(&redact_emails(t), 200))
}

/// One challenge's public summary. `has_feed`: a feed was written for it.
pub fn challenge(
    root: &Path,
    candidates: &Path,
    slug: &str,
    has_feed: bool,
) -> Result<Value, String> {
    let dir = root.join(slug);
    let spec = Spec::load(&dir)?;
    let results = read_results(root, slug)?;
    let maximise = spec.challenge.direction == "maximise";
    let m = spec.objective.min_improvement_bips as f64 / 10_000.0;
    let baseline = results.iter().find(|r| r["status"] == "baseline");
    let record = current_record(&results).filter(|r| r["status"] == "promoted");
    let current = current_record(&results).and_then(|r| r["value"].as_f64());
    // What a promotion must reach (pipeline::record, unpaired form).
    let to_beat = current.map(|v| {
        if maximise {
            v * (1.0 + m)
        } else {
            v * (1.0 - m)
        }
    });
    let state = match (spec.challenge.status.as_str(), baseline.is_some()) {
        ("open", false) => "pending",
        ("open", true) => "open",
        ("closed", _) => "closed",
        _ => "draft",
    };
    let (readme_title, summary) = readme(&dir, slug);
    let title = candidate_title(candidates, slug).or(readme_title);
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
    let base_value = baseline.and_then(|b| b["value"].as_f64());
    let board: Vec<Value> = promoted
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let v = r["value"].as_f64();
            let pct = v
                .zip(base_value)
                .filter(|(_, b)| *b != 0.0)
                .map(|(v, b)| ((v - b) / b * 10_000.0).round() / 100.0);
            json!({"rank": i + 1, "solver": str_of(&r["by"]), "model": str_of(&r["model"]), "value": v,
                   "delta_pct": pct, "at": str_of(&r["at"]), "commit": str_of(&r["commit"])})
        })
        .collect();
    // Newest first.
    let attempts: Vec<Value> = results
        .iter()
        .rev()
        .map(|r| {
            json!({"at": str_of(&r["at"]), "stage": str_of(&r["stage"]), "status": str_of(&r["status"]),
                   "value": r["value"].as_f64(), "record_before": r["record_before"].as_f64(),
                   "by": str_of(&r["by"]), "model": str_of(&r["model"]), "commit": str_of(&r["commit"]),
                   "reason": clip(&redact_emails(str_of(&r["reason"])), MAX_REASON)})
        })
        .collect();
    Ok(json!({
        "slug": slug,
        "title": title,
        "summary": summary,
        "status": spec.challenge.status,
        "state": state,
        "family": spec.challenge.family,
        "direction": spec.challenge.direction,
        "metric": spec.objective.expr,
        "min_improvement_bips": spec.objective.min_improvement_bips,
        "editable": spec.paths.editable,
        "max_bytes": spec.paths.max_bytes,
        "baseline": base_value.map(|v| json!({"value": v, "at": str_of(&baseline.unwrap()["at"])})),
        "record": record.map(|r| json!({"value": r["value"].as_f64(), "by": str_of(&r["by"]), "at": str_of(&r["at"])})),
        "to_beat": to_beat,
        "board": board,
        "attempts": attempts,
        "submit_steps": submit_steps(&dir),
        "feed": has_feed.then(|| format!("feeds/{slug}.json")),
    }))
}

/// Every challenge directory (one with a `challenge.toml`), sorted.
pub fn slugs(root: &Path) -> Result<Vec<String>, String> {
    let mut slugs: Vec<String> = std::fs::read_dir(root)
        .map_err(|e| format!("{}: {e}", root.display()))?
        .flatten()
        .filter(|e| e.path().join("challenge.toml").exists())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    slugs.sort();
    Ok(slugs)
}

/// JSON safe inside `<script type="application/json">`.
fn embed(v: &Value) -> String {
    serde_json::to_string(v)
        .unwrap()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
}

pub fn index_html(data: &Value) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <meta name=\"color-scheme\" content=\"light dark\"><link rel=\"icon\" href=\"data:,\">\
         <title>Challenge status</title>\
         <link rel=\"stylesheet\" href=\"assets/style.css\"></head><body>\
         <div id=\"app\"><noscript><p class=\"noscript\">This page needs JavaScript. \
         The data is in <a href=\"challenges.json\">challenges.json</a>.</p></noscript></div>\
         <script id=\"site-data\" type=\"application/json\">{}</script>\
         <script src=\"assets/app.js\"></script></body></html>\n",
        embed(data)
    )
}

/// Writes `index.html`, `assets/` and `challenges.json` into `out`.
pub fn write(root: &Path, out: &Path, feeds: &[String], generated_at: &str) -> Result<(), String> {
    let candidates = root
        .parent()
        .map(|p| p.join("candidates"))
        .unwrap_or_else(|| "candidates".into());
    let challenges = slugs(root)?
        .iter()
        .map(|s| challenge(root, &candidates, s, feeds.contains(s)))
        .collect::<Result<Vec<_>, _>>()?;
    let data = json!({
        "site_schema_version": SITE_SCHEMA_VERSION,
        "generated_at": generated_at,
        "challenges": challenges,
    });
    let w = |rel: &str, body: &str| {
        let p = out.join(rel);
        std::fs::write(&p, body).map_err(|e| format!("{}: {e}", p.display()))
    };
    std::fs::create_dir_all(out.join("assets")).map_err(|e| e.to_string())?;
    w("assets/style.css", STYLE)?;
    w("assets/app.js", SCRIPT)?;
    w(
        "challenges.json",
        &(serde_json::to_string_pretty(&data).unwrap() + "\n"),
    )?;
    // The page always fetches state.json; a placeholder (no generated_at)
    // keeps that fetch from 404ing until the deploy workflow writes the real one.
    if !out.join("state.json").exists() {
        w(
            "state.json",
            "{\"generated_at\": null, \"runs\": [], \"pulls\": []}\n",
        )?;
    }
    w("index.html", &index_html(&data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_to_plain_text() {
        assert_eq!(
            plain("Make [cardano-ledger](https://x.org/a) **fast**, `src/`\n  only"),
            "Make cardano-ledger fast, src/ only"
        );
        assert_eq!(plain("a [b] c"), "a [b] c");
        assert_eq!(
            plain("mail bob@example.com, or @alice"),
            "mail [email] or @alice"
        );
        assert_eq!(clip("abcdef", 3), "abc…");
    }

    #[test]
    fn embedded_json_cannot_close_the_script() {
        let html = index_html(&json!({"t": "</script><img src=x onerror=alert(1)>&"}));
        assert!(
            !html.contains("</script><img") && !html.contains("<img"),
            "{html}"
        );
        let start = html.find("application/json\">").unwrap() + 18;
        let end = start + html[start..].find("</script>").unwrap();
        let back: Value = serde_json::from_str(&html[start..end]).unwrap();
        assert_eq!(back["t"], "</script><img src=x onerror=alert(1)>&");
    }

    #[test]
    fn readme_and_submitting_excerpts() {
        let dir = crate::paths::scratch("pages-md");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("README.md"),
            "# toy: a [toy](https://x) problem\n\nMake it *small*. Ask a@b.io\n\n| t |\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("SUBMITTING.md"),
            "# Submitting\n\n1. Open a PR\n   under `candidate/`.\n2. CI scores it.\n\nAfter.\n",
        )
        .unwrap();
        let (t, s) = readme(&dir, "toy");
        assert_eq!(t.as_deref(), Some("a toy problem"));
        assert_eq!(s.as_deref(), Some("Make it *small*. Ask [email]"));
        assert_eq!(readme(&dir, "other"), (None, None));
        assert_eq!(
            submit_steps(&dir),
            vec!["Open a PR under candidate/.", "CI scores it."]
        );
    }
}
