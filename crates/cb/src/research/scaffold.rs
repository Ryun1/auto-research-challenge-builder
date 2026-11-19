//! `cb new --type research` (design spec §9).

use super::spec::{Declaration, ResearchSpec, Taxonomy};
use crate::challenge::is_slug;
use std::path::{Path, PathBuf};

fn spec_toml(slug: &str) -> String {
    format!(
        r#"[challenge]
slug   = "{slug}"
type   = "research"
status = "draft"            # draft | open | closed
runner = "ubuntu-24.04"     # pinned image

[commands]
setup = "./setup.sh"        # trusted, before any submission byte runs; may use the network

[submissions]
max_bytes  = 16777216       # per submission dir
toolchains = []             # what builds offline on the runner, e.g. ["python3 (ubuntu-24.04 image)"]
writable   = []             # build-sandbox overlays (writes vanish after the build)

[limits]                    # per participant process tree, enforced by `cb participant`
cpu_secs   = 60
memory_mib = 1024
processes  = 32
file_mib   = 64             # largest file it may write, and its private /tmp
wall_secs  = 120

[[contracts]]
id           = "main@1"
status       = "open"       # open | deferred | closed
judge        = "contracts/main/judge"
interface    = "contracts/main/INTERFACE.md"
choice_space = "contracts/main/CHOICES.md"

[[contracts]]
id          = "streaming@0"  # example of a deferred contract: listed, never measured
status      = "deferred"
admits_when = "TODO: what would make this direction checkable (or delete this block)"

[[properties]]
id       = "correct"
check    = "mechanical"     # mechanical | mechanical-conjectured | declared
contract = "main@1"

[[axes]]
id        = "time_ms"
unit      = "ms"
direction = "minimise"      # minimise | maximise; never combined
repeats   = 3               # judge runs that sample this axis (same seed)
tolerance = 0.2             # relative; the two runs on main must agree within it
tolerance_abs = 0.0         # absolute floor, in the axis unit, for axes near zero

[[axes]]
id        = "size_bytes"
unit      = "bytes"
direction = "minimise"
repeats   = 1
tolerance = 0.0

[taxonomy]
file = "taxonomy.toml"
"#
    )
}

const TAXONOMY: &str = r#"version = 1

[[axes]]
id         = "approach"
definition = "TODO: how the submission goes about the task"
values = [
  { id = "baseline", definition = "TODO: what the baseline does" },
  { id = "other",    definition = "TODO: replace with the real directions" },
]

[[rules]]
text = "TODO: example rule, e.g. a value that is only plausible with another"
"#;

const BASELINE_DECL: &str = r#"id        = "baseline"
contract  = "main@1"
novelty   = "new"
direction = "baseline"
taxonomy  = { approach = "baseline" }
summary   = "The maintainer's reference submission."
"#;

const TODO_BUILD: &str = "#!/usr/bin/env bash\n# TODO: build the submission (offline). Fails closed until then.\nset -euo pipefail\necho 'not implemented' >&2\nexit 1\n";

const JUDGE: &str = r#"#!/usr/bin/env bash
# TODO: the judge for contract main@1. Fails closed until implemented.
#
# cb runs this once per repeat, cwd = the workspace, with the same seed on
# every repeat. Env: CB_SEED, CB_REPEAT (0-based), CB_CONTRACT,
# CB_SUBMISSION_DIR (absolute), CB_BIN (this cb), CB_LIMITS (JSON), CB_SANDBOX.
#
# It must:
#   - generate inputs from CB_SEED and print a digest of them as "inputs";
#   - call the participant ONLY as
#       "$CB_BIN" participant --report <file> -- ./<entrypoint>
#     (use paths relative to the submission dir; the report gives wall/CPU/RSS);
#   - check every mechanical property AND measure every axis on the SAME
#     outputs, so a submission cannot behave in one pass and cheat in another;
#   - never trust numbers the participant prints about itself;
#   - keep the seed, reference results and temp files private (mktemp -d, mode 700).
#
# Print one JSON object (max 64 KiB) and exit 0 when it ran:
#   {"ok": bool, "inputs": "<digest>",
#    "checks": [{"property": "correct", "ok": true, "detail": ""}],
#    "axes": {"time_ms": 1.0, "size_bytes": 1}}
set -euo pipefail
echo '{"ok": false, "inputs": "", "checks": [], "axes": {}}'
exit 1
"#;

const INTERFACE: &str = "# Contract main@1: interface\n\nTODO: what a submission must provide (the files `build.sh` leaves behind, how the\njudge talks to it, input and output formats, time and memory limits).\n";

const DIRECTIONS: &str = "# Directions\n\nWhat has been tried and what is open. Maintainers keep this file up to date (in the\nsame PR as any taxonomy edit); participants never edit it. A direction missing from\nit is still welcome.\n\n## Explored\n\nTODO: one short paragraph per direction with accepted submissions: what it does,\nwhich axes it moved (numbers from `results.jsonl`) and the submission ids.\n\n## Open\n\nTODO: for each, the mechanism, the expected effect on which axes, and the cheapest\ntest that could show it wrong. Say what is unknown rather than guess.\n";

const CHOICES: &str = "# Contract main@1: choice space\n\nTODO: the decisions open to participants (algorithms, data structures, languages,\ntrade-offs between axes). Not rules: a map of where submissions can differ.\n";

const HARNESS_README: &str = "Shared judge code (generators, reference solutions, checkers) used by the\ncontracts' judges. Trusted: only maintainers change it. Delete if unused.\n";

const BASELINE_README: &str = "The reference submission. Declares novelty `new`, direction `baseline`. Must build\nand measure `valid` (preflight checks).\n";

pub const THREAT_MODEL_RESEARCH: &str = r#"# Threat model

Required before launch; reviewed in the PR that opens the challenge.
Answer every question. "N/A" needs a reason.

## How could a submission fake an axis?

- Reporting numbers about itself instead of being measured (does the judge measure the same outputs it checks?):
- Timing tricks (detecting the judge, doing less work when measured):

## How could a submission pass the checks while breaking a property?

- Which properties are mechanical, and what escapes each check:
- Declared properties (what stops a false claim from misleading readers?):

## Could a submission read the seed or the reference?

- Where the seed, reference results and judge temp files live, and who can read them:

## Could a submission overfit fixed inputs?

- Are inputs fresh per seed, and how many distinct ones are there?:

## Could a submission persist state between runs?

- Caches, files, processes or network that survive an invocation (the sandbox gives a private /tmp and no network):

## What does a large outlier trigger?

Manual review; the result is flagged, not trusted.

- Who reviews:
"#;

fn submitting(slug: &str) -> String {
    format!(
        r#"# Submitting to {slug}

A submission is one new directory, `challenges/{slug}/submissions/<id>/`, in a PR
that changes nothing else. Merged submissions are immutable.

1. Pick an `<id>` (lowercase a-z, 0-9, `-`). Put `submission.toml` and an
   executable `build.sh` in the directory, plus whatever your code needs. A
   whole codebase is fine, under `max_bytes` in `challenge.toml`. `build.sh`
   runs offline in a sandbox.
2. `submission.toml` declares: `id` (the dir name), `contract`, `novelty`,
   `direction`, `parent`, `taxonomy`, `summary`, optional `claims` (for declared
   properties only) and `model`.
3. Novelty is your own declaration and is never judged. `new` starts a direction
   `<name>` that no accepted submission uses. `builds-on` names an accepted
   `parent`; copy its directory and edit it. The direction is inherited.
4. CI builds your submission and runs the contract's judge, which checks the
   properties and measures every axis. Those numbers are a claim from the PR run.
5. There is no record to beat and no minimum improvement. If the checks pass, the
   PR is landed on main automatically, two clean runs there record it on the map,
   and the PR is closed. Failing and noisy attempts are recorded too.
"#
    )
}

fn prompt(slug: &str) -> String {
    format!(
        r#"# Prompt for {slug}

You are working on the research challenge `{slug}`. Read `contracts/main/INTERFACE.md`
and `contracts/main/CHOICES.md`, then `DIRECTIONS.md` (what has been tried and what is
open), `submissions/` and `taxonomy.toml`.

Create ONE new directory `submissions/<id>/` and change nothing else. It needs a
`submission.toml` and an executable `build.sh`; see `SUBMITTING.md` for the fields.
Either start a new direction or copy an accepted submission's directory and build on it.
Declare novelty honestly. You choose which axes in `challenge.toml` to push: they are
measured, recorded and never ranked or combined, so a trade-off is a fine result.
"#
    )
}

fn readme(slug: &str) -> String {
    format!(
        "# {slug}\n\nTODO: what this research challenge explores and why it has no single winner.\n\nSee `SUBMITTING.md`, `PROMPT.md`, `DIRECTIONS.md`, `contracts/main/` and `taxonomy.toml`.\n"
    )
}

const BROKEN: [(&str, &str); 4] = [
    (
        "off-by-one",
        "an answer that is wrong by one (last element, last bit, boundary)",
    ),
    ("partial", "an answer that does only part of the work"),
    (
        "constant",
        "a constant or cached answer that ignores the input",
    ),
    ("self-report", ""),
];

/// Creates `<root>/<slug>/`. Refuses to touch an existing directory.
pub fn scaffold(root: &Path, slug: &str) -> Result<PathBuf, String> {
    if !is_slug(slug) {
        return Err(format!("slug {slug:?}: use lowercase a-z, 0-9 and -"));
    }
    let spec = spec_toml(slug);
    ResearchSpec::parse(&spec).map_err(|e| format!("internal: generated spec invalid: {e}"))?;
    Taxonomy::parse(TAXONOMY).map_err(|e| format!("internal: generated taxonomy invalid: {e}"))?;
    Declaration::parse(BASELINE_DECL)
        .map_err(|e| format!("internal: generated declaration invalid: {e}"))?;
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
    write("challenge.toml", &spec, false)?;
    write("taxonomy.toml", TAXONOMY, false)?;
    write(
        "setup.sh",
        "#!/usr/bin/env bash\nset -euo pipefail\n# Install toolchains the judge needs. Idempotent.\n",
        true,
    )?;
    write("contracts/main/judge", JUDGE, true)?;
    write("contracts/main/INTERFACE.md", INTERFACE, false)?;
    write("contracts/main/CHOICES.md", CHOICES, false)?;
    write("harness/README.md", HARNESS_README, false)?;
    write("submissions/baseline/submission.toml", BASELINE_DECL, false)?;
    write("submissions/baseline/build.sh", TODO_BUILD, true)?;
    write("submissions/baseline/README.md", BASELINE_README, false)?;
    for (name, what) in BROKEN {
        let body = if name == "self-report" {
            "A whole submission directory (submission.toml, build.sh, ...): the baseline that\nalso prints fake self-reported axes and writes fake results. Unlike the other\nvariants it must build and be VALID, with axes matching the baseline, because the\njudge must ignore what a participant says about itself.\n".to_string()
        } else {
            format!(
                "A whole submission directory (submission.toml, build.sh, ...) with {what}.\nIt must build and then be REJECTED by the judge.\n"
            )
        };
        write(&format!("preflight/broken/{name}/README.md"), &body, false)?;
    }
    write("THREAT_MODEL.md", THREAT_MODEL_RESEARCH, false)?;
    write("SUBMITTING.md", &submitting(slug), false)?;
    write("PROMPT.md", &prompt(slug), false)?;
    write("README.md", &readme(slug), false)?;
    write("DIRECTIONS.md", DIRECTIONS, false)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::challenge::challenge_type;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cb-rnew-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn scaffold_research_loads() {
        let root = tmp("a");
        let dir = scaffold(&root, "toy").unwrap();
        let spec = ResearchSpec::load(&dir).unwrap();
        assert_eq!(challenge_type(&dir).unwrap(), "research");
        for rel in [
            "taxonomy.toml",
            "setup.sh",
            "contracts/main/judge",
            "contracts/main/INTERFACE.md",
            "contracts/main/CHOICES.md",
            "harness/README.md",
            "submissions/baseline/submission.toml",
            "submissions/baseline/build.sh",
            "submissions/baseline/README.md",
            "preflight/broken/off-by-one/README.md",
            "preflight/broken/partial/README.md",
            "preflight/broken/constant/README.md",
            "preflight/broken/self-report/README.md",
            "THREAT_MODEL.md",
            "SUBMITTING.md",
            "PROMPT.md",
            "README.md",
            "DIRECTIONS.md",
        ] {
            assert!(dir.join(rel).exists(), "{rel}");
        }
        use std::os::unix::fs::PermissionsExt;
        for rel in [
            "contracts/main/judge",
            "submissions/baseline/build.sh",
            "setup.sh",
        ] {
            let mode = std::fs::metadata(dir.join(rel))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o111, 0o111, "{rel}");
        }
        let out = std::process::Command::new(dir.join("contracts/main/judge"))
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).contains("\"ok\": false"));
        let text =
            std::fs::read_to_string(dir.join("submissions/baseline/submission.toml")).unwrap();
        let d = Declaration::parse(&text).unwrap();
        assert!(
            d.check(&spec, "baseline").is_empty(),
            "{:?}",
            d.check(&spec, "baseline")
        );
        assert_eq!(spec.axes.len(), 2);
        assert!(scaffold(&root, "toy").is_err());
        assert!(scaffold(&root, "Bad_Slug").is_err());
    }

    #[test]
    fn threat_model_has_questions() {
        assert!(THREAT_MODEL_RESEARCH.matches("\n- ").count() >= 6);
        assert!(THREAT_MODEL_RESEARCH.contains("- Who reviews:"));
    }
}
