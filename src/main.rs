use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use daemar::{PreflightError, preflight};

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

fn main() -> ExitCode {
    match Cli::parse().command {
        DaemarCommand::Run {
            change_request_path,
        } => run(change_request_path),
        DaemarCommand::Runs { .. } | DaemarCommand::Show { .. } => ExitCode::SUCCESS,
    }
}

fn run(change_request_path: PathBuf) -> ExitCode {
    let raw = match std::fs::read(&change_request_path) {
        Ok(raw) => raw,
        Err(error) => {
            report_invalid_request(
                &change_request_path,
                std::slice::from_ref(&PreflightError::io_error(&error)),
            );
            return ExitCode::from(1);
        }
    };

    match preflight(&raw) {
        Ok(_change_request) => ExitCode::SUCCESS,
        Err(diagnostics) => {
            report_invalid_request(&change_request_path, &diagnostics);
            ExitCode::from(1)
        }
    }
}

fn report_invalid_request(path: &std::path::Path, diagnostics: &[PreflightError]) {
    eprintln!(
        "error: invalid Change Request - {} problem(s) in {}\n",
        diagnostics.len(),
        path.display()
    );
    for diagnostic in diagnostics {
        eprintln!("{diagnostic}");
    }
    eprintln!("\nno Workflow Run created");
}
