//! `cb preflight <slug>` (PLAN.md §3): mechanical checks that block launch.
//!
//! Runs on whatever host it's on and records that host, so the same command
//! is the laptop profile (`--local`, or the preflight workflow on
//! macos/ubuntu runners) and the hostable measurement.

use crate::challenge::Spec;
use crate::policy;
use crate::rubric::{Fact, HOSTED};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Broken candidates every challenge must ship, each of which must fail verify.
pub const BROKEN: [&str; 3] = ["off-by-one", "partial", "constant"];
/// provably.fast runs three no-change sessions (campaign_freeze.py:3201).
const AA_SESSIONS: usize = 3;
const TIMEOUT: Duration = Duration::from_secs(60 * 60);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Status {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug)]
pub struct Check {
    pub name: String,
    pub status: Status,
    pub detail: String,
}

#[derive(Debug)]
pub struct Report {
    pub slug: String,
    pub host: String,
    pub seed: u64,
    pub checks: Vec<Check>,
    /// This host's produce / verify / score, if the baseline got that far.
    pub facts: Option<[Fact; 3]>,
    /// verify+score on the baseline: minutes, max process RSS (GB), disk delta (GB).
    pub peaks: Option<(f64, f64, f64)>,
}

impl Report {
    pub fn blocked(&self) -> bool {
        self.checks.iter().any(|c| c.status == Status::Fail)
    }

    fn push(&mut self, name: &str, status: Status, detail: impl Into<String>) {
        self.checks.push(Check {
            name: name.into(),
            status,
            detail: detail.into(),
        });
    }

    pub fn json(&self) -> Value {
        let fact = |f: Fact| match f {
            Fact::Yes => "yes",
            Fact::YesSlow => "yes-slow",
            Fact::No => "no",
        };
        json!({
            "slug": self.slug,
            "host": self.host,
            "seed": self.seed.to_string(),
            "blocked": self.blocked(),
            "checks": self.checks.iter().map(|c| json!({
                "name": c.name, "status": format!("{:?}", c.status).to_lowercase(), "detail": c.detail,
            })).collect::<Vec<_>>(),
            "laptop": self.facts.map(|[p, v, s]| json!({"produce": fact(p), "verify": fact(v), "score": fact(s)})),
            "peaks": self.peaks.map(|(m, r, d)| json!({"minutes": m, "max_process_rss_gb": r, "disk_gb": d})),
        })
    }
}

/// OS, arch and CPU model of this machine.
pub fn host() -> String {
    let cpu = if cfg!(target_os = "macos") {
        cmd_out("sysctl", &["-n", "machdep.cpu.brand_string"])
    } else {
        std::fs::read_to_string("/proc/cpuinfo").ok().and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("model name"))
                .map(|l| l.split(':').nth(1).unwrap_or("").trim().to_string())
        })
    };
    let where_ = match std::env::var("RUNNER_OS") {
        Ok(os) => format!(
            "github-actions {os} {}",
            std::env::var("ImageVersion").unwrap_or_default()
        ),
        Err(_) => "local".into(),
    };
    format!(
        "{where_}; {} {}; {}; {} cpus",
        std::env::consts::OS,
        std::env::consts::ARCH,
        cpu.unwrap_or_default(),
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(0)
    )
}

fn cmd_out(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn seed() -> u64 {
    use std::io::Read;
    let mut b = [0u8; 8];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut b))
        .expect("/dev/urandom");
    u64::from_le_bytes(b)
}

struct Run {
    ok: bool,
    stdout: String,
    secs: f64,
    detail: String,
}

/// Runs `bash -c cmd` in `dir` with the seed in `CB_SEED`, killed at the timeout.
fn run(env: &[(String, String)], dir: &Path, cmd: &str, seed: u64) -> Run {
    let start = Instant::now();
    let child = Command::new("bash")
        .args(["-c", cmd])
        .current_dir(dir)
        .env("CB_SEED", seed.to_string())
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            return Run {
                ok: false,
                stdout: String::new(),
                secs: 0.0,
                detail: e.to_string(),
            }
        }
    };
    // ponytail: poll loop; stdout/stderr are read after exit, so a command
    // writing >64 KiB blocks until the timeout. Fine for JSON-sized output.
    let out = loop {
        match child.try_wait() {
            Ok(Some(_)) => break child.wait_with_output(),
            Ok(None) if start.elapsed() > TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Run {
                    ok: false,
                    stdout: String::new(),
                    secs: start.elapsed().as_secs_f64(),
                    detail: "timed out".into(),
                };
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => {
                return Run {
                    ok: false,
                    stdout: String::new(),
                    secs: 0.0,
                    detail: e.to_string(),
                }
            }
        }
    };
    let secs = start.elapsed().as_secs_f64();
    match out {
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            let tail: Vec<&str> = stderr.lines().rev().take(3).collect();
            Run {
                ok: o.status.success(),
                stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
                secs,
                detail: format!(
                    "{} {}",
                    o.status,
                    tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
                ),
            }
        }
        Err(e) => Run {
            ok: false,
            stdout: String::new(),
            secs,
            detail: e.to_string(),
        },
    }
}

/// Verify passes only on exit 0 with `{"ok": true}`.
fn verify_ok(r: &Run) -> bool {
    r.ok && serde_json::from_str::<Value>(r.stdout.trim())
        .ok()
        .and_then(|v| v["ok"].as_bool())
        == Some(true)
}

fn metrics(r: &Run) -> Option<Value> {
    let v: Value = serde_json::from_str(r.stdout.trim()).ok()?;
    v.get("metrics").filter(|m| m.is_object()).cloned()
}

fn max_child_rss_gb() -> f64 {
    // SAFETY: getrusage writes into the zeroed struct we pass.
    let mut u: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_CHILDREN, &mut u) };
    let bytes = if cfg!(target_os = "macos") {
        u.ru_maxrss as f64
    } else {
        u.ru_maxrss as f64 * 1024.0
    };
    bytes / 1e9
}

fn du_bytes(dir: &Path) -> u64 {
    cmd_out("du", &["-sk", &dir.to_string_lossy()])
        .and_then(|s| s.split_whitespace().next()?.parse::<u64>().ok())
        .map(|kb| kb * 1024)
        .unwrap_or(0)
}

/// Copies the challenge to a scratch dir with `with` in place of the editable surface.
fn workspace(dir: &Path, editable: &str, with: &Path, tag: &str) -> Result<PathBuf, String> {
    let ws = crate::paths::scratch(&format!("preflight-{tag}"));
    let ok = |s: std::process::ExitStatus| s.success();
    let cp = Command::new("cp")
        .arg("-R")
        .arg(dir)
        .arg(&ws)
        .status()
        .map_err(|e| e.to_string())?;
    let target = ws.join(editable);
    let _ = std::fs::remove_dir_all(&target);
    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    let fill = Command::new("cp")
        .arg("-R")
        .arg(format!("{}/.", with.display()))
        .arg(&target)
        .status()
        .map_err(|e| e.to_string())?;
    if !ok(cp) || !ok(fill) {
        return Err(format!(
            "copying {} into {}",
            with.display(),
            target.display()
        ));
    }
    Ok(ws)
}

fn fact(ok: bool, secs: f64) -> Fact {
    match (ok, secs) {
        (false, _) => Fact::No,
        (true, s) if s <= 600.0 => Fact::Yes,
        (true, s) if s <= 3600.0 => Fact::YesSlow,
        _ => Fact::No,
    }
}

/// Unanswered template lines: a `- question:` bullet with nothing after it,
/// or a `## section` with no text before the next heading.
fn threat_model_gaps(text: &str) -> Vec<String> {
    let mut gaps = Vec::new();
    let mut section: Option<(&str, bool)> = None;
    for line in text.lines().chain(std::iter::once("## end")) {
        let t = line.trim();
        if let Some(h) = t.strip_prefix("## ") {
            if let Some((name, false)) = section {
                gaps.push(format!("section '{name}' is empty"));
            }
            section = Some((h, false));
        } else if t.starts_with("- ") && t.ends_with(':') {
            gaps.push(format!("unanswered: {t}"));
        } else if !t.is_empty() && !t.starts_with('(') && !t.starts_with('#') {
            if let Some((_, filled)) = section.as_mut() {
                *filled = true;
            }
        }
    }
    gaps
}

pub fn run_all(dir: &Path) -> Result<Report, String> {
    let spec = Spec::load(dir)?;
    let seed = seed();
    // [perf] settings reach the harness as env (the Rust kit reads these).
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
    let mut r = Report {
        slug: spec.challenge.slug.clone(),
        host: host(),
        seed,
        checks: Vec::new(),
        facts: None,
        peaks: None,
    };
    let [editable] = &spec.paths.editable[..] else {
        return Err("preflight supports exactly one editable path for now".into());
    };
    let editable = editable.trim_end_matches('/');

    // Threat model: required and answered.
    match std::fs::read_to_string(dir.join("THREAT_MODEL.md")) {
        Err(_) => r.push("threat model", Status::Fail, "THREAT_MODEL.md missing"),
        Ok(t) => match threat_model_gaps(&t) {
            g if g.is_empty() => r.push(
                "threat model",
                Status::Pass,
                "every question answered (human review in PR)",
            ),
            g => r.push("threat model", Status::Fail, g.join("; ")),
        },
    }

    // Path policy works on this machine's git.
    match policy::self_test(editable) {
        Ok(()) => r.push(
            "path policy",
            Status::Pass,
            "non-editable edit, symlink and gitlink refused; edits inside allowed",
        ),
        Err(e) => r.push("path policy", Status::Fail, e),
    }

    // Rust kit: the baseline must pass the token policy it imposes.
    if let Some(rust) = &spec.rust {
        match crate::rustpolicy::check_dir(&dir.join("baseline"), &rust.allow) {
            Ok(()) => r.push(
                "rust policy",
                Status::Pass,
                format!("baseline clean; allowed: {:?}", rust.allow),
            ),
            Err(e) => r.push("rust policy", Status::Fail, e.join("; ")),
        }
    }
    let size = du_bytes(&dir.join("baseline"));
    if size > spec.paths.max_bytes {
        r.push(
            "size cap",
            Status::Fail,
            format!(
                "baseline is {size} bytes > max_bytes {}",
                spec.paths.max_bytes
            ),
        );
    }

    // Baseline: setup, build, verify, score. Measured for laptop facts and peaks.
    let base = workspace(dir, editable, &dir.join("baseline"), "baseline")?;
    let setup = run(&env, &base, &spec.commands.setup, seed);
    if !setup.ok {
        r.push("setup", Status::Fail, setup.detail);
        return Ok(r);
    }
    let build = spec
        .commands
        .build
        .as_ref()
        .map(|b| run(&env, &base, b, seed));
    if let Some(b) = build.as_ref().filter(|b| !b.ok) {
        r.push("baseline builds", Status::Fail, b.detail.clone());
        return Ok(r);
    }
    let disk0 = du_bytes(&base);
    let verify = run(&env, &base, &spec.commands.verify, seed);
    let score = run(&env, &base, &spec.commands.score, seed);
    let peaks = (
        (verify.secs + score.secs) / 60.0,
        max_child_rss_gb(),
        du_bytes(&base).saturating_sub(disk0) as f64 / 1e9,
    );
    r.peaks = Some(peaks);
    let base_metrics = metrics(&score);
    r.facts = Some([
        fact(
            build.as_ref().is_none_or(|b| b.ok),
            build.as_ref().map_or(0.0, |b| b.secs),
        ),
        fact(verify_ok(&verify), verify.secs),
        fact(base_metrics.is_some(), score.secs),
    ]);
    if verify_ok(&verify) {
        r.push(
            "baseline passes",
            Status::Pass,
            format!("verify {:.1}s", verify.secs),
        );
    } else {
        r.push(
            "baseline passes",
            Status::Fail,
            format!("{} {}", verify.stdout.trim(), verify.detail),
        );
    }
    if spec.inputs.salted {
        let fresh = self::seed();
        let v2 = run(&env, &base, &spec.commands.verify, fresh);
        let status = if verify_ok(&v2) {
            Status::Pass
        } else {
            Status::Fail
        };
        r.push(
            "salted inputs",
            status,
            format!("baseline verify with a second seed ({fresh})"),
        );
    }
    let expr = &spec.objective.expr;
    let value = base_metrics
        .as_ref()
        .and_then(|m| m.get(expr))
        .and_then(Value::as_f64);
    if let Some(g) = base_metrics
        .as_ref()
        .and_then(|m| m.get("guard"))
        .and_then(Value::as_str)
    {
        let status = if g == "pass" {
            Status::Pass
        } else {
            Status::Warn
        };
        r.push(
            "wall-time guard",
            status,
            format!("A/A guard on the baseline: {g}"),
        );
    }
    match value {
        Some(v) => r.push(
            "baseline scores",
            Status::Pass,
            format!("{expr} = {v} in {:.1}s", score.secs),
        ),
        None => r.push(
            "baseline scores",
            Status::Fail,
            format!(
                "no numeric metrics.{expr} in score output: {} {}",
                score.stdout.trim(),
                score.detail
            ),
        ),
    }

    // Deliberately broken candidates must build and then fail verify.
    for name in BROKEN {
        let src = dir.join("preflight/broken").join(name);
        if !src.is_dir() {
            r.push(
                &format!("broken: {name}"),
                Status::Fail,
                format!("missing {}", src.display()),
            );
            continue;
        }
        let ws = workspace(dir, editable, &src, name)?;
        let b = spec
            .commands
            .build
            .as_ref()
            .map(|b| run(&env, &ws, b, seed));
        let v = run(&env, &ws, &spec.commands.verify, seed);
        let (status, detail) = match (b.as_ref().is_none_or(|b| b.ok), verify_ok(&v)) {
            (false, _) => (
                Status::Fail,
                "doesn't build, so it doesn't test the verifier".to_string(),
            ),
            (true, true) => (
                Status::Fail,
                "verifier ACCEPTED a broken candidate".to_string(),
            ),
            (true, false) => (Status::Pass, "rejected".to_string()),
        };
        r.push(&format!("broken: {name}"), status, detail);
        let _ = std::fs::remove_dir_all(&ws);
    }

    let bips = spec.objective.min_improvement_bips as f64 / 10_000.0;
    if spec.perf.is_some() {
        // A/A: baseline scored as the candidate must stay inside the bar.
        let mut values = Vec::new();
        for _ in 0..AA_SESSIONS {
            match metrics(&run(&env, &base, &spec.commands.score, self::seed()))
                .and_then(|m| m.get(expr).and_then(Value::as_f64))
            {
                Some(v) => values.push(v),
                None => break,
            }
        }
        if values.len() < AA_SESSIONS {
            r.push(
                "A/A band",
                Status::Fail,
                "a no-change session produced no score",
            );
        } else {
            let noise = values.iter().map(|v| (v - 1.0).abs()).fold(0.0, f64::max);
            let inside = values
                .iter()
                .all(|v| *v > 1.0 / (1.0 + bips) && *v < 1.0 + bips);
            r.push(
                "A/A band",
                if inside { Status::Pass } else { Status::Fail },
                format!("{AA_SESSIONS} no-change {expr}: {values:?}; must stay inside (1/(1+m), 1+m), m = {bips}"),
            );
            // Advisory (PLAN.md §3): the maker's call, but warn under ~2× noise.
            let status = if bips < 2.0 * noise {
                Status::Warn
            } else {
                Status::Pass
            };
            r.push(
                "noise floor",
                status,
                format!(
                    "max |A/A − 1| = {noise:.4}; min improvement {bips} vs 2× noise {:.4}",
                    2.0 * noise
                ),
            );
        }
    } else {
        // Determinism: same seed, same metrics.
        let again = metrics(&run(&env, &base, &spec.commands.score, seed));
        let v1 = verify_ok(&run(&env, &base, &spec.commands.verify, seed));
        match (&base_metrics, &again) {
            (Some(a), Some(b)) if a == b && v1 => r.push(
                "determinism",
                Status::Pass,
                "same seed, same metrics and verdict",
            ),
            _ => r.push(
                "determinism",
                Status::Fail,
                format!("seed {seed}: {base_metrics:?} then {again:?}"),
            ),
        }
    }

    // Hostable here: advisory, the evaluator records it.
    let (minutes, ram, disk) = peaks;
    let hostable = minutes <= HOSTED.minutes && ram <= HOSTED.ram_gb && disk <= HOSTED.disk_gb;
    r.push(
        "hostable",
        if hostable { Status::Pass } else { Status::Warn },
        format!("verify+score {minutes:.1} min, {ram:.2} GB max process RSS, {disk:.2} GB disk (limits 60 min / 8 GB / 14 GB)"),
    );
    let _ = std::fs::remove_dir_all(&base);
    Ok(r)
}

pub fn render(r: &Report) -> String {
    let mut out = format!("preflight {} on {}\nseed {}\n", r.slug, r.host, r.seed);
    for c in &r.checks {
        let mark = match c.status {
            Status::Pass => "ok  ",
            Status::Warn => "WARN",
            Status::Fail => "FAIL",
        };
        out += &format!("{mark} {:<18} {}\n", c.name, c.detail);
    }
    out += if r.blocked() {
        "\nBLOCKED: fix every FAIL before launch\n"
    } else {
        "\nready (WARN is advisory)\n"
    };
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::challenge::scaffold;

    const TM: &str = "# Threat model\n\n## How could a wrong answer pass the verifier?\n\n- Finite inputs: fresh per run\n\n## Interface\n\nmatches\n\n## Headroom\n\nunknown, see #6\n\n## Outlier\n\nmanual review by maintainer\n";

    /// A toy challenge: the answer file must hold 42; score reports a metric.
    fn toy(family: &str, verifier_accepts_anything: bool) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "cb-pf-{}-{family}-{verifier_accepts_anything}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let dir = scaffold(&root, "toy", family, None).unwrap();
        let w = |rel: &str, body: &str| {
            let p = dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, body).unwrap();
        };
        w("THREAT_MODEL.md", TM);
        let check = if verifier_accepts_anything {
            "true"
        } else {
            "[ \"$(cat candidate/answer)\" = 42 ]"
        };
        w("verifier/run", &format!("if {check}; then echo '{{\"ok\": true}}'; else echo '{{\"ok\": false}}'; exit 1; fi\n"));
        let metric = if family == "perf" {
            "\"speedup\": 1.0"
        } else {
            "\"count\": 7"
        };
        w(
            "bench/run",
            &format!("echo '{{\"metrics\": {{{metric}}}}}'\n"),
        );
        w("build.sh", "true\n");
        w("baseline/answer", "42\n");
        w("preflight/broken/off-by-one/answer", "41\n");
        w("preflight/broken/partial/answer", "4\n");
        w("preflight/broken/constant/answer", "0\n");
        dir
    }

    fn status(r: &Report, name: &str) -> Status {
        r.checks
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("{name}: {r:?}"))
            .status
    }

    #[test]
    fn toy_resource_passes() {
        let r = run_all(&toy("resource", false)).unwrap();
        assert!(!r.blocked(), "{}", render(&r));
        assert_eq!(status(&r, "determinism"), Status::Pass);
        assert_eq!(r.facts, Some([Fact::Yes; 3]));
    }

    #[test]
    fn toy_perf_aa() {
        let r = run_all(&toy("perf", false)).unwrap();
        assert!(!r.blocked(), "{}", render(&r));
        assert_eq!(status(&r, "A/A band"), Status::Pass);
        assert_eq!(status(&r, "noise floor"), Status::Pass); // exactly 1.0, zero noise
    }

    #[test]
    fn lax_verifier_is_blocked() {
        let r = run_all(&toy("resource", true)).unwrap();
        assert!(r.blocked());
        for name in BROKEN {
            assert_eq!(status(&r, &format!("broken: {name}")), Status::Fail);
        }
    }

    #[test]
    fn scaffold_fails_closed() {
        let root = std::env::temp_dir().join(format!("cb-pf-{}-raw", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let r = run_all(&scaffold(&root, "raw", "math", None).unwrap()).unwrap();
        assert!(r.blocked());
        assert_eq!(status(&r, "threat model"), Status::Fail);
        assert_eq!(status(&r, "baseline passes"), Status::Fail);
    }

    /// A fresh `cb new --kit rust` challenge passes preflight once its threat
    /// model is answered. Scoring needs valgrind (Linux); without it only the
    /// score checks are skipped.
    #[test]
    fn rust_kit_toy_passes() {
        let root = crate::paths::scratch("kit");
        let dir = scaffold(&root, "toy", "perf", Some("rust")).unwrap();
        std::fs::write(dir.join("THREAT_MODEL.md"), TM).unwrap();
        let r = run_all(&dir).unwrap();
        let text = render(&r);
        for name in [
            "path policy",
            "rust policy",
            "baseline passes",
            "salted inputs",
        ] {
            assert_eq!(status(&r, name), Status::Pass, "{name}\n{text}");
        }
        for name in BROKEN {
            assert_eq!(
                status(&r, &format!("broken: {name}")),
                Status::Pass,
                "{name}\n{text}"
            );
        }
        if cmd_out("valgrind", &["--version"]).is_some() {
            assert!(!r.blocked(), "{text}");
            assert_eq!(status(&r, "A/A band"), Status::Pass, "{text}"); // instruction ratio is exactly 1
        } else {
            eprintln!("valgrind not found: skipped score checks\n{text}");
        }
    }

    #[test]
    fn threat_model_template_is_unanswered() {
        assert!(!threat_model_gaps(crate::challenge::THREAT_MODEL).is_empty());
        assert!(threat_model_gaps(TM).is_empty());
    }
}
