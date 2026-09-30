//! Path policy for a submission diff (PLAN.md §3, §4). Used by preflight's
//! self-test and by the PR gate: only editable paths may change, and nothing
//! inside them may be a symlink or a gitlink (the Heesch V12 lesson).
//!
//! Approach from provably.fast `public_preflight.py:667-684` (git diff with
//! `-z --no-renames`, every path checked), but new and deleted files inside
//! the editable surface are allowed here.

use crate::paths;
use std::path::Path;
use std::process::Command;

const SYMLINK: &str = "120000";
const GITLINK: &str = "160000";

/// Every refusal reason, or `Ok` if the diff `base..head` in `repo` is allowed.
/// `editable` is relative to the repo root.
pub fn check_diff(
    repo: &Path,
    base: &str,
    head: &str,
    editable: &[String],
) -> Result<(), Vec<String>> {
    let out = Command::new("git")
        .args([
            "-c",
            "core.quotepath=false",
            "diff",
            "--raw",
            "-z",
            "--no-renames",
            "--no-abbrev",
            base,
            head,
        ])
        .current_dir(repo)
        .output()
        .map_err(|e| vec![format!("git diff: {e}")])?;
    if !out.status.success() {
        return Err(vec![format!(
            "git diff: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )]);
    }
    check_raw(&out.stdout, editable)
}

/// Parses `git diff --raw -z` output: `:old_mode new_mode old_sha new_sha status\0path\0`.
fn check_raw(raw: &[u8], editable: &[String]) -> Result<(), Vec<String>> {
    let roots: Vec<&str> = editable.iter().map(|s| s.trim_end_matches('/')).collect();
    let mut errors = Vec::new();
    let mut fields = raw.split(|b| *b == 0).filter(|f| !f.is_empty());
    let mut changed = 0;
    while let Some(header) = fields.next() {
        let header = String::from_utf8_lossy(header);
        let Some(path) = fields.next() else {
            errors.push(format!("malformed diff entry {header:?}"));
            break;
        };
        changed += 1;
        let path = String::from_utf8_lossy(path);
        let parts: Vec<&str> = header.trim_start_matches(':').split(' ').collect();
        let [_old_mode, new_mode, _, _, status] = parts[..] else {
            errors.push(format!("{path}: malformed diff entry"));
            continue;
        };
        if !paths::within(&[path.as_ref()], &roots) {
            errors.push(format!("{path}: outside the editable paths"));
        }
        if status != "D" {
            match new_mode {
                SYMLINK => errors.push(format!("{path}: symlink")),
                GITLINK => errors.push(format!("{path}: gitlink (submodule)")),
                "100644" | "100755" => {}
                m => errors.push(format!("{path}: unexpected mode {m}")),
            }
        }
    }
    if changed == 0 {
        errors.push("empty diff".into());
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Builds a scratch repo, commits one allowed and several forbidden changes
/// and checks each verdict. Proves the gate works on this machine's git.
pub fn self_test(editable: &str) -> Result<(), String> {
    let dir = paths::scratch("policy");
    std::fs::create_dir_all(dir.join(editable)).map_err(|e| e.to_string())?;
    let git = |args: &[&str]| -> Result<String, String> {
        let out = Command::new("git")
            .args([
                "-c",
                "user.name=cb",
                "-c",
                "user.email=cb@localhost",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(&dir)
            .output()
            .map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
        } else {
            Err(format!(
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            ))
        }
    };
    let write =
        |rel: &str, body: &str| std::fs::write(dir.join(rel), body).map_err(|e| e.to_string());
    git(&["init", "-q"])?;
    write("judge.sh", "judge\n")?;
    write(&format!("{editable}/a.txt"), "a\n")?;
    git(&["add", "-A"])?;
    git(&["commit", "-qm", "base"])?;
    let base = git(&["rev-parse", "HEAD"])?;
    let roots = vec![editable.to_string()];

    // `change` edits the tree; it is staged with `add -A` unless it staged itself.
    let case = |name: &str,
                change: &dyn Fn() -> Result<(), String>,
                allowed: bool|
     -> Result<(), String> {
        git(&["checkout", "-q", "--detach", &base])?;
        if name != "gitlink inside" {
            change()?;
            git(&["add", "-A"])?;
        } else {
            change()?; // a gitlink exists only in the index; add -A would drop it
        }
        git(&["commit", "-qm", name])?;
        let head = git(&["rev-parse", "HEAD"])?;
        let verdict = check_diff(&dir, &base, &head, &roots);
        if verdict.is_ok() != allowed {
            return Err(format!(
                "policy self-test {name}: expected {}, got {verdict:?}",
                if allowed { "allowed" } else { "refused" }
            ));
        }
        Ok(())
    };
    case(
        "edit inside",
        &|| write(&format!("{editable}/a.txt"), "b\n"),
        true,
    )?;
    case(
        "new file inside",
        &|| write(&format!("{editable}/new.txt"), "n\n"),
        true,
    )?;
    case("edit judge", &|| write("judge.sh", "pwned\n"), false)?;
    case(
        "symlink inside",
        &|| {
            std::os::unix::fs::symlink("../judge.sh", dir.join(editable).join("link"))
                .map_err(|e| e.to_string())
        },
        false,
    )?;
    case(
        "gitlink inside",
        &|| {
            let sha = git(&["rev-parse", "HEAD"])?;
            git(&[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("{GITLINK},{sha},{editable}/sub"),
            ])
            .map(|_| ())
        },
        false,
    )?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(entries: &[(&str, &str, &str)]) -> Vec<u8> {
        let z = "0".repeat(40);
        let mut out = Vec::new();
        for (mode, status, path) in entries {
            out.extend(format!(":100644 {mode} {z} {z} {status}\0{path}\0").as_bytes());
        }
        out
    }

    #[test]
    fn raw_parsing() {
        let ed = vec!["candidate/".to_string()];
        assert!(check_raw(&raw(&[("100644", "M", "candidate/src/lib.rs")]), &ed).is_ok());
        assert!(check_raw(&raw(&[("000000", "D", "candidate/old.rs")]), &ed).is_ok());
        assert!(check_raw(&raw(&[("100644", "M", "verifier/run")]), &ed).is_err());
        assert!(check_raw(&raw(&[("100644", "M", "candidates/x")]), &ed).is_err());
        assert!(check_raw(&raw(&[("120000", "A", "candidate/l")]), &ed).is_err());
        assert!(check_raw(&raw(&[("160000", "A", "candidate/s")]), &ed).is_err());
        assert!(check_raw(b"", &ed).is_err());
        let errs = check_raw(
            &raw(&[("100644", "M", "candidate/a"), ("120000", "A", "bench/x")]),
            &ed,
        )
        .unwrap_err();
        assert_eq!(errs.len(), 2, "{errs:?}"); // outside + symlink
    }

    #[test]
    fn self_test_passes_on_real_git() {
        self_test("challenges/toy/candidate").unwrap();
    }
}
