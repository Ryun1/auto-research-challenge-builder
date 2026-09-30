//! `candidates/<slug>.toml`: one candidate problem, its gates, scores and
//! measurements. Everything but `[candidate]` is optional, so an imported
//! draft loads and shows up in `cb rank` as unassessed.

use crate::rubric::{self, Criterion, Gates, Laptop, Peaks, Score};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct File {
    pub candidate: Meta,
    pub gates: Option<Gates>,
    #[serde(default)]
    pub scores: BTreeMap<Criterion, Score>,
    pub laptop: Option<Laptop>,
    pub peaks: Option<Peaks>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub family: String,
    pub direction: String,
    /// Toolchain, as in benchmark.json (`rust`, `lean`, `cuda`, `math`...).
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub editable: Vec<String>,
    pub min_improvement_bips: Option<u32>,
    pub max_bytes: Option<u64>,
}

impl File {
    pub fn load(path: &Path) -> Result<File, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let file: File = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if file.candidate.slug != stem {
            return Err(format!(
                "{}: slug {:?} doesn't match file name",
                path.display(),
                file.candidate.slug
            ));
        }
        if !matches!(
            file.candidate.family.as_str(),
            "perf" | "resource" | "math" | "proof"
        ) {
            return Err(format!(
                "{}: unknown family {:?}",
                path.display(),
                file.candidate.family
            ));
        }
        if !matches!(file.candidate.direction.as_str(), "minimise" | "maximise") {
            return Err(format!(
                "{}: direction must be minimise or maximise",
                path.display()
            ));
        }
        Ok(file)
    }

    /// `None` while gates are unassessed.
    pub fn rubric(&self) -> Option<rubric::Candidate> {
        Some(rubric::Candidate {
            gates: self.gates.clone()?,
            scores: self.scores.clone(),
            laptop: self.laptop.clone(),
            peaks: self.peaks.clone(),
        })
    }
}

// ---- cb import ----

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Benchmark {
    schema_version: u32,
    name: String,
    #[serde(flatten)]
    track: Option<Track>,
    #[serde(default)]
    tracks: Vec<Track>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Track {
    name: Option<String>,
    #[serde(default)]
    description: String,
    category: String,
    direction: String,
    #[serde(default)]
    editable_paths: Vec<String>,
    min_score_improvement_bips: Option<u32>,
    max_submission_bytes: Option<u64>,
}

/// Lowercase ASCII alphanumerics joined by single `-`.
pub fn slugify(s: &str) -> String {
    s.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join("-")
}

fn quote(s: &str) -> String {
    toml::Value::String(s.into()).to_string()
}

/// One draft TOML per track: `(slug, text)`. Family is a guess from
/// `category` and is marked as such.
pub fn import(json: &str, url: &str) -> Result<Vec<(String, String)>, String> {
    let b: Benchmark = serde_json::from_str(json).map_err(|e| format!("benchmark.json: {e}"))?;
    let tracks = match b.schema_version {
        1 => vec![b.track.ok_or("schemaVersion 1 without track fields")?],
        2 if !b.tracks.is_empty() => b.tracks,
        2 => return Err("schemaVersion 2 without tracks[]".into()),
        v => return Err(format!("unsupported schemaVersion {v}")),
    };
    let multi = tracks.len() > 1 || b.schema_version == 2;
    tracks
        .into_iter()
        .map(|t| {
            let title = match (&t.name, multi) {
                (Some(track), true) => format!("{} / {track}", b.name),
                _ => b.name.clone(),
            };
            let slug = slugify(&title);
            let direction = match t.direction.as_str() {
                "+" => "maximise",
                "-" => "minimise",
                d => return Err(format!("{slug}: unknown direction {d:?}")),
            };
            let family = match t.category.as_str() {
                "math" => "math",
                "lean" | "coq" | "isabelle" => "proof",
                _ => "perf",
            };
            let mut out = String::new();
            let w = &mut out;
            let _ = writeln!(w, "# Imported from benchmark.json (schemaVersion {}). Draft: assess gates, scores, laptop, peaks.", b.schema_version);
            let _ = writeln!(w, "[candidate]");
            let _ = writeln!(w, "slug = {}", quote(&slug));
            let _ = writeln!(w, "title = {}", quote(&title));
            let _ = writeln!(w, "description = {}", quote(&t.description));
            let _ = writeln!(w, "family = {}  # guessed from category; resource-min looks like perf here", quote(family));
            let _ = writeln!(w, "direction = {}", quote(direction));
            let _ = writeln!(w, "category = {}", quote(&t.category));
            let _ = writeln!(w, "url = {}", quote(url));
            let paths: Vec<_> = t.editable_paths.iter().map(|p| quote(p)).collect();
            let _ = writeln!(w, "editable = [{}]", paths.join(", "));
            if let Some(bips) = t.min_score_improvement_bips {
                let _ = writeln!(w, "min_improvement_bips = {bips}");
            }
            if let Some(bytes) = t.max_submission_bytes {
                let _ = writeln!(w, "max_bytes = {bytes}");
            }
            Ok((slug, out))
        })
        .collect()
}

// ---- cb rank ----

pub struct Row {
    pub file: File,
    /// ranked | unmeasured | rejected | incomplete | draft
    pub status: &'static str,
    pub total: Option<f64>,
    pub laptop: Option<u8>,
    pub hostable: Option<bool>,
    /// Rejection reasons, input errors, or laptop suggestions.
    pub notes: Vec<String>,
}

/// Loads and evaluates every `*.toml` in `dir`: ranked by total, the rest by slug.
pub fn rows(dir: &Path) -> Result<Vec<Row>, String> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    paths.sort();
    let mut rows = Vec::new();
    for path in &paths {
        let file = File::load(path)?;
        let row = |status, total, laptop, hostable, notes| (status, total, laptop, hostable, notes);
        let (status, total, laptop, hostable, notes) =
            match file.rubric().map(|c| rubric::evaluate(&c)) {
                None => row("draft", None, None, None, vec!["gates not assessed".into()]),
                Some(Err(errors)) => row("incomplete", None, None, None, errors),
                Some(Ok(e)) if !e.rejected.is_empty() => {
                    row("rejected", None, e.laptop_score, e.hostable, e.rejected)
                }
                Some(Ok(e)) => {
                    let status = if e.total.is_some() {
                        "ranked"
                    } else {
                        "unmeasured"
                    };
                    row(status, e.total, e.laptop_score, e.hostable, e.suggestions)
                }
            };
        rows.push(Row {
            file,
            status,
            total,
            laptop,
            hostable,
            notes,
        });
    }
    rows.sort_by(|a, b| {
        b.total
            .partial_cmp(&a.total)
            .unwrap()
            .then_with(|| a.file.candidate.slug.cmp(&b.file.candidate.slug))
    });
    Ok(rows)
}

/// The ranking as a text table, then notes per candidate.
pub fn rank(dir: &Path) -> Result<String, String> {
    let rows = rows(dir)?;
    let opt = |v: Option<String>| v.unwrap_or_else(|| "-".into());
    let mut out = format!("rubric v{}\n", rubric::RUBRIC_VERSION);
    let _ = writeln!(
        out,
        "{:<4} {:<40} {:<9} {:>6} {:>6} {:>8}  status",
        "#", "slug", "family", "total", "laptop", "hostable"
    );
    for (i, r) in rows.iter().enumerate() {
        let rank = if r.total.is_some() {
            (i + 1).to_string()
        } else {
            "-".into()
        };
        let hostable = r.hostable.map(|h| if h { "yes" } else { "no" }.to_string());
        let _ = writeln!(
            out,
            "{rank:<4} {:<40} {:<9} {:>6} {:>6} {:>8}  {}",
            r.file.candidate.slug,
            r.file.candidate.family,
            opt(r.total.map(|t| format!("{t:.1}"))),
            opt(r.laptop.map(|l| l.to_string())),
            opt(hostable),
            r.status,
        );
    }
    for r in &rows {
        if !r.notes.is_empty() {
            let _ = writeln!(out, "\n{} ({}):", r.file.candidate.slug, r.status);
            for n in &r.notes {
                let _ = writeln!(out, "  - {n}");
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const V1: &str = r#"{"schemaVersion":1,"name":"heesch","description":"Five \"rings\".",
        "category":"math","direction":"+","editablePaths":["submission"],
        "minScoreImprovementBips":0,"maxSubmissionBytes":536870912,
        "setupCommand":["x"],"runner":{"provider":"github-actions"}}"#;
    const V2: &str = r#"{"schemaVersion":2,"name":"EIP-8200 Precompile Challenges","tracks":[
        {"name":"modexp","category":"lean","direction":"-","editablePaths":["Challenge/Modexp/Submission"]},
        {"name":"ripemd160","category":"lean","direction":"-","editablePaths":["b"]}]}"#;

    fn roundtrip(slug: &str, text: &str) -> File {
        let dir = std::env::temp_dir().join(format!("cb-test-{}-{slug}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{slug}.toml"));
        std::fs::write(&path, text).unwrap();
        File::load(&path).unwrap()
    }

    #[test]
    fn import_v1() {
        let out = import(V1, "https://github.com/x/heesch").unwrap();
        assert_eq!(out.len(), 1);
        let f = roundtrip(&out[0].0, &out[0].1);
        let c = f.candidate;
        assert_eq!(
            (c.slug.as_str(), c.family.as_str(), c.direction.as_str()),
            ("heesch", "math", "maximise")
        );
        assert_eq!(c.description, "Five \"rings\".");
        assert_eq!(
            (c.editable, c.min_improvement_bips, c.max_bytes),
            (vec!["submission".into()], Some(0), Some(536870912))
        );
        assert!(f.gates.is_none());
    }

    #[test]
    fn import_v2_one_file_per_track() {
        let out = import(V2, "").unwrap();
        let slugs: Vec<_> = out.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(
            slugs,
            [
                "eip-8200-precompile-challenges-modexp",
                "eip-8200-precompile-challenges-ripemd160"
            ]
        );
        let f = roundtrip(&out[0].0, &out[0].1);
        assert_eq!(
            (f.candidate.family.as_str(), f.candidate.direction.as_str()),
            ("proof", "minimise")
        );
        assert_eq!(f.candidate.title, "EIP-8200 Precompile Challenges / modexp");
    }

    #[test]
    fn import_rejects_bad_input() {
        assert!(import(r#"{"schemaVersion":3,"name":"x"}"#, "").is_err());
        assert!(import(r#"{"schemaVersion":2,"name":"x","tracks":[]}"#, "").is_err());
        assert!(import(&V1.replace(r#""direction":"+""#, r#""direction":"up""#), "").is_err());
    }

    #[test]
    fn load_rejects_mismatch_and_unknown_fields() {
        let dir = std::env::temp_dir().join(format!("cb-test-{}-bad", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let base = "[candidate]\nslug = \"a\"\ntitle = \"A\"\nfamily = \"perf\"\ndirection = \"minimise\"\n";
        let write = |name: &str, text: &str| {
            let p = dir.join(name);
            std::fs::write(&p, text).unwrap();
            File::load(&p)
        };
        assert!(write("a.toml", base).is_ok());
        assert!(write("b.toml", base).is_err()); // slug ≠ file name
        assert!(write("a.toml", &format!("{base}typo = 1\n")).is_err());
        assert!(write("a.toml", &base.replace("perf", "speed")).is_err());
        let laptop_by_hand = format!("{base}[scores.laptop]\nvalue = 5\nevidence = \"x\"\n");
        let f = write("a.toml", &laptop_by_hand).unwrap();
        assert!(f.scores.contains_key(&Criterion::Laptop)); // loads; evaluate() refuses it
    }

    #[test]
    fn rank_orders_and_explains() {
        let dir = std::env::temp_dir().join(format!("cb-test-{}-rank", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let full = |slug: &str, v: u8, pass: bool| {
            let mut s = format!("[candidate]\nslug = \"{slug}\"\ntitle = \"T\"\nfamily = \"perf\"\ndirection = \"minimise\"\n");
            for g in ["verifiable", "scalar_objective", "bounded", "baseline"] {
                let pass = pass || g != "bounded";
                s += &format!("[gates.{g}]\npass = {pass}\nevidence = \"e\"\n");
            }
            for c in [
                "expert_scarcity",
                "headroom",
                "eval_cost",
                "noise",
                "gaming_resistance",
                "fidelity",
                "value",
                "decomposability",
                "prior_art",
            ] {
                s += &format!("[scores.{c}]\nvalue = {v}\nevidence = \"e\"\n");
            }
            s += "[laptop]\nevidence = \"m\"\napple = { produce = \"yes\", verify = \"yes\", score = \"yes-slow\" }\nx86 = { produce = \"yes\", verify = \"yes\", score = \"yes\" }\n";
            s += "[peaks]\nminutes = 5.0\nram_gb = 1.0\ndisk_gb = 1.0\ngpu = false\nevidence = \"ci\"\n";
            std::fs::write(dir.join(format!("{slug}.toml")), s).unwrap();
        };
        full("low", 2, true);
        full("high", 5, true);
        full("gated", 5, false);
        std::fs::write(
            dir.join("draft.toml"),
            import(V1, "").unwrap()[0].1.replace("heesch", "draft"),
        )
        .unwrap();
        let out = rank(&dir).unwrap();
        let lines: Vec<_> = out.lines().collect();
        assert!(lines[2].starts_with("1    high"), "{out}");
        assert!(lines[3].starts_with("2    low"), "{out}");
        assert!(out.contains("gated (rejected):\n  - G3"), "{out}");
        assert!(
            out.contains("draft (draft):\n  - gates not assessed"),
            "{out}"
        );
        assert!(out.contains("score: ship a small local profile"), "{out}");
    }
}
