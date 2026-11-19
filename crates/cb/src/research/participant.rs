//! `cb participant`: runs a participant's command under resource limits (and,
//! on Linux CI, inside a mount/net/pid sandbox as `nobody`), measuring wall
//! time, CPU time and peak RSS from outside via `wait4`. Stdio is inherited:
//! the judge speaks a line protocol through it, so this never reads it.

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

/// The mount script run as root inside the new namespaces: read-only view of
/// the cwd at /mnt, scratch tmpfs dirs, then the command as `nobody` under
/// rlimits and a wall-clock timeout.
pub fn sandbox_script(cwd: &Path, limits: &Limits, cmd: &[String]) -> String {
    // The cwd usually lives under /tmp, so bind it before /tmp is replaced.
    let mut s = format!(
        "set -eu\n\
         mount --bind {} /mnt\n\
         mount -o remount,bind,ro /mnt\n\
         mount -t tmpfs -o size={}m,mode=1777 tmpfs /tmp\n",
        sq(&cwd.display().to_string()),
        limits.file_mib
    );
    for d in ["/var/tmp", "/dev/shm"] {
        s += &format!(
            "[ ! -d {d} ] || mount -t tmpfs -o size={}m,mode=1777 tmpfs {d}\n",
            limits.file_mib
        );
    }
    s += "[ ! -d /run/user ] || mount -t tmpfs -o size=1m,mode=755 tmpfs /run/user\n\
          cd /mnt\n";
    s += &format!(
        "exec timeout -k 2 {} prlimit --cpu={} --as={} --nproc={} --fsize={} -- \
         setpriv --reuid=65534 --regid=65534 --clear-groups --no-new-privs -- \
         env -i PATH=/usr/local/bin:/usr/bin:/bin HOME=/tmp {}\n",
        limits.wall_secs,
        limits.cpu_secs,
        limits.memory_mib * MIB,
        limits.processes,
        limits.file_mib * MIB,
        cmd.iter().map(|a| sq(a)).collect::<Vec<_>>().join(" ")
    );
    s
}

/// Exit code to propagate: 124 on timeout, else the command's own, else
/// 128+signal.
pub fn exit_code(r: &Report) -> i32 {
    if r.status == "timeout" {
        return 124;
    }
    match (r.exit_code, r.signal) {
        (Some(c), _) => c,
        (None, Some(s)) => 128 + s,
        (None, None) => 1,
    }
}

/// Decides (status, exit code, signal) from the raw wait result.
fn classify(
    sandbox: bool,
    killed: bool,
    mut code: Option<i32>,
    mut signal: Option<i32>,
    elapsed: f64,
    cpu: f64,
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
    let mut c = if sandbox {
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
        .arg(sandbox_script(cwd, limits, cmd));
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
    let child = c.spawn().map_err(|e| format!("spawn {}: {e}", cmd[0]))?;
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

    let tv = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 / 1e6;
    let cpu = tv(ru.ru_utime) + tv(ru.ru_stime);
    let rss = ru.ru_maxrss as f64;
    let peak_rss_mib = if cfg!(target_os = "macos") {
        rss / MIB as f64 // bytes
    } else {
        rss / 1024.0 // KiB
    };

    let code = libc::WIFEXITED(status).then(|| libc::WEXITSTATUS(status));
    let signal = libc::WIFSIGNALED(status).then(|| libc::WTERMSIG(status));
    let (state, code, signal) = classify(sandbox, killed, code, signal, elapsed, cpu, limits);
    Ok(Report {
        status: state.into(),
        exit_code: code,
        signal,
        wall_secs: elapsed,
        cpu_secs: cpu,
        peak_rss_mib,
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
        let s = sandbox_script(Path::new("/tmp/w"), &l, &cmd);
        for needle in [
            "mount --bind '/tmp/w' /mnt",
            "remount,bind,ro /mnt",
            "tmpfs",
            "/tmp",
            "prlimit --cpu=60",
            "--as=1073741824",
            "--nproc=32",
            "--fsize=67108864",
            "setpriv --reuid=65534",
            "env -i",
            "timeout -k 2 120",
            "'python3' 'a'\\''b'",
        ] {
            assert!(s.contains(needle), "missing {needle:?} in:\n{s}");
        }
        assert!(s.find("mount --bind").unwrap() < s.find("tmpfs").unwrap());
    }

    #[test]
    fn classify_cases() {
        let l = Limits::default(); // cpu 60, wall 120
        let c = |sb, killed, code, sig, el, cpu| classify(sb, killed, code, sig, el, cpu, &l).0;
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

    #[test]
    fn sandboxed_runs_as_nobody_without_network() {
        let require = std::env::var("CB_REQUIRE_SANDBOX").is_ok_and(|v| v == "1");
        if !crate::preflight::build_sandbox_available() {
            assert!(!require, "CB_REQUIRE_SANDBOX=1 but no sandbox available");
            eprintln!("skipped: no sandbox");
            return;
        }
        let dir = crate::paths::scratch("participant-sandbox");
        std::fs::create_dir_all(&dir).unwrap();
        let script = "[ \"$(id -u)\" = 65534 ] || exit 10; \
             ( : > ./x ) 2>/dev/null && exit 11; \
             [ -z \"$(ls -A /tmp)\" ] || exit 12; \
             ( exec 3<>/dev/tcp/1.1.1.1/443 ) 2>/dev/null && exit 13; \
             exit 0";
        let cmd = vec!["bash".into(), "-c".into(), script.into()];
        let r = run_in(&dir, &cmd, &Limits::default(), true).unwrap();
        assert_eq!(r.status, "ok", "{r:?}");
    }
}
