//! `[[targets]]` in `challenge.toml` (#102): external reference points, such
//! as another implementation measured on our harness or a published figure,
//! shown on the site beside the record (simple) or the map (research).
//!
//! Each records how the number was obtained (`status`) and whether it is a
//! like-for-like comparison (`comparable`). The site only words a comparison
//! ("the record beats it") for `comparable = "exact"`; otherwise the target is
//! context. A target is never ranked with submissions.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// How the number was obtained.
pub const STATUSES: [&str; 4] = ["measured-here", "reported", "derived", "composite"];
/// Whether it is a like-for-like comparison with submissions.
pub const COMPARABLE: [&str; 3] = ["exact", "comparable", "different"];

const MAX_TARGETS: usize = 12;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub id: String,
    pub label: String,
    /// A citation, or for `measured-here` the run that measured it.
    pub source: String,
    #[serde(default)]
    pub year: Option<u16>,
    /// measured-here | reported | derived | composite
    pub status: String,
    /// exact | comparable | different
    pub comparable: String,
    #[serde(default)]
    pub note: String,
    /// Simple challenges: the value in the challenge's metric.
    #[serde(default)]
    pub value: Option<f64>,
    /// Research challenges: values by axis id (any subset of the axes).
    #[serde(default)]
    pub values: BTreeMap<String, f64>,
}

/// What a target's numbers must fit.
pub enum Kind<'a> {
    Simple,
    /// The challenge's axis ids.
    Research(&'a [&'a str]),
}

fn is_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Every problem with a challenge's targets.
pub fn check(targets: &[Target], kind: &Kind) -> Vec<String> {
    let mut p = Vec::new();
    if targets.len() > MAX_TARGETS {
        p.push(format!("targets: at most {MAX_TARGETS}"));
    }
    let mut seen = BTreeSet::new();
    for t in targets {
        let who = format!("target {:?}", t.id);
        if !is_id(&t.id) {
            p.push(format!("{who}: id must be lowercase a-z, 0-9 and -"));
        }
        if !seen.insert(t.id.as_str()) {
            p.push(format!("{who}: duplicate id"));
        }
        for (field, v, max) in [
            ("label", &t.label, 100),
            ("source", &t.source, 500),
            ("note", &t.note, 500),
        ] {
            if field != "note" && v.trim().is_empty() {
                p.push(format!("{who}: {field} must not be empty"));
            }
            if v.chars().count() > max {
                p.push(format!("{who}: {field} at most {max} characters"));
            }
        }
        if !STATUSES.contains(&t.status.as_str()) {
            p.push(format!(
                "{who}: status must be one of {}",
                STATUSES.join(", ")
            ));
        }
        if !COMPARABLE.contains(&t.comparable.as_str()) {
            p.push(format!(
                "{who}: comparable must be one of {}",
                COMPARABLE.join(", ")
            ));
        }
        let finite = |x: f64| x.is_finite();
        match kind {
            Kind::Simple => {
                if !t.values.is_empty() {
                    p.push(format!("{who}: a simple challenge takes value, not values"));
                }
                match t.value {
                    None => p.push(format!("{who}: value is required")),
                    Some(x) if !finite(x) => p.push(format!("{who}: value must be finite")),
                    _ => {}
                }
            }
            Kind::Research(axes) => {
                if t.value.is_some() {
                    p.push(format!(
                        "{who}: a research challenge takes values by axis, not value"
                    ));
                }
                if t.values.is_empty() {
                    p.push(format!("{who}: values needs at least one axis"));
                }
                for (k, x) in &t.values {
                    if !axes.contains(&k.as_str()) {
                        p.push(format!("{who}: values {k:?} is not an axis"));
                    }
                    if !finite(*x) {
                        p.push(format!("{who}: values {k:?} must be finite"));
                    }
                }
            }
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(extra: &str) -> Vec<Target> {
        #[derive(Deserialize)]
        struct W {
            targets: Vec<Target>,
        }
        let w: W = toml::from_str(&format!(
            "[[targets]]\nid = \"pallas\"\nlabel = \"pallas (Rust)\"\nsource = \"run 1\"\n\
             status = \"measured-here\"\ncomparable = \"different\"\n{extra}"
        ))
        .unwrap();
        w.targets
    }

    #[test]
    fn simple_target() {
        assert!(check(&t("value = 4.5"), &Kind::Simple).is_empty());
        assert!(check(&t(""), &Kind::Simple)[0].contains("value is required"));
        let both = check(&t("value = 1.0\nvalues = { fees = 1.0 }"), &Kind::Simple);
        assert!(both.iter().any(|e| e.contains("not values")));
        assert!(check(&t("value = nan"), &Kind::Simple)[0].contains("finite"));
    }

    #[test]
    fn research_target() {
        let axes = ["fees", "dust"];
        let k = Kind::Research(&axes);
        assert!(check(&t("values = { fees = 1.0 }"), &k).is_empty());
        assert!(check(&t("values = { colour = 1.0 }"), &k)[0].contains("not an axis"));
        assert!(check(&t("value = 1.0"), &k)
            .iter()
            .any(|e| e.contains("not value")));
        assert!(check(&t(""), &k)[0].contains("at least one axis"));
    }

    #[test]
    fn enums_and_ids() {
        let bad = t("value = 1.0").into_iter().map(|mut x| {
            x.status = "guessed".into();
            x.comparable = "sort-of".into();
            x.id = "Pallas".into();
            x
        });
        let p = check(&bad.collect::<Vec<_>>(), &Kind::Simple);
        assert_eq!(p.len(), 3, "{p:?}");
        let mut two = t("value = 1.0");
        two.extend(t("value = 2.0"));
        assert!(check(&two, &Kind::Simple)[0].contains("duplicate"));
    }

    #[test]
    fn unknown_field_refused() {
        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct W {
            targets: Vec<Target>,
        }
        let src = "[[targets]]\nid = \"a\"\nlabel = \"a\"\nsource = \"s\"\nstatus = \"reported\"\n\
                   comparable = \"exact\"\nvalue = 1.0\nconfidence = \"high\"\n";
        assert!(toml::from_str::<W>(src).is_err());
    }
}
