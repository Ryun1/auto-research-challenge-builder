//! Small helpers shared by the research commands.

use crate::paths;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Median of an ascending, non-empty slice.
pub fn median(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

/// Whether two axis values agree: `|a-b| <= max(rel * max(|a|,|b|), abs)`.
/// With `rel` and `abs` both 0 this is exact equality.
pub fn agrees(a: f64, b: f64, rel: f64, abs: f64) -> bool {
    if rel == 0.0 && abs == 0.0 {
        return a == b;
    }
    (a - b).abs() <= (rel * a.abs().max(b.abs())).max(abs)
}

/// `(property, ok)` for each check of a measurement.
pub fn verdicts(m: &Value) -> Vec<(String, bool)> {
    m["checks"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| {
            (
                str_of(&c["property"]).to_string(),
                c["ok"].as_bool().unwrap_or(false),
            )
        })
        .collect()
}

/// Copies a challenge dir to a fresh scratch dir (without `target/`).
pub fn copy_challenge(dir: &Path, tag: &str) -> Result<PathBuf, String> {
    let ws = paths::scratch(tag);
    let st = Command::new("cp")
        .arg("-R")
        .arg(dir)
        .arg(&ws)
        .status()
        .map_err(|e| e.to_string())?;
    if !st.success() {
        return Err("copying the challenge".into());
    }
    let _ = std::fs::remove_dir_all(ws.join("target"));
    Ok(ws)
}

pub fn str_of(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

pub fn clip(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn agrees_relative_absolute_exact() {
        use super::agrees;
        assert!(agrees(10.0, 12.0, 0.2, 0.0));
        assert!(!agrees(10.0, 13.0, 0.2, 0.0));
        assert!(!agrees(0.375, 0.125, 0.3, 0.0));
        assert!(agrees(0.375, 0.125, 0.3, 0.25));
        assert!(!agrees(3.0, 0.5, 0.3, 1.0));
        assert!(agrees(5.0, 5.0, 0.0, 0.0));
        assert!(!agrees(5.0, 5.0001, 0.0, 0.0));
    }

    use super::*;

    #[test]
    fn median_odd_even() {
        assert_eq!(median(&[1.0, 2.0, 9.0]), 2.0);
        assert_eq!(median(&[1.0, 3.0]), 2.0);
    }

    #[test]
    fn clip_by_chars() {
        assert_eq!(clip("héllo", 2), "hé");
    }
}
