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
