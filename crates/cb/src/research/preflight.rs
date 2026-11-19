//! `cb preflight` for research challenges (design spec §7): the launch gate.
//! Proves the judge rejects broken submissions, ignores self-reported numbers,
//! is deterministic and salted, and that axis tolerances cover measured noise.

use super::measure::{measure, Source};
use super::spec::{Declaration, ResearchSpec};
use super::util::{copy_challenge, str_of, verdicts};
use crate::challenge::BuildSandbox;
use crate::preflight::{self, Mode, Report, Status};
use crate::rubric::HOSTED;
use serde_json::Value;
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

const REQUIRED: [&str; 4] = ["off-by-one", "partial", "constant", "self-report"];

fn field(v: &Value, k: &str) -> String {
    str_of(&v[k]).to_string()
}

fn valid(m: &Value) -> bool {
    m["status"] == "valid"
}

/// `"status: reason"` for a measurement.
fn why(m: &Value) -> String {
    format!("{}: {}", field(m, "status"), field(m, "reason"))
}

fn median_of(m: &Value, axis: &str) -> Option<f64> {
    m["axes"][axis]["median"].as_f64()
}

/// Relative spread of three medians; absolute when their median is 0.
fn spread(v: [f64; 3]) -> f64 {
    let mut s = v;
    s.sort_by(|a, b| a.total_cmp(b));
    let range = s[2] - s[0];
    if range == 0.0 {
        0.0
    } else if s[1] == 0.0 {
        range
    } else {
        range / s[1].abs()
    }
}

fn ceil_cent(x: f64) -> f64 {
    ((x * 100.0) - 1e-9).ceil().max(0.0) / 100.0
}

/// Every submission directory with its declaration (None if unreadable).
fn declarations(dir: &Path) -> Vec<(String, Option<Declaration>)> {
    let mut v: Vec<_> = std::fs::read_dir(dir.join("submissions"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| {
            let id = e.file_name().to_string_lossy().into_owned();
            let d = std::fs::read_to_string(e.path().join("submission.toml"))
                .ok()
                .and_then(|t| Declaration::parse(&t).ok());
            (id, d)
        })
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

fn is_exec(p: &Path) -> bool {
    p.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

fn contracts_check(r: &mut Report, dir: &Path, spec: &ResearchSpec) {
    let mut problems = Vec::new();
    let mut deferred = Vec::new();
    let mut todo = false;
    for k in &spec.contracts {
        if k.status == "deferred" {
            let w = k.admits_when.clone().unwrap_or_default();
            todo |= w.trim_start().starts_with("TODO");
            deferred.push(format!("{} (admits when: {w})", k.id));
        }
        if k.status != "open" {
            continue;
        }
        if let Some(j) = &k.judge {
            let p = dir.join(j);
            if !p.is_file() {
                problems.push(format!("{}: judge {j} missing", k.id));
            } else if !is_exec(&p) {
                problems.push(format!("{}: judge {j} not executable", k.id));
            }
        }
        for f in [&k.interface, &k.choice_space].into_iter().flatten() {
            if !dir.join(f).is_file() {
                problems.push(format!("{}: {f} missing", k.id));
            }
        }
    }
    let listed = if deferred.is_empty() {
        "no deferred contracts".to_string()
    } else {
        format!("deferred: {}", deferred.join("; "))
    };
    if !problems.is_empty() {
        r.push("contracts", Status::Fail, problems.join("; "));
    } else if todo {
        r.push(
            "contracts",
            Status::Warn,
            format!("{listed}; an admits_when is still TODO"),
        );
    } else {
        r.push(
            "contracts",
            Status::Pass,
            format!("open contracts' judge, interface and choice space exist; {listed}"),
        );
    }
}

fn taxonomy_check(r: &mut Report, dir: &Path, spec: &ResearchSpec) {
    let mut bad = Vec::new();
    for (id, d) in declarations(dir) {
        match d {
            None => bad.push(format!("{id}: submission.toml missing or unreadable")),
            Some(d) => {
                for k in d.taxonomy.keys() {
                    if !spec.tax.axes.iter().any(|a| &a.id == k) {
                        bad.push(format!("{id}: taxonomy key {k:?} unknown"));
                    }
                }
            }
        }
    }
    if bad.is_empty() {
        r.push(
            "taxonomy",
            Status::Pass,
            "every submission names only taxonomy axes",
        );
    } else {
        r.push("taxonomy", Status::Fail, bad.join("; "));
    }
}

fn sandbox_check(r: &mut Report, dir: &Path, spec: &ResearchSpec, seed: u64) -> Result<(), String> {
    if !preflight::build_sandbox_available() {
        let ci = std::env::var_os("CI").is_some();
        r.push(
            "build sandbox",
            if ci { Status::Fail } else { Status::Warn },
            "not on Linux with passwordless sudo, unshare and setpriv: builds ran unsandboxed here",
        );
        return Ok(());
    }
    let ws = copy_challenge(dir, "pf-sandbox")?;
    let bs = BuildSandbox {
        writable: spec.submissions.writable.clone(),
        outputs: vec!["submissions/baseline".into()],
    };
    let home = std::env::var("HOME").unwrap_or_default();
    let persist = bs.writable.first().map(|w| {
        w.strip_prefix("~/")
            .map(|x| Path::new(&home).join(x))
            .unwrap_or_else(|| w.into())
            .join(".cb-overlay-probe")
    });
    let p_str = persist
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let probe = format!(
        "fail() {{ echo \"probe: $1\" >&2; exit 1; }}; \
         touch contracts/.cb-probe 2>/dev/null && fail 'contracts writable'; \
         touch \"$HOME/.cb-probe\" 2>/dev/null && fail 'home writable'; \
         sudo -n true 2>/dev/null && fail 'sudo works'; \
         (exec 3<>/dev/tcp/1.1.1.1/443) 2>/dev/null && fail 'network reachable'; \
         : > submissions/baseline/.cb-probe || fail 'submission dir not writable'; \
         [ -z {w} ] || {{ : > {w} && [ -e {w} ]; }} || fail 'cache dir not writable inside'; \
         exit 0",
        w = preflight::sq(&p_str)
    );
    let p = preflight::run_mode(Mode::Build(&bs), &[], &ws, &probe, seed);
    let leaked = persist.as_ref().is_some_and(|p| p.exists());
    let _ = std::fs::remove_dir_all(&ws);
    if leaked {
        r.push(
            "build sandbox",
            Status::Fail,
            format!("a write to {p_str} outlived the build"),
        );
    } else if !p.ok {
        r.push(
            "build sandbox",
            Status::Fail,
            format!("probe escaped or failed: {}", p.detail),
        );
    } else {
        r.push(
            "build sandbox",
            Status::Pass,
            "contracts and home read-only, no sudo, no network; submission dir writable; cache writes discarded",
        );
    }
    Ok(())
}

pub fn run_all(dir: &Path) -> Result<Report, String> {
    let spec = ResearchSpec::load(dir)?;
    let slug = spec.challenge.slug.clone();
    let root = dir.parent().ok_or("challenge dir has no parent")?;
    let sandbox = preflight::build_sandbox_available();
    let seed_a = preflight::seed();
    let seed_b = seed_a ^ 0x9e37_79b9_7f4a_7c15;
    let mut r = Report {
        slug: slug.clone(),
        host: preflight::host(),
        seed: seed_a,
        checks: Vec::new(),
        facts: None,
        peaks: None,
    };

    match std::fs::read_to_string(dir.join("THREAT_MODEL.md")) {
        Err(_) => r.push("threat model", Status::Fail, "THREAT_MODEL.md missing"),
        Ok(t) => match preflight::threat_model_gaps(&t) {
            g if g.is_empty() => r.push(
                "threat model",
                Status::Pass,
                "every question answered (human review in PR)",
            ),
            g => r.push("threat model", Status::Fail, g.join("; ")),
        },
    }
    contracts_check(&mut r, dir, &spec);
    taxonomy_check(&mut r, dir, &spec);
    sandbox_check(&mut r, dir, &spec, seed_a)?;

    // Baseline.
    let m = |src: Source, seed: u64| measure(root, &slug, src, sandbox, Some(seed));
    let base_decl = std::fs::read_to_string(dir.join("submissions/baseline/submission.toml"))
        .map_err(|e| e.to_string())
        .and_then(|t| Declaration::parse(&t));
    let s1 = m(Source::Id("baseline"), seed_a);
    let (decl, s1) = match (base_decl, s1) {
        (Ok(d), Ok(s)) if valid(&s) => (d, s),
        (Err(e), _) => {
            r.push(
                "baseline",
                Status::Fail,
                format!("submissions/baseline/submission.toml: {e}"),
            );
            return Ok(r);
        }
        (_, Err(e)) => {
            r.push("baseline", Status::Fail, e);
            return Ok(r);
        }
        (_, Ok(s)) => {
            r.push("baseline", Status::Fail, why(&s));
            return Ok(r);
        }
    };
    let mut problems = Vec::new();
    if decl.id != "baseline" {
        problems.push(format!("id is {:?}, not baseline", decl.id));
    }
    if decl.novelty != "new" {
        problems.push("novelty must be new".to_string());
    }
    let size = preflight::tree_bytes(&dir.join("submissions/baseline"));
    if size > spec.submissions.max_bytes {
        problems.push(format!(
            "{size} bytes > max_bytes {}",
            spec.submissions.max_bytes
        ));
    }
    let decls = declarations(dir);
    let mut covered = vec![decl.contract.clone()];
    for k in spec.contracts.iter().filter(|k| k.status == "open") {
        if k.id == decl.contract {
            continue;
        }
        let owner = decls
            .iter()
            .find(|(_, d)| d.as_ref().is_some_and(|d| d.contract == k.id));
        match owner {
            None => problems.push(format!("no submission declares open contract {}", k.id)),
            Some((id, _)) => match m(Source::Id(id), seed_a) {
                Ok(v) if valid(&v) => covered.push(k.id.clone()),
                Ok(v) => problems.push(format!("submissions/{id} ({}): {}", k.id, why(&v))),
                Err(e) => problems.push(format!("submissions/{id}: {e}")),
            },
        }
    }
    if problems.is_empty() {
        r.push(
            "baseline",
            Status::Pass,
            format!(
                "valid, {size} bytes; open contracts measured valid: {}",
                covered.join(", ")
            ),
        );
    } else {
        r.push("baseline", Status::Fail, problems.join("; "));
    }

    // Determinism, salting, noise: seeds A, A, B.
    let s2 = m(Source::Id("baseline"), seed_a)?;
    let s3 = m(Source::Id("baseline"), seed_b)?;
    if valid(&s2) && s1["inputs"] == s2["inputs"] && verdicts(&s1) == verdicts(&s2) {
        r.push(
            "determinism",
            Status::Pass,
            "same seed, same inputs and verdicts",
        );
    } else if !valid(&s2) {
        r.push(
            "determinism",
            Status::Fail,
            format!("second run {}", why(&s2)),
        );
    } else {
        r.push(
            "determinism",
            Status::Fail,
            format!(
                "seed {seed_a}: inputs {} then {}; verdicts {:?} then {:?}",
                s1["inputs"],
                s2["inputs"],
                verdicts(&s1),
                verdicts(&s2)
            ),
        );
    }
    if !valid(&s3) {
        r.push(
            "salted inputs",
            Status::Fail,
            format!("seed B {}", why(&s3)),
        );
    } else if s3["inputs"] == s1["inputs"] {
        r.push(
            "salted inputs",
            Status::Fail,
            format!(
                "seeds {seed_a} and {seed_b} gave identical inputs {}",
                s1["inputs"]
            ),
        );
    } else {
        r.push(
            "salted inputs",
            Status::Pass,
            "a different seed gives different inputs, still valid",
        );
    }
    let mut spreads: BTreeMap<String, f64> = BTreeMap::new();
    for a in spec.axes_for(&decl.contract) {
        let meds = [&s1, &s2, &s3].map(|s| median_of(s, &a.id));
        let name = format!("axis noise: {}", a.id);
        let [Some(x), Some(y), Some(z)] = meds else {
            r.push(
                &name,
                Status::Fail,
                "axis missing from a baseline measurement",
            );
            continue;
        };
        let sp = spread([x, y, z]);
        spreads.insert(a.id.clone(), sp);
        let detail = format!(
            "medians {x}, {y}, {z}; spread {sp:.4} vs tolerance {}; suggest tolerance >= {:.2}",
            a.tolerance,
            ceil_cent(2.0 * sp)
        );
        let st = if sp > a.tolerance {
            Status::Fail
        } else {
            Status::Pass
        };
        r.push(&name, st, detail);
    }

    // Broken variants.
    let mut names: Vec<String> = REQUIRED.iter().map(|s| s.to_string()).collect();
    let mut extra: Vec<String> = std::fs::read_dir(dir.join("preflight/broken"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !names.contains(n))
        .collect();
    extra.sort();
    names.extend(extra);
    for name in names {
        let label = format!("broken: {name}");
        let vdir = dir.join("preflight/broken").join(&name);
        if !vdir.is_dir() {
            r.push(
                &label,
                Status::Fail,
                "missing: ship this variant under preflight/broken/",
            );
            continue;
        }
        let v = match m(Source::Dir(&vdir), seed_a) {
            Ok(v) => v,
            Err(e) => {
                r.push(&label, Status::Fail, e);
                continue;
            }
        };
        if v["build"]["ok"] != true
            && v["status"] == "rejected"
            && field(&v, "reason").starts_with("build")
        {
            r.push(
                &label,
                Status::Fail,
                format!(
                    "does not build, so it doesn't test the judge ({})",
                    field(&v, "reason")
                ),
            );
        } else if v["status"] == "rejected" && v["build"]["ok"] != true {
            r.push(
                &label,
                Status::Fail,
                format!(
                    "rejected before the judge ran ({}), so it doesn't test the judge",
                    field(&v, "reason")
                ),
            );
        } else if name == "self-report" {
            if !valid(&v) {
                r.push(&label, Status::Fail, format!("must be valid: {}", why(&v)));
                continue;
            }
            let mut off = Vec::new();
            for a in spec.axes_for(&decl.contract) {
                let allow = a
                    .tolerance
                    .max(2.0 * spreads.get(&a.id).copied().unwrap_or(0.0));
                let (Some(b), Some(x)) = (median_of(&s1, &a.id), median_of(&v, &a.id)) else {
                    off.push(format!("{}: missing", a.id));
                    continue;
                };
                let rel = if b == x {
                    0.0
                } else if b == 0.0 {
                    f64::INFINITY
                } else {
                    (x - b).abs() / b.abs()
                };
                if rel > allow {
                    off.push(format!(
                        "{}: median {x} vs baseline {b} (off by {rel:.3}, allowed {allow:.3})",
                        a.id
                    ));
                }
            }
            if off.is_empty() {
                r.push(
                    &label,
                    Status::Pass,
                    "valid, axes match the baseline: self-reported numbers ignored",
                );
            } else {
                r.push(&label, Status::Fail, off.join("; "));
            }
        } else {
            match field(&v, "status").as_str() {
                "rejected" => r.push(
                    &label,
                    Status::Pass,
                    format!("rejected: {}", field(&v, "reason")),
                ),
                "infra_failed" => r.push(
                    &label,
                    Status::Fail,
                    format!("infra_failed: {}", field(&v, "reason")),
                ),
                _ => r.push(
                    &label,
                    Status::Fail,
                    "the judge accepted a broken submission",
                ),
            }
        }
    }

    // Hostable.
    let minutes = (s1["build"]["secs"].as_f64().unwrap_or(0.0)
        + s1["judge_secs"].as_f64().unwrap_or(0.0))
        / 60.0;
    let ram = preflight::max_child_rss_gb();
    let disk = preflight::tree_bytes(dir) as f64 / 1e9;
    let ok = minutes <= HOSTED.minutes && ram <= HOSTED.ram_gb && disk <= HOSTED.disk_gb;
    r.peaks = Some((minutes, ram, disk));
    r.push(
        "hostable",
        if ok { Status::Pass } else { Status::Warn },
        format!("build+judge {minutes:.2} min, {ram:.2} GB max process RSS, {disk:.2} GB disk (limits 60 min / 8 GB / 14 GB)"),
    );
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths;
    use crate::preflight::Status;
    use crate::research::testkit::{broken, toy, toy_ready, GOOD_JUDGE};

    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = paths::scratch(&format!("pf-{tag}"));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn check<'a>(r: &'a Report, name: &str) -> &'a crate::preflight::Check {
        r.checks
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("no check {name}: {:?}", r.checks))
    }
    fn fails(r: &Report) -> Vec<String> {
        r.checks
            .iter()
            .filter(|c| c.status == Status::Fail)
            .map(|c| format!("{}: {}", c.name, c.detail))
            .collect()
    }

    #[test]
    fn toy_preflight_passes() {
        let dir = toy_ready(&tmp("ok"), "toy", GOOD_JUDGE);
        let r = run_all(&dir).unwrap();
        assert!(!r.blocked(), "{:?}", fails(&r));
        for n in [
            "threat model",
            "contracts",
            "taxonomy",
            "build sandbox",
            "baseline",
            "determinism",
            "salted inputs",
            "axis noise: time_ms",
            "axis noise: size_bytes",
            "broken: off-by-one",
            "broken: self-report",
            "hostable",
        ] {
            check(&r, n);
        }
        assert!(r.peaks.is_some() && r.facts.is_none());
    }

    #[test]
    fn broken_variant_accepted_blocks() {
        let dir = toy_ready(&tmp("acc"), "toy", GOOD_JUDGE);
        broken(&dir, "off-by-one", "42");
        let r = run_all(&dir).unwrap();
        assert_eq!(check(&r, "broken: off-by-one").status, Status::Fail);
        assert!(r.blocked());
    }

    #[test]
    fn missing_required_variant_blocks() {
        let dir = toy_ready(&tmp("miss"), "toy", GOOD_JUDGE);
        std::fs::remove_dir_all(dir.join("preflight/broken/partial")).unwrap();
        let r = run_all(&dir).unwrap();
        let c = check(&r, "broken: partial");
        assert_eq!(c.status, Status::Fail);
        assert!(c.detail.contains("missing"));
    }

    #[test]
    fn self_report_far_from_baseline_blocks() {
        let dir = toy_ready(&tmp("sr"), "toy", GOOD_JUDGE);
        // Valid, but the judge measures something else for it: size_bytes 42 -> 1.
        let judge = GOOD_JUDGE
            .replace(
                "a=$(cat \"$CB_SUBMISSION_DIR/answer\")",
                "a=$(cat \"$CB_SUBMISSION_DIR/answer\"); s=$a; [ -e \"$CB_SUBMISSION_DIR/self-report.txt\" ] && s=1",
            )
            .replace("\"$ok\" \"$a\"", "\"$ok\" \"$s\"");
        assert!(judge.contains("\"$s\""));
        std::fs::write(dir.join("contracts/main/judge"), judge).unwrap();
        let r = run_all(&dir).unwrap();
        assert_eq!(check(&r, "broken: self-report").status, Status::Fail);
    }

    #[test]
    fn noisy_axis_over_tolerance_blocks() {
        // time_ms grows with every judge run, counted in a file outside the workspace.
        let counter = tmp("noisy-count").join("n");
        let judge = GOOD_JUDGE
            .replace(
                "a=$(cat",
                &format!(
                    "c={c}; n=$(( $(cat $c 2>/dev/null || echo 0) + 1 )); echo $n > $c; a=$(cat",
                    c = counter.display()
                ),
            )
            .replace("\"time_ms\": 1.5", "\"time_ms\": '\"$((n * 10))\"'");
        let dir = toy_ready(&tmp("noisy"), "toy", &judge);
        let r = run_all(&dir).unwrap();
        let c = check(&r, "axis noise: time_ms");
        assert_eq!(c.status, Status::Fail, "{}", c.detail);
        assert!(c.detail.contains("suggest tolerance >="), "{}", c.detail);
    }

    #[test]
    fn variant_rejected_before_judge_blocks() {
        let dir = toy_ready(&tmp("pre"), "toy", GOOD_JUDGE);
        let p = dir.join("preflight/broken/constant/submission.toml");
        let t = std::fs::read_to_string(&p).unwrap();
        std::fs::write(&p, t.replace("id = \"constant\"", "id = \"other\"")).unwrap();
        let r = run_all(&dir).unwrap();
        let c = check(&r, "broken: constant");
        assert_eq!(c.status, Status::Fail, "{}", c.detail);
        assert!(c.detail.contains("before the judge ran"), "{}", c.detail);
    }

    #[test]
    fn deferred_listed() {
        let dir = toy_ready(&tmp("def"), "toy", GOOD_JUDGE);
        let r = run_all(&dir).unwrap();
        let c = check(&r, "contracts");
        assert!(
            c.detail.contains("streaming@0 (admits when:"),
            "{}",
            c.detail
        );
        assert_eq!(c.status, Status::Warn); // the scaffold's TODO
    }

    #[test]
    fn scaffold_blocks_until_written() {
        let root = tmp("scaf");
        let dir = crate::research::scaffold::scaffold(&root, "toy").unwrap();
        let r = run_all(&dir).unwrap();
        assert!(r.blocked());
        assert_eq!(check(&r, "threat model").status, Status::Fail);
    }

    #[test]
    fn unsalted_judge_blocks() {
        let judge = GOOD_JUDGE.replace("seed-%s", "fixed%.0s");
        let dir = toy_ready(&tmp("salt"), "toy", &judge);
        let r = run_all(&dir).unwrap();
        assert_eq!(check(&r, "salted inputs").status, Status::Fail);
    }

    #[test]
    fn unknown_taxonomy_key_blocks() {
        let dir = toy_ready(&tmp("tax"), "toy", GOOD_JUDGE);
        let p = dir.join("submissions/baseline/submission.toml");
        let t = std::fs::read_to_string(&p).unwrap();
        std::fs::write(&p, t.replace("approach", "bogus")).unwrap();
        let r = run_all(&dir).unwrap();
        assert_eq!(check(&r, "taxonomy").status, Status::Fail);
    }

    #[test]
    fn missing_judge_blocks() {
        let dir = toy(&tmp("nj"), "toy", GOOD_JUDGE);
        std::fs::remove_file(dir.join("contracts/main/judge")).unwrap();
        let r = run_all(&dir).unwrap();
        assert_eq!(check(&r, "contracts").status, Status::Fail);
    }
}
