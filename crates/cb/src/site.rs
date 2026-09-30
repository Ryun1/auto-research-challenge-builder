//! `cb site`: the candidate ranking as one self-contained HTML page.
//!
//! Candidate evaluations are AMBER: this page is for SNF members only and is
//! written under `target/` (gitignored). Nothing here is deployed; the public
//! challenge leaderboard is a separate build (M3).

use crate::candidate::{rows, Row};
use crate::rubric::{Check, Criterion, Fact, Facts, RUBRIC_VERSION, WEIGHTS};
use std::fmt::Write as _;
use std::path::Path;

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// Evidence that is exactly one http(s) URL becomes a link; anything else is text.
fn evidence(s: &str) -> String {
    let s = s.trim();
    let is_url =
        (s.starts_with("https://") || s.starts_with("http://")) && !s.contains(char::is_whitespace);
    if is_url {
        format!("<a href=\"{0}\" rel=\"noreferrer\">{0}</a>", esc(s))
    } else {
        esc(s)
    }
}

fn fact(f: Fact) -> &'static str {
    match f {
        Fact::Yes => "yes",
        Fact::YesSlow => "yes-slow",
        Fact::No => "no",
    }
}

fn opt<T: ToString>(v: Option<T>) -> String {
    v.map(|v| v.to_string()).unwrap_or_else(|| "–".into())
}

const STYLE: &str = r#"
:root { --bg: #fff; --fg: #1a1a1a; --muted: #666; --line: #ddd; --warn: #8a4b00; --warn-bg: #fff4e0; --bad: #a00; --good: #060; }
@media (prefers-color-scheme: dark) {
  :root { --bg: #151515; --fg: #e8e8e8; --muted: #999; --line: #333; --warn: #ffc46b; --warn-bg: #2b2112; --bad: #ff7b7b; --good: #7bd88f; }
}
body { background: var(--bg); color: var(--fg); font: 15px/1.5 system-ui, sans-serif; margin: 0 auto; max-width: 1100px; padding: 16px; }
.banner { background: var(--warn-bg); color: var(--warn); border: 1px solid var(--warn); padding: 8px 12px; border-radius: 6px; }
table { border-collapse: collapse; width: 100%; margin: 8px 0 16px; }
.wrap { overflow-x: auto; }
th, td { border-bottom: 1px solid var(--line); padding: 4px 8px; text-align: left; vertical-align: top; }
td.n { text-align: right; font-variant-numeric: tabular-nums; }
.muted { color: var(--muted); }
.fail { color: var(--bad); } .pass { color: var(--good); }
section { border-top: 2px solid var(--line); margin-top: 24px; }
a { color: inherit; }
"#;

fn gates(out: &mut String, r: &Row) {
    let Some(g) = &r.file.gates else {
        out.push_str("<p class=\"muted\">Gates not assessed.</p>");
        return;
    };
    let list: [(&str, &Check); 4] = [
        ("G1 machine-verifiable correctness", &g.verifiable),
        ("G2 scalar objective", &g.scalar_objective),
        ("G3 bounded verification", &g.bounded),
        ("G4 baseline passes", &g.baseline),
    ];
    out.push_str("<div class=\"wrap\"><table><tr><th>Gate</th><th></th><th>Evidence</th></tr>");
    for (name, c) in list {
        let (class, word) = if c.pass {
            ("pass", "pass")
        } else {
            ("fail", "fail")
        };
        let _ = write!(
            out,
            "<tr><td>{name}</td><td class=\"{class}\">{word}</td><td>{}</td></tr>",
            evidence(&c.evidence)
        );
    }
    out.push_str("</table></div>");
}

fn scores(out: &mut String, r: &Row) {
    out.push_str("<div class=\"wrap\"><table><tr><th>Criterion</th><th>W</th><th>Score</th><th>Evidence</th></tr>");
    for (criterion, weight) in WEIGHTS {
        let (value, ev) = match criterion {
            Criterion::Laptop => (
                opt(r.laptop),
                r.file
                    .laptop
                    .as_ref()
                    .map(|l| evidence(&l.evidence))
                    .unwrap_or_default()
                    + " <span class=\"muted\">(derived)</span>",
            ),
            _ => match r.file.scores.get(&criterion) {
                Some(s) => (s.value.to_string(), evidence(&s.evidence)),
                None => ("–".into(), String::new()),
            },
        };
        let _ = write!(out, "<tr><td>{criterion:?}</td><td class=\"n\">{weight}</td><td class=\"n\">{value}</td><td>{ev}</td></tr>");
    }
    out.push_str("</table></div>");
}

fn measured(out: &mut String, r: &Row) {
    if let Some(l) = &r.file.laptop {
        out.push_str("<div class=\"wrap\"><table><tr><th>Laptop</th><th>produce</th><th>verify</th><th>score</th></tr>");
        for (name, f) in [
            ("Apple Silicon 16 GB", &l.apple),
            ("x86 Linux 16 GB", &l.x86),
        ] {
            let Facts {
                produce,
                verify,
                score,
            } = *f;
            let _ = write!(
                out,
                "<tr><td>{name}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                fact(produce),
                fact(verify),
                fact(score)
            );
        }
        out.push_str("</table></div>");
    }
    if let Some(p) = &r.file.peaks {
        let _ = write!(
            out,
            "<p>Peaks: {} min, {} GB RAM, {} GB disk, GPU {}. Hostable here: <b>{}</b>. {}</p>",
            p.minutes,
            p.ram_gb,
            p.disk_gb,
            if p.gpu { "yes" } else { "no" },
            opt(r.hostable.map(|h| if h { "yes" } else { "no" })),
            evidence(&p.evidence),
        );
    }
}

pub fn render(rows: &[Row]) -> String {
    let mut out = String::new();
    let _ = write!(
        out,
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <meta name=\"robots\" content=\"noindex\"><title>Candidate ranking</title><style>{STYLE}</style></head><body>\
         <h1>Candidate ranking</h1>\
         <p class=\"banner\">Internal (AMBER): candidate evaluations. Do not publish or share outside SNF.</p>\
         <p class=\"muted\">Rubric v{RUBRIC_VERSION}. Edit via PR to <code>candidates/</code>.</p>\
         <div class=\"wrap\"><table><tr><th>#</th><th>Candidate</th><th>Family</th><th>Total</th><th>Laptop</th><th>Hostable</th><th>Status</th></tr>"
    );
    for (i, r) in rows.iter().enumerate() {
        let c = &r.file.candidate;
        let rank = if r.total.is_some() {
            (i + 1).to_string()
        } else {
            "–".into()
        };
        let _ = write!(
            out,
            "<tr><td class=\"n\">{rank}</td><td><a href=\"#{slug}\">{title}</a></td><td>{family}</td>\
             <td class=\"n\">{total}</td><td class=\"n\">{laptop}</td><td>{hostable}</td><td>{status}</td></tr>",
            slug = esc(&c.slug),
            title = esc(&c.title),
            family = esc(&c.family),
            total = opt(r.total.map(|t| format!("{t:.1}"))),
            laptop = opt(r.laptop),
            hostable = opt(r.hostable.map(|h| if h { "yes" } else { "no" })),
            status = r.status,
        );
    }
    out.push_str("</table></div>");
    for r in rows {
        let c = &r.file.candidate;
        let _ = write!(
            out,
            "<section id=\"{}\"><h2>{}</h2>",
            esc(&c.slug),
            esc(&c.title)
        );
        let _ = write!(
            out,
            "<p class=\"muted\">{} · {} · {} · {}</p>",
            esc(&c.family),
            esc(&c.direction),
            esc(&c.category),
            evidence(&c.url)
        );
        if !c.description.is_empty() {
            let _ = write!(out, "<p>{}</p>", esc(&c.description));
        }
        if !c.editable.is_empty() {
            let paths: Vec<_> = c
                .editable
                .iter()
                .map(|p| format!("<code>{}</code>", esc(p)))
                .collect();
            let _ = write!(out, "<p>Editable: {}</p>", paths.join(", "));
        }
        if let Some(bips) = c.min_improvement_bips {
            let _ = write!(out, "<p>Min improvement: {bips} bips</p>");
        }
        if let Some(bytes) = c.max_bytes {
            let _ = write!(out, "<p>Max submission: {bytes} bytes</p>");
        }
        if !r.notes.is_empty() {
            let _ = write!(out, "<p><b>{}</b></p><ul>", r.status);
            for n in &r.notes {
                let _ = write!(out, "<li>{}</li>", esc(n));
            }
            out.push_str("</ul>");
        }
        gates(&mut out, r);
        scores(&mut out, r);
        measured(&mut out, r);
        out.push_str("</section>");
    }
    out.push_str("</body></html>\n");
    out
}

/// Writes `<out>/index.html`.
pub fn build(dir: &Path, out: &Path) -> Result<std::path::PathBuf, String> {
    let html = render(&rows(dir)?);
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let path = out.join("index.html");
    std::fs::write(&path, html).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_and_links() {
        assert_eq!(esc("<script>\"&'"), "&lt;script&gt;&quot;&amp;&#39;");
        assert!(evidence("https://x.org/a").starts_with("<a href=\"https://x.org/a\""));
        assert_eq!(evidence("see https://x.org"), "see https://x.org"); // not a bare URL
        assert_eq!(evidence("javascript:alert(1)"), "javascript:alert(1)");
        assert!(!evidence("https://x.org/\"onmouseover=\"x").contains("\"onmouseover"));
    }

    #[test]
    fn renders_hostile_candidate_safely() {
        let dir = std::env::temp_dir().join(format!("cb-site-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("x.toml"),
            "[candidate]\nslug = \"x\"\ntitle = \"<img src=x onerror=alert(1)>\"\nfamily = \"perf\"\ndirection = \"minimise\"\ndescription = \"</section><script>\"\n",
        )
        .unwrap();
        let html = render(&rows(&dir).unwrap());
        assert!(
            !html.contains("<img") && !html.contains("<script"),
            "{html}"
        );
        assert!(html.contains("AMBER") && html.contains("noindex"));
        assert!(html.contains("Gates not assessed"));
    }
}
