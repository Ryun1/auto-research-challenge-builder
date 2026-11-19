//! `challenges/<slug>/context.toml` (#101): why the problem matters, for the
//! public site. Optional, maintainer-owned (no submission PR can touch it:
//! the gate allows only editable paths or one new submission dir).
//!
//! The site is public although the repo is private, so everything here is
//! written to be published. Every number in `value_levers` and `calls` names
//! its source, or is an estimate that names its method.

use serde::{Deserialize, Serialize};
use std::path::Path;

pub const FILE: &str = "context.toml";

const MAX_LINE: usize = 200; // one_line, labels, quantities
const MAX_TEXT: usize = 1500; // paragraphs
const MAX_ITEMS: usize = 12; // each list

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Context {
    /// The problem in one sentence.
    pub one_line: String,
    /// Why anyone should care.
    pub stakes: String,
    /// Why these inputs, and what the results do not say.
    #[serde(default)]
    pub why_this_instance: String,
    /// What the score leaves out.
    #[serde(default)]
    pub cautions: Vec<String>,
    /// Citations: URLs or references.
    #[serde(default)]
    pub sources: Vec<String>,
    /// Steps of the system around the challenge; exactly one is this one.
    #[serde(default)]
    pub flow: Vec<Step>,
    /// How often the step this challenge covers runs.
    #[serde(default)]
    pub calls: Option<Calls>,
    /// Quantified effects of an improvement.
    #[serde(default)]
    pub value_levers: Vec<Lever>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub label: String,
    #[serde(default)]
    pub detail: String,
    /// This is the step the challenge covers.
    #[serde(default)]
    pub this_challenge: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Calls {
    /// e.g. "every transaction, twice"
    pub value: String,
    /// How the figure is arrived at.
    #[serde(default)]
    pub basis: String,
    pub source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lever {
    /// The effect, in a sentence.
    pub statement: String,
    /// The short form, e.g. "−10% decode → −2% block validation".
    pub quantity: String,
    /// Where the numbers come from: a citation, or the method if an estimate.
    pub source: String,
    #[serde(default)]
    pub is_estimate: bool,
}

fn text(p: &mut Vec<String>, field: &str, v: &str, max: usize, required: bool) {
    if required && v.trim().is_empty() {
        p.push(format!("{field}: must not be empty"));
    }
    if v.chars().count() > max {
        p.push(format!("{field}: at most {max} characters"));
    }
}

fn items<T>(p: &mut Vec<String>, field: &str, v: &[T]) {
    if v.len() > MAX_ITEMS {
        p.push(format!("{field}: at most {MAX_ITEMS} items"));
    }
}

impl Context {
    /// `Ok(None)` when the file has no keys (a scaffold stub, all comments).
    pub fn parse(src: &str) -> Result<Option<Self>, String> {
        let table: toml::Table = toml::from_str(src).map_err(|e| e.to_string())?;
        if table.is_empty() {
            return Ok(None);
        }
        let c: Context = toml::from_str(src).map_err(|e| e.to_string())?;
        let mut p = Vec::new();
        text(&mut p, "one_line", &c.one_line, MAX_LINE, true);
        text(&mut p, "stakes", &c.stakes, MAX_TEXT, true);
        text(
            &mut p,
            "why_this_instance",
            &c.why_this_instance,
            MAX_TEXT,
            false,
        );
        items(&mut p, "cautions", &c.cautions);
        for (i, s) in c.cautions.iter().enumerate() {
            text(&mut p, &format!("cautions[{i}]"), s, MAX_TEXT, true);
        }
        items(&mut p, "sources", &c.sources);
        for (i, s) in c.sources.iter().enumerate() {
            text(&mut p, &format!("sources[{i}]"), s, MAX_LINE * 2, true);
        }
        items(&mut p, "flow", &c.flow);
        for (i, s) in c.flow.iter().enumerate() {
            text(
                &mut p,
                &format!("flow[{i}].label"),
                &s.label,
                MAX_LINE,
                true,
            );
            text(
                &mut p,
                &format!("flow[{i}].detail"),
                &s.detail,
                MAX_TEXT,
                false,
            );
        }
        let here = c.flow.iter().filter(|s| s.this_challenge).count();
        if !c.flow.is_empty() && here != 1 {
            p.push(format!(
                "flow: exactly one step has this_challenge = true (found {here})"
            ));
        }
        if let Some(k) = &c.calls {
            text(&mut p, "calls.value", &k.value, MAX_LINE, true);
            text(&mut p, "calls.basis", &k.basis, MAX_TEXT, false);
            text(&mut p, "calls.source", &k.source, MAX_TEXT, true);
        }
        items(&mut p, "value_levers", &c.value_levers);
        for (i, l) in c.value_levers.iter().enumerate() {
            let f = |k: &str| format!("value_levers[{i}].{k}");
            text(&mut p, &f("statement"), &l.statement, MAX_TEXT, true);
            text(&mut p, &f("quantity"), &l.quantity, MAX_LINE, true);
            text(&mut p, &f("source"), &l.source, MAX_TEXT, true);
        }
        if p.is_empty() {
            Ok(Some(c))
        } else {
            Err(p.join("; "))
        }
    }

    /// `Ok(None)` when the challenge has no `context.toml` (it is optional).
    pub fn load(dir: &Path) -> Result<Option<Self>, String> {
        let path = dir.join(FILE);
        match std::fs::read_to_string(&path) {
            Ok(src) => Self::parse(&src).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }
}

/// The stub `cb new` writes: all comments, so it loads as no context until a
/// maintainer fills it in.
pub const STUB: &str = r#"# Why this challenge matters, shown on the public site (#101). Optional:
# while every line is a comment the site shows no context section.
# The site is public although the repo is private: write to be published.
# Every number in value_levers and calls names its source, or sets
# is_estimate = true and names its method. Lists hold at most 12 items.
#
# one_line          = "The problem in one sentence."
# stakes            = "Why anyone should care."
# why_this_instance = "Why these inputs, and what results do not say."
# cautions          = ["What the score leaves out."]
# sources           = ["https://..."]
#
# [[flow]]                      # the system around the challenge, in order
# label  = "A step"
# detail = "What happens in it"
#
# [[flow]]
# label          = "The step this challenge covers"
# detail         = "..."
# this_challenge = true         # exactly one step
#
# [calls]                       # how often that step runs
# value  = "e.g. once per transaction"
# basis  = "how the figure is arrived at"
# source = "where it comes from"
#
# [[value_levers]]              # what an improvement is worth
# statement   = "A 10% cut in X removes Y% of Z, because ..."
# quantity    = "−10% X → −Y% Z"
# source      = "a citation, or the method for an estimate"
# is_estimate = true
"#;

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
one_line = "Pick coins."
stakes = "Fees and dust."
cautions = ["A model, not the ledger."]
sources = ["https://cips.cardano.org/cip/CIP-0002"]

[[flow]]
label = "Request"

[[flow]]
label = "Select"
this_challenge = true

[calls]
value = "every payment"
source = "CIP-2"

[[value_levers]]
statement = "Less dust."
quantity = "−1 UTxO"
source = "our calculation"
is_estimate = true
"#;

    fn err(src: &str) -> String {
        Context::parse(src).unwrap_err()
    }

    #[test]
    fn good_parses() {
        let c = Context::parse(GOOD).unwrap().unwrap();
        assert_eq!(c.flow.len(), 2);
        assert!(c.flow[1].this_challenge);
        assert!(c.value_levers[0].is_estimate);
        assert_eq!(c.calls.unwrap().value, "every payment");
    }

    #[test]
    fn stub_is_no_context() {
        assert!(Context::parse(STUB).unwrap().is_none());
        assert!(Context::parse("").unwrap().is_none());
    }

    #[test]
    fn unknown_field_refused() {
        assert!(err(&format!("{GOOD}\nextra = 1\n")).contains("extra"));
        let typo = GOOD.replace("is_estimate", "estimate");
        assert!(err(&typo).contains("estimate"));
    }

    #[test]
    fn required_fields() {
        assert!(err("stakes = \"x\"").contains("one_line"));
        assert!(
            err(&GOOD.replace("one_line = \"Pick coins.\"", "one_line = \" \""))
                .contains("one_line")
        );
    }

    #[test]
    fn lever_needs_a_source() {
        assert!(
            err(&GOOD.replace("source = \"our calculation\"", "source = \"\""))
                .contains("value_levers[0].source")
        );
    }

    #[test]
    fn exactly_one_step_is_this_challenge() {
        let none = GOOD.replace("this_challenge = true", "");
        assert!(err(&none).contains("found 0"));
        let two = GOOD.replace(
            "label = \"Request\"",
            "label = \"Request\"\nthis_challenge = true",
        );
        assert!(err(&two).contains("found 2"));
    }

    #[test]
    fn limits() {
        let long = "x".repeat(MAX_LINE + 1);
        assert!(err(&GOOD.replace("Pick coins.", &long)).contains("at most"));
        let many = (0..=MAX_ITEMS)
            .map(|i| format!("\"c{i}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let src = GOOD.replace("[\"A model, not the ledger.\"]", &format!("[{many}]"));
        assert!(err(&src).contains("cautions: at most"));
    }

    #[test]
    fn missing_file_is_none() {
        let d = std::env::temp_dir().join(format!("cb-ctx-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        assert!(Context::load(&d).unwrap().is_none());
        std::fs::write(d.join(FILE), "one_line = 1").unwrap();
        assert!(Context::load(&d).unwrap_err().contains("context.toml"));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Every committed challenge's context.toml loads (CI's preflight skips
    /// the Haskell challenges, so this is the check that covers all of them).
    #[test]
    fn every_challenge_context_loads() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../challenges");
        for e in std::fs::read_dir(&root).unwrap() {
            let dir = e.unwrap().path();
            if dir.is_dir() {
                Context::load(&dir).unwrap_or_else(|e| panic!("{e}"));
            }
        }
    }
}
