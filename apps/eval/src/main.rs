//! The eval rig's CLI: arg parsing and presentation only.
//!
//!     daemar-eval run [--case <id>]... [--class <class>]... [--runs <n>]
//!     daemar-eval compare <dossier-a> <dossier-b>
//!
//! `run` flies real, paid flights; invoke it deliberately (via `just eval`).
//! Exit codes: 0 every graded flight passed · 1 something failed or was not
//! gradable (the dossier still landed) · 2 the run never started.

use std::path::PathBuf;
use std::process::ExitCode;

use daemar_eval::run::{Roots, RunOutcome, Selection};
use daemar_eval::{compare, run, EvalError};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("run") => match parse_selection(&args[1..]) {
            Some(selection) => fly(&selection),
            None => usage(),
        },
        Some("compare") => match &args[1..] {
            [left, right] => match compare::compare(&PathBuf::from(left), &PathBuf::from(right)) {
                Ok(text) => {
                    println!("{text}");
                    ExitCode::SUCCESS
                }
                Err(error) => refuse(&error),
            },
            _ => usage(),
        },
        _ => usage(),
    }
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: daemar-eval run [--case <id>]... [--class <class>]... [--runs <n>]\n       \
         daemar-eval compare <dossier-a> <dossier-b>"
    );
    ExitCode::from(2)
}

fn refuse(error: &EvalError) -> ExitCode {
    eprintln!("daemar-eval: {error}");
    ExitCode::from(2)
}

fn parse_selection(args: &[String]) -> Option<Selection> {
    let mut selection = Selection {
        ids: Vec::new(),
        classes: Vec::new(),
        runs: 1,
    };
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        let value = rest.next()?;
        match flag.as_str() {
            "--case" => selection.ids.push(value.clone()),
            "--class" => selection.classes.push(value.clone()),
            "--runs" => selection.runs = value.parse().ok().filter(|n| *n >= 1)?,
            _ => return None,
        }
    }
    Some(selection)
}

fn fly(selection: &Selection) -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(error) => {
            eprintln!("daemar-eval: cannot resolve cwd: {error}");
            return ExitCode::from(2);
        }
    };
    let roots = Roots::standard(&cwd);
    let mut narrate = |line: &str| eprintln!("daemar-eval: {line}");
    match run::run(&roots, selection, &mut narrate) {
        Ok(RunOutcome {
            dossier_dir,
            all_passed,
        }) => {
            println!("{}", dossier_dir.display());
            eprintln!(
                "daemar-eval: dossier at {} · summary.md inside · {}",
                dossier_dir.display(),
                if all_passed {
                    "all graded flights passed"
                } else {
                    "NOT all flights passed — see summary.md"
                }
            );
            if all_passed {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => refuse(&error),
    }
}
