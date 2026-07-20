use std::fmt;
use std::fs::File;
use std::io;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use daemar::{ChangeRequestProblem, change_request_document_byte_limit, preflight};

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
    let change_request_document = match read_change_request_document(path) {
        Ok(change_request_document) => change_request_document,
        Err(error) => return report_read_error(&error),
    };
    match preflight(&change_request_document) {
        Ok(_) => ExitCode::SUCCESS,
        Err(problems) => report_invalid_request(path, &problems),
    }
}

fn read_change_request_document(path: &Path) -> Result<Vec<u8>, ChangeRequestReadError<'_>> {
    let file = File::open(path).map_err(|source| ChangeRequestReadError { path, source })?;
    read_bounded_change_request_document(file)
        .map_err(|source| ChangeRequestReadError { path, source })
}

fn read_bounded_change_request_document(reader: impl Read) -> Result<Vec<u8>, io::Error> {
    let read_limit = change_request_document_byte_limit().saturating_add(1);
    let mut change_request_document = Vec::with_capacity(read_limit);
    let read_limit = u64::try_from(read_limit).unwrap_or(u64::MAX);
    reader
        .take(read_limit)
        .read_to_end(&mut change_request_document)?;
    Ok(change_request_document)
}

struct ChangeRequestReadError<'a> {
    path: &'a Path,
    source: io::Error,
}

impl fmt::Display for ChangeRequestReadError<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "cannot read Change Request from {}: {}",
            self.path.display(),
            self.source
        )
    }
}

fn report_read_error(error: &ChangeRequestReadError<'_>) -> ExitCode {
    eprintln!("error: {error}\n\nno Workflow Run created");
    ExitCode::from(1)
}

fn report_invalid_request(path: &Path, problems: &[ChangeRequestProblem]) -> ExitCode {
    eprintln!(
        "error: invalid Change Request - {} problem(s) in {}\n",
        problems.len(),
        path.display()
    );
    for problem in problems {
        eprintln!("  {problem}");
    }
    eprintln!("\nno Workflow Run created");
    ExitCode::from(1)
}

#[cfg(test)]
mod tests {
    use std::io::{self, Read};

    use daemar::change_request_document_byte_limit;

    use super::read_bounded_change_request_document;

    struct EndlessReader;

    impl Read for EndlessReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            buffer.fill(b'x');
            Ok(buffer.len())
        }
    }

    #[test]
    fn change_request_reading_stops_one_byte_past_the_policy_limit() {
        let document = read_bounded_change_request_document(EndlessReader)
            .expect("the bounded reader should finish");

        assert_eq!(document.len(), change_request_document_byte_limit() + 1);
    }
}
