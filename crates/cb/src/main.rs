use clap::{Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod candidate;
mod challenge;
mod feed;
mod haskellpolicy;
mod pages;
mod paths;
mod pipeline;
mod policy;
mod preflight;
mod research;
mod rubric;
mod rustpolicy;
mod site;

/// Challenge builder: evaluate candidate problems, build and run challenges.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
enum Family {
    Perf,
    Resource,
    Math,
    Proof,
}

#[derive(Clone, Copy, ValueEnum)]
enum ChallengeType {
    Simple,
    Research,
}

#[derive(Clone, Copy, ValueEnum)]
enum Kit {
    Rust,
}

#[derive(Subcommand)]
enum Command {
    /// Scaffold challenges/<slug>/
    New {
        slug: String,
        /// simple (one objective, a record) or research (an open space, a map)
        #[arg(long = "type", value_enum, default_value = "simple")]
        kind: ChallengeType,
        /// Required for simple challenges
        #[arg(long)]
        family: Option<Family>,
        #[arg(long)]
        kit: Option<Kit>,
        #[arg(long, default_value = "challenges")]
        root: PathBuf,
    },
    /// Run launch checks for a challenge on this machine (laptop or runner)
    Preflight {
        slug: String,
        #[arg(long, default_value = "challenges")]
        root: PathBuf,
        /// Where to write <slug>.json
        #[arg(long, default_value = "target/preflight")]
        out: PathBuf,
    },
    /// Pre-fill a candidate from a Yukon-style benchmark.json
    Import {
        benchmark_json: PathBuf,
        /// Repo URL to record (benchmark.json doesn't carry one)
        #[arg(long, default_value = "")]
        url: String,
        #[arg(long, default_value = "candidates")]
        out: PathBuf,
    },
    /// Render the candidate ranking to one HTML page (internal, AMBER: never deploy)
    Site {
        #[arg(long, default_value = "candidates")]
        dir: PathBuf,
        #[arg(long, default_value = "target/site")]
        out: PathBuf,
    },
    /// PR gate (trusted): check base..head and export the editable files
    Gate {
        #[arg(long)]
        base: String,
        #[arg(long)]
        head: String,
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        #[arg(long, default_value = "submission")]
        out: PathBuf,
        /// Where to write the gate result JSON
        #[arg(long, default_value = "gate.json")]
        json: PathBuf,
    },
    /// Build, verify and score a challenge; writes a score claim
    Score {
        slug: String,
        #[arg(long, default_value = "challenges")]
        root: PathBuf,
        /// Score baseline/ instead of the checkout's editable files
        #[arg(long, conflicts_with = "submission")]
        baseline: bool,
        /// Directory written by `cb gate`
        #[arg(long)]
        submission: Option<PathBuf>,
        /// Run verify/score as nobody with no network (Linux, sudo)
        #[arg(long)]
        sandbox: bool,
        /// Also score the current record's editable tree on the same sample
        /// and report candidate/record (promotion; needs git history)
        #[arg(long)]
        against_record: bool,
        #[arg(long, default_value = "score.json")]
        out: PathBuf,
    },
    /// Haskell source policy: check CANDIDATE against BASELINE (as `cb gate` does)
    HsPolicy {
        baseline: PathBuf,
        candidate: PathBuf,
        /// Source dirs (module roots), relative to both trees
        #[arg(long = "src", default_value = "src")]
        src: Vec<String>,
    },
    /// Decide an attempt's status and append it to results.jsonl (trusted)
    Record {
        slug: String,
        #[arg(long, default_value = "challenges")]
        root: PathBuf,
        /// baseline | promotion | pr
        #[arg(long)]
        stage: String,
        /// Score claims: two for baseline/promotion, one for pr
        #[arg(long = "run", required = true)]
        runs: Vec<PathBuf>,
        #[arg(long)]
        commit: String,
        #[arg(long)]
        by: String,
        #[arg(long, default_value = "")]
        model: String,
        /// UTC timestamp, e.g. $(date -u +%FT%TZ)
        #[arg(long)]
        at: String,
    },
    /// Render a PR comment from gate/score/record JSON (untrusted text neutralised)
    Comment {
        #[arg(long)]
        gate: PathBuf,
        #[arg(long)]
        score: Option<PathBuf>,
        #[arg(long)]
        entry: Option<PathBuf>,
    },
    /// Write per-challenge feed JSON (toolkit Snapshot fields) + the public status site
    Feed {
        #[arg(long, default_value = "challenges")]
        root: PathBuf,
        #[arg(long, default_value = "target/public")]
        out: PathBuf,
        /// Where <out> is served (feed URLs are built from it)
        #[arg(long)]
        base_url: String,
        /// UTC timestamp, e.g. $(date -u +%FT%TZ)
        #[arg(long)]
        at: String,
    },
    /// Run a participant command under limits (CB_LIMITS, CB_SANDBOX) and measure it
    Participant {
        /// Write the measurement report (JSON) here
        #[arg(long)]
        report: Option<PathBuf>,
        #[arg(last = true, required = true)]
        cmd: Vec<String>,
    },
    /// Score candidates against the rubric
    Rank {
        #[arg(long, default_value = "candidates")]
        dir: PathBuf,
    },
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Import {
            benchmark_json,
            url,
            out,
        } => import(&benchmark_json, &url, &out),
        Command::Participant { report, cmd } => {
            let sandbox = std::env::var("CB_SANDBOX").is_ok_and(|v| v == "1");
            let limits = research::participant::limits_from_env();
            match research::participant::run(&cmd, &limits, sandbox) {
                Ok(r) => {
                    if let Some(p) = report {
                        let text = serde_json::to_string_pretty(&r).unwrap();
                        if let Err(e) = std::fs::write(&p, text + "\n") {
                            eprintln!("cb: write {}: {e}", p.display());
                            std::process::exit(125);
                        }
                    }
                    std::process::exit(research::participant::exit_code(&r));
                }
                Err(e) => {
                    eprintln!("cb: {e}");
                    std::process::exit(125);
                }
            }
        }
        Command::Rank { dir } => candidate::rank(&dir).map(|table| print!("{table}")),
        Command::New {
            slug,
            kind,
            family,
            kit,
            root,
        } => match (kind, family) {
            (ChallengeType::Research, None) if kit.is_none() => {
                research::scaffold::scaffold(&root, &slug)
            }
            (ChallengeType::Research, _) => {
                Err("--family and --kit are for simple challenges".into())
            }
            (ChallengeType::Simple, None) => {
                Err("--family is required for simple challenges".into())
            }
            (ChallengeType::Simple, Some(family)) => {
                let family = family.to_possible_value().unwrap();
                let kit = kit.map(|k| k.to_possible_value().unwrap());
                challenge::scaffold(
                    &root,
                    &slug,
                    family.get_name(),
                    kit.as_ref().map(|k| k.get_name()),
                )
            }
        }
        .map(|dir| println!("created {}", dir.display())),
        Command::Preflight { slug, root, out } => run_preflight(&root.join(&slug), &out),
        Command::Site { dir, out } => {
            site::build(&dir, &out).map(|p| println!("wrote {} (internal only)", p.display()))
        }
        Command::Feed {
            root,
            out,
            base_url,
            at,
        } => feed::build(&root, &out, &base_url, &at).map(|slugs| {
            println!(
                "wrote {} feed(s) + status site to {}: {}",
                slugs.len(),
                out.display(),
                slugs.join(", ")
            )
        }),
        Command::Gate {
            base,
            head,
            repo,
            out,
            json,
        } => {
            let g = pipeline::gate(&repo, "challenges", &base, &head, &out);
            let text = serde_json::to_string_pretty(&g.json(&base, &head)).unwrap();
            std::fs::write(&json, text + "\n")
                .map_err(|e| e.to_string())
                .and_then(|_| {
                    if g.errors.is_empty() {
                        println!("gate ok: {}", g.slug.unwrap_or_default());
                        Ok(())
                    } else {
                        Err(format!("gate refused:\n  {}", g.errors.join("\n  ")))
                    }
                })
        }
        Command::Score {
            slug,
            root,
            baseline,
            submission,
            sandbox,
            against_record,
            out,
        } => {
            let source = match (&submission, baseline) {
                (Some(dir), _) => pipeline::Source::Submission(dir),
                (None, true) => pipeline::Source::Baseline,
                (None, false) => pipeline::Source::AsIs,
            };
            let rec = if against_record {
                pipeline::record_tree(&root, &slug)
            } else {
                Ok(None)
            };
            rec.and_then(|rec| {
                let against = rec.as_ref().map(|(c, t)| (c.as_str(), t.as_path()));
                pipeline::score(&root, &slug, source, sandbox, against)
            })
            .and_then(|v| {
                let text = serde_json::to_string_pretty(&v).unwrap();
                println!("{text}");
                std::fs::write(&out, text + "\n").map_err(|e| e.to_string())
            })
        }
        Command::HsPolicy {
            baseline,
            candidate,
            src,
        } => match haskellpolicy::check(&baseline, &candidate, &src) {
            Ok(()) => {
                println!("haskell policy: clean");
                Ok(())
            }
            Err(e) => Err(format!("haskell policy:\n  {}", e.join("\n  "))),
        },
        Command::Record {
            slug,
            root,
            stage,
            runs,
            commit,
            by,
            model,
            at,
        } => {
            if !matches!(stage.as_str(), "baseline" | "promotion" | "pr") {
                return fail("stage: baseline, promotion or pr");
            }
            runs.iter()
                .map(|p| pipeline::read_claim(p))
                .collect::<Result<Vec<_>, _>>()
                .and_then(|runs| {
                    let a = pipeline::Attempt {
                        stage: &stage,
                        commit: &commit,
                        by: &by,
                        model: &model,
                        at: &at,
                    };
                    let entry = pipeline::record(&root, &slug, &runs, &a)?;
                    let rc = pipeline::record_commit(&root, &slug)?;
                    let editable = challenge::Spec::load(&root.join(&slug))?.paths.editable;
                    println!(
                        "{}",
                        serde_json::json!({"entry": entry, "record_commit": rc, "editable": editable})
                    );
                    Ok(())
                })
        }
        Command::Comment { gate, score, entry } => {
            let load = |p: &PathBuf| pipeline::read_claim(p);
            let gate = load(&gate);
            let score = score.as_ref().map(load).transpose();
            let entry = entry.as_ref().map(load).transpose();
            match (gate, score, entry) {
                (Ok(g), Ok(s), Ok(e)) => {
                    print!(
                        "{}",
                        pipeline::comment(&g, s.as_ref(), e.as_ref().map(|e| &e["entry"]))
                    );
                    Ok(())
                }
                (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => Err(e),
            }
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("cb: {e}");
            ExitCode::FAILURE
        }
    }
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("cb: {msg}");
    ExitCode::FAILURE
}

/// Writes one draft per track; never overwrites an existing candidate.
fn import(json: &Path, url: &str, out: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(json).map_err(|e| format!("{}: {e}", json.display()))?;
    let drafts = candidate::import(&text, url)?;
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    for (slug, body) in drafts {
        let path = out.join(format!("{slug}.toml"));
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .and_then(|mut f| std::io::Write::write_all(&mut f, body.as_bytes()))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

/// Prints the report, writes it as JSON, and fails if anything blocks launch.
fn run_preflight(dir: &Path, out: &Path) -> Result<(), String> {
    let report = preflight::run_all(dir)?;
    print!("{}", preflight::render(&report));
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let path = out.join(format!("{}.json", report.slug));
    let json = serde_json::to_string_pretty(&report.json()).map_err(|e| e.to_string())?;
    std::fs::write(&path, json + "\n").map_err(|e| format!("{}: {e}", path.display()))?;
    println!("wrote {}", path.display());
    if report.blocked() {
        Err("preflight blocked".into())
    } else {
        Ok(())
    }
}
