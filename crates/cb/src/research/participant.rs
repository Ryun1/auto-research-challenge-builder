//! `cb participant`: runs a participant's command under resource limits (and,
//! on Linux CI, inside a mount/net/pid/ipc/uts sandbox as `nobody`), measuring
//! wall time and CPU time from outside via `wait4`. Stdio is inherited: the
//! judge speaks a line protocol through it, so this never reads it.
//!
//! Sandbox mode (CI) puts the whole process tree in one cgroup v2 with
//! `memory.max`, `memory.swap.max = 0` and `pids.max`, so the limits and
//! `peak_rss_mib` (= `memory.peak`, including page cache) are per tree. Plain
//! mode (a laptop; never the scored path) is approximate: per-process rlimits
//! and the largest single process's RSS from `wait4`.
//!
//! The sandbox script reports back through a status file (fd 9, opened by the
//! launcher): `ready` once setup is done, then `peak <bytes>` and `oom <n>`.
//! No `ready` means the sandbox, not the submission, failed: status `infra`.

use crate::preflight::{secret_var, sq};
use crate::research::spec::Limits;
use serde::Serialize;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

const MIB: u64 = 1024 * 1024;

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub status: String,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub wall_secs: f64,
    pub cpu_secs: f64,
    pub peak_rss_mib: f64,
    /// Why the run is `infra` (sandbox setup failed); absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl Report {
    pub fn infra(detail: String, wall_secs: f64) -> Report {
        Report {
            status: "infra".into(),
            exit_code: None,
            signal: None,
            wall_secs,
            cpu_secs: 0.0,
            peak_rss_mib: 0.0,
            detail: Some(detail),
        }
    }
}

/// `CB_LIMITS` (JSON) if set and valid, else the defaults.
pub fn limits_from_env() -> Limits {
    match std::env::var("CB_LIMITS") {
        Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
            eprintln!("cb: ignoring invalid CB_LIMITS ({e}); using defaults");
            Limits::default()
        }),
        Err(_) => Limits::default(),
    }
}

/// Directories the participant gets fresh tmpfs over, so it can keep no state
/// between invocations (`/dev/mqueue` gets an mqueue mount, falling back to
/// tmpfs). Together with `/run/user` these are the only writable mount points.
pub const SCRATCH_DIRS: [&str; 7] = [
    "/tmp",
    "/var/tmp",
    "/dev/shm",
    "/run/lock",
    "/var/crash",
    "/dev/mqueue",
    "/dev/hugepages",
];

/// The mount script run as root inside the new namespaces. It creates the
/// per-invocation cgroup, makes the filesystem read-only except fresh tmpfs
/// scratch dirs and a read-only view of the cwd at /mnt, then runs the command
/// as `nobody` under rlimits and a wall-clock timeout, and finally reports the
/// tree's peak memory and OOM kills on fd 9 (`status`).
///
/// `unique` makes the cgroup name unique (the shell is pid 1 in its pid
/// namespace, so `$$` is not). Everything up to `echo ready` runs under
/// `set -eu`: a failure there aborts before the participant starts. The
/// status fd is opened through a temporary bind mount of `status_dir` on
/// /mnt, because an fd open for writing on the root mount would make the
/// read-only remount of `/` fail with EBUSY.
pub fn sandbox_script(
    cwd: &Path,
    limits: &Limits,
    cmd: &[String],
    status_dir: &Path,
    unique: &str,
) -> String {
    let mem = limits.memory_mib * MIB;
    let mut s = format!(
        "set -eu\n\
         mount --bind {statusdir} /mnt\n\
         exec 9>>/mnt/status\n\
         cg={cg}\n\
         mkdir \"$cg\"\n\
         echo {mem} > \"$cg/memory.max\"\n\
         [ ! -e \"$cg/memory.swap.max\" ] || echo 0 > \"$cg/memory.swap.max\"\n\
         echo {pids} > \"$cg/pids.max\"\n\
         echo $$ > \"$cg/cgroup.procs\"\n\
         mount --bind {cwd} /mnt\n\
         mount -o remount,bind,ro /mnt\n\
         mount -o remount,bind,ro /\n\
         while read -r _ _ _ _ mp _; do\n\
           case \"$mp\" in */\\\\*|/|/mnt|/proc|/proc/*|/sys|/sys/*|/dev|/dev/*) continue;; esac\n\
           mount -o remount,bind,ro \"$mp\" 2>/dev/null || true\n\
         done < /proc/self/mountinfo\n",
        statusdir = sq(&status_dir.display().to_string()),
        cg = sq(&format!("/sys/fs/cgroup/cb-participant-{unique}")),
        // The shell and `timeout` live in the cgroup too.
        pids = limits.processes + 2,
        cwd = sq(&cwd.display().to_string()),
    );
    // The cwd usually lives under /tmp, so it was bound before /tmp is replaced.
    for d in SCRATCH_DIRS {
        let tmpfs = format!(
            "mount -t tmpfs -o size={}m,mode=1777 tmpfs {d}",
            limits.file_mib
        );
        if d == "/dev/mqueue" {
            s += &format!("[ ! -d {d} ] || mount -t mqueue mqueue {d} 2>/dev/null || {tmpfs}\n");
        } else {
            s += &format!("[ ! -d {d} ] || {tmpfs}\n");
        }
    }
    s += "[ ! -d /run/user ] || mount -t tmpfs -o size=1m,mode=755 tmpfs /run/user\n\
          cd /mnt\n\
          echo ready >&9\n\
          set +e\n";
    s += &format!(
        "timeout -k 2 {} prlimit --cpu={} --as={} --fsize={} -- \
         setpriv --reuid=65534 --regid=65534 --clear-groups --no-new-privs -- \
         env -i PATH=/usr/local/bin:/usr/bin:/bin HOME=/tmp {} 9>&-\n",
        limits.wall_secs,
        limits.cpu_secs,
        // A backstop only: the cgroup enforces the real (tree) limit.
        mem * 4,
        limits.file_mib * MIB,
        cmd.iter().map(|a| sq(a)).collect::<Vec<_>>().join(" ")
    );
    s += "rc=$?\n\
          if [ -r \"$cg/memory.peak\" ]; then echo \"peak $(cat \"$cg/memory.peak\")\" >&9; fi\n\
          oom=0\n\
          while read -r k v; do [ \"$k\" != oom_kill ] || oom=$v; done < \"$cg/memory.events\"\n\
          echo \"oom $oom\" >&9\n\
          echo $$ > /sys/fs/cgroup/cgroup.procs\n\
          echo 1 > \"$cg/cgroup.kill\" 2>/dev/null\n\
          for _ in 1 2 3 4 5 6 7 8 9 10; do rmdir \"$cg\" 2>/dev/null && break; sleep 0.1; done\n\
          exit $rc\n";
    s
}

/// What the sandbox script wrote to the status file.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct SandboxStatus {
    pub ready: bool,
    pub peak_bytes: Option<u64>,
    pub oom: u64,
}

pub(crate) fn parse_status(text: &str) -> SandboxStatus {
    let mut st = SandboxStatus::default();
    for line in text.lines() {
        let mut w = line.split_whitespace();
        match (w.next(), w.next().map(str::parse::<u64>)) {
            (Some("ready"), None) => st.ready = true,
            (Some("peak"), Some(Ok(n))) => st.peak_bytes = Some(n),
            (Some("oom"), Some(Ok(n))) => st.oom = n,
            _ => {}
        }
    }
    st
}

/// Exit code to propagate: 124 on timeout, 125 on infra, else the command's own, else
/// 128+signal.
pub fn exit_code(r: &Report) -> i32 {
    match r.status.as_str() {
        "timeout" => return 124,
        "infra" => return 125,
        // Never 0, even if the command swallowed an OOM kill.
        "memory_limit" => return r.exit_code.filter(|c| *c != 0).unwrap_or(128 + 9),
        _ => {}
    }
    match (r.exit_code, r.signal) {
        (Some(c), _) => c,
        (None, Some(s)) => 128 + s,
        (None, None) => 1,
    }
}

/// Decides (status, exit code, signal) from the raw wait result.
#[allow(clippy::too_many_arguments)]
fn classify(
    sandbox: bool,
    killed: bool,
    mut code: Option<i32>,
    mut signal: Option<i32>,
    elapsed: f64,
    cpu: f64,
    oom: bool,
    limits: &Limits,
) -> (&'static str, Option<i32>, Option<i32>) {
    if sandbox {
        // `timeout` reports a signalled command as 128+n.
        if let Some(c) = code.filter(|c| *c > 128 && *c < 128 + 65) {
            signal = Some(c - 128);
        }
    }
    // In the sandbox `timeout` may exit 124, 137 (-k escalation) or 128+SIGTERM:
    // any failure after the wall time is a timeout.
    let failed = code != Some(0) || signal.is_some();
    let timed_out = if sandbox {
        failed && elapsed >= limits.wall_secs as f64
    } else {
        killed
    };
    let hit_cpu = matches!(signal, Some(s) if s == libc::SIGXCPU || s == libc::SIGKILL)
        && cpu >= limits.cpu_secs as f64 - 0.5;
    let state = if timed_out {
        "timeout"
    } else if oom {
        "memory_limit"
    } else if hit_cpu {
        "cpu_limit"
    } else if !failed {
        "ok"
    } else {
        "failed"
    };
    if code.is_none() && signal.is_none() {
        code = Some(1);
    }
    (state, code, signal)
}

/// Runs `cmd` in the current directory.
pub fn run(cmd: &[String], limits: &Limits, sandbox: bool) -> Result<Report, String> {
    let cwd = std::env::current_dir().map_err(|e| format!("cwd: {e}"))?;
    run_in(&cwd, cmd, limits, sandbox)
}

fn run_in(cwd: &Path, cmd: &[String], limits: &Limits, sandbox: bool) -> Result<Report, String> {
    if cmd.is_empty() {
        return Err("no command given".into());
    }
    // The status file lives in its own (non-sticky) dir, so root may open it
    // even with fs.protected_regular. The script bind-mounts that dir at /mnt
    // and opens the file through it before anything else, so the open fd
    // pins that bind mount, not the root mount (which it then remounts ro).
    let status_dir = crate::paths::scratch("participant-status");
    let status_file = status_dir.join("status");
    let mut c = if sandbox {
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&status_dir)
            .map_err(|e| format!("status dir: {e}"))?;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&status_file)
            .map_err(|e| format!("status file: {e}"))?;
        let unique = status_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut c = Command::new("sudo");
        c.args([
            "-n",
            "unshare",
            "--mount",
            "--net",
            "--pid",
            "--ipc",
            "--uts",
            "--fork",
            "--kill-child",
            "--mount-proc",
            "--",
            "bash",
            "-c",
        ])
        .arg(sandbox_script(cwd, limits, cmd, &status_dir, &unique));
        c
    } else {
        let mut c = Command::new(&cmd[0]);
        c.args(&cmd[1..]);
        let (cpu, fsize) = (limits.cpu_secs, limits.file_mib * MIB);
        #[cfg(target_os = "linux")]
        let mem = limits.memory_mib * MIB;
        // SAFETY: runs between fork and exec in the child; only calls
        // async-signal-safe libc functions (setpgid, setrlimit) on values
        // copied in beforehand, and does not allocate on the success path.
        unsafe {
            c.pre_exec(move || {
                if libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                let set = |res, v: u64| {
                    let l = libc::rlimit {
                        rlim_cur: v as libc::rlim_t,
                        rlim_max: v as libc::rlim_t,
                    };
                    if libc::setrlimit(res, &l) != 0 {
                        Err(std::io::Error::last_os_error())
                    } else {
                        Ok(())
                    }
                };
                set(libc::RLIMIT_CPU, cpu)?;
                set(libc::RLIMIT_FSIZE, fsize)?;
                #[cfg(target_os = "linux")]
                set(libc::RLIMIT_AS, mem)?;
                Ok(())
            });
        }
        c
    };
    c.current_dir(cwd);
    for (k, _) in std::env::vars_os() {
        if k.to_str().is_some_and(secret_var) {
            c.env_remove(&k);
        }
    }

    let start = Instant::now();
    let child = match c.spawn() {
        Ok(ch) => ch,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&status_dir);
            return Err(format!("spawn {}: {e}", cmd[0]));
        }
    };
    let pid = child.id() as libc::pid_t;
    let wall = Duration::from_secs(limits.wall_secs);
    let mut killed = false;
    // Reaped only via wait4 (never `child.wait()`) so we get its rusage.
    let (status, ru) = loop {
        let mut status: libc::c_int = 0;
        // SAFETY: a zeroed rusage is a valid plain-data struct; wait4 fills it.
        let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
        let r = unsafe { libc::wait4(pid, &mut status, libc::WNOHANG, &mut ru) };
        if r == pid {
            break (status, ru);
        }
        if r < 0 {
            let e = std::io::Error::last_os_error();
            if e.kind() != std::io::ErrorKind::Interrupted {
                let _ = std::fs::remove_dir_all(&status_dir);
                return Err(format!("wait4: {e}"));
            }
        } else if !sandbox && !killed && start.elapsed() >= wall {
            // The child leads its own process group: kill the whole tree.
            // SAFETY: plain syscall on a pid we spawned.
            unsafe { libc::kill(-pid, libc::SIGKILL) };
            killed = true;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let elapsed = start.elapsed().as_secs_f64();

    let sb = sandbox.then(|| {
        let text = std::fs::read_to_string(&status_file).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&status_dir);
        parse_status(&text)
    });

    let tv = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 / 1e6;
    let cpu = tv(ru.ru_utime) + tv(ru.ru_stime);
    let rss = ru.ru_maxrss as f64;
    let mut peak_rss_mib = if cfg!(target_os = "macos") {
        rss / MIB as f64 // bytes
    } else {
        rss / 1024.0 // KiB
    };

    let code = libc::WIFEXITED(status).then(|| libc::WEXITSTATUS(status));
    let signal = libc::WIFSIGNALED(status).then(|| libc::WTERMSIG(status));
    let mut oom = false;
    if let Some(sb) = &sb {
        if !sb.ready {
            return Ok(Report::infra(
                format!(
                    "the participant sandbox failed before the command started \
                     (exit {code:?}, signal {signal:?}); see stderr"
                ),
                elapsed,
            ));
        }
        // The tree's peak (incl. page cache), not the largest process's.
        if let Some(b) = sb.peak_bytes {
            peak_rss_mib = b as f64 / MIB as f64;
        }
        oom = sb.oom > 0;
    }
    let (state, code, signal) = classify(sandbox, killed, code, signal, elapsed, cpu, oom, limits);
    Ok(Report {
        status: state.into(),
        exit_code: code,
        signal,
        wall_secs: elapsed,
        cpu_secs: cpu,
        peak_rss_mib,
        detail: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(s: &str) -> Vec<String> {
        vec!["sh".into(), "-c".into(), s.into()]
    }

    #[test]
    fn sandbox_script_shape() {
        let l = Limits::default();
        let cmd = vec!["python3".to_string(), "a'b".to_string()];
        let s = sandbox_script(Path::new("/tmp/w"), &l, &cmd, Path::new("/tmp/st'x"), "u1");
        for needle in [
            "set -eu\nmount --bind '/tmp/st'\\''x' /mnt\nexec 9>>/mnt/status\n",
            "cg='/sys/fs/cgroup/cb-participant-u1'",
            "echo 1073741824 > \"$cg/memory.max\"",
            "echo 0 > \"$cg/memory.swap.max\"",
            "echo 34 > \"$cg/pids.max\"",
            "echo $$ > \"$cg/cgroup.procs\"",
            "mount --bind '/tmp/w' /mnt",
            "remount,bind,ro /mnt",
            "mount -t tmpfs -o size=64m,mode=1777 tmpfs /tmp",
            "mount -t tmpfs -o size=64m,mode=1777 tmpfs /run/lock",
            "mount -t tmpfs -o size=64m,mode=1777 tmpfs /var/crash",
            "mount -t mqueue mqueue /dev/mqueue",
            "mount -t tmpfs -o size=1m,mode=755 tmpfs /run/user",
            "echo ready >&9\nset +e\n",
            "prlimit --cpu=60",
            "--as=4294967296",
            "--fsize=67108864",
            "setpriv --reuid=65534",
            "env -i",
            "timeout -k 2 120",
            "'python3' 'a'\\''b' 9>&-\n",
            "echo 1 > \"$cg/cgroup.kill\"",
            "exit $rc",
        ] {
            assert!(s.contains(needle), "missing {needle:?} in:\n{s}");
        }
        assert!(!s.contains("--nproc"));
        assert!(!s.contains("exec timeout"));
        let at = |n: &str| s.find(n).unwrap();
        // The cwd is bound before /tmp is replaced; setup precedes `ready`,
        // which precedes the participant.
        assert!(at("mount --bind") < at("tmpfs"));
        assert!(at("memory.max") < at("echo ready"));
        assert!(at("echo ready") < at("timeout -k"));
    }

    #[test]
    fn parses_status_file() {
        let st = parse_status("ready\npeak 123456\noom 2\n");
        assert_eq!(
            st,
            SandboxStatus {
                ready: true,
                peak_bytes: Some(123456),
                oom: 2
            }
        );
        // Garbage and a missing `ready` line are not ready.
        assert_eq!(parse_status(""), SandboxStatus::default());
        assert!(!parse_status("peak 5\noom 0\nreadyish\n").ready);
        assert_eq!(parse_status("ready\npeak x\noom\n").oom, 0);
    }

    #[test]
    fn infra_and_memory_exit_codes() {
        let infra = Report::infra("x".into(), 0.1);
        assert_eq!(infra.status, "infra");
        assert_eq!(infra.exit_code, None);
        assert_eq!(exit_code(&infra), 125);
        let mut r = infra.clone();
        r.status = "memory_limit".into();
        assert_eq!(exit_code(&r), 137);
        r.exit_code = Some(0);
        assert_eq!(exit_code(&r), 137);
        r.exit_code = Some(3);
        assert_eq!(exit_code(&r), 3);
    }

    #[test]
    fn classify_cases() {
        let l = Limits::default(); // cpu 60, wall 120
        let c =
            |sb, killed, code, sig, el, cpu| classify(sb, killed, code, sig, el, cpu, false, &l).0;
        assert_eq!(c(true, false, Some(137), None, 121.0, 1.0), "timeout");
        assert_eq!(c(true, false, Some(124), None, 120.5, 1.0), "timeout");
        assert_eq!(
            c(true, false, Some(128 + 24), None, 10.0, 60.0),
            "cpu_limit"
        );
        assert_eq!(c(true, false, Some(137), None, 10.0, 1.0), "failed");
        assert_eq!(c(false, true, None, Some(9), 121.0, 1.0), "timeout");
        assert_eq!(c(false, false, Some(0), None, 1.0, 0.1), "ok");
        assert_eq!(c(true, false, Some(0), None, 130.0, 0.1), "ok");
        // An OOM kill in the cgroup is memory_limit, unless the wall time ran out.
        let m = |code, el| classify(true, false, code, None, el, 1.0, true, &l).0;
        assert_eq!(m(Some(137), 5.0), "memory_limit");
        assert_eq!(m(Some(0), 5.0), "memory_limit");
        assert_eq!(m(Some(137), 130.0), "timeout");
    }

    #[test]
    fn plain_ok_exit_code() {
        let r = run(&sh("exit 3"), &Limits::default(), false).unwrap();
        assert_eq!(r.status, "failed");
        assert_eq!(r.exit_code, Some(3));
        assert_eq!(exit_code(&r), 3);
        let r = run(&sh("exit 0"), &Limits::default(), false).unwrap();
        assert_eq!(r.status, "ok");
    }

    #[test]
    fn plain_timeout() {
        let l = Limits {
            wall_secs: 1,
            ..Limits::default()
        };
        let r = run(&["sleep".into(), "30".into()], &l, false).unwrap();
        assert_eq!(r.status, "timeout");
        assert!(r.wall_secs < 5.0, "{}", r.wall_secs);
        assert_eq!(exit_code(&r), 124);
    }

    #[test]
    fn plain_cpu_limit() {
        let l = Limits {
            cpu_secs: 1,
            ..Limits::default()
        };
        let r = run(&sh("while :; do :; done"), &l, false).unwrap();
        assert_eq!(r.status, "cpu_limit", "{r:?}");
    }

    #[test]
    fn plain_peak_rss() {
        if Command::new("python3").arg("-V").output().is_err() {
            eprintln!("skipped: no python3");
            return;
        }
        let cmd = vec![
            "python3".into(),
            "-c".into(),
            "b=b'x'*(200*1024*1024)".into(),
        ];
        let r = run(&cmd, &Limits::default(), false).unwrap();
        assert!(r.peak_rss_mib >= 150.0, "{r:?}");
    }

    #[test]
    fn spawn_error_is_err() {
        assert!(run(&["/nonexistent/x".into()], &Limits::default(), false).is_err());
    }

    /// True when the Linux sandbox is usable here; panics under
    /// `CB_REQUIRE_SANDBOX=1` if not.
    fn have_sandbox() -> bool {
        let require = std::env::var("CB_REQUIRE_SANDBOX").is_ok_and(|v| v == "1");
        if !crate::preflight::build_sandbox_available() {
            assert!(!require, "CB_REQUIRE_SANDBOX=1 but no sandbox available");
            eprintln!("skipped: no sandbox");
            return false;
        }
        true
    }

    fn have_python() -> bool {
        let ok = Command::new("python3").arg("-V").output().is_ok();
        if !ok {
            eprintln!("skipped: no python3");
        }
        ok
    }

    fn sandbox_dir(tag: &str) -> std::path::PathBuf {
        let dir = crate::paths::scratch(tag);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn bash(script: &str) -> Vec<String> {
        vec!["bash".into(), "-c".into(), script.into()]
    }

    #[test]
    fn sandboxed_runs_as_nobody_without_network() {
        if !have_sandbox() {
            return;
        }
        let dir = sandbox_dir("participant-sandbox");
        let script = "[ \"$(id -u)\" = 65534 ] || exit 10; \
             ( : > ./x ) 2>/dev/null && exit 11; \
             [ -z \"$(ls -A /tmp)\" ] || exit 12; \
             ( exec 3<>/dev/tcp/1.1.1.1/443 ) 2>/dev/null && exit 13; \
             ( : > /etc/cb-probe ) 2>/dev/null && exit 14; \
             exit 0";
        let r = run_in(&dir, &bash(script), &Limits::default(), true).unwrap();
        assert_eq!(r.status, "ok", "{r:?}");
        assert_eq!(r.exit_code, Some(0));
        assert!(r.detail.is_none());
        assert!(r.peak_rss_mib > 0.0, "{r:?}");
    }

    #[test]
    fn sandbox_setup_failure_is_infra() {
        if !have_sandbox() {
            return;
        }
        let dir = sandbox_dir("participant-infra");
        // pids.max rejects a value that does not fit in a long long, so setup
        // aborts under `set -eu` before the participant starts.
        let l = Limits {
            processes: u64::MAX - 2,
            ..Limits::default()
        };
        let r = run_in(&dir, &bash("exit 0"), &l, true).unwrap();
        assert_eq!(r.status, "infra", "{r:?}");
        assert_eq!(r.exit_code, None);
        assert_eq!(exit_code(&r), 125);
        assert!(r.detail.is_some());
    }

    const SEEN: [&str; 7] = SCRATCH_DIRS;

    #[test]
    fn sandbox_keeps_no_state() {
        if !have_sandbox() {
            return;
        }
        let dir = sandbox_dir("participant-state");
        let l = Limits {
            wall_secs: 300,
            cpu_secs: 280,
            ..Limits::default()
        };
        let marker = format!("cb-state-marker-{}", std::process::id());
        let dirs = SEEN.join(" ");
        // Run 1: write a marker everywhere writable and create a SysV segment.
        let write = format!(
            "m={marker}; for d in {dirs} \"$HOME\"; do \
               if [ -d \"$d\" ]; then : > \"$d/$m\" 2>/dev/null || true; fi; done; \
             [ -e /tmp/$m ] || exit 21; \
             if command -v ipcmk >/dev/null && command -v ipcs >/dev/null; then \
               ipcmk -M 4096 >/dev/null || exit 22; \
               ipcs -m | grep -q nobody || exit 23; fi; \
             exit 0"
        );
        let r = run_in(&dir, &bash(&write), &l, true).unwrap();
        assert_eq!(r.status, "ok", "{r:?}");
        // Neither the marker nor the segment reached the host.
        for d in SEEN {
            let p = Path::new(d).join(&marker);
            assert!(!p.exists(), "marker leaked to the host at {}", p.display());
        }
        if let Ok(o) = Command::new("ipcs").arg("-m").output() {
            let t = String::from_utf8_lossy(&o.stdout);
            assert!(
                !t.lines()
                    .any(|l| l.split_whitespace().nth(2) == Some("nobody")),
                "SysV segment leaked to the host:\n{t}"
            );
        }
        // Run 2: nothing from run 1 is visible, and nothing else is writable.
        let check = format!(
            "m={marker}; for d in {dirs} \"$HOME\"; do \
               [ ! -e \"$d/$m\" ] || {{ echo \"state leaked in $d\" >&2; exit 31; }}; done; \
             if command -v ipcs >/dev/null; then \
               [ -z \"$(ipcs -m | awk '$3==\"nobody\" || $3==\"65534\"')\" ] \
                 || {{ ipcs -m >&2; exit 32; }}; fi; \
             bad=; \
             while read -r _ _ _ _ mp _; do \
               case \"$mp\" in /proc|/proc/*|/sys|/sys/*) continue;; esac; \
               for a in {dirs}; do [ \"$mp\" != \"$a\" ] || continue 2; done; \
               while read -r w; do \
                 for a in {dirs}; do [ \"$w\" != \"$a\" ] || continue 2; done; \
                 bad=\"$bad$w\n\"; \
               done < <(timeout 60 find \"$mp\" -xdev -maxdepth 6 -type d -writable 2>/dev/null); \
             done < /proc/self/mountinfo; \
             if ( : > /cb-probe ) 2>/dev/null; then bad=\"$bad /(writable)\"; fi; \
             if [ -n \"$bad\" ]; then \
               printf 'unexpected writable dirs:\\n%s\\n' \"$bad\" >&2; exit 33; fi; \
             exit 0"
        );
        let r = run_in(&dir, &bash(&check), &l, true).unwrap();
        assert_eq!(r.status, "ok", "see stderr above: {r:?}");
    }

    const HOG: &str = "python3 -c \"b=b'x'*(150*1024*1024); import time; time.sleep(3)\"";

    #[test]
    fn sandbox_tree_memory_limit() {
        if !have_sandbox() || !have_python() {
            return;
        }
        let dir = sandbox_dir("participant-treemem");
        let l = Limits {
            memory_mib: 256,
            ..Limits::default()
        };
        // Each process stays far under 256 MiB; the tree does not.
        let script = format!("for i in 1 2 3; do {HOG} & done; wait");
        let r = run_in(&dir, &bash(&script), &l, true).unwrap();
        assert_eq!(r.status, "memory_limit", "{r:?}");
        assert_ne!(exit_code(&r), 0);
    }

    #[test]
    fn sandbox_tree_peak() {
        if !have_sandbox() || !have_python() {
            return;
        }
        let dir = sandbox_dir("participant-treepeak");
        let script = format!("{HOG} & {HOG} & wait");
        let r = run_in(&dir, &bash(&script), &Limits::default(), true).unwrap();
        assert_eq!(r.status, "ok", "{r:?}");
        assert!(r.peak_rss_mib >= 280.0, "{r:?}");
    }
}
