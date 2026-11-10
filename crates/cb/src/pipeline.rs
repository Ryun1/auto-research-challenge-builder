//! The GitHub-driven pipeline (PLAN.md §4, #12). The judge always comes from
//! `main`; submission bytes are data until the build step.
//!
//! - `gate`   (trusted, never runs submission code): which challenge, is the
//!   diff allowed, is the head tree clean; exports the editable files.
//! - `score`  (untrusted job): judge from main + submission, build, then
//!   verify/score as `nobody` with no network. Its output is a claim.
//! - `record` (trusted): decides the status against the current record and
//!   appends to `challenges/<slug>/results.jsonl`.
//! - `comment` (trusted): renders a score claim as a PR comment.

use crate::challenge::Spec;
use crate::preflight::{self, host, metrics, run_in, verify_ok};
use crate::{haskellpolicy, paths, policy, rustpolicy};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const RESULTS_SCHEMA: u32 = 1;
const MAX_SCORE_JSON: u64 = 64 * 1024;

fn git(repo: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = Command::new("git")
        .args(["-c", "core.quotepath=false"])
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(format!(
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

fn git_str(repo: &Path, args: &[&str]) -> Result<String, String> {
    git(repo, args).map(|b| String::from_utf8_lossy(&b).trim().to_string())
}

// ---- gate ----

#[derive(Debug)]
pub struct Gate {
    pub slug: Option<String>,
    pub runner: Option<String>,
    pub kit: Option<String>,
    pub errors: Vec<String>,
}

impl Gate {
    pub fn json(&self, base: &str, head: &str) -> Value {
        json!({"ok": self.errors.is_empty(), "slug": self.slug, "runner": self.runner, "kit": self.kit,
               "base": base, "head": head, "errors": self.errors})
    }
}

/// Checks `base..head` in `repo` (a checkout of main with the PR head fetched)
/// and, if allowed, writes the head's editable files to `out`.
pub fn gate(repo: &Path, root: &str, base: &str, head: &str, out: &Path) -> Gate {
    let mut g = Gate {
        slug: None,
        runner: None,
        kit: None,
        errors: Vec::new(),
    };
    let fail = |mut g: Gate, e: String| {
        g.errors.push(e);
        g
    };
    let base = match git_str(repo, &["merge-base", base, head]) {
        Ok(b) => b,
        Err(e) => return fail(g, e),
    };
    let names = match git(
        repo,
        &["diff", "--name-only", "-z", "--no-renames", &base, head],
    ) {
        Ok(n) => n,
        Err(e) => return fail(g, e),
    };
    let changed: Vec<String> = names
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| String::from_utf8_lossy(p).into_owned())
        .collect();
    // Which challenge: every path must sit under one challenges/<slug>/.
    let slugs: std::collections::BTreeSet<&str> = changed
        .iter()
        .map(|p| {
            p.strip_prefix(&format!("{root}/"))
                .and_then(|r| r.split('/').next())
                .unwrap_or("")
        })
        .collect();
    let slug = match slugs.iter().collect::<Vec<_>>()[..] {
        [s] if !s.is_empty() => s.to_string(),
        [] => return fail(g, "empty diff".into()),
        _ => {
            return fail(
                g,
                format!(
                    "a submission must change exactly one challenge under {root}/; touches: {}",
                    changed.join(", ")
                ),
            )
        }
    };
    // The spec comes from main, never from the PR.
    let spec = match Spec::load(&repo.join(root).join(&slug)) {
        Ok(s) => s,
        Err(e) => return fail(g, e),
    };
    g.slug = Some(slug.clone());
    g.runner = Some(spec.challenge.runner.clone());
    g.kit = spec.challenge.kit.clone();
    if spec.challenge.status != "open" {
        g.errors.push(format!(
            "challenge status is {:?}, not open",
            spec.challenge.status
        ));
    }
    let editable: Vec<String> = spec
        .paths
        .editable
        .iter()
        .map(|e| format!("{root}/{slug}/{}", e.trim_end_matches('/')))
        .collect();
    if let Err(e) = policy::check_diff(repo, &base, head, &editable) {
        g.errors.extend(e);
    }
    // The whole head tree under the editable paths, not just the diff:
    // modes, size cap.
    let mut bytes = 0u64;
    for e in &editable {
        let listing = match git(repo, &["ls-tree", "-r", "-l", "-z", head, "--", e]) {
            Ok(l) => l,
            Err(err) => return fail(g, err),
        };
        for entry in listing.split(|b| *b == 0).filter(|l| !l.is_empty()) {
            let entry = String::from_utf8_lossy(entry);
            let (meta, path) = entry.split_once('\t').unwrap_or((&entry, ""));
            let f: Vec<&str> = meta.split_whitespace().collect();
            match f.first().copied() {
                Some("100644") | Some("100755") => {
                    bytes += f.get(3).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0)
                }
                Some(mode) => g.errors.push(format!(
                    "{path}: mode {mode} (symlink/gitlink) in the editable tree"
                )),
                None => {}
            }
            if !paths::is_canonical(path) {
                g.errors.push(format!("{path}: path not canonical"));
            }
        }
    }
    if bytes > spec.paths.max_bytes {
        g.errors.push(format!(
            "editable tree is {bytes} bytes > max_bytes {}",
            spec.paths.max_bytes
        ));
    }
    if !g.errors.is_empty() {
        return g;
    }
    // Export as data. Only regular files reach here.
    let _ = std::fs::remove_dir_all(out);
    for e in &editable {
        let tar = match git(repo, &["archive", "--format=tar", head, "--", e]) {
            Ok(t) => t,
            Err(err) => return fail(g, err),
        };
        std::fs::create_dir_all(out).ok();
        let mut child = match Command::new("tar")
            .args(["-x", "-C"])
            .arg(out)
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(err) => return fail(g, format!("tar: {err}")),
        };
        use std::io::Write;
        let _ = child.stdin.take().map(|mut s| s.write_all(&tar));
        if !child.wait().map(|s| s.success()).unwrap_or(false) {
            return fail(g, "tar extraction failed".into());
        }
    }
    if let Some(rust) = &spec.rust {
        for e in &spec.paths.editable {
            let dir = out.join(root).join(&slug).join(e.trim_end_matches('/'));
            if let Err(errs) = rustpolicy::check_dir(&dir, &rust.allow) {
                g.errors.extend(errs);
            }
        }
    }
    if let Some(hs) = &spec.haskell {
        let base = repo.join(root).join(&slug).join("baseline");
        for e in &spec.paths.editable {
            let dir = out.join(root).join(&slug).join(e.trim_end_matches('/'));
            if let Err(errs) = haskellpolicy::check(&base, &dir, &hs.src) {
                g.errors.extend(errs);
            }
        }
    }
    g
}

// ---- score ----

/// Which editable content to score.
pub enum Source<'a> {
    /// Whatever the checkout has (main after a merge).
    AsIs,
    /// The challenge's baseline/.
    Baseline,
    /// Files exported by `gate` (a directory mirroring the repo layout).
    Submission(&'a Path),
}

/// Builds and scores one challenge. Setup and build run as the current user
/// (the build is the only step that compiles submission bytes, and the kit
/// forbids build scripts); verify and score run sandboxed when asked.
pub fn score(root: &Path, slug: &str, source: Source, sandbox: bool) -> Result<Value, String> {
    let dir = root.join(slug);
    let spec = Spec::load(&dir)?;
    let [editable] = &spec.paths.editable[..] else {
        return Err("one editable path supported".into());
    };
    let editable = editable.trim_end_matches('/');
    // World-readable scratch copy: `nobody` can read and run it, not write it.
    let ws = paths::scratch(&format!("score-{slug}"));
    let status = Command::new("cp")
        .arg("-R")
        .arg(&dir)
        .arg(&ws)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("copying the challenge".into());
    }
    let _ = Command::new("chmod").args(["-R", "a+rX"]).arg(&ws).status();
    let _ = std::fs::remove_dir_all(ws.join("target"));
    let fill = match source {
        Source::AsIs => None,
        Source::Baseline => Some(dir.join("baseline")),
        Source::Submission(sub) => Some(
            sub.join(root.file_name().unwrap_or_default())
                .join(slug)
                .join(editable),
        ),
    };
    if let Some(from) = fill {
        let target = ws.join(editable);
        let _ = std::fs::remove_dir_all(&target);
        std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        let ok = Command::new("cp")
            .arg("-R")
            .arg(from.join("."))
            .arg(&target)
            .status()
            .map_err(|e| e.to_string())?
            .success();
        if !ok {
            return Err(format!("copying {} into the workspace", from.display()));
        }
    }
    let env: Vec<(String, String)> = spec
        .perf
        .as_ref()
        .map(|p| {
            vec![
                ("CB_PERF_BLOCKS".into(), p.blocks.to_string()),
                (
                    "CB_PERF_MAX_DISPERSION".into(),
                    p.max_dispersion.to_string(),
                ),
            ]
        })
        .unwrap_or_default();
    let mut out = json!({"schema": RESULTS_SCHEMA, "slug": slug, "host": host(), "host_policy": preflight::host_policy(),
                         "sandboxed": sandbox, "expr": spec.objective.expr});
    let setup = run_in(false, &env, &ws, &spec.commands.setup, 0);
    if !setup.ok {
        out["status"] = "infra_failed".into();
        out["reason"] = format!("setup: {}", setup.detail).into();
        return Ok(out);
    }
    if let Some(build) = &spec.commands.build {
        let b = run_in(false, &env, &ws, build, 0);
        if !b.ok {
            out["status"] = "rejected".into();
            out["reason"] = format!("build: {}", b.detail).into();
            return Ok(out);
        }
    }
    let _ = Command::new("chmod").args(["-R", "a+rX"]).arg(&ws).status();
    // The seed is drawn only now, after the build.
    let seed = preflight::seed();
    out["seed"] = seed.to_string().into();
    let v = run_in(sandbox, &env, &ws, &spec.commands.verify, seed);
    out["verify"] = json!({"ok": verify_ok(&v), "detail": v.stdout.trim().chars().take(2000).collect::<String>(), "secs": v.secs});
    if !verify_ok(&v) {
        out["status"] = "rejected".into();
        out["reason"] = "verify failed".into();
        return Ok(out);
    }
    let s = run_in(sandbox, &env, &ws, &spec.commands.score, seed);
    let Some(m) = metrics(&s) else {
        out["status"] = "infra_failed".into();
        out["reason"] = format!("score produced no metrics: {}", s.detail).into();
        return Ok(out);
    };
    let value = m.get(&spec.objective.expr).and_then(Value::as_f64);
    out["metrics"] = m.clone();
    out["value"] = value.into();
    out["status"] = match (value, m.get("guard").and_then(Value::as_str)) {
        (None, _) => "infra_failed",
        (_, Some("regressed")) => "rejected",
        (_, Some("unstable")) => "infra_failed",
        _ => "valid",
    }
    .into();
    if out["status"] == "rejected" {
        out["reason"] = "wall-time guard: regressed".into();
    }
    let _ = std::fs::remove_dir_all(&ws);
    Ok(out)
}

/// Reads a score claim written by an untrusted job: size-capped, must parse.
pub fn read_claim(path: &Path) -> Result<Value, String> {
    let len = std::fs::metadata(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .len();
    if len > MAX_SCORE_JSON {
        return Err(format!("{}: {len} bytes, over the cap", path.display()));
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

// ---- record ----

pub struct Attempt<'a> {
    pub stage: &'a str,
    pub commit: &'a str,
    pub by: &'a str,
    pub model: &'a str,
    pub at: &'a str,
}

fn results_path(root: &Path, slug: &str) -> PathBuf {
    root.join(slug).join("results.jsonl")
}

pub fn read_results(root: &Path, slug: &str) -> Result<Vec<Value>, String> {
    let path = results_path(root, slug);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(Vec::new());
    };
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .enumerate()
        .map(|(i, l)| {
            serde_json::from_str(l).map_err(|e| format!("{}:{}: {e}", path.display(), i + 1))
        })
        .collect()
}

/// The current record: the last promoted or baseline entry.
pub fn current_record(results: &[Value]) -> Option<&Value> {
    results
        .iter()
        .rev()
        .find(|r| matches!(r["status"].as_str(), Some("promoted") | Some("baseline")))
}

/// Decides and appends one attempt. `runs` are score outputs: two independent
/// runs for promotion and baseline, one claim for a PR.
pub fn record(root: &Path, slug: &str, runs: &[Value], a: &Attempt) -> Result<Value, String> {
    let spec = Spec::load(&root.join(slug))?;
    let results = read_results(root, slug)?;
    let m = spec.objective.min_improvement_bips as f64 / 10_000.0;
    let maximise = spec.challenge.direction == "maximise";
    let want = if a.stage == "pr" { 1 } else { 2 };
    if runs.len() != want {
        return Err(format!(
            "stage {} needs {want} run(s), got {}",
            a.stage,
            runs.len()
        ));
    }
    if runs.iter().any(|r| r["slug"] != slug) {
        return Err("a run is for a different challenge".into());
    }
    let statuses: Vec<&str> = runs
        .iter()
        .map(|r| r["status"].as_str().unwrap_or("infra_failed"))
        .collect();
    let values: Vec<Option<f64>> = runs.iter().map(|r| r["value"].as_f64()).collect();
    let record = current_record(&results);
    let record_value = record.and_then(|r| r["value"].as_f64());
    // Independent runs must agree within half the bar (exactly, if the bar is 0).
    let agree = match values[..] {
        [Some(x), Some(y)] => ((x - y).abs() / x.abs().max(f64::MIN_POSITIVE)) <= m / 2.0,
        [Some(_)] => true,
        _ => false,
    };
    let (status, reason) =
        if statuses.contains(&"infra_failed") {
            (
                "infra_failed",
                "a run failed for infrastructure reasons".to_string(),
            )
        } else if statuses.contains(&"rejected") {
            (
                "rejected",
                runs.iter()
                    .filter_map(|r| r["reason"].as_str())
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        } else if !agree {
            (
                "infra_failed",
                format!("independent runs disagree beyond {}: {values:?}", m / 2.0),
            )
        } else if a.stage == "baseline" {
            if record.is_some() {
                return Err("baseline already recorded".into());
            }
            ("baseline", String::new())
        } else {
            let v = values[0].unwrap();
            match record_value {
                None => return Err(
                    "no baseline in results.jsonl: run the promote workflow with baseline first"
                        .into(),
                ),
                Some(rec) => {
                    let better = if maximise {
                        v > rec && v >= rec * (1.0 + m)
                    } else {
                        v < rec && v <= rec * (1.0 - m)
                    };
                    if better && a.stage == "promotion" {
                        ("promoted", format!("{} beats {rec} by >= {m}", v))
                    } else if better {
                        (
                            "valid_claim_better",
                            format!("claims {v} vs record {rec}; promotion decides"),
                        )
                    } else {
                        (
                            "valid_not_promoted",
                            format!("{v} does not beat {rec} by {m}"),
                        )
                    }
                }
            }
        };
    let mean = values.iter().flatten().sum::<f64>() / values.iter().flatten().count().max(1) as f64;
    let entry = json!({
        "schema": RESULTS_SCHEMA, "at": a.at, "stage": a.stage, "status": status, "reason": reason,
        "slug": slug, "commit": a.commit, "by": a.by, "model": a.model,
        "expr": spec.objective.expr, "value": if values.iter().all(Option::is_some) { json!(mean) } else { Value::Null },
        "record_before": record_value,
        "runs": runs.iter().map(|r| json!({"seed": r["seed"], "host": r["host"], "host_policy": r["host_policy"], "value": r["value"], "status": r["status"],
                                           "metrics": r["metrics"], "sandboxed": r["sandboxed"]})).collect::<Vec<_>>(),
    });
    let path = results_path(root, slug);
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

/// The commit whose editable tree is the current record (for reverting a
/// merged attempt that wasn't promoted).
pub fn record_commit(root: &Path, slug: &str) -> Result<Option<String>, String> {
    Ok(current_record(&read_results(root, slug)?)
        .and_then(|r| r["commit"].as_str())
        .map(String::from))
}

// ---- comment ----

/// Neutralises markdown/HTML in untrusted text for a PR comment.
fn md(s: &str) -> String {
    s.chars()
        .take(300)
        .map(|c| match c {
            '<' | '>' | '&' | '`' | '|' | '[' | ']' | '@' | '*' | '_' | '#' => ' ',
            '\n' | '\r' => ' ',
            c => c,
        })
        .collect()
}

/// A PR comment for a gate result and (if the gate passed) a score claim.
pub fn comment(gate: &Value, claim: Option<&Value>, entry: Option<&Value>) -> String {
    let mut out = String::from("<!-- cb:score -->\n### Challenge builder\n\n");
    if gate["ok"] != true {
        out += "**Refused by the gate.** Nothing was built or run.\n\n";
        for e in gate["errors"].as_array().into_iter().flatten() {
            out += &format!("- {}\n", md(e.as_str().unwrap_or("?")));
        }
        return out;
    }
    let slug = md(gate["slug"].as_str().unwrap_or("?"));
    out += &format!("Challenge: `{slug}`\n\n");
    match claim {
        None => out += "No score was produced (the score job failed before writing one).\n",
        Some(c) => {
            let num = |v: &Value| {
                v.as_f64()
                    .map(|x| format!("{x}"))
                    .unwrap_or_else(|| "-".into())
            };
            out += &format!(
                "| status | {} | value | {} ({}) |\n|---|---|---|---|\n\n",
                md(c["status"].as_str().unwrap_or("?")),
                num(&c["value"]),
                md(c["expr"].as_str().unwrap_or("?"))
            );
            if let Some(r) = c["reason"].as_str() {
                out += &format!("Reason: {}\n\n", md(r));
            }
            if let Some(g) = c["metrics"]["guard"].as_str() {
                out += &format!(
                    "Wall-time guard: {} (speedup {})\n\n",
                    md(g),
                    num(&c["metrics"]["wall_speedup"])
                );
            }
        }
    }
    if let Some(e) = entry {
        out += &format!(
            "Recorded as `{}`: {}\n\n",
            md(e["status"].as_str().unwrap_or("?")),
            md(e["reason"].as_str().unwrap_or(""))
        );
    }
    out += "_This score is a claim from the PR run. On merge, two clean runs on `main` with fresh seeds decide the record._\n";
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::challenge::scaffold;

    fn sh(dir: &Path, script: &str) {
        let ok = Command::new("bash")
            .args(["-c", &format!("set -e; export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t; {script}")])
            .current_dir(dir)
            .status()
            .unwrap()
            .success();
        assert!(ok, "{script}");
    }

    /// A repo with an open data challenge (golomb-like) committed on main.
    pub(crate) fn repo() -> PathBuf {
        let dir = paths::scratch("repo");
        let ch = scaffold(&dir.join("challenges"), "toy", "math", None).unwrap();
        let spec = std::fs::read_to_string(ch.join("challenge.toml"))
            .unwrap()
            .replace("status    = \"draft\"", "status    = \"open\"")
            .replace("max_bytes = 8388608", "max_bytes = 64");
        std::fs::write(ch.join("challenge.toml"), spec).unwrap();
        std::fs::write(ch.join("verifier/run"), "#!/usr/bin/env bash\nif [ \"$(cat candidate/answer)\" -le 100 ]; then echo '{\"ok\": true}'; else echo '{\"ok\": false}'; exit 1; fi\n").unwrap();
        std::fs::write(ch.join("bench/run"), "#!/usr/bin/env bash\necho \"{\\\"metrics\\\": {\\\"value\\\": $(cat candidate/answer)}}\"\n").unwrap();
        std::fs::write(ch.join("candidate/answer"), "50\n").unwrap();
        std::fs::write(ch.join("baseline/answer"), "50\n").unwrap();
        std::fs::create_dir_all(dir.join(".github/workflows")).unwrap();
        std::fs::write(dir.join(".github/workflows/x.yml"), "on: push\n").unwrap();
        sh(
            &dir,
            "git init -q -b main && git add -A && git commit -qm base",
        );
        dir
    }

    fn head(dir: &Path, script: &str) -> String {
        sh(
            dir,
            &format!(
                "git checkout -q --detach main && {script} && git add -A && git commit -qm pr"
            ),
        );
        git_str(dir, &["rev-parse", "HEAD"]).unwrap()
    }

    #[test]
    fn gate_allows_an_editable_change_and_exports_it() {
        let dir = repo();
        let h = head(&dir, "echo 60 > challenges/toy/candidate/answer");
        let out = paths::scratch("gate-out");
        let g = gate(&dir, "challenges", "main", &h, &out);
        assert!(g.errors.is_empty(), "{:?}", g.errors);
        assert_eq!(g.slug.as_deref(), Some("toy"));
        assert_eq!(
            std::fs::read_to_string(out.join("challenges/toy/candidate/answer")).unwrap(),
            "60\n"
        );
    }

    #[test]
    fn gate_refusals() {
        let dir = repo();
        let cases = [
            ("workflow edit", "echo pwn >> .github/workflows/x.yml"),
            (
                "score.json written",
                "echo '{}' > challenges/toy/score.json",
            ),
            ("verifier edit", "echo exit 0 > challenges/toy/verifier/run"),
            (
                "symlink",
                "ln -s ../verifier/run challenges/toy/candidate/link",
            ),
            (
                "over size cap",
                "head -c 100 /dev/zero | tr '\\0' 1 > challenges/toy/candidate/answer",
            ),
            (
                "two places",
                "echo 1 > challenges/toy/candidate/answer && echo x > README.md",
            ),
        ];
        for (name, script) in cases {
            let h = head(&dir, script);
            let g = gate(&dir, "challenges", "main", &h, &paths::scratch("gate-out"));
            assert!(!g.errors.is_empty(), "{name} was allowed");
            eprintln!("{name}: {:?}", g.errors);
        }
    }

    #[test]
    fn score_and_record_flow() {
        let dir = repo();
        let root = dir.join("challenges");
        let run = |src: Source| score(&root, "toy", src, false).unwrap();
        let at = Attempt {
            stage: "baseline",
            commit: "c0",
            by: "baseline",
            model: "",
            at: "t0",
        };
        let b = [run(Source::Baseline), run(Source::Baseline)];
        assert_eq!(b[0]["status"], "valid", "{}", b[0]);
        assert_eq!(record(&root, "toy", &b, &at).unwrap()["status"], "baseline");
        assert!(record(&root, "toy", &b, &at).is_err()); // only once

        // Better (math family here: direction maximise, value 60 > 50).
        std::fs::write(root.join("toy/candidate/answer"), "60\n").unwrap();
        let r = [run(Source::AsIs), run(Source::AsIs)];
        let e = record(
            &root,
            "toy",
            &r,
            &Attempt {
                stage: "promotion",
                commit: "c1",
                by: "alice",
                ..at
            },
        )
        .unwrap();
        assert_eq!(e["status"], "promoted", "{e}");
        assert_eq!(record_commit(&root, "toy").unwrap().as_deref(), Some("c1"));

        // Not better, and invalid.
        let r = [run(Source::Baseline), run(Source::Baseline)];
        assert_eq!(
            record(
                &root,
                "toy",
                &r,
                &Attempt {
                    stage: "promotion",
                    commit: "c2",
                    ..at
                }
            )
            .unwrap()["status"],
            "valid_not_promoted"
        );
        std::fs::write(root.join("toy/candidate/answer"), "500\n").unwrap();
        let r = [run(Source::AsIs)];
        assert_eq!(
            record(
                &root,
                "toy",
                &r,
                &Attempt {
                    stage: "pr",
                    commit: "c3",
                    ..at
                }
            )
            .unwrap()["status"],
            "rejected"
        );
        assert_eq!(read_results(&root, "toy").unwrap().len(), 4);
        assert_eq!(record_commit(&root, "toy").unwrap().as_deref(), Some("c1"));
    }

    #[test]
    fn disagreeing_runs_are_infra_failures() {
        let dir = repo();
        let root = dir.join("challenges");
        let at = Attempt {
            stage: "baseline",
            commit: "c0",
            by: "b",
            model: "",
            at: "t",
        };
        let v = |x: f64| json!({"slug": "toy", "status": "valid", "value": x});
        assert_eq!(
            record(&root, "toy", &[v(50.0), v(51.0)], &at).unwrap()["status"],
            "infra_failed"
        );
    }

    /// Runtime containment (#13). Linux CI only (needs sudo, unshare, setpriv);
    /// CI sets CB_REQUIRE_SANDBOX so it can't silently skip.
    #[test]
    fn sandbox_contains_the_candidate() {
        if std::env::var_os("CB_REQUIRE_SANDBOX").is_none() {
            eprintln!("skipped: set CB_REQUIRE_SANDBOX on a Linux runner");
            return;
        }
        let ws = paths::scratch("sandbox");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(ws.join("secret"), "judge-only").unwrap();
        let _ = Command::new("chmod").args(["-R", "a+rX"]).arg(&ws).status();
        let env = vec![("CB_PERF_BLOCKS".to_string(), "4".to_string())];
        let sh = |cmd: &str| run_in(true, &env, &ws, cmd, 42);
        assert_eq!(sh("id -u").stdout.trim(), "65534");
        assert_eq!(sh("echo $CB_SEED $CB_PERF_BLOCKS").stdout.trim(), "42 4");
        assert!(!sh("echo pwn > score.json").ok, "wrote into the workspace");
        assert!(!sh("echo pwn >> secret").ok, "modified a judge file");
        assert!(!sh("exec 3<>/dev/tcp/1.1.1.1/53").ok, "reached the network");
        assert!(!sh("sudo -n true").ok, "escalated with sudo");
        let envs = sh("env").stdout;
        assert!(
            !envs.contains("TOKEN") && !envs.contains("ACTIONS_") && !envs.contains("GITHUB_"),
            "{envs}"
        );
    }

    #[test]
    fn comment_neutralises_untrusted_text() {
        let gate = json!({"ok": true, "slug": "toy"});
        let claim = json!({"status": "valid", "value": 1.5, "expr": "speedup", "reason": "<img src=x onerror=1> @everyone [x](javascript:1)"});
        let c = comment(&gate, Some(&claim), None);
        assert!(
            !c.contains("<img") && !c.contains("@everyone") && !c.contains("]("),
            "{c}"
        );
        let refused = comment(&json!({"ok": false, "errors": ["x: outside"]}), None, None);
        assert!(refused.contains("Refused"));
    }
}
