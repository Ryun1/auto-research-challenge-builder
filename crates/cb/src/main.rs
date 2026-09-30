use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use std::process::ExitCode;

mod paths;
mod rubric;

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

#[derive(Subcommand)]
enum Command {
    /// Scaffold challenges/<slug>/
    New {
        slug: String,
        #[arg(long)]
        family: Family,
        #[arg(long)]
        kit: Option<String>,
    },
    /// Run launch checks for a challenge
    Preflight {
        slug: String,
        #[arg(long)]
        local: bool,
    },
    /// Pre-fill a candidate from a Yukon-style benchmark.json
    Import { benchmark_json: PathBuf },
    /// Render the static ranking and leaderboard site
    Site,
    /// Write per-challenge feed JSON (toolkit Snapshot)
    Feed,
    /// Score candidates against the rubric
    Rank,
}

fn main() -> ExitCode {
    let name = match Cli::parse().command {
        Command::New { .. } => "new",
        Command::Preflight { .. } => "preflight",
        Command::Import { .. } => "import",
        Command::Site => "site",
        Command::Feed => "feed",
        Command::Rank => "rank",
    };
    eprintln!("cb {name}: not implemented yet");
    ExitCode::from(2)
}
