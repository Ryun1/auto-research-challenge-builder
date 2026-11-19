//! `cb preflight <slug>` (PLAN.md §3): mechanical checks that block launch.
//!
//! Runs on whatever host it's on and records that host, so the same command
//! is the laptop profile (`--local`, or the preflight workflow on
//! macos/ubuntu runners) and the hostable measurement.

use crate::challenge::BuildSandbox;
use crate::challenge::Spec;
use crate::policy;
use crate::rubric::{Fact, HOSTED};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Broken candidates every challenge must ship, each of which must fail verify.
/// A challenge may add more under `preflight/broken/` (see `broken_variants`).
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

    pub(crate) fn push(&mut self, name: &str, status: Status, detail: impl Into<String>) {
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

/// The host as the toolkit's `check()` reads it (host_policy keys).
pub fn host_policy() -> Value {
    let cpu = if cfg!(target_os = "macos") {
        cmd_out("sysctl", &["-n", "machdep.cpu.brand_string"])
    } else {
        std::fs::read_to_string("/proc/cpuinfo").ok().and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("model name"))
                .map(|l| l.split(':').nth(1).unwrap_or("").trim().to_string())
        })
    };
    let memory_bytes = if cfg!(target_os = "macos") {
        cmd_out("sysctl", &["-n", "hw.memsize"]).and_then(|s| s.parse::<u64>().ok())
    } else {
        std::fs::read_to_string("/proc/meminfo").ok().and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("MemTotal:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse::<u64>().ok())
                .map(|kb| kb * 1024)
        })
    };
    json!({
        "cpu_model": cpu,
        "logical_cpu_count": std::thread::available_parallelism().map(|n| n.get()).ok(),
        "memory_bytes": memory_bytes,
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    })
}

pub(crate) fn cmd_out(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub(crate) fn seed() -> u64 {
    use std::io::Read;
    let mut b = [0u8; 8];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut b))
        .expect("/dev/urandom");
    u64::from_le_bytes(b)
}

pub(crate) struct Run {
    pub ok: bool,
    pub stdout: String,
    pub secs: f64,
    pub detail: String,
}

/// Runs `bash -c cmd` in `dir` with the seed in `CB_SEED`, killed at the timeout.
fn run(env: &[(String, String)], dir: &Path, cmd: &str, seed: u64) -> Run {
    run_in(false, env, dir, cmd, seed)
}

/// With `sandbox`, the command runs as `nobody` (uid 65534) with no network,
/// via `sudo unshare --net` + `setpriv` (Linux runners have passwordless sudo).
/// `dir` must be readable by `nobody`; it can't write there.
pub(crate) fn run_in(
    sandbox: bool,
    env: &[(String, String)],
    dir: &Path,
    cmd: &str,
    seed: u64,
) -> Run {
    let mode = if sandbox { Mode::Judge } else { Mode::Plain };
    run_mode(mode, env, dir, cmd, seed)
}

/// How a challenge command runs.
pub(crate) enum Mode<'a> {
    /// As the current user (setup; builds outside CI).
    Plain,
    /// Verify/score: as `nobody`, no network (see [`run_in`]).
    Judge,
    /// The build in CI (#51): as the current user, so caches and toolchains
    /// work, but in a private mount namespace where `$HOME`, `/opt`,
    /// `/usr/local` and the workspace are read-only except the declared
    /// writable dirs and outputs. No network, its own pid namespace and /proc
    /// (nothing outlives it), no supplementary groups (docker), no_new_privs
    /// (no sudo), and `/run/user` hidden (no systemd/dbus user session). So
    /// it can't touch the checkout, the runner's action code, `$GITHUB_ENV`
    /// files or the reference binary that later trusted steps rely on.
    Build(&'a BuildSandbox),
}

/// Linux with passwordless sudo, unshare and setpriv (CI runners).
pub(crate) fn build_sandbox_available() -> bool {
    cfg!(target_os = "linux")
        && Command::new("sudo")
            .args(["-n", "sh", "-c", "command -v unshare && command -v setpriv"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
}

/// Single-quotes a string for bash.
pub(crate) fn sq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// The mount script for [`Mode::Build`]: runs as root inside the new
/// namespaces, then drops to the caller's uid.
pub(crate) fn build_script(
    b: &BuildSandbox,
    dir: &Path,
    home: &str,
    uid: u32,
    gid: u32,
    vars: &[(String, String)],
    cmd: &str,
) -> String {
    let expand = |p: &str| match p.strip_prefix("~/") {
        Some(r) => format!("{home}/{r}"),
        None => p.to_string(),
    };
    // Writable dirs are overlays whose upper layer lives in a root-only scratch
    // dir, deleted when the build ends: the build sees its own writes, but
    // nothing it writes to a cache or toolchain dir outlives it. Outputs are
    // the only writes that persist.
    let mut s = String::from(
        "set -eu\n\
         ro() { [ -e \"$1\" ] || return 0; mount --bind \"$1\" \"$1\"; mount -o remount,bind,ro \"$1\"; }\n\
         rw() { mount --bind \"$1\" \"$1\"; mount -o remount,bind,rw \"$1\"; }\n\
         ovl_n=0\n\
         ovl() { ovl_n=$((ovl_n+1)); mkdir -p \"$scratch/$ovl_n/u\" \"$scratch/$ovl_n/w\"; \
         chown --reference=\"$1\" \"$scratch/$ovl_n/u\"; chmod --reference=\"$1\" \"$scratch/$ovl_n/u\"; \
         mount -t overlay overlay -o \"lowerdir=$1,upperdir=$scratch/$ovl_n/u,workdir=$scratch/$ovl_n/w\" \"$1\"; }\n\
         scratch=$(mktemp -d /var/tmp/cb-build.XXXXXX)\n\
         trap 'rm -rf \"$scratch\"' EXIT\n",
    );
    for p in [home, "/opt", "/usr/local", &dir.display().to_string()] {
        s += &format!("ro {}\n", sq(p));
    }
    s += "[ ! -d /run/user ] || mount -t tmpfs -o size=1m,mode=755 tmpfs /run/user\n";
    for w in &b.writable {
        s += &format!("ovl {}\n", sq(&expand(w)));
    }
    for o in &b.outputs {
        s += &format!("rw {}\n", sq(&dir.join(o).display().to_string()));
    }
    // A bind mount copies the read-only flag of the mount it came from, so rw()
    // remounts its target writable. This shell's cwd still points at the
    // workspace's original writable mount, and the build would inherit it:
    // cd again so the cwd goes through the new mounts.
    s += "cd -- \"$PWD\"\n";
    // Not exec: this shell (pid 1 of the namespace) waits for the build, then
    // its trap deletes the overlay layers.
    s += &format!(
        "setpriv --reuid={uid} --regid={gid} --clear-groups --no-new-privs -- env -i {} bash -c {}\n",
        vars.iter()
            .map(|(k, v)| sq(&format!("{k}={v}")))
            .collect::<Vec<_>>()
            .join(" "),
        sq(cmd)
    );
    s
}

/// True if CI credentials or secrets: never handed to challenge commands.
pub(crate) fn secret_var(k: &str) -> bool {
    k.starts_with("ACTIONS_")
        || k.contains("TOKEN")
        || k.starts_with("GITHUB_")
        || k.contains("SECRET")
}

pub(crate) fn run_mode(
    mode: Mode,
    env: &[(String, String)],
    dir: &Path,
    cmd: &str,
    seed: u64,
) -> Run {
    run_mode_timeout(mode, env, dir, cmd, seed, TIMEOUT)
}

pub(crate) fn run_mode_timeout(
    mode: Mode,
    env: &[(String, String)],
    dir: &Path,
    cmd: &str,
    seed: u64,
    timeout: Duration,
) -> Run {
    let start = Instant::now();
    let seed_env = ("CB_SEED".to_string(), seed.to_string());
    if let Mode::Build(b) = &mode {
        // Writable dirs and outputs must exist before they can be bind-mounted.
        let home = std::env::var("HOME").unwrap_or_default();
        let mut prep = Ok(());
        for w in &b.writable {
            let p = w
                .strip_prefix("~/")
                .map(|r| Path::new(&home).join(r))
                .unwrap_or_else(|| w.into());
            prep = prep.and(std::fs::create_dir_all(&p));
        }
        for o in &b.outputs {
            let p = dir.join(o);
            if let Some(parent) = p.parent() {
                prep = prep.and(std::fs::create_dir_all(parent));
            }
            if !p.exists() {
                prep = prep.and(std::fs::File::create(&p).map(|_| ()));
            }
        }
        if let Err(e) = prep {
            return Run {
                ok: false,
                stdout: String::new(),
                secs: 0.0,
                detail: format!("build sandbox: {e}"),
            };
        }
    }
    let mut command = if let Mode::Build(b) = &mode {
        let vars: Vec<(String, String)> = std::env::vars()
            .filter(|(k, _)| !secret_var(k))
            .chain(env.iter().cloned())
            .chain(std::iter::once(seed_env.clone()))
            .collect();
        // SAFETY: getuid/getgid have no preconditions.
        let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
        let home = std::env::var("HOME").unwrap_or_default();
        let mut c = Command::new("sudo");
        c.args([
            "-n",
            "unshare",
            "--mount",
            "--net",
            "--pid",
            "--fork",
            "--kill-child",
            "--mount-proc",
            "--",
            "bash",
            "-c",
        ])
        .arg(build_script(b, dir, &home, uid, gid, &vars, cmd));
        c
    } else if matches!(mode, Mode::Judge) {
        let mut c = Command::new("sudo");
        c.args([
            "unshare",
            "--net",
            "--",
            "setpriv",
            "--reuid=65534",
            "--regid=65534",
            "--clear-groups",
            "--no-new-privs",
            "--",
            "env",
            "-i",
            "PATH=/usr/local/bin:/usr/bin:/bin",
            "HOME=/tmp",
        ]);
        c.args(
            env.iter()
                .chain(std::iter::once(&seed_env))
                .map(|(k, v)| format!("{k}={v}")),
        );
        c.args(["bash", "-c", cmd]);
        c
    } else {
        let mut c = Command::new("bash");
        c.args(["-c", cmd])
            .env(&seed_env.0, &seed_env.1)
            .envs(env.iter().map(|(k, v)| (k, v)));
        c
    };
    // Never hand CI credentials to challenge commands (the build compiles
    // submission bytes): ACTIONS_RUNTIME_TOKEN can upload artifacts.
    for (k, _) in std::env::vars() {
        if secret_var(&k) {
            command.env_remove(k);
        }
    }
    let child = command
        .current_dir(dir)
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
    // Drain both pipes while waiting: a command that writes more than a pipe
    // buffer (64 KiB) would otherwise block until the timeout (a verbose
    // cabal build did, preflight 42302837449).
    let drain = |pipe: Option<Box<dyn std::io::Read + Send>>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            buf
        })
    };
    let out_t = drain(child.stdout.take().map(|p| Box::new(p) as _));
    let err_t = drain(child.stderr.take().map(|p| Box::new(p) as _));
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                // Not joined: a surviving grandchild may hold the pipes open.
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
    let stdout = out_t.join().unwrap_or_default();
    let stderr = err_t.join().unwrap_or_default();
    let stderr = String::from_utf8_lossy(&stderr);
    let tail: Vec<&str> = stderr.lines().rev().take(3).collect();
    Run {
        ok: status.success(),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        secs,
        detail: format!(
            "{} {}",
            status,
            tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
        ),
    }
}

/// Verify passes only on exit 0 with `{"ok": true}`.
pub(crate) fn verify_ok(r: &Run) -> bool {
    r.ok && serde_json::from_str::<Value>(r.stdout.trim())
        .ok()
        .and_then(|v| v["ok"].as_bool())
        == Some(true)
}

/// A failed verify that the judge blames on the infrastructure, not the
/// candidate (`"infra": true`, e.g. the pristine reference misbehaving on a
/// fresh pool): scored as infra_failed, not rejected.
pub(crate) fn verify_infra(r: &Run) -> bool {
    serde_json::from_str::<Value>(r.stdout.trim())
        .ok()
        .and_then(|v| v["infra"].as_bool())
        == Some(true)
        && !verify_ok(r)
}

pub(crate) fn metrics(r: &Run) -> Option<Value> {
    let v: Value = serde_json::from_str(r.stdout.trim()).ok()?;
    v.get("metrics").filter(|m| m.is_object()).cloned()
}

pub(crate) fn max_child_rss_gb() -> f64 {
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

pub(crate) fn du_bytes(dir: &Path) -> u64 {
    cmd_out("du", &["-sk", &dir.to_string_lossy()])
        .and_then(|s| s.split_whitespace().next()?.parse::<u64>().ok())
        .map(|kb| kb * 1024)
        .unwrap_or(0)
}

/// Sum of file lengths under `dir` (the size cap counts bytes, not disk blocks).
pub fn tree_bytes(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => tree_bytes(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

/// Copies the challenge to a scratch dir whose editable surface is the
/// baseline with each of `overlays` copied on top, in order. Broken variants
/// ship only the files they change.
/// The required broken variants, then any other directory under
/// `preflight/broken/` (sorted): a challenge-specific cheat its verifier must catch.
fn broken_variants(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = BROKEN.iter().map(|n| n.to_string()).collect();
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
    names
}

fn workspace(
    dir: &Path,
    editable: &str,
    overlays: &[PathBuf],
    tag: &str,
) -> Result<PathBuf, String> {
    let ws = crate::paths::scratch(&format!("preflight-{tag}"));
    let cp = |from: &Path, to: &Path| -> Result<(), String> {
        let ok = Command::new("cp")
            .arg("-R")
            .arg(from)
            .arg(to)
            .status()
            .map_err(|e| e.to_string())?
            .success();
        if ok {
            Ok(())
        } else {
            Err(format!("copying {} into {}", from.display(), to.display()))
        }
    };
    cp(dir, &ws)?;
    let target = ws.join(editable);
    let _ = std::fs::remove_dir_all(&target);
    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    for layer in std::iter::once(&dir.join("baseline")).chain(overlays) {
        cp(&layer.join("."), &target)?;
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
pub(crate) fn threat_model_gaps(text: &str) -> Vec<String> {
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
    if crate::challenge::challenge_type(dir)? == "research" {
        return crate::research::preflight::run_all(dir);
    }
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
    // Haskell: the baseline passes its own policy, and so does every broken
    // variant (else the gate would refuse it before the verifier sees it).
    if let Some(hs) = &spec.haskell {
        let base = dir.join("baseline");
        let mut errs: Vec<String> = crate::haskellpolicy::check(&base, &base, &hs.src)
            .err()
            .unwrap_or_default();
        for name in broken_variants(dir) {
            let ws = workspace(
                dir,
                editable,
                &[dir.join("preflight/broken").join(&name)],
                &format!("hs-{name}"),
            )?;
            if let Err(e) = crate::haskellpolicy::check(&base, &ws.join(editable), &hs.src) {
                errs.extend(e.into_iter().map(|e| format!("broken/{name}: {e}")));
            }
            let _ = std::fs::remove_dir_all(&ws);
        }
        if errs.is_empty() {
            r.push(
                "haskell policy",
                Status::Pass,
                "baseline and broken variants clean",
            );
        } else {
            r.push("haskell policy", Status::Fail, errs.join("; "));
        }
    }
    let size = tree_bytes(&dir.join("baseline"));
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
    let base = workspace(dir, editable, &[], "baseline")?;
    let setup = run(&env, &base, &spec.commands.setup, seed);
    if !setup.ok {
        r.push("setup", Status::Fail, setup.detail);
        return Ok(r);
    }
    // The build sandbox CI uses (#51): prove it holds, then build in it.
    let sandbox = spec
        .build_sandbox
        .as_ref()
        .filter(|_| build_sandbox_available());
    if let Some(bs) = sandbox {
        let out = bs.outputs.first().map(String::as_str).unwrap_or("");
        // A file the probe writes into the first writable (overlay) dir: it
        // must be visible inside and gone afterwards.
        let home = std::env::var("HOME").unwrap_or_default();
        let persist = bs.writable.first().map(|w| {
            let d = w
                .strip_prefix("~/")
                .map(|r| Path::new(&home).join(r))
                .unwrap_or_else(|| w.into());
            d.join(".cb-overlay-probe")
        });
        let p_str = persist
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let probe = format!(
            "fail() {{ echo \"probe: $1\" >&2; exit 1; }}; \
             touch \"$HOME/.cb-probe\" 2>/dev/null && fail 'home writable'; \
             touch cb-probe 2>/dev/null && fail 'workspace writable'; \
             sudo -n true 2>/dev/null && fail 'sudo works'; \
             (exec 3<>/dev/tcp/1.1.1.1/443) 2>/dev/null && fail 'network reachable'; \
             [ -z {o} ] || : > {o} || fail 'output not writable'; \
             [ -z {w} ] || {{ : > {w} && [ -e {w} ]; }} || fail 'cache dir not writable inside'; \
             exit 0",
            o = sq(out),
            w = sq(&p_str)
        );
        let p = run_mode(Mode::Build(bs), &env, &base, &probe, seed);
        let leaked = persist.as_ref().is_some_and(|p| p.exists());
        if p.ok && !leaked {
            r.push(
                "build sandbox",
                Status::Pass,
                "home and workspace read-only, no sudo, no network; outputs writable; cache writes discarded",
            );
        } else if leaked {
            r.push(
                "build sandbox",
                Status::Fail,
                format!("a write to {p_str} outlived the build"),
            );
        } else {
            r.push(
                "build sandbox",
                Status::Fail,
                format!("probe escaped or failed: {}", p.detail),
            );
        }
    } else if spec.build_sandbox.is_some() {
        r.push(
            "build sandbox",
            Status::Warn,
            "not on Linux with passwordless sudo: built unsandboxed here",
        );
    }
    let build = spec.commands.build.as_ref().map(|b| match sandbox {
        Some(bs) => run_mode(Mode::Build(bs), &env, &base, b, seed),
        None => run(&env, &base, b, seed),
    });
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
    for name in &broken_variants(dir) {
        let src = dir.join("preflight/broken").join(name);
        if !src.is_dir() {
            r.push(
                &format!("broken: {name}"),
                Status::Fail,
                format!("missing {}", src.display()),
            );
            continue;
        }
        let ws = workspace(dir, editable, std::slice::from_ref(&src), name)?;
        // Same setup as the baseline, so a variant fails for its own reason.
        let setup = run(&env, &ws, &spec.commands.setup, seed);
        let b = spec
            .commands
            .build
            .as_ref()
            .map(|b| run(&env, &ws, b, seed));
        let v = run(&env, &ws, &spec.commands.verify, seed);
        let failed = if !setup.ok {
            Some(format!("setup: {}", setup.detail))
        } else {
            b.as_ref()
                .filter(|b| !b.ok)
                .map(|b| format!("build: {}", b.detail))
        };
        let (status, detail) = match (failed, verify_ok(&v)) {
            (Some(why), _) => (
                Status::Fail,
                format!("{why}; so it doesn't test the verifier"),
            ),
            (None, true) => (
                Status::Fail,
                "verifier ACCEPTED a broken candidate".to_string(),
            ),
            (None, false) => (Status::Pass, "rejected".to_string()),
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

    /// Only a failed verify that says `"infra": true` is the infrastructure's.
    #[test]
    fn verify_infra_needs_the_judges_flag() {
        let r = |ok: bool, out: &str| Run {
            ok,
            stdout: out.into(),
            secs: 0.0,
            detail: String::new(),
        };
        assert!(verify_infra(&r(
            false,
            r#"{"ok": false, "infra": true, "detail": "x"}"#
        )));
        assert!(!verify_infra(&r(
            false,
            r#"{"ok": false, "detail": "infra: x"}"#
        )));
        assert!(!verify_infra(&r(false, "not json")));
        // A pass is a pass, whatever else the output says.
        assert!(!verify_infra(&r(true, r#"{"ok": true, "infra": true}"#)));
    }

    /// Output bigger than a pipe buffer must not block until the timeout.
    #[test]
    fn large_output_does_not_block() {
        let dir = crate::paths::scratch("bigout");
        std::fs::create_dir_all(&dir).unwrap();
        let r = run(
            &[],
            &dir,
            "head -c 1000000 /dev/zero | tr '\\0' x; head -c 1000000 /dev/zero >&2; echo done >&2",
            0,
        );
        assert!(r.ok, "{}", r.detail);
        assert_eq!(r.stdout.len(), 1_000_000);
        assert!(r.detail.ends_with("done"), "{}", r.detail);
        assert!(r.secs < 30.0, "took {}s", r.secs);
    }

    const TM: &str = "# Threat model\n\n## How could a wrong answer pass the verifier?\n\n- Finite inputs: fresh per run\n\n## Interface\n\nmatches\n\n## Headroom\n\nunknown, see #6\n\n## Outlier\n\nmanual review by maintainer\n";

    /// A toy challenge: the answer file must hold 42; score reports a metric.
    fn toy(family: &str, verifier_accepts_anything: bool) -> PathBuf {
        toy_in("", family, verifier_accepts_anything)
    }

    /// `tag` keeps tests that change their toy apart: tests run in parallel.
    fn toy_in(tag: &str, family: &str, verifier_accepts_anything: bool) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "cb-pf-{}-{family}-{verifier_accepts_anything}{tag}",
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
    fn extra_broken_variants_are_run() {
        let dir = toy_in("-extra", "resource", false);
        std::fs::create_dir_all(dir.join("preflight/broken/lazy")).unwrap();
        std::fs::write(dir.join("preflight/broken/lazy/answer"), "40\n").unwrap();
        let r = run_all(&dir).unwrap();
        assert_eq!(status(&r, "broken: lazy"), Status::Pass);
        // One the verifier accepts blocks the challenge, like a required one.
        std::fs::write(dir.join("preflight/broken/lazy/answer"), "42\n").unwrap();
        let r = run_all(&dir).unwrap();
        assert_eq!(status(&r, "broken: lazy"), Status::Fail);
        assert!(r.blocked());
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
            assert!(
                std::env::var_os("CB_REQUIRE_VALGRIND").is_none(),
                "CI requires valgrind"
            );
            eprintln!("valgrind not found: skipped score checks\n{text}");
        }
    }

    #[test]
    fn build_script_quotes_and_orders_mounts() {
        let b = crate::challenge::BuildSandbox {
            writable: vec!["~/.cabal".into(), "/abs/it's".into()],
            outputs: vec![".bin/candidate".into()],
        };
        let vars = vec![("PATH".to_string(), "/a b:/c".to_string())];
        let s = build_script(
            &b,
            Path::new("/tmp/ws"),
            "/home/r",
            1001,
            118,
            &vars,
            "./build.sh 'x'",
        );
        let ro = s.find("ro '/home/r'").unwrap();
        let rw = s.find("ovl '/home/r/.cabal'").unwrap();
        assert!(
            !s.contains("exec setpriv"),
            "the shell must outlive the build to clean up"
        );
        assert!(ro < rw, "writable holes go after the read-only mounts");
        assert!(s.contains("ro '/tmp/ws'"));
        assert!(s.contains("rw '/tmp/ws/.bin/candidate'"));
        assert!(s.contains(r#"ovl '/abs/it'\''s'"#));
        assert!(s.contains("--reuid=1001 --regid=118 --clear-groups --no-new-privs"));
        assert!(s.contains("'PATH=/a b:/c'"));
        assert!(s.ends_with("bash -c './build.sh '\\''x'\\'''\n"), "{s}");
        // The script itself must parse.
        let ok = Command::new("bash")
            .args(["-n", "-c", &s])
            .status()
            .unwrap();
        assert!(ok.success());
    }

    #[test]
    fn tree_bytes_counts_bytes() {
        let d = crate::paths::scratch("bytes");
        std::fs::create_dir_all(d.join("a/b")).unwrap();
        std::fs::write(d.join("x"), "12345").unwrap();
        std::fs::write(d.join("a/b/y"), "123").unwrap();
        assert_eq!(tree_bytes(&d), 8);
    }

    #[test]
    fn threat_model_template_is_unanswered() {
        assert!(!threat_model_gaps(crate::challenge::THREAT_MODEL).is_empty());
        assert!(threat_model_gaps(TM).is_empty());
    }
}
