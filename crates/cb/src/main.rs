use clap::{Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod candidate;
mod challenge;
mod paths;
mod policy;
mod preflight;
mod rubric;
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
enum Kit {
    Rust,
}

#[derive(Subcommand)]
enum Command {
    /// Scaffold challenges/<slug>/
    New {
        slug: String,
        #[arg(long)]
        family: Family,
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
    /// Write per-challenge feed JSON (toolkit Snapshot)
    Feed,
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
        Command::Rank { dir } => candidate::rank(&dir).map(|table| print!("{table}")),
        Command::New {
            slug,
            family,
            kit,
            root,
        } => {
            let family = family.to_possible_value().unwrap();
            let kit = kit.map(|k| k.to_possible_value().unwrap());
            challenge::scaffold(
                &root,
                &slug,
                family.get_name(),
                kit.as_ref().map(|k| k.get_name()),
            )
            .map(|dir| println!("created {}", dir.display()))
        }
        Command::Preflight { slug, root, out } => run_preflight(&root.join(&slug), &out),
        Command::Site { dir, out } => {
            site::build(&dir, &out).map(|p| println!("wrote {} (internal only)", p.display()))
        }
        Command::Feed => stub("feed"),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("cb: {e}");
            ExitCode::FAILURE
        }
    }
}

fn stub(name: &str) -> Result<(), String> {
    Err(format!("{name}: not implemented yet"))
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
