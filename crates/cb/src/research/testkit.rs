//! Test fixtures for research commands.

use super::scaffold::scaffold;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// A judge reading `$CB_SUBMISSION_DIR/answer`; ok when it is 42.
pub const GOOD_JUDGE: &str = r#"#!/usr/bin/env bash
a=$(cat "$CB_SUBMISSION_DIR/answer")
if [ "$a" = 42 ]; then ok=true; else ok=false; fi
printf '{"ok": %s, "inputs": "seed-%s", "checks": [{"property": "correct", "ok": %s, "detail": ""}], "axes": {"time_ms": 1.5, "size_bytes": %s}}\n' "$ok" "$CB_SEED" "$ok" "$a"
"#;

fn put(path: &Path, body: &str, mode: u32) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// A scaffolded research challenge, open, with the given judge and a passing baseline.
pub fn toy(root: &Path, slug: &str, judge: &str) -> PathBuf {
    let dir = scaffold(root, slug).unwrap();
    let p = dir.join("challenge.toml");
    let t = std::fs::read_to_string(&p).unwrap();
    assert!(t.contains("status = \"draft\""));
    std::fs::write(
        &p,
        t.replacen("status = \"draft\"", "status = \"open\"  ", 1),
    )
    .unwrap();
    put(&dir.join("contracts/main/judge"), judge, 0o755);
    let b = dir.join("submissions/baseline");
    put(&b.join("build.sh"), "#!/usr/bin/env bash\nexit 0\n", 0o755);
    put(&b.join("answer"), "42\n", 0o644);
    dir
}

/// Answers every question in THREAT_MODEL.md.
pub fn answer_threat_model(dir: &Path) {
    let p = dir.join("THREAT_MODEL.md");
    let t = std::fs::read_to_string(&p).unwrap();
    let out: String = t
        .lines()
        .map(|l| {
            if l.trim().starts_with("- ") && l.trim().ends_with(':') {
                format!("{l} answered in the toy\n")
            } else {
                format!("{l}\n")
            }
        })
        .collect();
    std::fs::write(&p, out).unwrap();
}

/// One broken variant: a whole submission whose `answer` is `answer`.
pub fn broken(dir: &Path, name: &str, answer: &str) {
    let b = dir.join("preflight/broken").join(name);
    put(
        &b.join("submission.toml"),
        &format!(
            "id = \"{name}\"\ncontract = \"main@1\"\nnovelty = \"new\"\ndirection = \"{name}\"\nsummary = \"broken variant {name}\"\n"
        ),
        0o644,
    );
    put(&b.join("build.sh"), "#!/usr/bin/env bash\nexit 0\n", 0o755);
    put(&b.join("answer"), &format!("{answer}\n"), 0o644);
}

/// The four required broken variants (self-report claims fake numbers but answers 42).
pub fn broken_all(dir: &Path) {
    broken(dir, "off-by-one", "41");
    broken(dir, "partial", "0");
    broken(dir, "constant", "7");
    broken(dir, "self-report", "42");
    put(
        &dir.join("preflight/broken/self-report/self-report.txt"),
        "time_ms=0.001 size_bytes=1\n",
        0o644,
    );
}

/// A toy ready to launch: threat model answered, broken variants in place.
pub fn toy_ready(root: &Path, slug: &str, judge: &str) -> PathBuf {
    let dir = toy(root, slug, judge);
    answer_threat_model(&dir);
    broken_all(&dir);
    dir
}
