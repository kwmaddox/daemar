use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use daemar_sandbox::{Change, RunSpec};

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
        #[arg(long, default_value_t = 300)]
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
        spec.image = image;
    }
    spec.timeout = Duration::from_secs(timeout);

    let outcome = match daemar_sandbox::run(&spec) {
        Ok(outcome) => outcome,
        Err(e) => {
            eprintln!("dsbx: {e}");
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
                    eprintln!("  symlink   {} -> {}", path.display(), target.display())
                }
            }
        }
        for (path, why) in outcome.changes.unsupported() {
            eprintln!("  !unsupported {} ({why})", path.display());
        }
    }

    let dest = if apply {
        Some(worktree)
    } else {
        apply_to
    };
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
                    eprintln!("  rejected {}: {why}", path.display());
                }
            }
            Err(e) => {
                eprintln!("dsbx: apply failed: {e}");
                return ExitCode::from(2);
            }
        }
    }

    // Mirror the workload's exit code so dsbx composes in scripts (B10).
    u8::try_from(outcome.exit_code)
        .map(ExitCode::from)
        .unwrap_or(ExitCode::FAILURE)
}
