//! Research challenge spec (`challenge.toml` with `type = "research"`),
//! taxonomy, and per-submission declarations (`submission.toml`).
//! See docs/superpowers/specs/2026-11-19-research-challenges-design.md §1-§2.

use crate::challenge::is_slug;
use crate::paths;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResearchSpec {
    pub challenge: Meta,
    pub commands: Commands,
    pub submissions: Submissions,
    #[serde(default)]
    pub limits: Limits,
    pub contracts: Vec<Contract>,
    #[serde(default)]
    pub properties: Vec<Property>,
    pub axes: Vec<Axis>,
    pub taxonomy: TaxonomyRef,
    #[serde(skip)]
    pub tax: Taxonomy,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    pub slug: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub status: String,
    pub runner: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Commands {
    pub setup: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Submissions {
    pub max_bytes: u64,
    #[serde(default)]
    pub toolchains: Vec<String>,
    #[serde(default)]
    pub writable: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub cpu_secs: u64,
    pub memory_mib: u64,
    pub processes: u64,
    pub file_mib: u64,
    pub wall_secs: u64,
    /// Time limit for the (untrusted) submission build.
    #[serde(default = "default_build_secs")]
    pub build_secs: u64,
}

fn default_build_secs() -> u64 {
    1800
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            cpu_secs: 60,
            memory_mib: 1024,
            processes: 32,
            file_mib: 64,
            wall_secs: 120,
            build_secs: default_build_secs(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    pub id: String,
    pub status: String,
    pub judge: Option<String>,
    pub interface: Option<String>,
    pub choice_space: Option<String>,
    pub admits_when: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Property {
    pub id: String,
    pub check: String,
    pub contract: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Axis {
    pub id: String,
    pub unit: String,
    pub direction: String,
    #[serde(default = "one")]
    pub repeats: u32,
    #[serde(default)]
    pub tolerance: f64,
    /// Absolute floor, in the axis unit, for axes whose values sit near zero.
    #[serde(default)]
    pub tolerance_abs: f64,
    #[serde(default)]
    pub contracts: Vec<String>,
}

fn one() -> u32 {
    1
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxonomyRef {
    pub file: String,
}

fn is_contract_id(s: &str) -> bool {
    match s.rsplit_once('@') {
        Some((name, n)) => is_slug(name) && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()),
        None => false,
    }
}

fn is_axis_id(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn canon(p: &str) -> bool {
    paths::is_canonical(p.trim_end_matches('/'))
}

impl ResearchSpec {
    pub fn parse(text: &str) -> Result<Self, String> {
        let spec: ResearchSpec = toml::from_str(text).map_err(|e| e.to_string())?;
        spec.validate()?;
        Ok(spec)
    }

    pub fn load(dir: &Path) -> Result<Self, String> {
        let path = dir.join("challenge.toml");
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut spec = Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let dir_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if spec.challenge.slug != dir_name {
            return Err(format!(
                "{}: slug {:?} doesn't match directory",
                path.display(),
                spec.challenge.slug
            ));
        }
        let tpath = dir.join(&spec.taxonomy.file);
        let ttext =
            std::fs::read_to_string(&tpath).map_err(|e| format!("{}: {e}", tpath.display()))?;
        spec.tax = Taxonomy::parse(&ttext).map_err(|e| format!("{}: {e}", tpath.display()))?;
        Ok(spec)
    }

    fn validate(&self) -> Result<(), String> {
        let c = &self.challenge;
        if !is_slug(&c.slug) {
            return Err(format!("slug {:?}: use lowercase a-z, 0-9 and -", c.slug));
        }
        if c.kind != "research" {
            return Err(format!("type {:?}: expected \"research\"", c.kind));
        }
        if !matches!(c.status.as_str(), "draft" | "open" | "closed") {
            return Err("status: draft, open or closed".into());
        }
        if c.runner.trim().is_empty() || c.runner.ends_with("-latest") {
            return Err(format!(
                "runner {:?}: pin an image (e.g. ubuntu-24.04), not -latest",
                c.runner
            ));
        }
        if self.commands.setup.trim().is_empty() {
            return Err("commands.setup must not be empty".into());
        }
        if self.submissions.max_bytes == 0 {
            return Err("submissions.max_bytes must be > 0".into());
        }
        let l = &self.limits;
        for (name, v) in [
            ("cpu_secs", l.cpu_secs),
            ("memory_mib", l.memory_mib),
            ("processes", l.processes),
            ("file_mib", l.file_mib),
            ("wall_secs", l.wall_secs),
            ("build_secs", l.build_secs),
        ] {
            if v == 0 {
                return Err(format!("limits.{name} must be > 0"));
            }
        }
        if self.contracts.is_empty() {
            return Err("contracts: at least one".into());
        }
        let mut seen = BTreeSet::new();
        for k in &self.contracts {
            if !is_contract_id(&k.id) {
                return Err(format!("contract id {:?}: use <slug>@<digits>", k.id));
            }
            if !seen.insert(k.id.as_str()) {
                return Err(format!("contract {:?}: duplicate id", k.id));
            }
            match k.status.as_str() {
                "open" | "closed" => {
                    for (name, v) in [("judge", &k.judge), ("interface", &k.interface)] {
                        match v {
                            Some(p) if canon(p) => {}
                            Some(p) => {
                                return Err(format!(
                                    "contract {:?}: {name} {p:?}: relative, no . or .. components",
                                    k.id
                                ))
                            }
                            None => {
                                return Err(format!(
                                    "contract {:?}: {} contracts need {name}",
                                    k.id, k.status
                                ))
                            }
                        }
                    }
                    if let Some(p) = &k.choice_space {
                        if !canon(p) {
                            return Err(format!(
                                "contract {:?}: choice_space {p:?}: relative, no . or .. components",
                                k.id
                            ));
                        }
                    }
                }
                "deferred" => {
                    if k.admits_when.as_deref().is_none_or(|s| s.trim().is_empty()) {
                        return Err(format!("contract {:?}: deferred needs admits_when", k.id));
                    }
                }
                s => {
                    return Err(format!(
                        "contract {:?}: status {s:?}: open, deferred or closed",
                        k.id
                    ))
                }
            }
        }
        if self.contracts.iter().all(|k| k.status == "deferred") {
            return Err("contracts: at least one must not be deferred".into());
        }
        let mut pids = BTreeSet::new();
        for p in &self.properties {
            if !is_slug(&p.id) {
                return Err(format!(
                    "property id {:?}: use lowercase a-z, 0-9 and -",
                    p.id
                ));
            }
            if !pids.insert((p.contract.as_str(), p.id.as_str())) {
                return Err(format!("property {:?}: duplicate id", p.id));
            }
            if !matches!(
                p.check.as_str(),
                "mechanical" | "mechanical-conjectured" | "declared"
            ) {
                return Err(format!(
                    "property {:?}: check {:?}: mechanical, mechanical-conjectured or declared",
                    p.id, p.check
                ));
            }
            if self.contract(&p.contract).is_none() {
                return Err(format!(
                    "property {:?}: unknown contract {:?}",
                    p.id, p.contract
                ));
            }
        }
        if self.axes.is_empty() {
            return Err("axes: at least one".into());
        }
        let mut aids = BTreeSet::new();
        for a in &self.axes {
            if !is_axis_id(&a.id) {
                return Err(format!("axis id {:?}: use a-z, 0-9, - and _", a.id));
            }
            if !aids.insert(a.id.as_str()) {
                return Err(format!("axis {:?}: duplicate id", a.id));
            }
            if a.unit.trim().is_empty() {
                return Err(format!("axis {:?}: unit must not be empty", a.id));
            }
            if !matches!(a.direction.as_str(), "minimise" | "maximise") {
                return Err(format!("axis {:?}: direction: minimise or maximise", a.id));
            }
            if !(1..=20).contains(&a.repeats) {
                return Err(format!("axis {:?}: repeats must be 1..=20", a.id));
            }
            if !(0.0..1.0).contains(&a.tolerance) {
                return Err(format!(
                    "axis {:?}: tolerance must satisfy 0 <= t < 1",
                    a.id
                ));
            }
            if !a.tolerance_abs.is_finite() || a.tolerance_abs < 0.0 {
                return Err(format!(
                    "axis {:?}: tolerance_abs must be finite and >= 0",
                    a.id
                ));
            }
            if let Some(x) = a.contracts.iter().find(|x| self.contract(x).is_none()) {
                return Err(format!("axis {:?}: unknown contract {x:?}", a.id));
            }
        }
        if !canon(&self.taxonomy.file) {
            return Err("taxonomy.file: relative, no . or .. components".into());
        }
        Ok(())
    }

    pub fn contract(&self, id: &str) -> Option<&Contract> {
        self.contracts.iter().find(|c| c.id == id)
    }

    /// Axes measured for a contract: those naming no contracts, or naming it.
    pub fn axes_for(&self, contract: &str) -> Vec<&Axis> {
        self.axes
            .iter()
            .filter(|a| a.contracts.is_empty() || a.contracts.iter().any(|c| c == contract))
            .collect()
    }

    pub fn properties_for(&self, contract: &str) -> Vec<&Property> {
        self.properties
            .iter()
            .filter(|p| p.contract == contract)
            .collect()
    }

    pub fn limits_json(&self) -> Value {
        json!({
            "cpu_secs": self.limits.cpu_secs,
            "memory_mib": self.limits.memory_mib,
            "processes": self.limits.processes,
            "file_mib": self.limits.file_mib,
            "wall_secs": self.limits.wall_secs,
            "build_secs": self.limits.build_secs,
        })
    }
}

// ---- taxonomy ----

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Taxonomy {
    pub version: u32,
    pub axes: Vec<TaxAxis>,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxAxis {
    pub id: String,
    pub definition: String,
    pub values: Vec<TaxValue>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxValue {
    pub id: String,
    pub definition: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub text: String,
    /// Why the rule holds; shown beside it on the site.
    #[serde(default)]
    pub reason: String,
}

impl Taxonomy {
    pub fn parse(text: &str) -> Result<Self, String> {
        let t: Taxonomy = toml::from_str(text).map_err(|e| e.to_string())?;
        let mut axes = BTreeSet::new();
        for a in &t.axes {
            if !is_slug(&a.id) {
                return Err(format!("taxonomy axis {:?}: not a slug", a.id));
            }
            if !axes.insert(a.id.as_str()) {
                return Err(format!("taxonomy axis {:?}: duplicate id", a.id));
            }
            let mut vals = BTreeSet::new();
            for v in &a.values {
                if !is_slug(&v.id) {
                    return Err(format!("taxonomy {:?}: value {:?}: not a slug", a.id, v.id));
                }
                if !vals.insert(v.id.as_str()) {
                    return Err(format!("taxonomy {:?}: duplicate value {:?}", a.id, v.id));
                }
            }
        }
        Ok(t)
    }
}

// ---- submission declaration ----

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Declaration {
    pub id: String,
    pub contract: String,
    pub novelty: String,
    #[serde(default)]
    pub direction: String,
    #[serde(default)]
    pub parent: String,
    #[serde(default)]
    pub taxonomy: BTreeMap<String, String>,
    pub summary: String,
    #[serde(default)]
    pub claims: Vec<Claim>,
    #[serde(default)]
    pub model: String,
    /// What should happen and why, before it is measured (#104).
    #[serde(default)]
    pub hypothesis: String,
    /// Axis id → `lower` | `same` | `higher`, relative to the parent
    /// (builds-on) or the baseline (new). See `research::predict`.
    #[serde(default)]
    pub expected: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Claim {
    pub property: String,
    pub argument: String,
}

impl Declaration {
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    /// Every static problem with this declaration (spec §2/§3).
    pub fn check(&self, spec: &ResearchSpec, dir_name: &str) -> Vec<String> {
        let mut p = Vec::new();
        if !is_slug(&self.id) {
            p.push(format!("id {:?}: use lowercase a-z, 0-9 and -", self.id));
        }
        if self.id != dir_name {
            p.push(format!(
                "id {:?} doesn't match directory {dir_name:?}",
                self.id
            ));
        }
        match spec.contract(&self.contract) {
            None => p.push(format!(
                "contract {:?}: not in challenge.toml",
                self.contract
            )),
            Some(c) if c.status != "open" => p.push(format!(
                "contract {:?} is {}: only open contracts take submissions",
                self.contract, c.status
            )),
            _ => {}
        }
        match self.novelty.as_str() {
            "new" => {
                if !is_slug(&self.direction) {
                    p.push("novelty new: direction must be a slug".into());
                }
                if !self.parent.is_empty() {
                    p.push("novelty new: parent must be empty".into());
                }
            }
            "builds-on" => {
                if !is_slug(&self.parent) {
                    p.push("novelty builds-on: parent must be a submission id".into());
                }
                if !self.direction.is_empty() {
                    p.push("novelty builds-on: direction is inherited; leave it empty".into());
                }
            }
            n => p.push(format!("novelty {n:?}: new or builds-on")),
        }
        if self.summary.trim().is_empty() {
            p.push("summary must not be empty".into());
        }
        if self.summary.chars().count() > 500 {
            p.push("summary: at most 500 chars".into());
        }
        if self.model.chars().count() > 100 {
            p.push("model: at most 100 chars".into());
        }
        if self.taxonomy.len() > 16 {
            p.push("taxonomy: at most 16 keys".into());
        }
        if self.claims.len() > 16 {
            p.push("claims: at most 16".into());
        }
        if self.hypothesis.chars().count() > 300 {
            p.push("hypothesis: at most 300 chars".into());
        }
        let axes = spec.axes_for(&self.contract);
        for (k, v) in &self.expected {
            if !axes.iter().any(|a| &a.id == k) {
                p.push(format!(
                    "expected {k:?}: not an axis of {:?}",
                    self.contract
                ));
            }
            if !super::predict::OUTCOMES.contains(&v.as_str()) {
                p.push(format!(
                    "expected {k:?}: {v:?} is not lower, same or higher"
                ));
            }
        }
        for (k, v) in &self.taxonomy {
            if !spec.tax.axes.iter().any(|a| &a.id == k) {
                p.push(format!("taxonomy key {k:?}: not an axis in the taxonomy"));
            }
            if !is_slug(v) {
                p.push(format!("taxonomy {k:?}: value {v:?} is not a slug"));
            }
        }
        for c in &self.claims {
            let declared = spec
                .properties_for(&self.contract)
                .into_iter()
                .any(|pr| pr.id == c.property && pr.check == "declared");
            if !declared {
                p.push(format!(
                    "claim {:?}: not a declared property of {:?}",
                    c.property, self.contract
                ));
            }
            if c.argument.chars().count() > 2000 {
                p.push(format!(
                    "claim {:?}: argument at most 2000 chars",
                    c.property
                ));
            }
        }
        p
    }

    /// Taxonomy keys whose value the taxonomy does not list yet.
    pub fn proposed(&self, tax: &Taxonomy) -> Vec<String> {
        self.taxonomy
            .iter()
            .filter(|(k, v)| {
                !tax.axes
                    .iter()
                    .any(|a| &a.id == *k && a.values.iter().any(|x| &x.id == *v))
            })
            .map(|(k, _)| k.clone())
            .collect()
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"[challenge]
slug   = "coin-select"
type   = "research"
status = "draft"
runner = "ubuntu-24.04"

[commands]
setup = "./setup.sh"

[submissions]
max_bytes  = 4194304
toolchains = ["python3"]
writable   = ["~/.cargo"]

[limits]
cpu_secs   = 60
memory_mib = 1024
processes  = 32
file_mib   = 64
wall_secs  = 120

[[contracts]]
id           = "wallet@1"
status       = "open"
judge        = "contracts/wallet/judge"
interface    = "contracts/wallet/INTERFACE.md"
choice_space = "contracts/wallet/CHOICES.md"

[[contracts]]
id          = "wallet-multiasset@0"
status      = "deferred"
admits_when = "a multi-asset model"

[[properties]]
id       = "valid-txs"
check    = "mechanical"
contract = "wallet@1"

[[properties]]
id       = "change-privacy"
check    = "declared"
contract = "wallet@1"

[[axes]]
id        = "fees"
unit      = "ADA"
direction = "minimise"
repeats   = 1
tolerance = 0.05

[[axes]]
id        = "time"
unit      = "ms"
direction = "minimise"
contracts = ["wallet@1"]

[taxonomy]
file = "taxonomy.toml"
"#;

    const TAX: &str = r#"version = 1
[[axes]]
id = "strategy"
definition = "How inputs are chosen"
values = [ { id = "largest-first", definition = "a" }, { id = "random-improve", definition = "b" } ]
[[rules]]
text = "no-change requires exact search"
reason = "largest-first rarely lands on an exact match"
[[rules]]
text = "a rule without a reason"
"#;

    const DECL: &str = r#"id = "random-improve"
contract = "wallet@1"
novelty = "new"
direction = "random-improve"
taxonomy = { strategy = "random-improve" }
summary = "CIP-2"
claims = [ { property = "change-privacy", argument = "because" } ]
"#;

    fn spec() -> ResearchSpec {
        let mut s = ResearchSpec::parse(GOOD).unwrap();
        s.tax = Taxonomy::parse(TAX).unwrap();
        s
    }

    #[test]
    fn valid_research_spec_parses() {
        let s = ResearchSpec::parse(GOOD).unwrap();
        assert_eq!(s.contracts.len(), 2);
        assert_eq!(s.axes[0].repeats, 1);
        assert!(s.contract("wallet@1").is_some());
        assert!(s.contract("nope@1").is_none());
        assert_eq!(s.axes_for("wallet@1").len(), 2);
        assert_eq!(s.axes_for("wallet-multiasset@0").len(), 1);
        assert_eq!(s.properties_for("wallet@1").len(), 2);
        assert_eq!(s.limits_json()["cpu_secs"], 60);
        assert_eq!(s.limits_json()["build_secs"], 1800);
        let t = Taxonomy::parse(TAX).unwrap();
        assert_eq!(
            t.rules[0].reason,
            "largest-first rarely lands on an exact match"
        );
        assert_eq!(t.rules[1].reason, "");
    }

    fn bad(from: &str, to: &str) -> String {
        assert!(GOOD.contains(from), "{from}");
        ResearchSpec::parse(&GOOD.replacen(from, to, 1)).unwrap_err()
    }

    #[test]
    fn deferred_contract_needs_admits_when() {
        bad(
            "admits_when = \"a multi-asset model\"",
            "admits_when = \"\"",
        );
        bad("admits_when = \"a multi-asset model\"\n", "");
    }

    #[test]
    fn open_contract_needs_judge() {
        bad("judge        = \"contracts/wallet/judge\"\n", "");
        bad("interface    = \"contracts/wallet/INTERFACE.md\"\n", "");
        bad("contracts/wallet/judge", "../judge");
    }

    #[test]
    fn bad_values_refused() {
        bad("ubuntu-24.04", "ubuntu-latest");
        bad("repeats   = 1", "repeats   = 0");
        bad("tolerance = 0.05", "tolerance = 1.5");
        bad("tolerance = 0.05", "tolerance = 0.05\ntolerance_abs = -1.0");
        bad(
            "direction = \"minimise\"\nrepeats",
            "direction = \"up\"\nrepeats",
        );
        bad("id        = \"time\"", "id        = \"fees\"");
        bad(
            "contract = \"wallet@1\"\n\n[[properties]]",
            "contract = \"nope@1\"\n\n[[properties]]",
        );
        bad("id           = \"wallet@1\"", "id           = \"wallet\"");
        bad("[taxonomy]", "typo = 1\n[taxonomy]");
        bad("cpu_secs   = 60", "cpu_secs   = 0");
        bad("max_bytes  = 4194304", "max_bytes  = 0");
        bad("status = \"draft\"", "status = \"live\"");
        bad("file = \"taxonomy.toml\"", "file = \"../t.toml\"");
        bad("contracts = [\"wallet@1\"]", "contracts = [\"x@1\"]");
        bad("check    = \"declared\"", "check    = \"vibes\"");
    }

    fn check(text: &str) -> Vec<String> {
        Declaration::parse(text)
            .unwrap()
            .check(&spec(), "random-improve")
    }

    #[test]
    fn declaration_rules() {
        assert_eq!(check(DECL), Vec::<String>::new());
        let d = Declaration::parse(DECL).unwrap();
        assert!(d.check(&spec(), "other").len() == 1);
        let with = |from: &str, to: &str| {
            assert!(DECL.contains(from), "{from}");
            let p = check(&DECL.replacen(from, to, 1));
            assert!(!p.is_empty(), "{to}");
        };
        with("direction = \"random-improve\"\n", "");
        with(
            "novelty = \"new\"",
            "novelty = \"builds-on\"\nparent = \"x\"",
        );
        with("novelty = \"new\"", "novelty = \"builds-on\"");
        with("property = \"change-privacy\"", "property = \"valid-txs\"");
        with("strategy = ", "colour = ");
        with("= \"random-improve\" }", "= \"Bad Value\" }");
        with("wallet@1", "wallet-multiasset@0");
        with("wallet@1", "nope@1");
        with("novelty = \"new\"", "novelty = \"maybe\"");
    }

    #[test]
    fn hypothesis_and_expected() {
        let ok = format!("{DECL}hypothesis = \"fewer inputs\"\nexpected = {{ fees = \"lower\", time = \"same\" }}\n");
        assert_eq!(check(&ok), Vec::<String>::new());
        let d = Declaration::parse(&ok).unwrap();
        assert_eq!(d.expected["fees"], "lower");
        let bad = |extra: &str, want: &str| {
            let p = check(&format!("{DECL}{extra}\n"));
            assert!(p.iter().any(|x| x.contains(want)), "{extra}: {p:?}");
        };
        bad("expected = { colour = \"lower\" }", "not an axis");
        bad(
            "expected = { fees = \"less\" }",
            "not lower, same or higher",
        );
        bad(
            &format!("hypothesis = \"{}\"", "x".repeat(301)),
            "hypothesis",
        );
    }

    #[test]
    fn proposed_values() {
        let t = Taxonomy::parse(TAX).unwrap();
        let d = Declaration::parse(DECL).unwrap();
        assert!(d.proposed(&t).is_empty());
        let d = Declaration::parse(&DECL.replace("= \"random-improve\" }", "= \"brand-new\" }"))
            .unwrap();
        assert_eq!(d.proposed(&t), vec!["strategy".to_string()]);
        assert_eq!(d.to_json()["taxonomy"]["strategy"], "brand-new");
    }
}
