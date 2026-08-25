//! `dsbx`: manual driver for `daemar-sandbox`. Run one command in the
//! sandbox, print its output and change-set, optionally promote the
//! changes (behaviors: `specs/sandbox.md`).

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use daemar_sandbox::{Change, RunSpec, DEFAULT_TIMEOUT};

/// Run one command inside the daemar sandbox (specs/sandbox.md).
#[derive(Parser)]
#[command(name = "dsbx", version)]
enum Cli {
    /// Run a command against a worktree; print output and the change-set.
    Run {
        /// Host directory mounted read-only as the command's working tree.
        #[arg(long)]
        worktree: PathBuf,
        /// Guest image (default: pinned ubuntu digest).
        #[arg(long)]
        image: Option<String>,
        /// Timeout in seconds.
        #[arg(long, default_value_t = DEFAULT_TIMEOUT.as_secs())]
        timeout: u64,
        /// Promote the changes back into the worktree after the run.
        #[arg(long, conflicts_with = "apply_to")]
        apply: bool,
        /// Promote the changes into this directory instead of the worktree.
        #[arg(long)]
        apply_to: Option<PathBuf>,
        /// The command to run (everything after `--`).
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
}

/// Print an error and its full `source()` chain. Messages describe the
/// failure; causes live in the chain (conventions.md C2).
fn report(context: &str, e: &daemar_sandbox::Error) {
    eprintln!("dsbx: {context}{e}");
    let mut cause = std::error::Error::source(e);
    while let Some(c) = cause {
        eprintln!("dsbx:   caused by: {c}");
        cause = c.source();
    }
}

fn main() -> ExitCode {
    let Cli::Run {
        worktree,
        image,
        timeout,
        apply,
        apply_to,
        command,
    } = Cli::parse();

    let mut spec = RunSpec::new(command, &worktree);
    if let Some(image) = image {
        spec.image = image.into();
    }
    spec.timeout = Duration::from_secs(timeout);

    let outcome = match daemar_sandbox::run(&spec) {
        Ok(outcome) => outcome,
        Err(e) => {
            report("", &e);
            return ExitCode::from(2);
        }
    };

    std::io::stdout().write_all(&outcome.stdout).ok();
    std::io::stderr().write_all(&outcome.stderr).ok();
    eprintln!("── exit code: {}", outcome.exit_code);

    if outcome.changes.is_empty() {
        eprintln!("── changes: none");
    } else {
        eprintln!("── changes:");
        for change in outcome.changes.entries() {
            match change {
                Change::Added { path, .. } => eprintln!("  added     {}", path.display()),
                Change::Modified { path, .. } => eprintln!("  modified  {}", path.display()),
                Change::Deleted { path } => eprintln!("  deleted   {}", path.display()),
                Change::DirAdded { path } => eprintln!("  dir       {}", path.display()),
                Change::Symlink { path, target } => {
                    eprintln!("  symlink   {} -> {}", path.display(), target.display());
                }
            }
        }
        for (path, why) in outcome.changes.unsupported() {
            eprintln!("  !unsupported {} ({why})", path.display());
        }
    }

    let dest = if apply { Some(worktree) } else { apply_to };
    if let Some(dest) = dest {
        match outcome.changes.apply_to(&dest) {
            Ok(report) => {
                eprintln!(
                    "── applied {} change(s), {} deletion(s) to {}",
                    report.applied.len(),
                    report.deleted.len(),
                    dest.display()
                );
                for path in &report.stripped {
                    eprintln!("  stripped setuid/setgid: {}", path.display());
                }
                for (path, why) in &report.rejected {
                    // The reason's io cause lives in its source() (C2);
                    // surface it inline so no detail is lost at the CLI.
                    match std::error::Error::source(why) {
                        Some(cause) => {
                            eprintln!("  rejected {}: {why}: {cause}", path.display());
                        }
                        None => eprintln!("  rejected {}: {why}", path.display()),
                    }
                }
            }
            Err(e) => {
                report("apply failed: ", &e);
                return ExitCode::from(2);
            }
        }
    }

    // Mirror the workload's exit code so dsbx composes in scripts (B10).
    u8::try_from(outcome.exit_code).map_or(ExitCode::FAILURE, ExitCode::from)
}
