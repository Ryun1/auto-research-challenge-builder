//! `cb measure` as a process, driving `cb participant` from the judge.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn put(p: &Path, body: &str, mode: u32) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode)).unwrap();
}

const SPEC: &str = r#"[challenge]
slug   = "toy"
type   = "research"
status = "open"
runner = "ubuntu-24.04"

[commands]
setup = "./setup.sh"

[submissions]
max_bytes  = 1048576
toolchains = []
writable   = []

[limits]
cpu_secs   = 60
memory_mib = 1024
processes  = 32
file_mib   = 64
wall_secs  = 60

[[contracts]]
id     = "main@1"
status = "open"
judge  = "contracts/main/judge"
interface    = "contracts/main/INTERFACE.md"
choice_space = "contracts/main/CHOICES.md"

[[properties]]
id       = "correct"
check    = "mechanical"
contract = "main@1"

[[axes]]
id        = "wall_secs"
unit      = "s"
direction = "minimise"
repeats   = 2
tolerance = 0.5

[[axes]]
id        = "peak_rss_mib"
unit      = "MiB"
direction = "minimise"
repeats   = 1
tolerance = 0.5

[taxonomy]
file = "taxonomy.toml"
"#;

const TAXONOMY: &str = r#"version = 1

[[axes]]
id         = "approach"
definition = "how"
values = [ { id = "baseline", definition = "the baseline" } ]
"#;

const DECL: &str = r#"id        = "baseline"
contract  = "main@1"
novelty   = "new"
direction = "baseline"
taxonomy  = { approach = "baseline" }
summary   = "reference"
"#;

const JUDGE: &str = r#"#!/usr/bin/env bash
set -u
T=$(mktemp -d)
cd "$CB_SUBMISSION_DIR"
out=$("$CB_BIN" participant --report "$T/r.json" -- ./answer.sh)
field() { sed -n "s/.*\"$1\": *\([0-9.]*\).*/\1/p" "$T/r.json" | head -1; }
ok=false; [ "$out" = 42 ] && ok=true
printf '{"ok": %s, "inputs": "fixed", "checks": [{"property": "correct", "ok": %s, "detail": ""}], "axes": {"wall_secs": %s, "peak_rss_mib": %s}}\n' \
  "$ok" "$ok" "$(field wall_secs)" "$(field peak_rss_mib)"
"#;

#[test]
fn measure_drives_participant() {
    let tmp = std::env::temp_dir().join(format!("cb-it-measure-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let root = tmp.join("challenges");
    let d = root.join("toy");
    put(&d.join("challenge.toml"), SPEC, 0o644);
    put(&d.join("taxonomy.toml"), TAXONOMY, 0o644);
    put(&d.join("setup.sh"), "#!/usr/bin/env bash\nexit 0\n", 0o755);
    put(&d.join("contracts/main/judge"), JUDGE, 0o755);
    put(&d.join("contracts/main/INTERFACE.md"), "interface\n", 0o644);
    put(&d.join("contracts/main/CHOICES.md"), "choices\n", 0o644);
    let s = d.join("submissions/baseline");
    put(&s.join("submission.toml"), DECL, 0o644);
    put(&s.join("build.sh"), "#!/usr/bin/env bash\nexit 0\n", 0o755);
    put(
        &s.join("answer.sh"),
        "#!/usr/bin/env bash\necho 42\n",
        0o755,
    );
    let out = tmp.join("m.json");
    let st = std::process::Command::new(env!("CARGO_BIN_EXE_cb"))
        .args(["measure", "toy", "--root"])
        .arg(&root)
        .args(["--id", "baseline", "--out"])
        .arg(&out)
        .env("CB_BIN", env!("CARGO_BIN_EXE_cb"))
        .status()
        .unwrap();
    assert!(st.success());
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(v["status"], "valid", "{v}");
    assert_eq!(v["axes"]["wall_secs"]["n"], 2);
    assert!(v["axes"]["peak_rss_mib"]["median"].as_f64().unwrap() >= 0.0);
    let _ = std::fs::remove_dir_all(&tmp);
}
