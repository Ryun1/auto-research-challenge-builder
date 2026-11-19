//! `cb measure`: build an untrusted submission, draw the seed, run the trusted
//! judge n times (design spec §4).

use super::spec::{Declaration, ResearchSpec};
use crate::challenge::BuildSandbox;
use crate::paths;
use crate::preflight::{self, Mode};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

pub enum Source<'a> {
    /// A gate export dir (`<dir>/<root name>/<slug>/submissions/<id>`).
    Export(&'a Path),
    /// Already in the challenge.
    Id(&'a str),
    /// Any submission dir; the id is its name.
    Dir(&'a Path),
}

const JUDGE_CAP: usize = 64 * 1024;

struct Guard(PathBuf);
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn cap(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn finish(mut m: Value, status: &str, reason: &str) -> Result<Value, String> {
    m["status"] = json!(status);
    m["reason"] = json!(reason);
    Ok(m)
}

fn one_dir(dir: &Path) -> Result<String, String> {
    let mut names = Vec::new();
    for e in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let e = e.map_err(|e| e.to_string())?;
        if e.path().is_dir() {
            names.push(e.file_name().to_string_lossy().into_owned());
        }
    }
    match names.as_slice() {
        [n] => Ok(n.clone()),
        _ => Err(format!(
            "{}: expected exactly one submission dir, found {}",
            dir.display(),
            names.len()
        )),
    }
}

fn median(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

pub fn measure(
    root: &Path,
    slug: &str,
    source: Source,
    sandbox: bool,
    seed_override: Option<u64>,
) -> Result<Value, String> {
    let dir = root.join(slug);
    let spec = ResearchSpec::load(&dir)?;
    let ws = paths::scratch(&format!("measure-{slug}"));
    let _guard = Guard(ws.clone());
    let st = Command::new("cp")
        .arg("-R")
        .arg(&dir)
        .arg(&ws)
        .status()
        .map_err(|e| e.to_string())?;
    if !st.success() {
        return Err("copying the challenge".into());
    }
    let _ = std::fs::remove_dir_all(ws.join("target"));
    let subs = ws.join("submissions");
    std::fs::create_dir_all(&subs).map_err(|e| e.to_string())?;
    let id = match source {
        Source::Id(id) => {
            if !subs.join(id).is_dir() {
                return Err(format!("submissions/{id} not in the challenge"));
            }
            id.to_string()
        }
        Source::Export(out) => {
            let base = out
                .join(root.file_name().unwrap_or_default())
                .join(slug)
                .join("submissions");
            let id = one_dir(&base)?;
            place(&base.join(&id), &subs.join(&id))?;
            id
        }
        Source::Dir(p) => {
            let id = p
                .file_name()
                .ok_or("submission dir has no name")?
                .to_string_lossy()
                .into_owned();
            place(p, &subs.join(&id))?;
            id
        }
    };
    let sub = subs.join(&id);
    let mut m = json!({
        "schema": 2, "type": "research", "slug": slug, "submission": id,
        "contract": null, "seed": "", "host": preflight::host(),
        "host_policy": preflight::host_policy(), "sandboxed": sandbox,
        "build": {"ok": false, "secs": 0.0}, "inputs": null,
        "checks": [], "axes": {}, "judge_secs": 0.0,
    });

    // Declaration.
    let decl = std::fs::read_to_string(sub.join("submission.toml"))
        .map_err(|e| e.to_string())
        .and_then(|t| Declaration::parse(&t));
    let decl = match decl {
        Ok(d) => d,
        Err(e) => return finish(m, "rejected", &format!("submission.toml: {e}")),
    };
    m["contract"] = json!(decl.contract);
    if let Some(p) = decl.check(&spec, &id).first() {
        return finish(m, "rejected", &format!("submission.toml: {p}"));
    }
    let Some(judge) = spec.contract(&decl.contract).and_then(|c| c.judge.clone()) else {
        return finish(
            m,
            "rejected",
            &format!("submission.toml: contract {:?} has no judge", decl.contract),
        );
    };

    // Setup (trusted).
    let r = preflight::run_in(false, &[], &ws, &spec.commands.setup, 0);
    if !r.ok {
        return finish(
            m,
            "infra_failed",
            &format!("setup: {}", cap(&r.detail, 300)),
        );
    }

    // Build.
    let b = if sandbox {
        if !preflight::build_sandbox_available() {
            return finish(
                m,
                "infra_failed",
                "build sandbox unavailable (needs Linux, sudo, unshare, setpriv)",
            );
        }
        let bs = BuildSandbox {
            writable: spec.submissions.writable.clone(),
            outputs: vec![format!("submissions/{id}")],
        };
        preflight::run_mode(
            Mode::Build(&bs),
            &[],
            &ws,
            &format!("cd submissions/{id} && ./build.sh"),
            0,
        )
    } else {
        preflight::run_in(false, &[], &sub, "./build.sh", 0)
    };
    m["build"] = json!({"ok": b.ok, "secs": b.secs});
    if !b.ok {
        return finish(m, "rejected", &format!("build: {}", cap(&b.detail, 300)));
    }
    let _ = Command::new("chmod").args(["-R", "a+rX"]).arg(&ws).status();
    let seed = seed_override.unwrap_or_else(preflight::seed);
    m["seed"] = json!(seed.to_string());

    // Judge.
    let axes = spec.axes_for(&decl.contract);
    let n = axes.iter().map(|a| a.repeats).max().unwrap_or(1).max(1);
    let props = spec.properties_for(&decl.contract);
    let bin = std::env::var("CB_BIN")
        .ok()
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
        .ok_or("cannot locate cb binary")?;
    let abs_sub = std::fs::canonicalize(&sub).map_err(|e| e.to_string())?;
    let limits = spec.limits_json().to_string();
    let cmd = format!("./{judge}");
    let mut samples: Vec<Map<String, Value>> = Vec::new();
    let mut inputs0: Option<String> = None;
    let mut judge_secs = 0.0;
    for rep in 0..n {
        let env = vec![
            ("CB_REPEAT".to_string(), rep.to_string()),
            ("CB_CONTRACT".to_string(), decl.contract.clone()),
            (
                "CB_SUBMISSION_DIR".to_string(),
                abs_sub.to_string_lossy().into_owned(),
            ),
            ("CB_BIN".to_string(), bin.to_string_lossy().into_owned()),
            ("CB_LIMITS".to_string(), limits.clone()),
            (
                "CB_SANDBOX".to_string(),
                if sandbox { "1" } else { "0" }.into(),
            ),
        ];
        let r = preflight::run_in(false, &env, &ws, &cmd, seed);
        judge_secs += r.secs;
        m["judge_secs"] = json!(judge_secs);
        let infra = |m: Value, why: &str| finish(m, "infra_failed", &cap(why, 300));
        if r.stdout.len() > JUDGE_CAP {
            return infra(m, "judge output over 64 KiB");
        }
        let Ok(Value::Object(o)) = serde_json::from_str::<Value>(r.stdout.trim()) else {
            return infra(
                m,
                &format!("judge output is not a JSON object: {}", r.detail),
            );
        };
        if o.get("infra").and_then(Value::as_bool) == Some(true) {
            return infra(m, "judge reported infra failure");
        }
        let Some(inputs) = o.get("inputs").and_then(Value::as_str) else {
            return infra(m, "judge output missing inputs");
        };
        let empty = Vec::new();
        let checks = o.get("checks").and_then(Value::as_array).unwrap_or(&empty);
        let mut out_checks = Vec::new();
        let mut first_fail: Option<String> = None;
        let mut seen = Vec::new();
        for c in checks {
            let pid = c.get("property").and_then(Value::as_str).unwrap_or("");
            let Some(p) = props.iter().find(|p| p.id == pid) else {
                return infra(m, &format!("judge check for unknown property {pid:?}"));
            };
            let ok = c.get("ok").and_then(Value::as_bool).unwrap_or(false);
            let detail = cap(c.get("detail").and_then(Value::as_str).unwrap_or(""), 300);
            if !ok && first_fail.is_none() {
                first_fail = Some(format!("check {pid}: {detail}"));
            }
            seen.push(pid.to_string());
            out_checks.push(json!({"property": pid, "check": p.check, "ok": ok, "detail": detail}));
        }
        if let Some(p) = props
            .iter()
            .find(|p| p.check != "declared" && !seen.contains(&p.id))
        {
            return infra(m, &format!("judge omitted check for property {:?}", p.id));
        }
        m["checks"] = json!(out_checks);
        match &inputs0 {
            None => inputs0 = Some(inputs.to_string()),
            Some(i) if i != inputs => {
                return infra(m, "inputs differ across repeats on the same seed")
            }
            _ => {}
        }
        m["inputs"] = json!(inputs);
        let pass = o.get("ok").and_then(Value::as_bool) == Some(true) && first_fail.is_none();
        if !pass {
            if rep > 0 {
                return finish(m, "rejected", "nondeterministic verdicts on the same seed");
            }
            let why = first_fail.unwrap_or_else(|| "judge: not ok".into());
            return finish(m, "rejected", &why);
        }
        let ax = o.get("axes").and_then(Value::as_object);
        let mut row = Map::new();
        for a in &axes {
            match ax.and_then(|x| x.get(&a.id)).and_then(Value::as_f64) {
                Some(v) if v.is_finite() => {
                    row.insert(a.id.clone(), json!(v));
                }
                _ => {
                    return infra(
                        m,
                        &format!("axis {:?} is missing or not a finite number", a.id),
                    )
                }
            }
        }
        samples.push(row);
    }

    let mut out = Map::new();
    for a in &axes {
        let mut v: Vec<f64> = samples
            .iter()
            .take(a.repeats as usize)
            .map(|r| r[&a.id].as_f64().unwrap())
            .collect();
        let raw = v.clone();
        v.sort_by(|x, y| x.partial_cmp(y).unwrap());
        out.insert(
            a.id.clone(),
            json!({"median": median(&v), "min": v[0], "max": v[v.len() - 1],
                   "n": v.len(), "samples": raw}),
        );
    }
    m["axes"] = Value::Object(out);
    finish(m, "valid", "")
}

fn place(from: &Path, to: &Path) -> Result<(), String> {
    let _ = std::fs::remove_dir_all(to);
    let st = Command::new("cp")
        .arg("-R")
        .arg(from)
        .arg(to)
        .status()
        .map_err(|e| e.to_string())?;
    if st.success() {
        Ok(())
    } else {
        Err(format!("copying {}", from.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research::testkit::{toy, GOOD_JUDGE};

    fn tmp(tag: &str) -> PathBuf {
        let d = paths::scratch(&format!("mt-{tag}"));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn run(judge: &str, tag: &str) -> Value {
        let root = tmp(tag);
        toy(&root, "toy", judge);
        measure(&root, "toy", Source::Id("baseline"), false, None).unwrap()
    }
    fn reason(v: &Value) -> String {
        v["reason"].as_str().unwrap().to_string()
    }

    #[test]
    fn valid_submission_measures() {
        let v = run(GOOD_JUDGE, "valid");
        assert_eq!(v["status"], "valid", "{v}");
        assert_eq!(v["axes"]["time_ms"]["n"], 3);
        assert_eq!(v["axes"]["size_bytes"]["n"], 1);
        assert!(v["seed"].is_string());
        assert_eq!(v["build"]["ok"], true);
        assert_eq!(v["checks"][0]["check"], "mechanical");
    }

    #[test]
    fn build_failure_rejected() {
        let root = tmp("build");
        let dir = toy(&root, "toy", GOOD_JUDGE);
        std::fs::write(
            dir.join("submissions/baseline/build.sh"),
            "#!/bin/sh\nexit 1\n",
        )
        .unwrap();
        let v = measure(&root, "toy", Source::Id("baseline"), false, None).unwrap();
        assert_eq!(v["status"], "rejected");
        assert!(reason(&v).starts_with("build:"), "{v}");
    }

    #[test]
    fn failing_check_rejected() {
        let root = tmp("fail");
        let dir = toy(&root, "toy", GOOD_JUDGE);
        std::fs::write(dir.join("submissions/baseline/answer"), "7\n").unwrap();
        let v = measure(&root, "toy", Source::Id("baseline"), false, None).unwrap();
        assert_eq!(v["status"], "rejected");
        assert!(reason(&v).contains("correct"), "{v}");
    }

    fn judge(inputs: &str, checks: &str, axes: &str) -> String {
        format!(
            "#!/usr/bin/env bash\necho '{{\"ok\": true, \"inputs\": \"{inputs}\", \"checks\": {checks}, \"axes\": {axes}}}'\n"
        )
    }
    const OK_CHECK: &str = r#"[{"property": "correct", "ok": true, "detail": ""}]"#;
    const OK_AXES: &str = r#"{"time_ms": 1.0, "size_bytes": 2}"#;

    #[test]
    fn explicit_judge_is_valid() {
        let v = run(&judge("x", OK_CHECK, OK_AXES), "explicit");
        assert_eq!(v["status"], "valid", "{v}");
    }

    #[test]
    fn missing_mechanical_property_is_infra() {
        let v = run(&judge("x", "[]", OK_AXES), "nocheck");
        assert_eq!(v["status"], "infra_failed", "{v}");
        assert!(reason(&v).contains("omitted"), "{v}");
    }

    #[test]
    fn missing_axis_is_infra() {
        let v = run(&judge("x", OK_CHECK, r#"{"time_ms": 1.0}"#), "noaxis");
        assert_eq!(v["status"], "infra_failed", "{v}");
        assert!(reason(&v).contains("size_bytes"), "{v}");
    }

    #[test]
    fn judge_not_json_is_infra() {
        let v = run("#!/usr/bin/env bash\necho hello\n", "nojson");
        assert_eq!(v["status"], "infra_failed", "{v}");
    }

    #[test]
    fn inputs_differ_across_repeats_is_infra() {
        let j = "#!/usr/bin/env bash\necho \"{\\\"ok\\\": true, \\\"inputs\\\": \\\"r$CB_REPEAT\\\", \\\"checks\\\": [{\\\"property\\\": \\\"correct\\\", \\\"ok\\\": true, \\\"detail\\\": \\\"\\\"}], \\\"axes\\\": {\\\"time_ms\\\": 1, \\\"size_bytes\\\": 2}}\"\n";
        let v = run(j, "inputs");
        assert_eq!(v["status"], "infra_failed", "{v}");
        assert!(reason(&v).contains("inputs differ"), "{v}");
    }

    #[test]
    fn verdicts_differ_across_repeats_rejected() {
        let j = GOOD_JUDGE.replace(
            "a=$(cat",
            "if [ \"$CB_REPEAT\" = 1 ]; then echo 7 > \"$CB_SUBMISSION_DIR/answer\"; fi\na=$(cat",
        );
        let v = run(&j, "verdicts");
        assert_eq!(v["status"], "rejected", "{v}");
        assert!(reason(&v).contains("nondeterministic"), "{v}");
    }

    #[test]
    fn seed_override_used() {
        let root = tmp("seed");
        toy(&root, "toy", GOOD_JUDGE);
        let v = measure(&root, "toy", Source::Id("baseline"), false, Some(1234)).unwrap();
        assert_eq!(v["seed"], "1234");
        assert_eq!(v["inputs"], "seed-1234");
    }

    #[test]
    fn export_source_reads_gate_layout() {
        let root = tmp("exp");
        toy(&root, "toy", GOOD_JUDGE);
        let out = tmp("exp-out");
        let sub = out
            .join(root.file_name().unwrap())
            .join("toy/submissions/baseline");
        std::fs::create_dir_all(&sub).unwrap();
        for f in ["submission.toml", "build.sh", "answer"] {
            std::fs::copy(root.join("toy/submissions/baseline").join(f), sub.join(f)).unwrap();
        }
        std::fs::write(sub.join("answer"), "42\n").unwrap();
        let v = measure(&root, "toy", Source::Export(&out), false, None).unwrap();
        assert_eq!(v["status"], "valid", "{v}");
    }
}
