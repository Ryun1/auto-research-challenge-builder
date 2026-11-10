//! `challenges/<slug>/challenge.toml` (PLAN.md §2) and `cb new`.
//!
//! Field names follow Yukon's benchmark.json where the two overlap, so a
//! later `cb export yukon` is mechanical:
//! `paths.editable` = editablePaths, `paths.max_bytes` = maxSubmissionBytes,
//! `objective.min_improvement_bips` = minScoreImprovementBips,
//! `commands.setup` = setupCommand, `challenge.direction` = direction (+/-).

use crate::paths;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub challenge: Meta,
    pub paths: Paths,
    pub commands: Commands,
    pub objective: Objective,
    pub perf: Option<Perf>,
    /// Rust kit only.
    pub rust: Option<RustOpts>,
    /// Haskell source policy (no kit): checked by `cb gate` and preflight.
    pub haskell: Option<HaskellOpts>,
    /// Build in CI with a read-only filesystem except these (see preflight::Mode::Build).
    pub build_sandbox: Option<BuildSandbox>,
    #[serde(default)]
    pub inputs: Inputs,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    pub slug: String,
    pub family: String,
    pub direction: String,
    pub status: String,
    pub kit: Option<String>,
    /// GitHub Actions runner label. Pin the image: counts shift between OS versions.
    pub runner: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paths {
    /// Relative to the challenge dir. Everything else is refused.
    pub editable: Vec<String>,
    pub max_bytes: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Commands {
    pub setup: String,
    /// The only step that treats submission bytes as code. Absent for math:
    /// the verifier checks data and never runs participant code.
    pub build: Option<String>,
    /// Exit 0 + JSON `{ok, detail}`.
    pub verify: String,
    /// JSON `{metrics: {...}}`.
    pub score: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Objective {
    /// Metric name (or expression over metrics) that ranks.
    pub expr: String,
    /// The challenge maker's call; preflight only warns if it's under noise.
    pub min_improvement_bips: u32,
}

/// Paired wall-time runs: mirrored blocks BCCB/CBBC (provably.fast v3).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Perf {
    pub blocks: u32,
    /// Cap on rel-MAD = median(|x − median|) / median, per side.
    pub max_dispersion: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RustOpts {
    /// Opt-in token groups: "unsafe", "asm", "target_feature".
    #[serde(default)]
    pub allow: Vec<String>,
}

/// What a sandboxed CI build may write. Everything else under `$HOME`,
/// `/opt`, `/usr/local` and the workspace is read-only.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildSandbox {
    /// Absolute or `~/`-relative dirs (caches, build dirs).
    #[serde(default)]
    pub writable: Vec<String>,
    /// Files in the workspace the build writes (e.g. `.bin/candidate`).
    #[serde(default)]
    pub outputs: Vec<String>,
}

/// The editable path is a Haskell package; only `src` may differ from the
/// baseline, under the allowlist in `haskellpolicy`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HaskellOpts {
    #[serde(default = "default_hs_src")]
    pub src: String,
}

fn default_hs_src() -> String {
    "src".into()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inputs {
    /// Fresh inputs per run; seed drawn by the harness after the build, recorded after.
    pub salted: bool,
}

impl Default for Inputs {
    fn default() -> Self {
        Inputs { salted: true }
    }
}

pub const FAMILIES: [&str; 4] = ["perf", "resource", "math", "proof"];
pub const KITS: [&str; 1] = ["rust"];

impl Spec {
    pub fn parse(text: &str) -> Result<Spec, String> {
        let spec: Spec = toml::from_str(text).map_err(|e| e.to_string())?;
        spec.validate()?;
        Ok(spec)
    }

    pub fn load(dir: &Path) -> Result<Spec, String> {
        let path = dir.join("challenge.toml");
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let spec = Spec::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let dir_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if spec.challenge.slug != dir_name {
            return Err(format!(
                "{}: slug {:?} doesn't match directory",
                path.display(),
                spec.challenge.slug
            ));
        }
        Ok(spec)
    }

    fn validate(&self) -> Result<(), String> {
        let c = &self.challenge;
        if !is_slug(&c.slug) {
            return Err(format!("slug {:?}: use lowercase a-z, 0-9 and -", c.slug));
        }
        if !FAMILIES.contains(&c.family.as_str()) {
            return Err(format!("family {:?}: one of {FAMILIES:?}", c.family));
        }
        if !matches!(c.direction.as_str(), "minimise" | "maximise") {
            return Err("direction: minimise or maximise".into());
        }
        if !matches!(c.status.as_str(), "draft" | "open" | "closed") {
            return Err("status: draft, open or closed".into());
        }
        if let Some(kit) = &c.kit {
            if !KITS.contains(&kit.as_str()) {
                return Err(format!("kit {kit:?}: one of {KITS:?}"));
            }
        }
        if c.runner.trim().is_empty() || c.runner.ends_with("-latest") {
            return Err(format!(
                "runner {:?}: pin an image (e.g. ubuntu-24.04), not -latest",
                c.runner
            ));
        }
        let editable: Vec<&str> = self
            .paths
            .editable
            .iter()
            .map(|p| p.trim_end_matches('/'))
            .collect();
        if editable.is_empty() || !editable.iter().all(|p| paths::is_canonical(p)) {
            return Err("paths.editable: non-empty, relative, no . or .. components".into());
        }
        for protected in [
            "challenge.toml",
            "verifier",
            "bench",
            "baseline",
            "reference",
            "oracle",
        ] {
            if editable
                .iter()
                .any(|p| *p == protected || p.starts_with(&format!("{protected}/")))
            {
                return Err(format!("paths.editable: {protected} is part of the judge"));
            }
        }
        if self.paths.max_bytes == 0 {
            return Err("paths.max_bytes must be > 0".into());
        }
        match (c.family.as_str(), &self.perf, &self.commands.build) {
            ("perf", None, _) => return Err("perf family needs [perf]".into()),
            (f, Some(_), _) if f != "perf" => {
                return Err(format!("[perf] is only for the perf family, not {f}"))
            }
            ("math", _, Some(_)) => {
                return Err("math family: no build step; the verifier checks data only".into())
            }
            _ => {}
        }
        match (c.kit.as_deref(), &self.rust) {
            (Some("rust"), None) => return Err("kit rust needs a [rust] section".into()),
            (Some("rust"), Some(_)) if c.family != "perf" => {
                return Err(
                    "kit rust is for the perf family (instruction count + wall-time guard)".into(),
                )
            }
            (k, Some(_)) if k != Some("rust") => {
                return Err("[rust] is only for kit = \"rust\"".into())
            }
            (_, Some(r)) => {
                if let Some(bad) = r
                    .allow
                    .iter()
                    .find(|a| !crate::rustpolicy::OPT_IN.contains(&a.as_str()))
                {
                    return Err(format!(
                        "[rust] allow: {bad:?} is not one of {:?}",
                        crate::rustpolicy::OPT_IN
                    ));
                }
            }
            _ => {}
        }
        if let Some(p) = &self.perf {
            if p.blocks == 0 || !(p.max_dispersion > 0.0 && p.max_dispersion < 1.0) {
                return Err("perf: blocks > 0 and 0 < max_dispersion < 1".into());
            }
        }
        Ok(())
    }
}

pub fn is_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !s.starts_with('-')
        && !s.ends_with('-')
}

// ---- cb new ----

fn spec_toml(slug: &str, family: &str, kit: Option<&str>) -> String {
    let (direction, expr, build, perf) = match family {
        "perf" => ("maximise", "speedup", Some("./build.sh"), "\n[perf]\nblocks = 4             # mirrored BCCB/CBBC blocks\nmax_dispersion = 0.08  # rel-MAD cap per side (provably.fast v3)\n"),
        "resource" => ("minimise", "count", Some("./build.sh"), ""),
        "math" => ("maximise", "value", None, ""),
        _ => ("minimise", "cost", Some("./build.sh"), ""),
    };
    let (editable, rust) = match kit {
        Some("rust") => (
            "candidate/src",
            "\n[rust]\nallow = []  # opt in: \"unsafe\", \"asm\", \"target_feature\" (and drop the forbid lint in candidate/Cargo.toml)\n",
        ),
        _ => ("candidate/", ""),
    };
    let kit = kit.map(|k| format!("kit = \"{k}\"\n")).unwrap_or_default();
    let build = build
        .map(|b| {
            format!("build  = \"{b}\"     # the only step that treats submission bytes as code\n")
        })
        .unwrap_or_else(|| {
            "# no build: the verifier checks data and never runs participant code\n".into()
        });
    format!(
        r#"[challenge]
slug      = "{slug}"
family    = "{family}"
direction = "{direction}"
status    = "draft"          # draft | open | closed
{kit}runner    = "ubuntu-24.04"   # pinned image

[paths]
editable  = ["{editable}"]   # everything else is refused
max_bytes = 8388608

[commands]
setup  = "./setup.sh"
{build}verify = "./verifier/run"  # exit 0 + JSON {{ok, detail}}
score  = "./bench/run"     # JSON {{metrics: {{...}}}}

[objective]
expr                 = "{expr}"
min_improvement_bips = 100  # your call (100 = 1%); preflight warns if it's under measured noise
{perf}{rust}
[inputs]
salted = true  # seed drawn by the harness after the build, recorded after
"#
    )
}

pub const THREAT_MODEL: &str = r#"# Threat model

Required before launch; reviewed in the PR that opens the challenge.
Answer every question. "N/A" needs a reason.

## How could a wrong answer pass the verifier?

- Finite or reusable inputs (is the input set fresh per run? how many?):
- Caching or precomputation (can a table or memo pay off without knowing the seed?):
- Reading the reference, oracle, seed or expected output:
- Timing tricks (detecting the benchmark, doing less work when measured):
- Writing the score file directly, or editing anything outside the editable paths:

## Does the benchmark interface match real use?

(The ECDSA.fail lesson: its windowed-Shor interface diverged from deployment.)

## What is the known bound or headroom?

(Link a paper, a faster implementation, or a lower bound.)

## What does a large outlier (+100%) trigger?

Manual review; never auto-promote.

- Who reviews:
"#;

fn submitting(slug: &str) -> String {
    format!(
        r#"# Submitting to {slug}

1. Open a PR that changes only files under the editable paths in
   `challenge.toml` (`candidate/` by default). Anything else is refused,
   as are symlinks and submodules.
2. CI verifies and scores it and comments the result. That score is a claim.
3. A maintainer merges; a clean re-run on `main` with a fresh seed decides the
   record and appends it to `results.jsonl`. Rejected and slower attempts are
   recorded too.
"#
    )
}

const TODO_SCRIPT: &str = "#!/usr/bin/env bash\n# TODO: implement. Fails closed until then.\nset -euo pipefail\necho '{\"ok\": false, \"detail\": \"not implemented\"}'\nexit 1\n";

/// Kit templates, e.g. kits/rust. Resolved at build time: `cb` runs from this repo.
const KITS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../kits");

/// Recursive copy, skipping build output. Keeps the executable bit.
fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let (from, to) = (entry.path(), dst.join(entry.file_name()));
        if entry.file_name() == "target" {
            continue;
        }
        if entry.file_type()?.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?; // copies permissions
        }
    }
    Ok(())
}

/// Creates `<root>/<slug>/`. Refuses to touch an existing directory.
pub fn scaffold(
    root: &Path,
    slug: &str,
    family: &str,
    kit: Option<&str>,
) -> Result<std::path::PathBuf, String> {
    if !is_slug(slug) {
        return Err(format!("slug {slug:?}: use lowercase a-z, 0-9 and -"));
    }
    let spec = spec_toml(slug, family, kit);
    Spec::parse(&spec).map_err(|e| format!("internal: generated spec invalid: {e}"))?;
    let dir = root.join(slug);
    if dir.exists() {
        return Err(format!("{} already exists", dir.display()));
    }
    let write = |rel: &str, body: &str, exec: bool| -> Result<(), String> {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap())
            .map_err(|e| format!("{}: {e}", path.display()))?;
        std::fs::write(&path, body).map_err(|e| format!("{}: {e}", path.display()))?;
        if exec {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
        Ok(())
    };
    if let Some(kit) = kit {
        let src = Path::new(KITS_DIR).join(kit);
        copy_tree(&src, &dir).map_err(|e| format!("copying kit {}: {e}", src.display()))?;
    }
    write("challenge.toml", &spec, false)?;
    write("THREAT_MODEL.md", THREAT_MODEL, false)?;
    write("SUBMITTING.md", &submitting(slug), false)?;
    if kit.is_some() {
        return Ok(dir); // the kit ships setup, build, verifier, bench, baseline, broken
    }
    write(
        "setup.sh",
        "#!/usr/bin/env bash\nset -euo pipefail\n# Install toolchains. Idempotent.\n",
        true,
    )?;
    if family != "math" {
        write("build.sh", TODO_SCRIPT, true)?;
    }
    write("verifier/run", TODO_SCRIPT, true)?;
    write("bench/run", TODO_SCRIPT, true)?;
    write(
        "baseline/README.md",
        "Reference solution. Must pass the verifier (preflight checks).\n",
        false,
    )?;
    write(
        "candidate/README.md",
        "The editable surface. Starts as a copy of the baseline.\n",
        false,
    )?;
    for (name, what) in [
        (
            "off-by-one",
            "an answer that is wrong by one (last element, last bit, boundary)",
        ),
        ("partial", "an answer that does only part of the work"),
        (
            "constant",
            "a constant or cached answer that ignores the input",
        ),
    ] {
        let body = format!(
            "Put the files that turn the baseline into {what} here: preflight copies them\nover baseline/. The result must build and then FAIL the verifier.\n"
        );
        write(&format!("preflight/broken/{name}/README.md"), &body, false)?;
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("cb-new-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn scaffold_every_family_loads() {
        let root = tmp("fam");
        for f in FAMILIES {
            let slug = format!("toy-{f}");
            let dir = scaffold(&root, &slug, f, None).unwrap();
            let spec = Spec::load(&dir).unwrap();
            assert_eq!(spec.challenge.family, f);
            assert_eq!(spec.perf.is_some(), f == "perf");
            assert_eq!(spec.commands.build.is_some(), f != "math");
            assert_eq!(dir.join("build.sh").exists(), f != "math");
            for rel in [
                "THREAT_MODEL.md",
                "SUBMITTING.md",
                "verifier/run",
                "bench/run",
                "baseline",
                "candidate",
                "preflight/broken/constant",
            ] {
                assert!(dir.join(rel).exists(), "{f}: {rel}");
            }
            // Scaffolded verifier fails closed.
            let out = std::process::Command::new(dir.join("verifier/run"))
                .output()
                .unwrap();
            assert!(!out.status.success());
        }
        assert!(scaffold(&root, "toy-perf", "perf", None).is_err()); // exists
        assert!(scaffold(&root, "Bad_Slug", "perf", None).is_err());
        let kit = scaffold(&root, "kit", "perf", Some("rust")).unwrap();
        assert_eq!(
            Spec::load(&kit).unwrap().challenge.kit.as_deref(),
            Some("rust")
        );
    }

    #[test]
    fn validation() {
        let good = spec_toml("x", "perf", None);
        assert!(Spec::parse(&good).is_ok());
        let bad = |from: &str, to: &str| {
            assert!(good.contains(from), "{from}");
            Spec::parse(&good.replacen(from, to, 1)).unwrap_err()
        };
        bad("ubuntu-24.04", "ubuntu-latest");
        bad(r#"["candidate/"]"#, r#"["../x"]"#);
        bad(r#"["candidate/"]"#, r#"["verifier/run"]"#);
        bad(r#"["candidate/"]"#, "[]");
        bad("family    = \"perf\"", "family    = \"speed\"");
        bad("direction = \"maximise\"", "direction = \"up\"");
        bad("max_dispersion = 0.08", "max_dispersion = 0");
        bad("[inputs]", "typo = 1\n[inputs]");
        // perf without [perf], math with a build step.
        let resource = spec_toml("x", "resource", None);
        assert!(
            Spec::parse(&resource.replace("family    = \"resource\"", "family    = \"perf\""))
                .is_err()
        );
        let math = spec_toml("x", "math", None);
        assert!(
            Spec::parse(&math.replace("[commands]\n", "[commands]\nbuild = \"./b\"\n")).is_err()
        );
    }
}
