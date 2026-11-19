//! The public status site written by `cb feed` beside the feeds: one static
//! page (`index.html` + `assets/`) and `challenges.json`, the data it shows.
//!
//! Public although the repo is private, so it carries only what the feeds
//! already expose (results.jsonl), `challenge.toml` metadata, a short README
//! excerpt, the steps from SUBMITTING.md, the participant prompt (PROMPT.md,
//! written to be published), the challenge's context.toml (also written to be
//! published, #101) and a candidate's title. Nothing else
//! from `candidates/` is read. The page renders every string with
//! `textContent`; the data is embedded as JSON with `<`, `>` and `&` escaped,
//! so no file content can close the script element.
//!
//! Each scored submission on the record history (a promotion, or a PR that
//! claimed a record) also gets its diff, `diffs/<slug>/<commit>.patch`: the
//! change to the challenge's editable paths, taken from the git history the
//! site is built in. A commit that isn't in that history gets no diff.
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
/// Instrument Sans (variable weight and width), SIL Open Font License 1.1.
/// Served from the site itself: the page loads nothing from other origins.
const FONT: &[u8] = include_bytes!("../assets/site/instrument-sans.woff2");
/// Attempt reasons can quote verifier output: keep them short.
const MAX_REASON: usize = 240;
const MAX_SUMMARY: usize = 900;
const MAX_PROMPT: usize = 8000;
/// A patch past this is cut, with a note saying so.
const MAX_PATCH: usize = 512 * 1024;

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

/// `## 3. Title` -> `Title`.
fn numbered_heading(para: &str) -> Option<&str> {
    let (n, title) = para.strip_prefix("## ")?.split_once(". ")?;
    (!n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) && !title.contains('\n'))
        .then_some(title)
}

/// A walkthrough SUBMITTING.md (`## 1. Title` sections): each section's title
/// and its first paragraph of prose. Its numbered lists are sub-steps.
fn submit_sections(text: &str) -> Vec<String> {
    let paras = paragraphs(text);
    let mut steps = Vec::new();
    for (i, p) in paras.iter().enumerate() {
        let Some(title) = numbered_heading(p) else {
            continue;
        };
        let lead = paras
            .get(i + 1)
            .filter(|n| !n.starts_with(['#', '|', '-', '*', '`', '<', '>']))
            .filter(|n| !n.starts_with(|c: char| c.is_ascii_digit()))
            .map(|n| match plain(n).strip_suffix(':') {
                Some(lead) => format!("{lead}."),
                None => plain(n),
            });
        steps.push(match lead {
            Some(lead) => format!("{}. {lead}", plain(title).trim_end_matches('.')),
            None => plain(title),
        });
    }
    steps
}

/// The steps of SUBMITTING.md, as plain text: its numbered sections if it is
/// a walkthrough, else its numbered list.
fn submit_steps(dir: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(dir.join("SUBMITTING.md")) else {
        return Vec::new();
    };
    let sections = submit_sections(&text);
    if sections.len() >= 2 {
        return sections.iter().map(|s| clip(s, 400)).take(8).collect();
    }
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

/// `PROMPT.md`: the text a participant pastes into their coding agent to start
/// on the challenge. Its author writes it for publication, and it goes out as
/// written: stripping Markdown or redacting would mangle the commands in it.
fn prompt(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("PROMPT.md")).ok()?;
    let text = text.replace("\r\n", "\n");
    let text = text.trim();
    (!text.is_empty()).then(|| clip(text, MAX_PROMPT))
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

/// What the public page says about why an attempt got its status. Reasons
/// are `<category>: <detail>`, and the detail of a failed setup, build or
/// score is the tail of that command's stderr, which a submission's build can
/// write. Only the category is published; the detail stays in results.jsonl
/// and the PR comment. Reasons with no detail are cb's own sentences.
fn public_reason(reason: &str) -> String {
    const WHOLE: &[&str] = &["wall-time guard: regressed"];
    let kept = if WHOLE.contains(&reason) {
        reason
    } else {
        reason
            .split_once(':')
            .map_or(reason, |(category, _)| category)
    };
    clip(&redact_emails(kept), MAX_REASON)
}

fn git(repo: &Path, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// What a scored commit changed in the challenge's editable paths. A
/// promotion's commit is the squash merge on main, so its parent is the base;
/// a PR's commit is the branch head, compared from where it left main.
fn patch(
    repo: &Path,
    slug: &str,
    editable: &[String],
    stage: &str,
    commit: &str,
) -> Option<String> {
    if commit.len() != 40 || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let base = match stage {
        "promotion" => format!("{commit}^"),
        "pr" => git(repo, &["merge-base", "HEAD", commit])?
            .trim()
            .to_string(),
        _ => return None,
    };
    let specs: Vec<String> = editable
        .iter()
        .map(|e| format!("challenges/{slug}/{e}"))
        .collect();
    let mut args = vec![
        "diff",
        "--no-color",
        "--no-ext-diff",
        "--no-textconv",
        "-M",
        &base,
        commit,
        "--",
    ];
    args.extend(specs.iter().map(String::as_str));
    let text = git(repo, &args)?;
    if text.is_empty() {
        return None;
    }
    Some(cap_patch(text))
}

/// The challenge's `context.toml`, if any. A broken one leaves the context
/// off the page rather than the whole challenge (preflight and the tests fail
/// on it before it gets here).
fn context(dir: &Path, slug: &str) -> Value {
    match crate::context::Context::load(dir) {
        Ok(c) => json!(c),
        Err(e) => {
            eprintln!("site: {slug}: no context section: {e}");
            Value::Null
        }
    }
}

/// One challenge's public summary. `has_feed`: a feed was written for it.
/// The diffs of its scored submissions go into `patches` as (path, text).
pub fn challenge(
    root: &Path,
    candidates: &Path,
    slug: &str,
    has_feed: bool,
    patches: &mut Vec<(String, String)>,
) -> Result<Value, String> {
    let dir = root.join(slug);
    if crate::challenge::challenge_type(&dir)? == "research" {
        return research_challenge(root, candidates, slug, has_feed, patches);
    }
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
    let repo = root.parent().unwrap_or(root);
    // Newest first.
    let attempts: Vec<Value> = results
        .iter()
        .rev()
        .map(|r| {
            let commit = str_of(&r["commit"]);
            let diff = matches!(str_of(&r["status"]), "promoted" | "valid_claim_better")
                .then(|| patch(repo, slug, &spec.paths.editable, str_of(&r["stage"]), commit))
                .flatten()
                .map(|text| {
                    let path = format!("diffs/{slug}/{commit}.patch");
                    patches.push((path.clone(), text));
                    path
                });
            json!({"diff": diff,"at": str_of(&r["at"]), "stage": str_of(&r["stage"]), "status": str_of(&r["status"]),
                   "value": r["value"].as_f64(), "record_before": r["record_before"].as_f64(),
                   "by": str_of(&r["by"]), "model": str_of(&r["model"]), "commit": str_of(&r["commit"]),
                   "reason": public_reason(str_of(&r["reason"]))})
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
        "prompt": prompt(&dir),
        "context": context(&dir, slug),
        "feed": has_feed.then(|| format!("feeds/{slug}.json")),
    }))
}

/// A submission id or parent that is a single path component.
fn safe_component(s: &str) -> bool {
    !s.is_empty() && s != "." && s != ".." && !s.contains(['/', '\\', '\0'])
}

fn cap_patch(text: String) -> String {
    if text.len() <= MAX_PATCH {
        return text;
    }
    let mut cut = MAX_PATCH;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    let cut = text[..cut].rfind('\n').map_or(cut, |i| i + 1);
    format!(
        "{}\n[diff cut at {} KiB of {} KiB]\n",
        &text[..cut],
        MAX_PATCH / 1024,
        text.len() / 1024
    )
}

/// The change from one submission directory to another, as a patch. Paths are
/// relative to the repo so the patch names no local directory.
fn submission_patch(root: &Path, slug: &str, parent: &str, id: &str) -> Option<String> {
    if !safe_component(parent) || !safe_component(id) {
        return None;
    }
    let repo = root.parent().unwrap_or(root);
    let base = root.strip_prefix(repo).ok()?;
    // A relative root has an empty parent: that is the current directory.
    let repo = if repo.as_os_str().is_empty() {
        Path::new(".")
    } else {
        repo
    };
    let sub = |n: &str| base.join(slug).join("submissions").join(n);
    let out = std::process::Command::new("git")
        .current_dir(repo)
        .args([
            "diff",
            "--no-index",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--",
        ])
        .arg(sub(parent))
        .arg(sub(id))
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    // Exit code 1 means the trees differ.
    if !matches!(out.status.code(), Some(0 | 1)) {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    (!text.is_empty()).then(|| cap_patch(text))
}

/// Sorted file paths under `dir`, relative to it, at most 200.
fn file_listing(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                walk(base, &p, out);
            } else if ft.is_file() {
                if let Ok(r) = p.strip_prefix(base) {
                    out.push(r.to_string_lossy().into_owned());
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out.truncate(200);
    out
}

/// A research challenge's entry: its schema-2 feed, plus the page's own fields
/// and, per accepted submission, its diff against the parent (builds-on) or its
/// files and README excerpt (new).
fn research_challenge(
    root: &Path,
    candidates: &Path,
    slug: &str,
    has_feed: bool,
    patches: &mut Vec<(String, String)>,
) -> Result<Value, String> {
    let dir = root.join(slug);
    let f = crate::research::feed::feed(root, slug, &format!("feeds/{slug}.json"), "")?;
    let mut entry = f.as_object().cloned().unwrap_or_default();
    for k in ["generated_at", "url", "feed_schema_version"] {
        entry.remove(k);
    }
    let status = str_of(&entry["status"]).to_string();
    let has_baseline = entry["submissions"]
        .as_array()
        .is_some_and(|s| s.iter().any(|r| r["baseline"] == true));
    let state = match (status.as_str(), has_baseline) {
        ("open", false) => "pending",
        ("open", true) => "open",
        ("closed", _) => "closed",
        _ => "draft",
    };
    let (readme_title, summary) = readme(&dir, slug);
    let title = candidate_title(candidates, slug).or(readme_title);
    let subs = entry["submissions"].as_array().cloned().unwrap_or_default();
    let subs: Vec<Value> = subs
        .into_iter()
        .map(|mut s| {
            let id = str_of(&s["id"]).to_string();
            let parent = str_of(&s["parent"]).to_string();
            if s["novelty"] == "builds-on" {
                let diff = submission_patch(root, slug, &parent, &id).map(|text| {
                    let path = format!("diffs/{slug}/{id}.patch");
                    patches.push((path.clone(), text));
                    path
                });
                s["diff"] = json!(diff);
            } else if safe_component(&id) {
                let sd = dir.join("submissions").join(&id);
                s["files"] = json!(file_listing(&sd));
                let text = std::fs::read_to_string(sd.join("README.md")).unwrap_or_default();
                let excerpt: Vec<String> = paragraphs(&text)
                    .iter()
                    .filter(|p| !p.starts_with(['#', '|', '`', '<']))
                    .map(|p| plain(p))
                    .take(3)
                    .collect();
                s["readme"] = json!(clip(&excerpt.join("\n\n"), MAX_SUMMARY));
            }
            s
        })
        .collect();
    entry.insert("submissions".into(), Value::Array(subs));
    entry.insert("slug".into(), json!(slug));
    entry.insert("title".into(), json!(title));
    entry.insert("summary".into(), json!(summary));
    entry.insert("state".into(), json!(state));
    entry.insert("type".into(), json!("research"));
    entry.insert("submit_steps".into(), json!(submit_steps(&dir)));
    entry.insert("prompt".into(), json!(prompt(&dir)));
    entry.insert("context".into(), context(&dir, slug));
    entry.insert(
        "feed".into(),
        json!(has_feed.then(|| format!("feeds/{slug}.json"))),
    );
    Ok(Value::Object(entry))
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
         <title>Auto-research challenges</title>\
         <link rel=\"preload\" href=\"assets/instrument-sans.woff2\" as=\"font\" type=\"font/woff2\" crossorigin>\
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
    // A challenge that doesn't load is left off the page, not fatal: Comment
    // and Promote run `cb feed` before they commit results and comment.
    let mut patches = Vec::new();
    let challenges: Vec<Value> = slugs(root)?
        .iter()
        .filter_map(|s| {
            let mut mine = Vec::new();
            let c = challenge(root, &candidates, s, feeds.contains(s), &mut mine);
            if c.as_ref().is_ok_and(|c| c["status"] == "open") {
                patches.append(&mut mine);
            }
            c.map_err(|e| eprintln!("site: skipping {s}: {e}")).ok()
        })
        // The site lists open challenges only; drafts and closed ones keep
        // their feeds but aren't published on the page.
        .filter(|c| c["status"] == "open")
        .collect();
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
    // Rewritten whole each build: a diff whose row left the page goes too.
    let _ = std::fs::remove_dir_all(out.join("diffs"));
    for (rel, text) in &patches {
        let p = out.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
        w(rel, text)?;
    }
    let font = out.join("assets/instrument-sans.woff2");
    std::fs::write(&font, FONT).map_err(|e| format!("{}: {e}", font.display()))?;
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
    fn only_the_category_of_a_reason_is_public() {
        for (reason, public) in [
            ("build: exit status: 1 /home/runner/x.hs:3: SECRET", "build"),
            ("record build: killed after 60 min", "record build"),
            (
                "the record (abc) did not verify and score on this sample: boom",
                "the record (abc) did not verify and score on this sample",
            ),
            ("wall-time guard: regressed", "wall-time guard: regressed"),
            ("verify failed", "verify failed"),
            ("1.02 beats 1 by >= 0.01", "1.02 beats 1 by >= 0.01"),
            ("", ""),
        ] {
            assert_eq!(public_reason(reason), public);
        }
    }

    #[test]
    fn a_challenge_that_does_not_load_is_skipped() {
        let root = crate::paths::scratch("pages-skip").join("challenges");
        let out = root.parent().unwrap().join("out");
        std::fs::create_dir_all(root.join("wip")).unwrap();
        std::fs::write(root.join("wip/challenge.toml"), "[challenge]\n").unwrap();
        write(&root, &out, &[], "2026-01-01T00:00:00Z").unwrap();
        let data: Value =
            serde_json::from_str(&std::fs::read_to_string(out.join("challenges.json")).unwrap())
                .unwrap();
        assert_eq!(data["challenges"], json!([]));
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
        // A walkthrough: the numbered sections are the steps, not its lists.
        std::fs::write(
            dir.join("SUBMITTING.md"),
            "# Submitting\n\nIntro.\n\n## 1. Branch, don't fork\n\nPush a `branch`\nhere.\n\n```sh\ngit x\n```\n\n\
             ## 2. What you may change\n\n- Only `.hs` files.\n\n## 3. Read the result\n\n1. **Gate**, a minute.\n2. Score.\n",
        )
        .unwrap();
        assert_eq!(
            submit_steps(&dir),
            vec![
                "Branch, don't fork. Push a branch here.",
                "What you may change",
                "Read the result"
            ]
        );
    }

    #[test]
    fn the_participant_prompt_is_published_as_written() {
        let dir = crate::paths::scratch("pages-prompt");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(prompt(&dir), None);
        std::fs::write(dir.join("PROMPT.md"), " \n\n").unwrap();
        assert_eq!(prompt(&dir), None);
        // Commands survive: no Markdown stripping, no email redaction of the clone URL.
        let text = "Read `README.md`.\r\n\r\n  git clone git@github.com:o/r.git\r\n- *one* idea\n";
        std::fs::write(dir.join("PROMPT.md"), text).unwrap();
        assert_eq!(
            prompt(&dir).as_deref(),
            Some("Read `README.md`.\n\n  git clone git@github.com:o/r.git\n- *one* idea")
        );
        std::fs::write(dir.join("PROMPT.md"), "x".repeat(MAX_PROMPT + 50)).unwrap();
        assert_eq!(prompt(&dir).unwrap().chars().count(), MAX_PROMPT + 1);
    }

    #[test]
    fn a_submission_diff_covers_its_editable_paths_only() {
        let repo = crate::paths::scratch("pages-diff");
        std::fs::create_dir_all(repo.join("challenges/toy/candidate")).unwrap();
        let git = |args: &[&str]| {
            let ok = std::process::Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args([
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@t",
                    "-c",
                    "commit.gpgsign=false",
                ])
                .args(args)
                .status()
                .unwrap()
                .success();
            assert!(ok, "git {args:?}");
        };
        let head = || {
            super::git(&repo, &["rev-parse", "HEAD"])
                .unwrap()
                .trim()
                .to_string()
        };
        git(&["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("challenges/toy/candidate/a.txt"), "one\n").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-qm", "base"]);
        std::fs::write(repo.join("challenges/toy/candidate/a.txt"), "two\n").unwrap();
        std::fs::write(repo.join("challenges/toy/results.jsonl"), "{}\n").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-qm", "promote"]);
        let promoted = head();
        let ed = vec!["candidate/".to_string()];
        let p = patch(&repo, "toy", &ed, "promotion", &promoted).unwrap();
        assert!(
            p.contains("-one\n+two\n") && !p.contains("results.jsonl"),
            "{p}"
        );
        // A PR head is compared from where it left main.
        git(&["checkout", "-qb", "pr", "HEAD~1"]);
        std::fs::write(repo.join("challenges/toy/candidate/a.txt"), "three\n").unwrap();
        git(&["commit", "-qam", "pr"]);
        let pr = head();
        git(&["checkout", "-q", "main"]);
        let p = patch(&repo, "toy", &ed, "pr", &pr).unwrap();
        assert!(p.contains("-one\n+three\n"), "{p}");
        assert_eq!(patch(&repo, "toy", &ed, "baseline", &promoted), None);
        assert_eq!(patch(&repo, "toy", &ed, "pr", "HEAD"), None);
        assert_eq!(patch(&repo, "toy", &ed, "pr", &"0".repeat(40)), None);
    }

    /// A research challenge under `<scratch>/challenges/t` with accepted rows:
    /// baseline, `a` (new) and `b` (builds on a, answer changed).
    fn research_fixture(name: &str) -> std::path::PathBuf {
        let repo = crate::paths::scratch(name);
        let root = repo.join("challenges");
        std::fs::create_dir_all(&root).unwrap();
        let dir = crate::research::testkit::toy(&root, "t", crate::research::testkit::GOOD_JUDGE);
        let subs = dir.join("submissions");
        for id in ["a", "b"] {
            std::fs::create_dir_all(subs.join(id)).unwrap();
            for f in ["answer", "build.sh"] {
                std::fs::copy(subs.join("baseline").join(f), subs.join(id).join(f)).unwrap();
            }
        }
        std::fs::write(subs.join("a/README.md"), "# a\n\nA **new** approach.\n").unwrap();
        std::fs::write(subs.join("b/answer"), "43\n").unwrap();
        let row = |stage: &str,
                   id: &str,
                   at: &str,
                   novelty: &str,
                   dir: &str,
                   parent: Option<&str>| {
            json!({"schema": 2, "type": "research", "at": at, "stage": stage, "status": "accepted",
                   "slug": "t", "submission": id, "commit": "abc", "by": "x", "model": "m",
                   "contract": "main@1", "novelty": novelty, "direction": dir, "parent": parent,
                   "taxonomy": {"approach": "baseline"}, "proposed": [], "summary": "s",
                   "claims": [],
                   "axes": {"time_ms": {"median": 1.5, "min": 1.0, "max": 2.0, "n": 3},
                            "size_bytes": {"median": 42.0, "min": 42.0, "max": 42.0, "n": 3}},
                   "checks": []})
        };
        let rows = [
            row(
                "baseline",
                "baseline",
                "2026-01-01T00:00:00Z",
                "new",
                "baseline",
                None,
            ),
            row("accept", "a", "2026-01-02T00:00:00Z", "new", "alpha", None),
            row(
                "accept",
                "b",
                "2026-01-03T00:00:00Z",
                "builds-on",
                "alpha",
                Some("a"),
            ),
        ];
        let body: String = rows.iter().map(|r| format!("{r}\n")).collect();
        std::fs::write(dir.join("results.jsonl"), body).unwrap();
        root
    }

    #[test]
    fn research_entry_has_map_data() {
        let root = research_fixture("pages-research");
        let mut patches = Vec::new();
        let c = challenge(&root, &root.join("none"), "t", true, &mut patches).unwrap();
        assert_eq!(c["type"], "research");
        assert_eq!(c["state"], "open");
        assert_eq!(c["feed"], "feeds/t.json");
        assert!(c["axes"].as_array().unwrap().len() >= 2);
        assert_eq!(c["directions"].as_array().unwrap().len(), 2);
        let subs = c["submissions"].as_array().unwrap();
        assert_eq!(subs.len(), 3);
        assert_eq!(subs[0]["axes"]["time_ms"]["median"], 1.5);
        assert_eq!(c["attempts"]["accepted"], 3);
    }

    #[test]
    fn builds_on_gets_diff_against_parent() {
        let root = research_fixture("pages-research-diff");
        let mut patches = Vec::new();
        let c = challenge(&root, &root.join("none"), "t", true, &mut patches).unwrap();
        let b = c["submissions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "b")
            .unwrap();
        assert_eq!(b["diff"], "diffs/t/b.patch");
        assert_eq!(patches.len(), 1);
        let (path, text) = &patches[0];
        assert_eq!(path, "diffs/t/b.patch");
        assert!(text.contains("-42\n+43\n"), "{text}");
        assert!(
            !text.contains("baseline") && !text.contains("build.sh"),
            "{text}"
        );
        assert!(
            !text.contains(&root.to_string_lossy().to_string()),
            "{text}"
        );
    }

    #[test]
    fn new_submission_gets_files_not_diff() {
        let root = research_fixture("pages-research-new");
        let mut patches = Vec::new();
        let c = challenge(&root, &root.join("none"), "t", true, &mut patches).unwrap();
        let a = c["submissions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "a")
            .unwrap();
        assert!(a.get("diff").is_none());
        assert_eq!(a["files"], json!(["README.md", "answer", "build.sh"]));
        assert_eq!(a["readme"], "A new approach.");
    }
}
