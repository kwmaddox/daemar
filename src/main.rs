use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use daemar::{ChangeRequestProblem, ChangeRequestRule, execute_run};

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
        } => run(&change_request_path),
        DaemarCommand::Runs { .. } | DaemarCommand::Show { .. } => ExitCode::SUCCESS,
    }
}

fn run(path: &Path) -> ExitCode {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return report_invalid_request(
                path,
                &[ChangeRequestProblem {
                    code: ChangeRequestRule::IoError,
                    pointer: "/".to_owned(),
                    message: format!("cannot read file: {error}"),
                }],
            );
        }
    };
    match execute_run(&bytes, |_| ()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(problems) => report_invalid_request(path, &problems),
    }
}

fn report_invalid_request(path: &Path, problems: &[ChangeRequestProblem]) -> ExitCode {
    eprintln!(
        "error: invalid Change Request - {} problem(s) in {}\n",
        problems.len(),
        path.display()
    );
    for problem in problems {
        eprintln!(
            "  [{}] {} (at {})",
            problem.code, problem.message, problem.pointer
        );
    }
    eprintln!("\nno Workflow Run created");
    ExitCode::from(1)
}
