use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "daemar", about = "Execute and inspect Daemar Workflows")]
struct Cli {
    #[command(subcommand)]
    command: DaemarCommand,
}

#[derive(Subcommand)]
enum DaemarCommand {
    /// Start a Workflow Run from a Change Request.
    Run {
        /// Path to the human-approved Change Request.
        change_request_path: PathBuf,
    },
    /// List Workflow Runs, optionally filtered by Change Request slug.
    Runs {
        /// Human-authored Change Request slug.
        change_request_slug: Option<String>,
    },
    /// Show one Workflow Run by an unambiguous ID prefix.
    Show {
        /// Workflow Run ID or unambiguous prefix.
        workflow_run_id_or_prefix: String,
    },
}

fn main() {
    let _cli = Cli::parse();
}
